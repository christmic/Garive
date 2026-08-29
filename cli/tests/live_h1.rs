use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};

#[test]
fn cli_uses_real_h1_and_prints_committed_completion() {
    let (url, server) = host_server("turn.completed", "durable answer");
    let output = Command::new(env!("CARGO_BIN_EXE_garive"))
        .args([&url, "definition-1", "private prompt"])
        .output()
        .expect("CLI must launch");
    server.join().expect("Host server must finish");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "durable answer\n"
    );
}

#[test]
fn cli_maps_durable_failure_to_exit_five() {
    let (url, server) = host_server("turn.failed", "");
    let status = Command::new(env!("CARGO_BIN_EXE_garive"))
        .args([&url, "definition-1", "private prompt"])
        .status()
        .expect("CLI must launch");
    server.join().expect("Host server must finish");
    assert_eq!(status.code(), Some(5));
}

fn host_server(terminal: &str, text: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let terminal = terminal.to_owned();
    let text = text.to_owned();
    let task = thread::spawn(move || {
        let responses = [
            json_response(
                r#"{"session_id":"session-1","agent_instance_id":"agent-1","committed_position":1}"#,
            ),
            json_response(
                r#"{"session_id":"session-1","turn_id":"turn-1","execution_id":"execution-1","committed_position":2}"#,
            ),
            sse_response(&terminal, &text),
        ];
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8_192];
            let _ = socket.read(&mut request).unwrap();
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    (format!("http://{address}/"), task)
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn sse_response(terminal: &str, text: &str) -> String {
    let body = format!(
        "data: {{\"api_version\":\"v1\",\"session_id\":\"session-1\",\"position\":3,\"event\":\"{terminal}\",\"turn_id\":\"turn-1\",\"execution_id\":\"execution-1\",\"text\":\"{text}\"}}\n\n"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
