use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};

#[test]
fn shipping_tui_boots_and_restores_a_real_pty() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0; 8_192];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            let body = if request.contains("GET /v1/agent-definitions ") {
                r#"{"api_version":"v1","definitions":[{"api_version":"v1","definition_id":"definition-1","definition_revision":"revision-1","capabilities":[]}]}"#
            } else if request.contains("GET /v1/sessions?") {
                r#"{"api_version":"v1","sessions":[],"next_before":null}"#
            } else {
                panic!("unexpected request line")
            };
            socket.write_all(json_response(body).as_bytes()).unwrap();
        }
    });

    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("pty.log");
    let status = Command::new("expect")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .args([
            "-c",
            r#"
                set timeout 5
                log_file -noappend $env(GARIVE_TUI_LOG)
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --theme mono}
                expect -exact "\033\[6n"
                send "\033\[1;1R"
                expect { "Garive" {} timeout { exit 2 } }
                send "\021"
                expect { "Garive?" {} timeout { exit 3 } }
                send "\r"
                expect { eof {} timeout { exit 4 } }
            "#,
        ])
        .status()
        .expect("expect must launch the shipping binary in a PTY");
    server.join().unwrap();
    assert!(status.success());
    let output = fs::read(&transcript).unwrap();
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("Garive"));
    assert!(text.contains("Quit"));
    assert!(text.contains("Garive?"));
    assert!(text.contains("\x1b[?1049h"), "alternate screen entered");
    assert!(text.contains("\x1b[?1049l"), "alternate screen restored");
    assert!(text.contains("\x1b[?2004l"), "bracketed paste restored");
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
