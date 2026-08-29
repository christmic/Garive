use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use garive_context_pressure::{
    build_publication_provider_counter, load_corpus, measure_context_pressure,
    CredentialReferenceResolver, CredentialResolutionFailure, ProviderCounterBuildError,
    ProviderCounterRunConfig, TokenCounter,
};
use garive_provider_profile::SecretValue;
use serde_json::{json, Value};

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/context-pressure-corpus-v1.json"
));

struct RecordingResolver {
    references: Arc<Mutex<Vec<String>>>,
    available: bool,
}

impl CredentialReferenceResolver for RecordingResolver {
    fn resolve(&self, reference: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        self.references.lock().unwrap().push(reference.into());
        self.available
            .then(|| SecretValue::new("resolved-fixture-secret").unwrap())
            .ok_or(CredentialResolutionFailure)
    }
}

fn config(endpoint: &str, publishable: bool) -> Value {
    json!({
        "counter_revision":"publication-v1",
        "publishable":publishable,
        "credential_ref":"fixture-key",
        "endpoint":endpoint,
        "target_id":"evidence-target",
        "model_id":"evidence-model",
        "capabilities":["text","tools"],
        "projection_max_output_tokens":1,
        "extra_headers":[{"name":"x-trace","value":"trace-v1"}],
        "http":{
            "connect_timeout_ms":500,
            "request_timeout_ms":500,
            "max_response_bytes":128
        }
    })
}

fn loopback() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/messages/count_tokens",
        listener.local_addr().unwrap()
    );
    let handle = thread::spawn(move || {
        let mut bodies = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 2048];
            loop {
                let count = stream.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..count]);
                let Some(split) = request.windows(4).position(|value| value == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..split]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap();
                if request.len() >= split + 4 + length {
                    bodies.push(String::from_utf8(request[split + 4..].to_vec()).unwrap());
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 19\r\n\r\n{\"input_tokens\":32}")
                .unwrap();
        }
        bodies
    });
    (endpoint, handle)
}

#[test]
fn injected_reference_builds_the_real_four_case_route() {
    let (endpoint, server) = loopback();
    let references = Arc::new(Mutex::new(Vec::new()));
    let resolver = RecordingResolver {
        references: Arc::clone(&references),
        available: true,
    };
    let parsed: ProviderCounterRunConfig =
        serde_json::from_value(config(&endpoint, false)).unwrap();
    let counter = build_publication_provider_counter(parsed, &resolver).unwrap();
    assert!(!counter.descriptor().publishable);
    let corpus = load_corpus(CORPUS.as_bytes()).unwrap();
    assert_eq!(
        measure_context_pressure(&corpus, &counter)
            .unwrap()
            .summary
            .ordered_cases
            .len(),
        4
    );
    assert_eq!(&*references.lock().unwrap(), &["fixture-key"]);
    let bodies = server.join().unwrap();
    assert_eq!(bodies.len(), 4);
    assert!(bodies.iter().all(|body| !body.contains("max_tokens")));
}

#[test]
fn invalid_nonsecret_configuration_fails_before_resolution() {
    let references = Arc::new(Mutex::new(Vec::new()));
    let resolver = RecordingResolver {
        references: Arc::clone(&references),
        available: true,
    };
    let mut duplicate = config("https://api.anthropic.com/v1/messages/count_tokens", false);
    duplicate["capabilities"] = json!(["text", "text"]);
    let parsed = serde_json::from_value(duplicate).unwrap();
    assert!(matches!(
        build_publication_provider_counter(parsed, &resolver),
        Err(ProviderCounterBuildError::InvalidConfiguration)
    ));
    let local_publish =
        serde_json::from_value(config("http://127.0.0.1:1/v1/messages/count_tokens", true))
            .unwrap();
    assert!(build_publication_provider_counter(local_publish, &resolver).is_err());
    assert!(references.lock().unwrap().is_empty());

    let mut plaintext = config("https://api.anthropic.com/v1/messages/count_tokens", false);
    plaintext["credential"] = json!("forbidden");
    assert!(serde_json::from_value::<ProviderCounterRunConfig>(plaintext).is_err());
}

#[test]
fn strict_public_route_is_eligible_and_missing_secret_is_stable() {
    let references = Arc::new(Mutex::new(Vec::new()));
    let resolver = RecordingResolver {
        references,
        available: false,
    };
    let parsed = serde_json::from_value(config(
        "https://api.anthropic.com/v1/messages/count_tokens",
        true,
    ))
    .unwrap();
    assert!(matches!(
        build_publication_provider_counter(parsed, &resolver),
        Err(ProviderCounterBuildError::CredentialUnavailable)
    ));
}
