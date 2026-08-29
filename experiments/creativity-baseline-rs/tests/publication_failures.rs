use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use garive_creativity_baseline::{
    build_publication_generator, CreativityGeneratorPort, CredentialReferenceResolver,
    CredentialResolutionFailure, GeneratorRequest, ModelEndpointConfig, ModelProtocol,
};
use garive_eval::{CreativityArm, EvaluationCaseId};
use garive_provider_profile::SecretValue;

#[test]
fn redirect_timeout_status_size_and_malformed_protocol_fail_without_retry() {
    let cases = [
        Failure::Redirect,
        Failure::Timeout,
        Failure::Status,
        Failure::Oversized,
        Failure::Malformed,
    ];
    for failure in cases {
        let server = Server::new(failure);
        let mut config = config(&server.url);
        if matches!(failure, Failure::Timeout) {
            config.request_timeout_ms = 10;
        }
        if matches!(failure, Failure::Oversized) {
            config.max_response_bytes = 16;
        }
        let (generator, _) = build_publication_generator(config, &Resolver).unwrap();
        let result = generator.generate(GeneratorRequest {
            task_id: &EvaluationCaseId::new("failure-task").unwrap(),
            arm: CreativityArm::Control,
            prompt: "bounded prompt",
            seed: 1,
            max_candidates: 1,
            max_candidate_utf8_bytes: 64,
            max_total_candidate_utf8_bytes: 64,
        });
        assert!(result.is_err(), "accepted failure case {failure:?}");
        assert_eq!(server.join(), 1, "retried failure case {failure:?}");
    }
}

fn config(endpoint: &str) -> ModelEndpointConfig {
    ModelEndpointConfig {
        protocol: ModelProtocol::ResponsesCompatible,
        target_id: "target".into(),
        model_id: "model".into(),
        model_revision: "model-v1".into(),
        endpoint: endpoint.into(),
        credential_ref: "fixture".into(),
        credential_header_name: "authorization".into(),
        credential_header_prefix: "Bearer ".into(),
        non_secret_headers: Vec::new(),
        messages_version_header_name: None,
        messages_protocol_version: None,
        max_output_tokens: 100,
        connect_timeout_ms: 100,
        request_timeout_ms: 100,
        max_response_bytes: 65536,
    }
}

struct Resolver;
impl CredentialReferenceResolver for Resolver {
    fn resolve(&self, _: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        SecretValue::new("secret").map_err(|_| CredentialResolutionFailure)
    }
}

#[derive(Clone, Copy, Debug)]
enum Failure {
    Redirect,
    Timeout,
    Status,
    Oversized,
    Malformed,
}

struct Server {
    url: String,
    calls: Arc<AtomicUsize>,
    worker: thread::JoinHandle<()>,
}

impl Server {
    fn new(failure: Failure) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            count.fetch_add(1, Ordering::SeqCst);
            let mut request = Vec::new();
            let mut buffer = [0_u8; 4096];
            let header_end = loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    return;
                }
                request.extend_from_slice(&buffer[..read]);
                if let Some(index) = request.windows(4).position(|value| value == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            while request.len() < header_end + length {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }
            if matches!(failure, Failure::Timeout) {
                thread::sleep(Duration::from_millis(80));
            }
            let response = match failure {
                Failure::Redirect => b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                Failure::Status => b"HTTP/1.1 500 Error\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
                Failure::Timeout | Failure::Malformed => b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
                Failure::Oversized => {
                    let body = vec![b'x'; 128];
                    let mut value = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
                    value.extend(body);
                    value
                }
            };
            let _ = stream.write_all(&response);
        });
        Self {
            url: format!("http://{address}/v1/responses"),
            calls,
            worker,
        }
    }

    fn join(self) -> usize {
        self.worker.join().unwrap();
        self.calls.load(Ordering::SeqCst)
    }
}
