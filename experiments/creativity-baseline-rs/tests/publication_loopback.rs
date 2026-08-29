use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
};

use garive_creativity_baseline::{
    build_publication_evaluator, build_publication_generator, load_creativity_corpus,
    run_creativity_baseline, CredentialReferenceResolver, CredentialResolutionFailure,
    ModelEndpointConfig, ModelProtocol, NonSecretHeader,
};
use garive_provider_profile::SecretValue;
use serde_json::{json, Value};

const CORPUS: &[u8] = include_bytes!("../../../spec/fixtures/eval/creativity-corpus-v1.json");

#[test]
fn paired_run_uses_normal_responses_generator_and_blind_messages_evaluator() {
    let generator_server = Server::new(8, Dialect::ResponsesGenerator);
    let evaluator_server = Server::new(8, Dialect::MessagesEvaluator);
    let resolver = Resolver;
    let (generator, generator_coordinate) = build_publication_generator(
        config(ModelProtocol::ResponsesCompatible, &generator_server.url),
        &resolver,
    )
    .unwrap();
    let (evaluator, evaluator_coordinate) = build_publication_evaluator(
        config(ModelProtocol::MessagesCompatible, &evaluator_server.url),
        &resolver,
    )
    .unwrap();
    assert!(!generator_coordinate.port.publishable);
    assert!(!evaluator_coordinate.port.publishable);

    let corpus = load_creativity_corpus(CORPUS).unwrap();
    let run = run_creativity_baseline(&corpus, &generator, &evaluator, 42).unwrap();
    assert_eq!(run.summary.ordered_pairs.len(), 4);
    assert_eq!(run.summary.control.candidate_count, 4);
    assert_eq!(run.summary.bounded_alternatives.candidate_count, 8);

    let generator_requests = generator_server.join();
    let evaluator_requests = evaluator_server.join();
    assert_eq!(generator_requests.len(), 8);
    assert_eq!(evaluator_requests.len(), 8);
    for request in generator_requests {
        assert!(request
            .headers
            .contains("authorization: Bearer fixture-secret\r\n"));
        let payload = user_payload(&request.body, Dialect::ResponsesGenerator);
        assert!(payload.get("arm").is_some());
        assert!(payload.get("prompt").is_some());
        assert!(payload.get("evaluator_rubric").is_none());
    }
    for request in evaluator_requests {
        assert!(request.headers.contains("x-api-key: fixture-secret\r\n"));
        assert!(request.headers.contains("protocol-version: 2026-01-01\r\n"));
        let payload = user_payload(&request.body, Dialect::MessagesEvaluator);
        assert!(payload.get("evaluator_rubric").is_some());
        assert!(payload.get("arm").is_none());
        assert!(payload.get("selected_candidate_id").is_none());
        assert!(payload.get("generator").is_none());
    }
}

fn config(protocol: ModelProtocol, endpoint: &str) -> ModelEndpointConfig {
    let messages = matches!(protocol, ModelProtocol::MessagesCompatible);
    ModelEndpointConfig {
        protocol,
        target_id: "creativity-target".into(),
        model_id: "fixture-model".into(),
        model_revision: "fixture-model-v1".into(),
        endpoint: endpoint.into(),
        credential_ref: "fixture-account".into(),
        credential_header_name: if messages {
            "x-api-key"
        } else {
            "authorization"
        }
        .into(),
        credential_header_prefix: if messages { "" } else { "Bearer " }.into(),
        non_secret_headers: vec![NonSecretHeader {
            name: "x-route".into(),
            value: "fixture".into(),
        }],
        messages_version_header_name: messages.then(|| "protocol-version".into()),
        messages_protocol_version: messages.then(|| "2026-01-01".into()),
        max_output_tokens: 2048,
        connect_timeout_ms: 1000,
        request_timeout_ms: 1000,
        max_response_bytes: 65536,
    }
}

struct Resolver;
impl CredentialReferenceResolver for Resolver {
    fn resolve(&self, _: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        SecretValue::new("fixture-secret").map_err(|_| CredentialResolutionFailure)
    }
}

#[derive(Clone, Copy)]
enum Dialect {
    ResponsesGenerator,
    MessagesEvaluator,
}

struct Request {
    headers: String,
    body: Value,
}

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<Request>>>,
    worker: thread::JoinHandle<()>,
}

impl Server {
    fn new(count: usize, dialect: Dialect) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let worker = thread::spawn(move || {
            for stream in listener.incoming().take(count) {
                let mut stream = stream.unwrap();
                let request = read_request(&mut stream);
                let payload = user_payload(&request.body, dialect);
                let response = response(payload, dialect);
                captured.lock().unwrap().push(request);
                write_response(&mut stream, &response);
            }
        });
        Self {
            url: format!("http://{address}/v1/model"),
            requests,
            worker,
        }
    }

    fn join(self) -> Vec<Request> {
        self.worker.join().unwrap();
        Arc::try_unwrap(self.requests)
            .ok()
            .unwrap()
            .into_inner()
            .unwrap()
    }
}

fn read_request(stream: &mut TcpStream) -> Request {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    let header_end = loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .map(str::to_owned)
        })
        .unwrap()
        .parse::<usize>()
        .unwrap();
    while bytes.len() < header_end + length {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    Request {
        headers,
        body: serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap(),
    }
}

fn user_payload(body: &Value, dialect: Dialect) -> Value {
    let text = match dialect {
        Dialect::ResponsesGenerator => body["input"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["role"] == "user")
            .unwrap()["content"][0]["text"]
            .as_str()
            .unwrap(),
        Dialect::MessagesEvaluator => body["messages"][0]["content"][0]["text"].as_str().unwrap(),
    };
    serde_json::from_str(text).unwrap()
}

fn response(payload: Value, dialect: Dialect) -> Value {
    let result = match dialect {
        Dialect::ResponsesGenerator => {
            let count = if payload["arm"] == "control" { 1 } else { 2 };
            let candidates = (0..count)
                .map(|index| {
                    json!({"candidate_id":format!("candidate-{index}"),
                    "content":format!("alternative {index}")})
                })
                .collect::<Vec<_>>();
            json!({"schema_version":1,"candidates":candidates,
                "selected_candidate_id":"candidate-0"})
        }
        Dialect::MessagesEvaluator => {
            let verdicts = payload["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|candidate| json!({"candidate_id":candidate["candidate_id"],
                    "correct":true,
                    "correct_cluster_id":format!("cluster-{}",candidate["candidate_id"].as_str().unwrap())}))
                .collect::<Vec<_>>();
            json!({"schema_version":1,"verdicts":verdicts})
        }
    };
    match dialect {
        Dialect::ResponsesGenerator => json!({"id":"resp_fixture","created_at":1.0,
            "error":null,"incomplete_details":null,"instructions":null,"metadata":null,
            "model":"fixture-model","object":"response","output":[{"id":"msg_fixture",
            "type":"message","status":"completed","role":"assistant","content":[{
            "type":"output_text","text":result.to_string(),"annotations":[]}]}],
            "parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],
            "top_p":null,"status":"completed","usage":{"input_tokens":10,
            "input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},
            "output_tokens":5,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":15}}),
        Dialect::MessagesEvaluator => json!({"id":"msg_fixture","type":"message",
            "role":"assistant","model":"fixture-model","content":[{"type":"text",
            "text":result.to_string()}],"stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":10,"cache_creation_input_tokens":0,
            "cache_read_input_tokens":0,"output_tokens":5}}),
    }
}

fn write_response(stream: &mut TcpStream, value: &Value) {
    let body = serde_json::to_vec(value).unwrap();
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    stream.write_all(&body).unwrap();
}
