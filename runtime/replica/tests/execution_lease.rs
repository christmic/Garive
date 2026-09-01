use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId,
};
use garive_runtime::{
    commit_planned_turn, plan_recovery_restart, plan_start_turn, EffectiveRuntimeLimits,
    ExecutionLeaseError, ExecutionLeaseRequest, RecoveryRestartCommand, RuntimeCommandId,
    SqliteLedger, SqliteLedgerError, StartTurnCommand,
};
use serde_json::json;
use tempfile::tempdir;

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("lease-session").unwrap(),
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

fn start(session_id: SessionId) -> StartTurnCommand {
    StartTurnCommand {
        command_id: RuntimeCommandId::new("lease-start").unwrap(),
        session_id,
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        snapshot_digest: "11".repeat(32),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

#[test]
fn lease_takeover_requires_expiry_and_durable_recovery() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("lease.sqlite3")).unwrap();
    let session = SessionId::try_from("session").unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let started = plan_start_turn(&start(session.clone()), 1).unwrap();
    let old_execution = started.execution_id.clone().unwrap();
    ledger
        .commit(session.clone(), 1, started.facts.clone())
        .unwrap();

    let old_request = ExecutionLeaseRequest {
        turn_id: started.turn_id.clone(),
        execution_id: old_execution.clone(),
        owner_id: "worker-a".into(),
        lease_token: "token-a".into(),
        now_ms: 100,
        duration_ms: 10,
    };
    let old = ledger.acquire_execution_lease(&old_request).unwrap();
    let mut competing = old_request.clone();
    competing.owner_id = "worker-b".into();
    competing.lease_token = "token-b".into();
    assert_eq!(
        ledger.acquire_execution_lease(&competing),
        Err(ExecutionLeaseError::AlreadyHeld)
    );
    competing.now_ms = 110;
    assert_eq!(
        ledger.acquire_execution_lease(&competing),
        Err(ExecutionLeaseError::RecoveryRequired)
    );

    let recovery = plan_recovery_restart(&RecoveryRestartCommand {
        recovery_id: RuntimeCommandId::new("lease-recovery").unwrap(),
        turn_id: started.turn_id.clone(),
        lost_execution_id: old_execution,
        snapshot_digest: "11".repeat(32),
        last_safe_position: 4,
        completed_iterations: 0,
        recovery_ordinal: 1,
        limits: start(session.clone()).limits,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    })
    .unwrap();
    commit_planned_turn(&mut ledger, session.clone(), 2, &recovery).unwrap();
    competing.execution_id = recovery.execution_id.clone().unwrap();
    competing.now_ms = 105;
    let replacement = ledger.acquire_execution_lease(&competing).unwrap();
    assert_eq!(replacement.generation, 2);

    let stale = FactDraft {
        fact_id: FactId::try_from("stale-write").unwrap(),
        turn_id: Some(started.turn_id),
        execution_id: Some(competing.execution_id),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new("execution.iteration_started").unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&json!({"iteration":1})).unwrap(),
        recorded_at: "2026-08-29T00:00:02Z".into(),
    };
    assert!(matches!(
        ledger.commit_leased(&old, session, 3, vec![stale]),
        Err(SqliteLedgerError::Lease(ExecutionLeaseError::LeaseLost))
    ));
    assert_eq!(
        ledger.release_execution_lease(&replacement),
        Err(ExecutionLeaseError::RecoveryRequired)
    );
}
