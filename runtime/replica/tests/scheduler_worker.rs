use std::{cell::Cell, path::PathBuf};

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId,
};
use garive_runtime::{
    commit_planned_turn, plan_schedule_claimed, plan_schedule_created, plan_start_turn,
    reconstruct_schedule_state, run_schedule_once, EffectiveRuntimeLimits,
    ScheduleAuthorityOperation, ScheduleAuthorityPort, ScheduleClock, ScheduleClockReading,
    ScheduleCommandDispatcher, ScheduleCommandReceipt, ScheduleDispatchDisposition,
    ScheduleLeaseRequest, ScheduleLifecycleContext, ScheduleTickConfig, ScheduleTickOutcome,
    SqliteLedger, StartTurnCommand,
};
use garive_scheduler::{
    next_occurrence, MisfirePolicy, ScheduleDecision, ScheduleErrorCode, ScheduleIntent,
    ScheduleSubject, ScheduleTiming,
};
use serde_json::{json, Value};
use tempfile::tempdir;

struct FixedClock {
    utc: &'static str,
    monotonic_ms: u64,
}
impl ScheduleClock for FixedClock {
    fn observe(&self) -> Result<ScheduleClockReading, ScheduleErrorCode> {
        Ok(ScheduleClockReading {
            observed_at_utc: self.utc.into(),
            monotonic_ms: self.monotonic_ms,
        })
    }
}

struct Authority {
    allowed: bool,
    checks: Cell<usize>,
}
impl ScheduleAuthorityPort for Authority {
    fn authorize(
        &self,
        _: &SessionId,
        _: &ScheduleIntent,
        _: ScheduleAuthorityOperation,
    ) -> Result<(), ScheduleErrorCode> {
        self.checks.set(self.checks.get() + 1);
        if self.allowed {
            Ok(())
        } else {
            Err(ScheduleErrorCode::AuthorityDenied)
        }
    }
}

struct C6Dispatcher {
    path: PathBuf,
    submits: usize,
}
impl ScheduleCommandDispatcher for C6Dispatcher {
    fn reconcile(
        &mut self,
        session_id: &SessionId,
        runtime_command_id: &str,
    ) -> Result<Option<ScheduleCommandReceipt>, ScheduleErrorCode> {
        let ledger =
            SqliteLedger::open(&self.path).map_err(|_| ScheduleErrorCode::DurabilityFailure)?;
        let Some(watermark) = ledger
            .session_watermark(session_id)
            .map_err(|_| ScheduleErrorCode::DurabilityFailure)?
        else {
            return Ok(None);
        };
        let facts = ledger
            .read_facts(session_id, 0, watermark.max_position, None)
            .map_err(|_| ScheduleErrorCode::DurabilityFailure)?;
        Ok(facts.into_iter().find_map(|fact| {
            let value: Value = serde_json::from_str(fact.payload.as_json()).ok()?;
            (value.get("command_id").and_then(Value::as_str) == Some(runtime_command_id)).then(
                || ScheduleCommandReceipt {
                    runtime_command_id: runtime_command_id.into(),
                    disposition: ScheduleDispatchDisposition::Replayed,
                    committed_position: fact.position,
                },
            )
        }))
    }

    fn submit(
        &mut self,
        session_id: &SessionId,
        _: &ScheduleIntent,
        occurrence: &garive_scheduler::DueOccurrence,
    ) -> Result<ScheduleCommandReceipt, ScheduleErrorCode> {
        self.submits += 1;
        let mut ledger =
            SqliteLedger::open(&self.path).map_err(|_| ScheduleErrorCode::DurabilityFailure)?;
        let watermark = ledger
            .session_watermark(session_id)
            .map_err(|_| ScheduleErrorCode::DurabilityFailure)?
            .ok_or(ScheduleErrorCode::ScheduleNotFound)?;
        let command = StartTurnCommand {
            command_id: garive_runtime::RuntimeCommandId::new(
                occurrence.runtime_command_id.as_str(),
            )
            .map_err(|_| ScheduleErrorCode::DispatchConflict)?,
            session_id: session_id.clone(),
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
            recorded_at: "2026-08-29T00:00:00Z".into(),
        };
        let plan = plan_start_turn(&command, watermark.max_position)
            .map_err(|_| ScheduleErrorCode::DispatchConflict)?;
        let result = commit_planned_turn(
            &mut ledger,
            session_id.clone(),
            watermark.session_version,
            &plan,
        )
        .map_err(|_| ScheduleErrorCode::DurabilityFailure)?;
        Ok(ScheduleCommandReceipt {
            runtime_command_id: occurrence.runtime_command_id.clone(),
            disposition: match result.disposition {
                garive_ledger::CommitDisposition::Committed => {
                    ScheduleDispatchDisposition::Committed
                }
                garive_ledger::CommitDisposition::Replayed => ScheduleDispatchDisposition::Replayed,
            },
            committed_position: result.positions[0],
        })
    }
}

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("worker-session").unwrap(),
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

