use std::{env, fs, io::Write, path::Path, thread, time::Duration};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, ExecutionId,
    FactDraft, FactId, FactKind, ModelRequestId, SessionId, ToolInvocationId, TurnId,
};
use garive_runtime::{
    plan_cancel_turn, plan_continue_turn, plan_schedule_claimed, plan_schedule_created,
    plan_start_turn, reconstruct_suspended_turn, CancelReason, CancelTurnCommand,
    ContinuationInput, ContinueTurnCommand, EffectiveRuntimeLimits, ExecutionLeaseRequest,
    InteractionContinuation, InteractionExpiry, InteractionInputRepresentation, RuntimeCommandId,
    ScheduleLeaseRequest, ScheduleLifecycleContext, SqliteLedger, StartTurnCommand,
};
use garive_scheduler::{
    next_occurrence, MisfirePolicy, ScheduleDecision, ScheduleIntent, ScheduleSubject,
    ScheduleTiming,
};
use serde_json::{json, Value};

mod delegation_fixture_support;

fn main() {
    let arguments: Vec<_> = env::args().collect();
    let database = arguments.get(1).expect("database path");
    let repo = arguments.get(2).expect("repository path");
    let checkpoint = arguments.get(3).expect("checkpoint");
    run(Path::new(database), Path::new(repo), checkpoint);
    println!("READY");
    std::io::stdout().flush().unwrap();
    loop {
        thread::sleep(Duration::from_secs(30));
    }
}

fn run(database: &Path, repo: &Path, checkpoint: &str) {
    if checkpoint.starts_with("delegation_") {
        delegation_fixture_support::run(database, checkpoint);
        return;
    }
    if checkpoint.starts_with("scheduler_") {
        run_scheduler(database, checkpoint);
        return;
    }
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(database).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    if checkpoint == "before_start" {
        return;
    }
    let start = start_command(session.clone());
    let planned = plan_start_turn(&start, 1).unwrap();
    let turn = planned.turn_id.clone();
    let execution = planned.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, planned.facts).unwrap();
    ledger
        .acquire_execution_lease(&ExecutionLeaseRequest {
            turn_id: turn.clone(),
            execution_id: execution.clone(),
            owner_id: "crash-worker".into(),
            lease_token: "crash-token".into(),
            now_ms: 100,
            duration_ms: 10,
        })
        .unwrap();
    if checkpoint == "after_start" {
        return;
    }
    if checkpoint == "iteration_started" {
        ledger
            .commit(
                session,
                2,
                vec![fact(
                    repo,
                    "execution.iteration_started",
                    &turn,
                    &execution,
                    None,
                    None,
                )],
            )
            .unwrap();
        return;
    }
    if checkpoint == "cancel_requested" {
        let cancel = plan_cancel_turn(&CancelTurnCommand {
            command_id: RuntimeCommandId::new("cancel-command").unwrap(),
            session_id: session.clone(),
            turn_id: turn,
            reason: CancelReason::User,
            requested_through_position: 4,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        })
        .unwrap();
        ledger.commit(session, 2, cancel.facts).unwrap();
        return;
    }
    let model_path = checkpoint.starts_with("model_");
    let effect_path = checkpoint.starts_with("effect_")
        || matches!(checkpoint, "before_terminal" | "after_terminal");
    let mut version = 2;
    if model_path {
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            fact(
                repo,
                "model.prepared",
                &turn,
                &execution,
                Some("request"),
                None,
            ),
        );
        if checkpoint == "model_prepared" {
            return;
        }
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            fact(
                repo,
                "model.started",
                &turn,
                &execution,
                Some("request"),
                None,
            ),
        );
        if checkpoint == "model_started" {
            return;
        }
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            fact(
                repo,
                "model.completed",
                &turn,
                &execution,
                Some("request"),
                None,
            ),
        );
        return;
    }
    if checkpoint.starts_with("interaction_") {
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            fact(
                repo,
                "effect.prepared",
                &turn,
                &execution,
                None,
                Some("tool"),
            ),
        );
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            fact(
                repo,
                "interaction.requested",
                &turn,
                &execution,
                None,
                Some("tool"),
            ),
        );
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            suspension(repo, "execution.suspended", &turn, &execution),
        );
        commit_one(
            &mut ledger,
            &session,
            &mut version,
            suspension(repo, "turn.suspended", &turn, &execution),
        );
        if checkpoint == "interaction_requested" {
            return;
        }
        let state = reconstruct_suspended_turn(&ledger.load_turn(&turn).unwrap()).unwrap();
        let continuation = plan_continue_turn(
            &ContinueTurnCommand {
                command_id: RuntimeCommandId::new("continue-command").unwrap(),
                session_id: session.clone(),
                turn_id: turn.clone(),
                expected_suspension_id: "suspension".into(),
                expected_session_version: version,
                continuation_input: ContinuationInput::InteractionResponse {
                    canonical_json: "true".into(),
                    representation: InteractionInputRepresentation::JsonField,
                },
                interaction: Some(InteractionContinuation {
                    execution_id: execution.clone(),
                    tool_invocation_id: ToolInvocationId::try_from("tool").unwrap(),
                    interaction_id: "interaction".into(),
                    prepared_digest: empty_digest().into(),
                    prompt: json!({"message":"","schema_version":1}),
                    response_schema_digest:
                        "7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553".into(),
                    response_schema: json!({"type":"boolean"}),
                    expiry: InteractionExpiry::None,
                }),
                recorded_at: "2026-08-29T00:00:02Z".into(),
            },
            &state,
        )
        .unwrap();
        ledger.commit(session, version, continuation.facts).unwrap();
        return;
    }
    if effect_path {
        for kind in [
            "effect.prepared",
            "effect.authorized",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
        ] {
            commit_one(
                &mut ledger,
                &session,
                &mut version,
                fact(repo, kind, &turn, &execution, None, Some("tool")),
            );
            if checkpoint == kind.replace('.', "_") {
                return;
            }
        }
    }
    if checkpoint == "after_terminal" {
        let execution_terminal = terminal(repo, "execution.completed", &turn, &execution);
        let mut turn_terminal = terminal(repo, "turn.completed", &turn, &execution);
        turn_terminal.execution_id = None;
        ledger
            .commit(session, version, vec![execution_terminal, turn_terminal])
            .unwrap();
    }
}

