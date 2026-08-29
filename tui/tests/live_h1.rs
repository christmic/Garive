use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};

#[test]
fn tui_renders_ordered_real_h1_events_and_terminal() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            json_response(
                r#"{"session_id":"session-1","agent_instance_id":"agent-1","committed_position":1}"#,
            ),
            json_response(
                r#"{"session_id":"session-1","turn_id":"turn-1","execution_id":"execution-1","committed_position":2}"#,
            ),
            sse_response(),
        ];
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8_192];
            let _ = socket.read(&mut request).unwrap();
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_garive-tui"))
        .args([
            &format!("http://{address}/"),
            "definition-1",
            "private prompt",
        ])
        .output()
        .expect("TUI must launch");
    server.join().expect("Host server must finish");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let started = stdout.find("       3  future.event").expect("future event");
    let completed = stdout
        .find("       7  turn.completed")
        .expect("terminal event");
    assert!(started < completed);
    assert!(stdout.contains("Agent: durable answer"));
    assert!(stdout.contains("completed @ position 7"));
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn sse_response() -> String {
    let body = [
        r#"data: {"api_version":"v1","session_id":"session-1","position":3,"event":"future.event","turn_id":"turn-1","execution_id":"execution-1","text":""}

"#,
        r#"data: {"api_version":"v1","session_id":"session-1","position":7,"event":"turn.completed","turn_id":"turn-1","execution_id":"execution-1","text":"durable answer"}

"#,
    ]
    .concat();
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
