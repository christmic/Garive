use std::time::{SystemTime, UNIX_EPOCH};

use garive_host_client::{ClientLimits, HostTerminal, LiveHostClient};

const LIMITS: ClientLimits = ClientLimits {
    max_command_bytes: 4_096,
    max_event_bytes: 8_192,
    max_events: 4_096,
    follow_deadline_ms: 120_000,
};

#[tokio::main]
async fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        eprintln!("usage: garive-tui <loopback-host-url> <agent-definition-id> <message>");
        std::process::exit(2);
    }
    if let Err(error) = run(&arguments[0], &arguments[1], &arguments[2]).await {
        eprintln!("garive-tui: {error}");
        std::process::exit(2);
    }
}

async fn run(
    host_url: &str,
    definition_id: &str,
    message: &str,
) -> Result<(), garive_host_client::HostClientError> {
    let client = LiveHostClient::new(host_url, LIMITS)?;
    let identity = command_identity();
    let session = client
        .create_session(&format!("create-{identity}"), definition_id)
        .await?;
    client
        .start_turn(&format!("turn-{identity}"), &session.session_id, message)
        .await?;
    println!("┌─ Garive Agent ─────────────────────────┐");
    println!("│ Session: {}", session.session_id);
    let view = client
        .follow_until_terminal_with(&session.session_id, 0, |event| {
            println!("│ {:>8}  {}", event.position, event.event);
        })
        .await?;
    if view.terminal == Some(HostTerminal::Completed) {
        println!("│ Agent: {}", view.text);
    }
    println!(
        "└─ {} @ position {} ────────────────────┘",
        terminal_name(view.terminal.expect("follow returns at terminal")),
        view.cursor
    );
    Ok(())
}

fn terminal_name(terminal: HostTerminal) -> &'static str {
    match terminal {
        HostTerminal::Completed => "completed",
        HostTerminal::Suspended => "suspended",
        HostTerminal::Stopped => "stopped",
        HostTerminal::Failed => "failed",
    }
}

fn command_identity() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}
