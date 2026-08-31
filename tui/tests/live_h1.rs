use serde_json::json;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    io::{Read, Write},
    net::{SocketAddr, TcpListener},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
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
        assert!(text.contains("connecting"), "connection state rendered");
        assert!(
            !text.contains("· connecting")
                && !text.contains("• connecting")
                && !text.contains("● connecting"),
            "connection state stayed stable; motion is reserved for active execution"
        );
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
                send "\033\[<0;21;7M"
                expect "Status details"
                send "\033"
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
        assert!(text.contains("\x1b[?1000h"), "mouse capture entered");
        assert!(text.contains("Host: online") && text.contains("Cursor: 0"));
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
fn shift_selection_is_visible_in_a_real_mono_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-selection.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "a界b"
            after 100
            send "\033\[1;2D\033\[1;2D"
            after 100
            send "\033\[D"
            send "X"
            expect "aX界b"
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
    assert!(text.contains("\x1b[7m界"));
    assert!(text.contains("X界"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn alt_c_copies_only_the_composer_selection_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-selection-copy.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "alpha beta"
            after 100
            send "\033\[1;2D\033\[1;2D\033\[1;2D\033\[1;2D"
            expect "Alt+C"
            send "\033c"
            expect -exact "\033\]52;c;YmV0YQ==\007"
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
    assert!(text.contains("\x1b]52;c;YmV0YQ==\x07"));
    assert!(!text.contains("YWxwaGEgYmV0YQ=="));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn up_moves_across_a_soft_wrapped_visual_row_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-visual-up.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 16 columns 40; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "hello wonderful world crosses the boundary"
            after 100
            send "\033\[A"
            send "X"
            expect "hello woX"
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
    assert!(text.contains("hello woX"));
    assert!(text.contains('X'));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn end_stays_on_the_current_soft_wrapped_row_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-visual-end.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 16 columns 40; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "hello wonderful world crosses the boundary"
            after 100
            send "\033\[A"
            send "\033\[F"
            send "X"
            expect "theX boundary"
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
    assert!(text.contains("theX"));
    assert!(text.contains("boundary"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn boundary_history_browsing_restores_the_draft_cursor_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let state_dir = temporary.path().join("state");
    fs::create_dir(&state_dir).unwrap();
    #[cfg(unix)]
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700)).unwrap();
    let history_path = state_dir.join("prompt-history.v1.jsonl");
    fs::write(
        &history_path,
        concat!(
            "{\"schema_version\":1,\"entry_id\":\"old\",\"session_id\":\"s\",\"submitted_text\":\"oldest prompt\",\"submitted_at\":\"2026-08-31T00:00:00Z\"}\n",
            "{\"schema_version\":1,\"entry_id\":\"new\",\"session_id\":\"s\",\"submitted_text\":\"newest prompt\",\"submitted_at\":\"2026-08-31T00:00:01Z\"}\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    fs::set_permissions(&history_path, fs::Permissions::from_mode(0o600)).unwrap();
    let transcript = temporary.path().join("composer-boundary-history.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", &state_dir)
        .args(["-c", r#"
            set timeout 5
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 16 columns 40; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse off}
            expect -exact "\033\[6n"
            send "\033\[1;1R"
            expect "Garive"
            send "work"
            send "\033\[D\033\[D"
            send "\033\[A"
            expect {
                -exact "newest" {}
                timeout { exit 31 }
            }
            expect {
                -exact "prompt" {}
                timeout { exit 32 }
            }
            send "\033\[A"
            expect {
                -exact "old" {}
                timeout { exit 33 }
            }
            send "\033\[B"
            expect {
                -exact "new" {}
                timeout { exit 34 }
            }
            send "\033\[B"
            expect {
                -exact "work" {}
                timeout { exit 35 }
            }
            send "X"
            expect {
                -exact "Xrk" {}
                timeout { exit 36 }
            }
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
    assert!(text.contains("newest"));
    assert!(text.contains("old"));
    assert!(text.contains("Xrk"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn mouse_drag_selects_composer_graphemes_in_a_real_mono_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-mouse-selection.log");
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
            send "a界b"
            after 100
            send "\033\[<0;6;22M"
            send "\033\[<32;8;22M"
            send "\033\[<0;8;22m"
            after 100
            send "X"
            expect "aX"
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
    assert!(text.contains("\x1b[7m界"));
    assert!(text.contains("aX"));
    assert!(text.contains("\x1b[?1000l"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn double_and_triple_click_replace_a_word_then_the_line_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-multi-click.log");
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
            send "alpha beta"
            after 100
            send "\033\[<0;11;22M\033\[<0;11;22m"
            send "\033\[<0;11;22M\033\[<0;11;22m"
            after 100
            send "X"
            expect "alpha X"
            after 600
            send "\033\[<0;6;22M\033\[<0;6;22m"
            send "\033\[<0;6;22M\033\[<0;6;22m"
            send "\033\[<0;6;22M\033\[<0;6;22m"
            after 100
            send "Y"
            expect "Y"
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
    assert!(text.contains("\x1b[7mbeta"));
    assert!(text.contains("\x1b[7malpha"));
    assert!(text.contains("\x1b[7m X"));
    assert!(text.contains("\x1b[?1000l"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn composer_kill_yank_undo_and_redo_work_in_a_real_pty() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-kill-yank.log");
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
            send "alpha\012beta"
            expect "beta"
            send "\025"
            expect "alpha"
            send "\031"
            expect "beta"
            send "\032"
            after 100
            send "\033z"
            expect "beta"
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
    assert!(text.contains("alpha"));
    assert!(text.contains("beta"));
    assert!(text.contains("\x1b[?1049l"));
}

#[test]
fn typed_editor_aliases_drive_the_shipping_composer() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("composer-key-aliases.log");
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
            send "abc def"
            send "\001X\005Y\027\010\002\004"
            expect "Xab"
            send "\033b\033fZ"
            expect "XabZ"
            send "\033b\033d"
            send "done"
            expect "done"
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
    assert!(text.contains("Xabc def") && text.contains("done"));
    assert!(text.contains("\x1b[?1049l"));
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
            after 100
            send "\021"
            after 100
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
fn external_editor_owns_the_tty_applies_once_and_preserves_failures() {
    let (address, server) = empty_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("external-editor.log");
    let editor = temporary.path().join("editor.sh");
    let mode = temporary.path().join("editor-mode");
    let marker = temporary.path().join("editor-path");
    fs::write(
        &editor,
        r#"#!/bin/sh
printf '%s' "$1" > "$GARIVE_EDITOR_MARKER"
if [ ! -e "$GARIVE_EDITOR_MODE" ]; then
    : > "$GARIVE_EDITOR_MODE"
    if [ -t 0 ] && [ -t 1 ] && [ -t 2 ]; then printf 'EDITOR_TTY=yes\n'; fi
    printf 'edited\nline\n' > "$1"
    exit 0
fi
printf 'EDITOR_FAILURE\n'
exit 7
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&editor).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&editor, permissions).unwrap();

    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("VISUAL", &editor)
        .env("GARIVE_EDITOR_MODE", &mode)
        .env("GARIVE_EDITOR_MARKER", &marker)
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 8
            proc must_expect {pattern code} {
                expect {
                    -exact $pattern { return }
                    timeout { exit $code }
                    eof { exit $code }
                }
            }
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse on}
            must_expect "\033\[6n" 40
            send "\033\[1;1R"
            must_expect "Garive" 41
            send "\033\[Z"
            after 100
            send "\007"
            after 200
            if {[file exists $env(GARIVE_EDITOR_MODE)]} { exit 56 }
            send "\t"
            after 100
            send "seed"
            after 100
            send "\007"
            must_expect "Garive paused." 42
            must_expect "EDITOR_TTY=yes" 43
            must_expect "\033\[6n" 44
            send "\033\[1;1R"
            must_expect "edited" 45
            must_expect "line" 46
            send "\032"
            must_expect "seed" 47
            after 100
            send "\007"
            must_expect "Garive paused." 48
            must_expect "EDITOR_FAILURE" 49
            must_expect "\033\[6n" 50
            send "\033\[1;1R"
            must_expect "seed" 51
            must_expect "exited" 52
            must_expect "unsuccessfully" 53
            send "\021"
            must_expect "Garive?" 54
            send "\r"
            expect {
                eof { exit 0 }
                timeout { exit 55 }
            }
        "#])
        .status()
        .unwrap();
    server.join().unwrap();
    assert!(status.success());
    let text = fs::read_to_string(transcript).unwrap();
    assert!(text.contains("EDITOR_TTY=yes"));
    assert!(text.contains("\x1b[?1000l") && text.contains("\x1b[?1000h"));
    assert!(text.contains("\x1b[?1049l") && text.contains("\x1b[?1049h"));
    let editor_path = fs::read_to_string(marker).unwrap();
    assert!(!std::path::Path::new(&editor_path).exists());
}

#[test]
fn turn_navigator_filters_commits_only_on_activation_and_shares_mouse_geometry() {
    let (address, stop, h4_seen, server) = timeline_host();
    let temporary = tempfile::tempdir().unwrap();
    let transcript = temporary.path().join("turn-navigator.log");
    let status = Command::new("expect")
        .env("TERM", "xterm-256color")
        .env("GARIVE_TUI_BIN", env!("CARGO_BIN_EXE_garive-tui"))
        .env("GARIVE_TUI_HOST", format!("http://{address}/"))
        .env("GARIVE_TUI_LOG", &transcript)
        .env("GARIVE_TUI_STATE", temporary.path().join("state"))
        .args(["-c", r#"
            set timeout 8
            proc must_expect {pattern code} {
                expect {
                    -exact $pattern { return }
                    timeout { exit $code }
                    eof { exit $code }
                }
            }
            log_user 0
            log_file -noappend $env(GARIVE_TUI_LOG)
            spawn -noecho /bin/sh -c {stty rows 24 columns 100; exec "$GARIVE_TUI_BIN" --host "$GARIVE_TUI_HOST" --session session-rail --state-dir "$GARIVE_TUI_STATE" --theme mono --mouse on}
            must_expect "\033\[6n" 70
            send "\033\[1;1R"
            must_expect "question-19" 71
            send "/jump \r"
            must_expect "Jump to a Turn" 72
            send "\033\[H"
            send "\033"
            must_expect "answer-19" 73
            send "/jump question-11\r"
            must_expect "12  question-11" 74
            send "\r"
            must_expect "answer-11" 75
            send "/jump \r"
            must_expect "Jump to a Turn" 76
            send "\033\[H"
            after 100
            send "\033\[<0;50;5M"
            must_expect "answer-0" 77
            send "\021"
            must_expect "Garive?" 78
            send "\r"
            expect {
                eof { exit 0 }
                timeout { exit 79 }
            }
        "#])
        .status()
        .unwrap();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert!(
        status.success(),
        "turn navigator walkthrough exited with {status}"
    );
    assert!(
        h4_seen.load(Ordering::Relaxed),
        "the fixture served the session H4 subscription"
    );
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

fn timeline_host() -> (
    SocketAddr,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let h4_seen = Arc::new(AtomicBool::new(false));
    let server_h4_seen = Arc::clone(&h4_seen);
    let items = (0..20)
        .map(|index| {
            json!({
                "turn_id": format!("turn-{index}"),
                "started_position": index * 2 + 1,
                "latest_position": index * 2 + 2,
                "state": "completed",
                "user_text": format!("question-{index}"),
                "completion_text": format!("answer-{index}"),
                "suspension": null,
                "content_truncated": false,
                "activities": []
            })
        })
        .collect::<Vec<_>>();
    let summary = json!({
        "api_version": "v1",
        "session_id": "session-rail",
        "agent_instance_id": "agent-instance",
        "definition_id": "definition-1",
        "definition_revision": "revision-1",
        "opened_at": "2026-08-31T00:00:00Z",
        "latest_position": 40,
        "latest_turn_id": "turn-19",
        "latest_turn_state": "completed",
        "turn_count": 20
    });
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Relaxed) {
            let (mut socket, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("timeline host accept failed: {error}"),
            };
            socket.set_nonblocking(false).unwrap();
            let mut request = [0; 8_192];
            let read = socket.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            if request.contains("/events?") {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .unwrap();
                continue;
            }
            if request.contains("GET /v1/sessions/session-rail/live ") {
                let event = json!({
                    "api_version": "v1",
                    "session_id": "session-rail",
                    "turn_id": "turn-19",
                    "execution_id": "execution-19",
                    "stream_id": "12345678-1234-4234-8234-123456789abc",
                    "sequence": 1,
                    "kind": "snapshot",
                    "text": "answer-19",
                    "through_sequence": 1
                });
                let body = format!("event: live\ndata: {event}\n\n");
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .unwrap();
                server_h4_seen.store(true, Ordering::Relaxed);
                continue;
            }
            let body = if request.contains("GET /v1/agent-definitions ") {
                json!({"api_version":"v1","definitions":[{"api_version":"v1","definition_id":"definition-1","definition_revision":"revision-1","capabilities":[]}]}).to_string()
            } else if request.contains("GET /v1/sessions?") {
                json!({"api_version":"v1","sessions":[summary.clone()],"next_before":null})
                    .to_string()
            } else if request.contains("/timeline?") {
                json!({"api_version":"v1","session_id":"session-rail","items":items.clone(),"scanned_through_position":40,"observed_max_position":40,"has_more":false}).to_string()
            } else if request.contains("GET /v1/sessions/session-rail ") {
                json!({"api_version":"v1","session":summary.clone(),"observed_max_position":40})
                    .to_string()
            } else {
                panic!("unexpected timeline request: {request}")
            };
            socket.write_all(json_response(&body).as_bytes()).unwrap();
        }
    });
    (address, stop, h4_seen, server)
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}
