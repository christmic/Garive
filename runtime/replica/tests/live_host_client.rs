use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};

use garive_core::{AgentOutcome, ExecutionReport, UsageSummary};
use garive_host_client::{ClientLimits, HostTerminal, LiveHostClient};
use garive_ledger::{ExecutionId, SessionId, TurnId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CoreTerminalContext, EffectiveRuntimeLimits, HostClock, InstalledAgent,
    LiveHost, LiveHostLimits, LiveHostServer, SqliteLedger, TurnDispatchError, TurnDispatcher,
};
use tempfile::tempdir;
use tokio::sync::oneshot;

const NOW: &str = "2026-08-30T00:00:00Z";

struct FixedClock;

impl HostClock for FixedClock {
    fn recorded_at(&self) -> String {
        NOW.to_owned()
    }
}

struct NoopDispatcher;

impl TurnDispatcher for NoopDispatcher {
    fn dispatch(&self, _turn: &garive_runtime::CommittedTurn) -> Result<(), TurnDispatchError> {
        Ok(())
    }
}

#[tokio::test]
async fn shared_client_completes_a_real_runtime_host_turn() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("host.sqlite3");
    let host = LiveHost::new(
        &database,
        InstalledAgent {
            definition_id: "definition-main".into(),
            definition_revision: "revision-1".into(),
            snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            agent_instance_namespace: "installed-main".into(),
            public_capabilities: Vec::new(),
            runtime_limits: EffectiveRuntimeLimits {
                max_iterations: 4,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            public_activity_catalogue: None,
        },
        LiveHostLimits {
            max_command_bytes: 4096,
            event_batch_size: 64,
            event_poll_interval_ms: 5,
            activity: None,
        },
        Arc::new(FixedClock),
        Arc::new(NoopDispatcher),
    )
    .unwrap();
    let server = LiveHostServer::bind(host, SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))
        .await
        .unwrap();
    let address = server.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(server.serve(async move {
        let _ = shutdown_rx.await;
    }));
    let client = LiveHostClient::new(
        &format!("http://{address}/"),
        ClientLimits {
            max_command_bytes: 4096,
            max_event_bytes: 8192,
            max_events: 16,
            follow_deadline_ms: 2_000,
        },
    )
    .unwrap();

    let session = client
        .create_session("create-e2e", "definition-main")
        .await
        .unwrap();
    let goals = client.get_goals(&session.session_id).await.unwrap();
    assert!(goals.goals.is_empty());
    assert_eq!(goals.session_version, 1);
    let plans = client.get_plans(&session.session_id).await.unwrap();
    assert!(plans.plans.is_empty());
    assert_eq!(plans.session_version, 1);
    let started = client
        .start_turn("start-e2e", &session.session_id, "hello")
        .await
        .unwrap();
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: TurnId::try_from(started.turn_id.as_str()).unwrap(),
            execution_id: ExecutionId::try_from(started.execution_id.as_str()).unwrap(),
            recorded_at: NOW.into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: "durable answer".into(),
                }],
                usage: UsageSummary {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(2),
                    estimated: false,
                },
            },
            completed_iterations: 1,
            usage: UsageSummary {
                input_tokens: TokenCount::Known(1),
                output_tokens: TokenCount::Known(2),
                estimated: false,
            },
        },
    )
    .unwrap();
    SqliteLedger::open(&database)
        .unwrap()
        .commit(
            SessionId::try_from(session.session_id.as_str()).unwrap(),
            2,
            terminal,
        )
        .unwrap();

    let view = client
        .follow_until_terminal(&session.session_id, 0)
        .await
        .unwrap();
    assert_eq!(view.terminal, Some(HostTerminal::Completed));
    assert_eq!(view.text, "durable answer");

    shutdown_tx.send(()).unwrap();
    task.await.unwrap().unwrap();
}
