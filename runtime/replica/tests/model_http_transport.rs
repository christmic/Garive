use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelInputContent, ModelInputItem,
    ModelObserver, ModelOutputSettings, ModelPort, ModelPortFailure, ModelRequest, ModelRequestId,
    ModelRole, ModelStreamEvent, ModelTargetId, ObserverDecision, TextMode, UnavailableKind,
};
use garive_provider_anthropic::build_profile as build_anthropic_profile;
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy, ResponsesDeployment};
use garive_provider_openai::build_profile as build_openai_profile;
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use garive_runtime::{RuntimeHttpLimits, RuntimeHttpTransportError, RuntimeModelHttpTransport};

const OPENAI_JSON: &str =
    include_str!("../../../spec/fixtures/protocols/openai-responses/ordinary.json");
const OPENAI_SSE: &str =
    include_str!("../../../spec/fixtures/protocols/openai-responses/complete.sse");
const ANTHROPIC_JSON: &str =
    include_str!("../../../spec/fixtures/protocols/anthropic-messages/ordinary.json");
const ANTHROPIC_SSE: &str =
    include_str!("../../../spec/fixtures/protocols/anthropic-messages/complete.sse");

#[tokio::test]
async fn official_buffered_transports_send_exact_auth_and_normalize_terminals() {
    let openai_server = Server::one(Response::json(200, OPENAI_JSON));
    let openai = openai_transport(&openai_server.url, false, limits(1_000_000));
    let outcome = invoke(&openai, request(false)).await.unwrap();
    assert!(matches!(outcome, InvokeOutcome::Completed { .. }));
    let openai_request = openai_server.join();
    assert!(openai_request.contains("authorization: Bearer fixture-secret\r\n"));
    assert!(openai_request.contains("accept: application/json\r\n"));

    let anthropic_server = Server::one(Response::json(200, ANTHROPIC_JSON));
    let anthropic = anthropic_transport(&anthropic_server.url, false, limits(1_000_000));
    let outcome = invoke(&anthropic, request(false)).await.unwrap();
    assert!(matches!(outcome, InvokeOutcome::Completed { .. }));
    let anthropic_request = anthropic_server.join();
    assert!(anthropic_request.contains("x-api-key: fixture-secret\r\n"));
    assert!(anthropic_request.contains("anthropic-version: 2023-06-01\r\n"));
}

#[tokio::test]
async fn official_exact_errors_classify_without_message_guessing() {
    let openai_server = Server::one(
        Response::json(
            429,
            r#"{"error":{"code":"rate_limit_exceeded","message":"ignored","param":null,"type":"rate_limit_error"}}"#,
        )
        .header("Retry-After", "7"),
    );
    let openai = openai_transport(&openai_server.url, false, limits(10_000));
    assert_eq!(
        invoke(&openai, request(false)).await.unwrap(),
        InvokeOutcome::Unavailable {
            kind: UnavailableKind::RateLimited,
            retry_after: Some(Duration::from_secs(7)),
        }
    );
    openai_server.join();

    let anthropic_server = Server::one(Response::json(
        529,
        r#"{"type":"error","error":{"type":"overloaded_error","message":"ignored"}}"#,
    ));
    let anthropic = anthropic_transport(&anthropic_server.url, false, limits(10_000));
    assert_eq!(
        invoke(&anthropic, request(false)).await.unwrap(),
        InvokeOutcome::Unavailable {
            kind: UnavailableKind::ModelUnavailable,
            retry_after: None,
        }
    );
    anthropic_server.join();
}

#[tokio::test]
async fn fragmented_protocol_streams_emit_events_and_match_completed_text() {
    let openai_server = Server::one(Response::sse(OPENAI_SSE, 7));
    let openai = openai_transport(&openai_server.url, true, limits(1_000_000));
    let (outcome, events) = invoke_observed(&openai, request(true)).await.unwrap();
    assert!(events.len() >= 4);
    assert!(completed_text(&outcome).contains("hello back"));
    assert!(openai_server
        .join()
        .contains("accept: text/event-stream\r\n"));

    let anthropic_server = Server::one(Response::sse(ANTHROPIC_SSE, 5));
    let anthropic = anthropic_transport(&anthropic_server.url, true, limits(1_000_000));
    let (outcome, events) = invoke_observed(&anthropic, request(true)).await.unwrap();
    assert!(events.len() >= 5);
    assert!(completed_text(&outcome).contains("hello back"));
    assert!(anthropic_server
        .join()
        .contains("accept: text/event-stream\r\n"));
}

#[tokio::test]
async fn observer_cancellation_returns_only_validated_partial_state() {
    let server = Server::one(Response::sse(OPENAI_SSE, 32));
    let transport = openai_transport(&server.url, true, limits(1_000_000));
    let mut observer = CancelFirst { calls: 0 };
    let outcome = transport
        .invoke(&request(true), &mut observer, &NeverCancel)
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        InvokeOutcome::Interrupted {
            kind: garive_llm::InterruptionKind::Cancelled,
            ..
        }
    ));
    assert_eq!(observer.calls, 1);
    server.join();
}

