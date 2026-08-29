use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,
};

use garive_adapter_anthropic_messages::CreateMessageRequest;
use garive_context_pressure::{
    ReqwestTokenCountExchangePort, TokenCountExchangePort, TokenCountHttpLimits,
};
use garive_provider_anthropic::{build_token_count_profile, project_token_count_request};
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use serde_json::json;

fn limits(max_response_bytes: usize) -> TokenCountHttpLimits {
    TokenCountHttpLimits {
        connect_timeout_ms: 500,
        request_timeout_ms: 500,
        max_response_bytes,
    }
}

fn request(endpoint: &str) -> garive_provider_anthropic::TokenCountHttpRequest {
    let profile = build_token_count_profile(&ConnectionInput::new(
        EndpointSelection::Explicit(endpoint.into()),
        SecretValue::new("fixture-secret").unwrap(),
        vec![],
    ))
    .unwrap();
    let create: CreateMessageRequest = serde_json::from_value(json!({
        "model":"fixture-model",
        "max_tokens":1,
        "messages":[{"role":"user","content":"count this"}],
        "stream":false
    }))
    .unwrap();
    profile
        .prepare(&project_token_count_request(&create).unwrap())
        .unwrap()
}

fn server(handler: impl FnOnce(TcpStream) + Send + 'static) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/v1/messages/count_tokens",
        listener.local_addr().unwrap()
    );
    let handle = thread::spawn(move || handler(listener.accept().unwrap().0));
    (endpoint, handle)
}

fn read_request(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        bytes.extend_from_slice(&buffer[..count]);
        let Some(header_end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length: ")
                    .and_then(|value| value.parse::<usize>().ok())
            })
            .unwrap();
        if bytes.len() >= header_end + 4 + content_length {
            return bytes;
        }
    }
}

#[test]
fn one_loopback_attempt_preserves_exact_prepared_exchange() {
    let (observed_tx, observed_rx) = mpsc::channel();
    let (endpoint, handle) = server(move |mut stream| {
        observed_tx.send(read_request(&mut stream)).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 18\r\n\r\n{\"input_tokens\":7}")
            .unwrap();
    });
    let port = ReqwestTokenCountExchangePort::new(&endpoint, limits(128)).unwrap();
    assert!(!port.publication_eligible());
    assert_eq!(
        port.execute(&request(&endpoint)).unwrap(),
        br#"{"input_tokens":7}"#
    );
    handle.join().unwrap();

    let observed = String::from_utf8(observed_rx.recv().unwrap()).unwrap();
    assert!(observed.starts_with("POST /v1/messages/count_tokens HTTP/1.1\r\n"));
    assert!(observed
        .to_ascii_lowercase()
        .contains("x-api-key: fixture-secret"));
    assert!(observed.contains(r#"{"model":"fixture-model","messages"#));
    assert!(!observed.contains("max_tokens"));
    assert!(!observed.contains("stream"));
}

#[test]
fn endpoint_status_size_and_timeout_fail_closed() {
    let (endpoint, handle) = server(|mut stream| {
        let _ = read_request(&mut stream);
        stream
            .write_all(
                b"HTTP/1.1 307 Temporary Redirect\r\nLocation: /other\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
    });
    let port = ReqwestTokenCountExchangePort::new(&endpoint, limits(64)).unwrap();
    assert!(port.execute(&request(&endpoint)).is_err());
    handle.join().unwrap();

    let (endpoint, handle) = server(|mut stream| {
        let _ = read_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 65\r\n\r\n")
            .unwrap();
    });
    let port = ReqwestTokenCountExchangePort::new(&endpoint, limits(64)).unwrap();
    assert!(port.execute(&request(&endpoint)).is_err());
    handle.join().unwrap();

    let (endpoint, handle) = server(|mut stream| {
        let _ = read_request(&mut stream);
        thread::sleep(Duration::from_millis(100));
    });
    let port = ReqwestTokenCountExchangePort::new(
        &endpoint,
        TokenCountHttpLimits {
            request_timeout_ms: 20,
            ..limits(64)
        },
    )
    .unwrap();
    assert!(port.execute(&request(&endpoint)).is_err());
    handle.join().unwrap();

    let other = request("http://127.0.0.1:9/v1/messages/count_tokens");
    assert!(port.execute(&other).is_err());
}

#[test]
fn only_strict_public_https_endpoints_are_publication_eligible() {
    let strict = ReqwestTokenCountExchangePort::new(
        "https://api.anthropic.com/v1/messages/count_tokens",
        limits(128),
    )
    .unwrap();
    assert!(strict.publication_eligible());

    for endpoint in [
        "http://api.anthropic.com/v1/messages/count_tokens",
        "https://localhost/v1/messages/count_tokens",
        "https://127.0.0.1/v1/messages/count_tokens",
        "https://api.anthropic.com/v1/messages/count_tokens?debug=true",
        "https://user@api.anthropic.com/v1/messages/count_tokens",
    ] {
        assert!(!ReqwestTokenCountExchangePort::new(endpoint, limits(128))
            .unwrap()
            .publication_eligible());
    }
}
