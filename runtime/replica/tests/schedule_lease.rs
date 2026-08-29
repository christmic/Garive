use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_runtime::{
    plan_schedule_claimed, plan_schedule_created, plan_schedule_fired, ScheduleDispatchDisposition,
    ScheduleLeaseError, ScheduleLeaseRequest, ScheduleLifecycleContext, SqliteLedger,
    SqliteLedgerError,
};
use garive_scheduler::{
    next_occurrence, MisfirePolicy, ScheduleDecision, ScheduleIntent, ScheduleSubject,
    ScheduleTiming,
};
use serde_json::json;
use tempfile::tempdir;

fn open_session() -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from("schedule-session").unwrap(),
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

fn intent() -> ScheduleIntent {
    ScheduleIntent::new(
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
    .unwrap()
}

fn request(
    session_id: SessionId,
    occurrence_id: &str,
    owner_id: &str,
    lease_id: &str,
) -> ScheduleLeaseRequest {
    ScheduleLeaseRequest {
        session_id,
        schedule_id: "schedule-1".into(),
        revision_id: "revision-1".into(),
        occurrence_id: occurrence_id.into(),
        ordinal: 1,
        owner_id: owner_id.into(),
        lease_id: lease_id.into(),
        now_ms: 100,
        duration_ms: 10,
    }
}

#[test]
fn two_workers_fence_claim_takeover_and_terminal_write() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("schedule.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    let schedule = intent();
    let at = ScheduleLifecycleContext {
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let occurrence = match next_occurrence(&schedule, None, &at.recorded_at).unwrap() {
        ScheduleDecision::Due(value) => value,
        other => panic!("unexpected decision: {other:?}"),
    };
    let mut worker_a = SqliteLedger::open(&path).unwrap();
    worker_a
        .commit(
            session.clone(),
            0,
            vec![
                open_session(),
                plan_schedule_created(&at, "create", &schedule).unwrap(),
            ],
        )
        .unwrap();
    let lease_a = worker_a
        .acquire_schedule_lease(&request(
            session.clone(),
            &occurrence.occurrence_id,
            "worker-a",
            "lease-a",
        ))
        .unwrap();
    assert_eq!(lease_a.epoch, 1);
    let claim_a = plan_schedule_claimed(&at, &schedule, &occurrence, "lease-a", 1, 2).unwrap();
    worker_a
        .commit_schedule_leased(&lease_a, 100, 1, vec![claim_a])
        .unwrap();

    let mut worker_b = SqliteLedger::open(&path).unwrap();
    let mut competing = request(session, &occurrence.occurrence_id, "worker-b", "lease-b");
    assert_eq!(
        worker_b.acquire_schedule_lease(&competing),
        Err(ScheduleLeaseError::AlreadyHeld)
    );
    competing.now_ms = 110;
    let lease_b = worker_b.acquire_schedule_lease(&competing).unwrap();
    assert_eq!(lease_b.epoch, 2);
    let claim_b = plan_schedule_claimed(&at, &schedule, &occurrence, "lease-b", 2, 3).unwrap();
    worker_b
        .commit_schedule_leased(&lease_b, 110, 2, vec![claim_b])
        .unwrap();

    let fired = plan_schedule_fired(
        &at,
        &schedule,
        &occurrence,
        ScheduleDispatchDisposition::Committed,
        9,
    )
    .unwrap();
    assert!(matches!(
        worker_a.commit_schedule_leased(&lease_a, 105, 3, vec![fired.clone()]),
        Err(SqliteLedgerError::ScheduleLease(
            ScheduleLeaseError::LeaseLost
        ))
    ));
    worker_b
        .commit_schedule_leased(&lease_b, 111, 3, vec![fired])
        .unwrap();
    worker_b.release_schedule_lease(&lease_b).unwrap();
}

#[test]
fn lease_requires_an_active_exact_revision() {
    let directory = tempdir().unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("inactive.sqlite3")).unwrap();
    let session = SessionId::try_from("session").unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    assert_eq!(
        ledger.acquire_schedule_lease(&request(session, "occurrence", "worker", "lease")),
        Err(ScheduleLeaseError::RevisionNotActive)
    );
}