#[tokio::test]
async fn redirect_timeout_size_truncation_and_preflight_fail_closed() {
    let oversized_server = Server::one(Response::json(200, OPENAI_JSON));
    let oversized = openai_transport(&oversized_server.url, false, limits(16));
    assert_eq!(
        invoke(&oversized, request(false)).await,
        Err(ModelPortFailure::RequiredPortFailure)
    );
    oversized_server.join();

    let truncated_server = Server::one(Response::sse("event: response.created\ndata: {}\n\n", 3));
    let truncated = openai_transport(&truncated_server.url, true, limits(10_000));
    assert_eq!(
        invoke(&truncated, request(true)).await,
        Err(ModelPortFailure::AdapterInvariant)
    );
    truncated_server.join();

    let timeout_server = Server::one(Response::delayed_json(OPENAI_JSON, 100));
    let timeout = openai_transport(
        &timeout_server.url,
        false,
        RuntimeHttpLimits {
            connect_timeout_ms: 50,
            request_timeout_ms: 20,
            max_response_bytes: 1_000_000,
        },
    );
    assert_eq!(
        invoke(&timeout, request(false)).await,
        Err(ModelPortFailure::RequiredPortFailure)
    );
    timeout_server.join();

    let redirect_server = Server::one(Response::redirect("http://127.0.0.1:9/forbidden"));
    let redirect = openai_transport(&redirect_server.url, false, limits(10_000));
    assert_eq!(
        invoke(&redirect, request(false)).await,
        Err(ModelPortFailure::AdapterInvariant)
    );
    redirect_server.join();

    let idle = Server::idle();
    let transport = openai_transport(&idle.url, false, limits(10_000));
    let mut mismatched = request(false);
    mismatched.target_id = ModelTargetId::new("other-target");
    assert_eq!(
        transport.preflight(&mismatched),
        Err(ModelPortFailure::UnsupportedCapability)
    );
    assert_eq!(idle.requests(), 0);
    idle.stop();

    assert_eq!(
        RuntimeModelHttpTransport::openai(
            responses_deployment(false),
            openai_profile("http://127.0.0.1:9/responses"),
            limits(0),
        )
        .unwrap_err(),
        RuntimeHttpTransportError::InvalidLimits
    );
}

fn openai_transport(
    endpoint: &str,
    streaming: bool,
    limits: RuntimeHttpLimits,
) -> RuntimeModelHttpTransport {
    RuntimeModelHttpTransport::openai(
        responses_deployment(streaming),
        openai_profile(endpoint),
        limits,
    )
    .unwrap()
}

fn anthropic_transport(
    endpoint: &str,
    streaming: bool,
    limits: RuntimeHttpLimits,
) -> RuntimeModelHttpTransport {
    RuntimeModelHttpTransport::anthropic(
        messages_deployment(streaming),
        anthropic_profile(endpoint),
        limits,
    )
    .unwrap()
}

fn openai_profile(endpoint: &str) -> garive_provider_openai::OpenAiProfile {
    build_openai_profile(&ConnectionInput::new(
        EndpointSelection::Explicit(endpoint.into()),
        SecretValue::new("fixture-secret").unwrap(),
        vec![],
    ))
    .unwrap()
}

fn anthropic_profile(endpoint: &str) -> garive_provider_anthropic::AnthropicProfile {
    build_anthropic_profile(&ConnectionInput::new(
        EndpointSelection::Explicit(endpoint.into()),
        SecretValue::new("fixture-secret").unwrap(),
        vec![],
    ))
    .unwrap()
}

fn responses_deployment(streaming: bool) -> ResponsesDeployment {
    ResponsesDeployment {
        target_id: "target".into(),
        model_id: "model-fixture".into(),
        capabilities: capabilities(streaming),
        default_max_output_tokens: Some(32),
        media_bindings: BTreeMap::new(),
        reasoning: None,
        error_policy: ProtocolErrorPolicy::default(),
    }
}

fn messages_deployment(streaming: bool) -> MessagesDeployment {
    MessagesDeployment {
        target_id: "target".into(),
        model_id: "claude-sonnet-4-5".into(),
        capabilities: capabilities(streaming),
        default_max_output_tokens: Some(32),
        media_bindings: BTreeMap::new(),
        thinking: None,
        error_policy: ProtocolErrorPolicy::default(),
    }
}

fn capabilities(streaming: bool) -> BTreeSet<ModelCapability> {
    let mut values = BTreeSet::from([ModelCapability::Text]);
    if streaming {
        values.insert(ModelCapability::Streaming);
    }
    values
}