fn intent(policy: MisfirePolicy) -> ScheduleIntent {
    ScheduleIntent::new(
        "schedule-1",
        "revision-1",
        ScheduleSubject::StartTurn,
        "aa".repeat(32),
        ScheduleTiming::At {
            due_at_utc: "2026-08-29T00:00:00Z".into(),
        },
        policy,
        500,
        "bb".repeat(32),
    )
    .unwrap()
}

fn setup(path: &PathBuf, policy: MisfirePolicy) -> (SessionId, ScheduleIntent) {
    let session = SessionId::try_from("session").unwrap();
    let intent = intent(policy);
    let context = ScheduleLifecycleContext {
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let mut ledger = SqliteLedger::open(path).unwrap();
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
    (session, intent)
}

fn config(owner: &str, lease: &str) -> ScheduleTickConfig {
    ScheduleTickConfig {
        owner_id: owner.into(),
        lease_id: lease.into(),
        lease_duration_ms: 10,
    }
}

#[test]
fn worker_commits_real_c6_then_fired_and_exhausted() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("worker.sqlite3");
    let (session, _) = setup(&path, MisfirePolicy::FireOnce);
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let authority = Authority {
        allowed: true,
        checks: Cell::new(0),
    };
    let mut dispatcher = C6Dispatcher { path, submits: 0 };
    let fired = run_schedule_once(
        &mut ledger,
        &session,
        "schedule-1",
        &config("worker", "lease-1"),
        &FixedClock {
            utc: "2026-08-29T00:00:00Z",
            monotonic_ms: 100,
        },
        &authority,
        &mut dispatcher,
    )
    .unwrap();
    assert!(matches!(fired, ScheduleTickOutcome::Fired(_)));
    assert_eq!(dispatcher.submits, 1);
    assert_eq!(
        run_schedule_once(
            &mut ledger,
            &session,
            "schedule-1",
            &config("worker", "lease-2"),
            &FixedClock {
                utc: "2026-08-29T00:00:01Z",
                monotonic_ms: 101,
            },
            &authority,
            &mut dispatcher,
        )
        .unwrap(),
        ScheduleTickOutcome::Exhausted
    );
    assert!(
        !reconstruct_schedule_state(&ledger, &session, "schedule-1")
            .unwrap()
            .active
    );
    assert_eq!(authority.checks.get(), 2);
}

#[test]
fn restart_after_claim_redispatches_same_identity_once() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("restart.sqlite3");
    let (session, intent) = setup(&path, MisfirePolicy::FireOnce);
    let occurrence = match next_occurrence(&intent, None, "2026-08-29T00:00:00Z").unwrap() {
        ScheduleDecision::Due(value) => value,
        other => panic!("unexpected decision: {other:?}"),
    };
    let mut before_kill = SqliteLedger::open(&path).unwrap();
    let lease = before_kill
        .acquire_schedule_lease(&ScheduleLeaseRequest {
            session_id: session.clone(),
            schedule_id: "schedule-1".into(),
            revision_id: "revision-1".into(),
            occurrence_id: occurrence.occurrence_id.clone(),
            ordinal: 1,
            owner_id: "dead-worker".into(),
            lease_id: "dead-lease".into(),
            now_ms: 100,
            duration_ms: 10,
        })
        .unwrap();
    let claim = plan_schedule_claimed(
        &ScheduleLifecycleContext {
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        &intent,
        &occurrence,
        "dead-lease",
        lease.epoch,
        2,
    )
    .unwrap();
    before_kill
        .commit_schedule_leased(&lease, 100, 1, vec![claim])
        .unwrap();
    drop(before_kill);

    let mut restarted = SqliteLedger::open(&path).unwrap();
    let authority = Authority {
        allowed: true,
        checks: Cell::new(0),
    };
    let mut dispatcher = C6Dispatcher { path, submits: 0 };
    let outcome = run_schedule_once(
        &mut restarted,
        &session,
        "schedule-1",
        &config("replacement", "replacement-lease"),
        &FixedClock {
            utc: "2026-08-29T00:00:01Z",
            monotonic_ms: 110,
        },
        &authority,
        &mut dispatcher,
    )
    .unwrap();
    assert!(matches!(outcome, ScheduleTickOutcome::Fired(_)));
    assert_eq!(dispatcher.submits, 1);
}

