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
    let command = match arguments.as_slice() {
        [host_url, definition_id, message] => CommandTarget::Create {
            host_url,
            definition_id,
            message,
        },
        [host_url, flag, session_id, message] if flag == "--session" => CommandTarget::Reuse {
            host_url,
            session_id,
            message,
        },
        _ => {
            eprintln!("usage: garive <host-url> <definition-id> <message>");
            eprintln!("       garive <host-url> --session <session-id> <message>");
            std::process::exit(2);
        }
    };
    let terminal = run(command).await.unwrap_or_else(|error| {
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

enum CommandTarget<'a> {
    Create {
        host_url: &'a str,
        definition_id: &'a str,
        message: &'a str,
    },
    Reuse {
        host_url: &'a str,
        session_id: &'a str,
        message: &'a str,
    },
}

async fn run(
    command: CommandTarget<'_>,
) -> Result<HostTerminal, garive_host_client::HostClientError> {
    let (host_url, session_id, message) = match command {
        CommandTarget::Create {
            host_url,
            definition_id,
            message,
        } => {
            let client = LiveHostClient::new(host_url, LIMITS)?;
            let identity = command_identity();
            let session = client
                .create_session(&format!("create-{identity}"), definition_id)
                .await?;
            return run_turn(client, session.session_id, message, identity).await;
        }
        CommandTarget::Reuse {
            host_url,
            session_id,
            message,
        } => (host_url, session_id.to_owned(), message),
    };
    let client = LiveHostClient::new(host_url, LIMITS)?;
    let identity = command_identity();
    run_turn(client, session_id, message, identity).await
}

async fn run_turn(
    client: LiveHostClient,
    session_id: String,
    message: &str,
    identity: String,
) -> Result<HostTerminal, garive_host_client::HostClientError> {
    let turn = client
        .start_turn(&format!("turn-{identity}"), &session_id, message)
        .await?;
    let view = client
        .follow_until_terminal(&session_id, turn.committed_position)
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
