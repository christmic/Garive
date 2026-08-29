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
        eprintln!("usage: garive <loopback-host-url> <agent-definition-id> <message>");
        std::process::exit(2);
    }
    let terminal = run(&arguments[0], &arguments[1], &arguments[2])
        .await
        .unwrap_or_else(|error| {
            eprintln!("garive: {error}");
            std::process::exit(2);
        });
    std::process::exit(match terminal {
        HostTerminal::Completed => 0,
        HostTerminal::Suspended => 3,
        HostTerminal::Stopped => 4,
        HostTerminal::Failed => 5,
    });
}

async fn run(
    host_url: &str,
    definition_id: &str,
    message: &str,
) -> Result<HostTerminal, garive_host_client::HostClientError> {
    let client = LiveHostClient::new(host_url, LIMITS)?;
    let identity = command_identity();
    let session = client
        .create_session(&format!("create-{identity}"), definition_id)
        .await?;
    let turn = client
        .start_turn(&format!("turn-{identity}"), &session.session_id, message)
        .await?;
    let view = client
        .follow_until_terminal(&session.session_id, turn.committed_position)
        .await?;
    let terminal = view
        .terminal
        .expect("follow contract returns only at terminal");
    if terminal == HostTerminal::Completed {
        println!("{}", view.text);
    }
    Ok(terminal)
}

fn command_identity() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}
