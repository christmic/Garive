//! Deterministic production Host composition for TUI documentation capture.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use garive_core::{AgentOutcome, ExecutionReport, StopReason, SuspensionReason, UsageSummary};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{
    plan_core_terminal, CommittedTurn, CoreTerminalContext, EffectiveRuntimeLimits, HostClock,
    InstalledAgent, LiveHost, LiveHostLimits, LiveHostServer, SqliteLedger, TurnDispatchError,
    TurnDispatcher,
};

struct DemoClock;

impl HostClock for DemoClock {
    fn recorded_at(&self) -> String {
        chrono::Utc::now().to_rfc3339()
    }
}

struct DemoDispatcher {
    database: PathBuf,
    calls: AtomicUsize,
}

impl TurnDispatcher for DemoDispatcher {
    fn dispatch(&self, turn: &CommittedTurn) -> Result<(), TurnDispatchError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let database = self.database.clone();
        let turn = turn.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(if call == 4 { 3_000 } else { 1_200 }));
            if call == 3 {
                wait_for_cancel(database, turn);
            } else {
                commit_demo_outcome(database, turn, call);
            }
        });
        Ok(())
    }
}

fn commit_demo_outcome(database: PathBuf, turn: CommittedTurn, call: usize) {
    let usage = usage();
    let outcome = match call {
        1 => AgentOutcome::Suspended {
            reason: SuspensionReason::PartialOutput,
            partial_items: vec![ModelItem::Text {
                text: "I prepared the release plan and need your confirmation before continuing."
                    .into(),
            }],
            last_durable_position: turn.committed_position,
            governed_binding: None,
        },
        0 | 4 => AgentOutcome::Completed {
            response_items: vec![ModelItem::Text {
                text: if call == 0 {
                    "## Release brief\n\n- Runtime checks passed\n- Durable state verified\n- Unicode ready: 你好 🦀\n\n`cargo test -p garive-tui`"
                } else {
                    "Background review completed without blocking the active Session."
                }
                .into(),
            }],
            usage,
        },
        _ => AgentOutcome::Completed {
            response_items: vec![ModelItem::Text {
                text: "Confirmed. The same durable Turn continued and completed successfully."
                    .into(),
            }],
            usage,
        },
    };
    commit_report(database, turn, outcome);
}

fn wait_for_cancel(database: PathBuf, mut turn: CommittedTurn) {
    for _ in 0..600 {
        let ledger = SqliteLedger::open(&database).unwrap();
        if ledger.load_turn(&turn.turn_id).is_ok_and(|snapshot| {
            snapshot
                .facts
                .iter()
                .any(|fact| fact.kind.as_str() == "turn.cancel_requested")
        }) {
            turn.session_version = ledger
                .session_watermark(&turn.session_id)
                .unwrap()
                .unwrap()
                .session_version;
            drop(ledger);
            commit_report(
                database,
                turn,
                AgentOutcome::Stopped {
                    reason: StopReason::Cancelled,
                },
            );
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn commit_report(database: PathBuf, turn: CommittedTurn, outcome: AgentOutcome) {
    let usage = usage();
    let report = ExecutionReport {
        outcome,
        completed_iterations: 1,
        usage,
    };
    let facts = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: turn.turn_id,
            execution_id: turn.execution_id,
            recorded_at: chrono::Utc::now().to_rfc3339(),
        },
        &report,
    )
    .unwrap();
    SqliteLedger::open(&database)
        .unwrap()
        .commit(turn.session_id, turn.session_version, facts)
        .unwrap();
}

fn usage() -> UsageSummary {
    UsageSummary {
        input_tokens: TokenCount::Known(24),
        output_tokens: TokenCount::Known(42),
        estimated: false,
    }
}

fn installed() -> InstalledAgent {
    InstalledAgent {
        definition_id: "garive-demo".into(),
        definition_revision: "visual-v1".into(),
        snapshot_digest: "a".repeat(64),
        agent_instance_namespace: "garive-visual-demo".into(),
        public_capabilities: Vec::new(),
        public_activity_catalogue: None,
        runtime_limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: Some(4_096),
            max_output_tokens: Some(2_048),
            deadline_budget_ms: Some(30_000),
        },
    }
}

#[tokio::main]
async fn main() {
    let mut arguments = std::env::args_os().skip(1);
    let database = arguments
        .next()
        .map(PathBuf::from)
        .expect("usage: visual_demo_host <database-path> [loopback-address]");
    let address = arguments
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse().ok()))
        .unwrap_or_else(|| "127.0.0.1:4317".parse().unwrap());
    assert!(arguments.next().is_none(), "unexpected argument");

    let host = LiveHost::new(
        &database,
        installed(),
        LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 20,
            activity: None,
        },
        Arc::new(DemoClock),
        Arc::new(DemoDispatcher {
            database: database.clone(),
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap();
    let server = LiveHostServer::bind(host, address).await.unwrap();
    eprintln!("GARIVE_TUI_DEMO_HOST=http://{}/", server.local_addr());
    server
        .serve(async {
            tokio::signal::ctrl_c().await.unwrap();
        })
        .await
        .unwrap();
}