fn run_scheduler(database: &Path, checkpoint: &str) {
    let session = SessionId::try_from("session").unwrap();
    let intent = ScheduleIntent::new(
        "schedule-1",
        "revision-1",
        ScheduleSubject::StartTurn,
        "aa".repeat(32),
        ScheduleTiming::At {
            due_at_utc: "2026-08-29T00:00:00Z".into(),
        },
        MisfirePolicy::FireOnce,
        500,
        "bb".repeat(32),
    )
    .unwrap();
    let context = ScheduleLifecycleContext {
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let mut ledger = SqliteLedger::open(database).unwrap();
    ledger
        .commit(
            session.clone(),
            0,
            vec![
                open_session(),
                plan_schedule_created(&context, "create", &intent).unwrap(),
            ],
        )
        .unwrap();
    if checkpoint == "scheduler_before_claim" {
        return;
    }
    let occurrence = match next_occurrence(&intent, None, &context.recorded_at).unwrap() {
        ScheduleDecision::Due(value) => value,
        _ => unreachable!(),
    };
    let lease = ledger
        .acquire_schedule_lease(&ScheduleLeaseRequest {
            session_id: session.clone(),
            schedule_id: "schedule-1".into(),
            revision_id: "revision-1".into(),
            occurrence_id: occurrence.occurrence_id.clone(),
            ordinal: occurrence.ordinal,
            owner_id: "crash-scheduler".into(),
            lease_id: "crash-schedule-lease".into(),
            now_ms: 100,
            duration_ms: 10,
        })
        .unwrap();
    let claimed = plan_schedule_claimed(
        &context,
        &intent,
        &occurrence,
        "crash-schedule-lease",
        lease.epoch,
        2,
    )
    .unwrap();
    ledger
        .commit_schedule_leased(&lease, 100, 1, vec![claimed])
        .unwrap();
    if checkpoint == "scheduler_after_claim" {
        return;
    }
    let command = StartTurnCommand {
        command_id: RuntimeCommandId::new(occurrence.runtime_command_id.as_str()).unwrap(),
        session_id: session.clone(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        snapshot_digest: "11".repeat(32),
        trusted_input: "scheduled".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: context.recorded_at,
    };
    let planned = plan_start_turn(&command, 3).unwrap();
    ledger.commit(session, 2, planned.facts).unwrap();
    assert_eq!(checkpoint, "scheduler_after_dispatch");
}

fn suspension(repo: &Path, kind: &str, turn: &TurnId, execution: &ExecutionId) -> FactDraft {
    let mut output = fact(repo, kind, turn, execution, None, None);
    if kind == "execution.suspended" {
        let mut payload = payload(repo, kind);
        payload
            .as_object_mut()
            .unwrap()
            .insert("reason".into(), json!("approval_required"));
        output.payload = CanonicalPayload::from_value(&payload).unwrap();
    }
    if kind == "turn.suspended" {
        output.execution_id = None;
        let mut payload = payload(repo, kind);
        payload
            .as_object_mut()
            .unwrap()
            .insert("execution_id".into(), json!(execution.as_str()));
        output.payload = CanonicalPayload::from_value(&payload).unwrap();
    }
    output
}

fn commit_one(ledger: &mut SqliteLedger, session: &SessionId, version: &mut u64, fact: FactDraft) {
    ledger
        .commit(session.clone(), *version, vec![fact])
        .unwrap();
    *version += 1;
}

fn start_command(session_id: SessionId) -> StartTurnCommand {
    StartTurnCommand {
        command_id: RuntimeCommandId::new("start-command").unwrap(),
        session_id,
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        snapshot_digest: empty_digest().into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 4,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn fact(
    repo: &Path,
    kind: &str,
    turn: &TurnId,
    execution: &ExecutionId,
    request: Option<&str>,
    tool: Option<&str>,
) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(format!("fact-{}", kind.replace('.', "-")).as_str()).unwrap(),
        turn_id: Some(turn.clone()),
        execution_id: Some(execution.clone()),
        model_request_id: request.map(|value| ModelRequestId::try_from(value).unwrap()),
        tool_invocation_id: tool.map(|value| ToolInvocationId::try_from(value).unwrap()),
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload(repo, kind)).unwrap(),
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

fn terminal(repo: &Path, kind: &str, turn: &TurnId, execution: &ExecutionId) -> FactDraft {
    let mut value = fact(repo, kind, turn, execution, None, None);
    if kind == "turn.completed" {
        let mut payload = payload(repo, kind);
        payload
            .as_object_mut()
            .unwrap()
            .insert("execution_id".into(), json!(execution.as_str()));
        value.payload = CanonicalPayload::from_value(&payload).unwrap();
    }
    value
}

fn payload(repo: &Path, kind: &str) -> Value {
    let document: Value = serde_json::from_str(
        &fs::read_to_string(repo.join("spec/fixtures/ledger/runtime-facts-v1.json")).unwrap(),
    )
    .unwrap();
    document["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["kind"].as_str() == Some(kind))
        .unwrap()["payload"]
        .clone()
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("session-open").unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("session.opened").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn empty_digest() -> &'static str {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
