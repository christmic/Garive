use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    process::Command,
    thread,
};

#[test]
fn shipping_tui_boots_and_restores_a_real_pty() {
    for reduced_motion in [false, true] {
        let (address, server) = empty_host();

        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("pty.log");
        let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env(
            "GARIVE_TUI_MOTION",
            if reduced_motion {
                "--reduced-motion"
            } else {
                ""
            },
        )
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args([
            "-c",
            r#"
                set timeout 5
                log_file -noappend $env(GARIVE_TUI_LOG)
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono $GARIVE_TUI_MOTION}
                expect -exact "\033\[6n"
                send "\033\[1;1R"
                expect { "Garive" {} timeout { exit 2 } }
                send "\003"
                after 100
                send "\003"
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
        assert!(text.contains("Press Ctrl+C"));
        assert!(text.contains("Garive?"));
        if reduced_motion {
            assert!(
                text.contains("○ connecting"),
                "stable connecting glyph rendered"
            );
            assert!(
                !text.contains("· connecting"),
                "motion pulse stayed disabled"
            );
        } else {
            assert!(text.contains("· connecting"), "first motion frame rendered");
        }
        assert!(text.contains("\x1b[?1049h"), "alternate screen entered");
        assert!(text.contains("\x1b[?1049l"), "alternate screen restored");
        assert!(text.contains("\x1b[?2004l"), "bracketed paste restored");
        assert!(
            text.contains("\x1b]0;Garive · Workspace · Connecting · Ready\x07"),
            "safe semantic title emitted"
        );
        assert!(text.contains("\x1b]0;Garive\x07"), "title reset on exit");
    }
}

#[test]
fn mouse_click_activates_the_visible_overlay_row_without_background_routing() {
    for _ in 0..2 {
        let (address, server) = empty_host();
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("mouse-overlay.log");
        let status = Command::new("expect")
            .env("TERM", "xterm-256color")
            .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
            .env("GARIVE_TUI_HOST", format!("http://{address}/"))
            .env("GARIVE_TUI_LOG", &transcript)
            .env("GARIVE_TUI_STATE", temporary.path().join("state"))
            .args(["-c", r#"
                set timeout 5
                log_file -noappend $env(GARIVE_TUI_LOG)
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse on}
                expect -exact "\033\[6n"
                send "\033\[1;1R"
                expect "Garive"
                send "\020"
                expect "/help"
                send "\033\[<0;21;16M"
                expect "Keyboard guide"
                send "\033"
                send "\021"
                send "\r"
                expect eof
            "#])
            .status()
            .unwrap();
        server.join().unwrap();
        assert!(status.success());
        let text = fs::read_to_string(transcript).unwrap();
        assert!(text.contains("\x1b[?1000h"), "mouse capture entered");
        assert!(text.contains("Keyboard") && text.contains("guide"));
        assert!(text.contains("\x1b[?1000l"), "mouse capture restored");
    }
}

#[test]
fn slash_prefix_opens_adjacent_suggestions_and_tab_completes_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("command-suggestions.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "/theme d"
            expect "Use dark theme"
            send "\011"
            expect "/theme dark"
            send "\r"
            after 100
            send "\021"
            expect "Garive?"
            send "\r"
            expect eof
        "#])
        .status()
        .unwrap();
    server.join().unwrap();
    assert!(status.success());
    let text = fs::read_to_string(transcript).unwrap();
    assert!(text.contains("Commands"));
    assert!(text.contains("Use dark theme"));
    assert!(text.contains("/theme dark"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn slash_suggestion_mouse_click_completes_only_the_visible_row() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("command-suggestion-mouse.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse on}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "/theme d"
            expect "Use dark theme"
            send "\033\[<0;32;19M"
            expect "/theme dark"
            send "\r"
            after 100
            send "\021"
            expect "Garive?"
            send "\r"
            expect eof
        "#])
        .status()
        .unwrap();
    server.join().unwrap();
    assert!(status.success());
    let text = fs::read_to_string(transcript).unwrap();
    assert!(text.contains("Use dark theme"));
    assert!(text.contains("/theme dark"));
    assert!(text.contains("\x1b[?1000h"));
    assert!(text.contains("\x1b[?1000l"));
}

#[test]
fn screen_reader_mode_is_linear_and_has_no_cursor_addressing() {
    for _ in 0..2 {
        let (address, server) = empty_host();
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("linear.log");
        let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            proc must_expect {pattern code} {
                expect {
                    -exact $pattern { return }
                    timeout { exit $code }
                    eof { exit $code }
                }
            }
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --screen-reader}
            must_expect "Garive. Connecting" 20
            must_expect "Connection online" 22
            send "/not-a-command\r"
            must_expect "The slash command is invalid; nothing was sent." 24
            send "\033"
            after 100
            send "\020"
            must_expect "Command palette." 26
            send "retry"
            must_expect "> 1. /retry: Retry unknown command. Unavailable: no pending command" 27
            send "\177\177\177\177\177"
            send "keyboard"
            must_expect "Filter: keyboard." 28
            must_expect "> 1. /help: Keyboard guide" 29
            send "\r"
            must_expect "No function keys are required." 30
            send "\033"
            send "\021"
            send "\r"
            must_expect "Terminal restored." 32
        "#])
        .status()
        .unwrap();
        server.join().unwrap();
        assert!(status.success());
        let output = fs::read(&transcript).unwrap();
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("Connection online"));
        assert!(text.contains("Command palette."));
        assert!(text.contains("Unavailable: no pending command"));
        assert!(text.contains("Filter: keyboard."));
        assert!(text.contains("No function keys are required."));
        assert!(!text.contains("\x1b[6n"));
        assert!(!text.contains("\x1b[2J"));
        assert!(!text.contains("\x1b[?1049h"));
        assert!(text.contains("\x1b[?2004l"));
        assert!(text.contains("\x1b]0;Garive · Workspace · Connecting · Ready\x07"));
        assert!(text.contains("\x1b]0;Garive\x07"));
    }
}

#[test]
fn termination_signal_restores_the_shipping_terminal() {
    for _ in 0..2 {
        let (address, server) = empty_host();
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("signal.log");
        let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            set child $spawn_id
            exec kill -TERM [exp_pid -i $child]
            expect eof
            catch wait result
            set code [lindex $result 3]
            if {$code != 143} { exit 5 }
        "#])
        .status()
        .unwrap();
        server.join().unwrap();
        assert!(status.success());
        let text = fs::read_to_string(transcript).unwrap();
        assert!(text.contains("\x1b[?1049l"));
        assert!(text.contains("\x1b[?2004l"));
        assert!(text.contains("\x1b[?1004l"));
    }
}

#[test]
fn termination_before_the_first_frame_restores_the_shipping_terminal() {
    for _ in 0..2 {
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("early-signal.log");
        let status = Command::new("expect")
            .env("TERM", "xterm-256color")
            .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
            .env("GARIVE_TUI_LOG", &transcript)
            .env("GARIVE_TUI_STATE", temporary.path().join("state"))
            .args(["-c", r#"
                set timeout 5
                log_file -noappend $env(GARIVE_TUI_LOG)
                spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host http://127.0.0.1:9/ --state-dir "$GARIVE_TUI_STATE" --theme mono}
                expect -exact "\033\[?1004h"
                exec kill -TERM [exp_pid]
                expect eof
                catch wait result
                if {[lindex $result 3] != 143} { exit 6 }
            "#])
            .status()
            .unwrap();
        assert!(status.success());
        let text = fs::read_to_string(transcript).unwrap();
        assert!(text.contains("\x1b[?1049h"));
        assert!(text.contains("\x1b[?1049l"));
        assert!(text.contains("\x1b[?2004l"));
        assert!(text.contains("\x1b[?1004l"));
        assert!(text.contains("\x1b[?25h"));
    }
}

#[test]
fn live_resize_crosses_layout_breakpoints_without_losing_draft() {
    for _ in 0..2 {
        let (address, server) = empty_host();
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join("resize.log");
        let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "draft survives"
            exec stty rows 7 columns 19 < $spawn_out(slave,name)
            expect "Need 20"
            exec stty rows 12 columns 40 < $spawn_out(slave,name)
            expect "draft survives"
            exec stty rows 28 columns 160 < $spawn_out(slave,name)
            expect "draft survives"
            send "\011"
            expect "select"
            send "\177"
            send "\011"
            expect "scroll"
            send "\177"
            send "\011"
            expect "draft survives"
            send "\021"
            send "\r"
            expect eof
        "#])
        .status()
        .unwrap();
        server.join().unwrap();
        assert!(status.success());
        let text = fs::read_to_string(transcript).unwrap();
        assert!(text.contains("Need 20"));
        assert!(text.contains("draft"));
        assert!(text.contains("survives"));
        assert!(text.contains("\x1b[?1049l"));
    }
}

#[cfg(feature = "test-hooks")]
#[test]
fn injected_panic_restores_the_real_pty_before_unwind_exit() {
    for attempt in 0..2 {
        let temporary = tempfile::tempdir().unwrap();
        let transcript = temporary.path().join(format!("panic-{attempt}.log"));
        let status = Command::new("expect")
            .env("TERM", "xterm-256color")
            .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
            .env("GARIVE_TUI_LOG", &transcript)
            .env("GARIVE_TUI_STATE", temporary.path().join("state"))
            .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; before=$(stty -g); "$GARIVE_TUI_BIN" --host http://127.0.0.1:9/ --state-dir "$GARIVE_TUI_STATE" --theme mono --test-crash-hook terminal-acquired-panic; code=$?; after=$(stty -g); test "$code" -ne 0 || exit 92; echo BEFORE_TERMIOS=$before; echo AFTER_TERMIOS=$after; echo SHELL_RESTORED}
            expect {
                -exact "SHELL_RESTORED" {}
                timeout { exit 20 }
                eof { exit 21 }
            }
        "#])
            .status()
            .unwrap();
        assert!(status.success());
        let text = fs::read_to_string(transcript).unwrap();
        assert_restored_termios(&text);
        assert!(text.contains("injected panic after terminal acquisition"));
        assert!(text.contains("\x1b[?1049h"));
        assert!(text.contains("\x1b[?1049l"));
        assert!(text.contains("\x1b[?2004l"));
        assert!(text.contains("\x1b[?1004l"));
        assert!(text.contains("\x1b[?25h"));
    }
}

#[cfg(feature = "test-hooks")]
fn assert_restored_termios(transcript: &str) {
    let before = marked_value(transcript, "BEFORE_TERMIOS=");
    let after = marked_value(transcript, "AFTER_TERMIOS=");
    let normalize = |snapshot: &str| {
        snapshot
            .split(':')
            .map(|field| {
                field.strip_prefix("lflag=").map_or_else(
                    || field.to_owned(),
                    |hex| {
                        let flags = u64::from_str_radix(hex, 16).unwrap();
                        // Darwin marks input for reprint whenever canonical mode is restored.
                        // PENDIN is kernel-owned here: even `stty -pendin` cannot clear it.
                        #[cfg(target_os = "macos")]
                        let flags = flags & !0x2000_0000;
                        format!("lflag={flags:x}")
                    },
                )
            })
            .collect::<Vec<_>>()
            .join(":")
    };
    assert_eq!(normalize(before), normalize(after));
}

#[cfg(feature = "test-hooks")]
fn marked_value<'a>(transcript: &'a str, marker: &str) -> &'a str {
    transcript
        .lines()
        .find_map(|line| line.find(marker).map(|index| &line[index + marker.len()..]))
        .unwrap()
        .trim_end_matches('\r')
}

fn empty_host() -> (SocketAddr, thread::JoinHandle<()>) {
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
    (address, server)
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