#[test]
fn restart_after_c6_commit_reconciles_without_second_submit() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("reconcile.sqlite3");
    let (session, intent) = setup(&path, MisfirePolicy::FireOnce);
    let occurrence = match next_occurrence(&intent, None, "2026-08-29T00:00:00Z").unwrap() {
        ScheduleDecision::Due(value) => value,
        other => panic!("unexpected decision: {other:?}"),
    };
    let mut before_kill = SqliteLedger::open(&path).unwrap();
    let lease = before_kill
        .acquire_schedule_lease(&ScheduleLeaseRequest {
            session_id: session.clone(),
            schedule_id: "schedule-1".into(),
            revision_id: "revision-1".into(),
            occurrence_id: occurrence.occurrence_id.clone(),
            ordinal: 1,
            owner_id: "dead-worker".into(),
            lease_id: "dead-lease".into(),
            now_ms: 100,
            duration_ms: 10,
        })
        .unwrap();
    let context = ScheduleLifecycleContext {
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let claim = plan_schedule_claimed(&context, &intent, &occurrence, "dead-lease", lease.epoch, 2)
        .unwrap();
    before_kill
        .commit_schedule_leased(&lease, 100, 1, vec![claim])
        .unwrap();
    let mut dispatcher = C6Dispatcher {
        path: path.clone(),
        submits: 0,
    };
    dispatcher.submit(&session, &intent, &occurrence).unwrap();
    assert_eq!(dispatcher.submits, 1);
    drop(before_kill);

    let mut restarted = SqliteLedger::open(&path).unwrap();
    let authority = Authority {
        allowed: true,
        checks: Cell::new(0),
    };
    let outcome = run_schedule_once(
        &mut restarted,
        &session,
        "schedule-1",
        &config("replacement", "replacement-lease"),
        &FixedClock {
            utc: "2026-08-29T00:00:01Z",
            monotonic_ms: 110,
        },
        &authority,
        &mut dispatcher,
    )
    .unwrap();
    let ScheduleTickOutcome::Fired(receipt) = outcome else {
        panic!("expected fired")
    };
    assert_eq!(receipt.disposition, ScheduleDispatchDisposition::Replayed);
    assert_eq!(dispatcher.submits, 1);
}

#[test]
fn authority_is_revalidated_and_denial_becomes_durable_failure() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("denied.sqlite3");
    let (session, _) = setup(&path, MisfirePolicy::FireOnce);
    let mut ledger = SqliteLedger::open(&path).unwrap();
    let authority = Authority {
        allowed: false,
        checks: Cell::new(0),
    };
    let mut dispatcher = C6Dispatcher { path, submits: 0 };
    assert_eq!(
        run_schedule_once(
            &mut ledger,
            &session,
            "schedule-1",
            &config("worker", "lease"),
            &FixedClock {
                utc: "2026-08-29T00:00:00Z",
                monotonic_ms: 100,
            },
            &authority,
            &mut dispatcher,
        )
        .unwrap(),
        ScheduleTickOutcome::Failed(ScheduleErrorCode::AuthorityDenied)
    );
    assert_eq!(dispatcher.submits, 0);
    assert!(
        !reconstruct_schedule_state(&ledger, &session, "schedule-1")
            .unwrap()
            .active
    );
}

#[test]
fn overdue_skip_and_fail_policies_commit_exact_terminals() {
    for (policy, expected) in [
        (
            MisfirePolicy::Skip,
            ScheduleTickOutcome::Skipped {
                first_ordinal: 1,
                last_ordinal: 1,
            },
        ),
        (
            MisfirePolicy::Fail,
            ScheduleTickOutcome::Failed(ScheduleErrorCode::MisfireLimitExceeded),
        ),
    ] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("misfire.sqlite3");
        let (session, _) = setup(&path, policy);
        let mut ledger = SqliteLedger::open(&path).unwrap();
        let authority = Authority {
            allowed: true,
            checks: Cell::new(0),
        };
        let mut dispatcher = C6Dispatcher { path, submits: 0 };
        assert_eq!(
            run_schedule_once(
                &mut ledger,
                &session,
                "schedule-1",
                &config("worker", "lease"),
                &FixedClock {
                    utc: "2026-08-29T00:00:01Z",
                    monotonic_ms: 100,
                },
                &authority,
                &mut dispatcher,
            )
            .unwrap(),
            expected
        );
        assert_eq!(dispatcher.submits, 0);
    }
}