fn request(streaming: bool) -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request-1"),
        target_id: ModelTargetId::new("target"),
        required_capabilities: if streaming {
            vec![ModelCapability::Text, ModelCapability::Streaming]
        } else {
            vec![ModelCapability::Text]
        },
        input_items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("hello".into())],
        }],
        tools: vec![],
        output: ModelOutputSettings {
            max_output_tokens: Some(32),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        trace_metadata: vec![],
    }
}

fn limits(maximum: usize) -> RuntimeHttpLimits {
    RuntimeHttpLimits {
        connect_timeout_ms: 1_000,
        request_timeout_ms: 2_000,
        max_response_bytes: maximum,
    }
}

async fn invoke(
    transport: &RuntimeModelHttpTransport,
    request: ModelRequest,
) -> Result<InvokeOutcome, ModelPortFailure> {
    invoke_observed(transport, request)
        .await
        .map(|(outcome, _)| outcome)
}

async fn invoke_observed(
    transport: &RuntimeModelHttpTransport,
    request: ModelRequest,
) -> Result<(InvokeOutcome, Vec<ModelStreamEvent>), ModelPortFailure> {
    let mut observer = Events::default();
    let outcome = transport
        .invoke(&request, &mut observer, &NeverCancel)
        .await?;
    Ok((outcome, observer.values))
}

fn completed_text(outcome: &InvokeOutcome) -> String {
    match outcome {
        InvokeOutcome::Completed { items, .. } => items
            .iter()
            .filter_map(|item| match item {
                garive_llm::ModelItem::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect(),
        _ => String::new(),
    }
}

#[derive(Default)]
struct Events {
    values: Vec<ModelStreamEvent>,
}

impl ModelObserver for Events {
    fn observe(&mut self, event: &ModelStreamEvent) -> ObserverDecision {
        self.values.push(event.clone());
        ObserverDecision::Continue
    }
}

struct NeverCancel;

impl ModelCancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

struct CancelFirst {
    calls: usize,
}

impl ModelObserver for CancelFirst {
    fn observe(&mut self, _: &ModelStreamEvent) -> ObserverDecision {
        self.calls += 1;
        ObserverDecision::Cancel
    }
}

struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
    listener: Option<TcpListener>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Server {
    fn one(response: Response) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1/model", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let thread = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            captured.lock().unwrap().push(request);
            if response.delay_ms != 0 {
                thread::sleep(Duration::from_millis(response.delay_ms));
            }
            response.write(&mut stream);
        });
        Self {
            url,
            requests,
            listener: None,
            thread: Some(thread),
        }
    }

    fn idle() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1/model", listener.local_addr().unwrap());
        Self {
            url,
            requests: Arc::new(Mutex::new(Vec::new())),
            listener: Some(listener),
            thread: None,
        }
    }

    fn requests(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    fn join(mut self) -> String {
        self.thread.take().unwrap().join().unwrap();
        self.requests.lock().unwrap().remove(0)
    }

    fn stop(self) {
        drop(self.listener);
    }
}

struct Response {
    status: u16,
    content_type: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
    chunk_size: Option<usize>,
    delay_ms: u64,
}

impl Response {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            content_type: "application/json",
            headers: vec![],
            body: body.into(),
            chunk_size: None,
            delay_ms: 0,
        }
    }

    fn delayed_json(body: &str, delay_ms: u64) -> Self {
        Self {
            delay_ms,
            ..Self::json(200, body)
        }
    }

    fn sse(body: &str, chunk_size: usize) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            headers: vec![],
            body: body.into(),
            chunk_size: Some(chunk_size),
            delay_ms: 0,
        }
    }

    fn redirect(location: &'static str) -> Self {
        Self {
            status: 302,
            content_type: "application/json",
            headers: vec![("Location", location)],
            body: "{}".into(),
            chunk_size: None,
            delay_ms: 0,
        }
    }

    fn header(mut self, name: &'static str, value: &'static str) -> Self {
        self.headers.push((name, value));
        self
    }

    fn write(self, stream: &mut TcpStream) {
        let reason = match self.status {
            200 => "OK",
            302 => "Found",
            429 => "Too Many Requests",
            529 => "Overloaded",
            _ => "Response",
        };
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nConnection: close\r\n",
            self.status, reason, self.content_type
        );
        for (name, value) in self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        match self.chunk_size {
            Some(size) => {
                head.push_str("Transfer-Encoding: chunked\r\n\r\n");
                stream.write_all(head.as_bytes()).unwrap();
                for chunk in self.body.as_bytes().chunks(size) {
                    write!(stream, "{:x}\r\n", chunk.len()).unwrap();
                    stream.write_all(chunk).unwrap();
                    stream.write_all(b"\r\n").unwrap();
                    stream.flush().unwrap();
                }
                stream.write_all(b"0\r\n\r\n").unwrap();
            }
            None => {
                head.push_str(&format!("Content-Length: {}\r\n\r\n", self.body.len()));
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(self.body.as_bytes()).unwrap();
            }
        }
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..header_end + 4]);
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}
