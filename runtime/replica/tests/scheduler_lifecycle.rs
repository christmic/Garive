use garive_ledger::FactDraft;
use garive_runtime::{
    plan_schedule_cancelled, plan_schedule_claimed, plan_schedule_created, plan_schedule_failed,
    plan_schedule_fired, plan_schedule_skipped, ScheduleCancelReason, ScheduleDispatchDisposition,
    ScheduleLifecycleContext,
};
use garive_scheduler::{
    next_occurrence, MisfirePolicy, ScheduleDecision, ScheduleErrorCode, ScheduleIntent,
    ScheduleSubject, ScheduleTiming,
};
use serde_json::Value;

fn context(recorded_at: &str) -> ScheduleLifecycleContext {
    ScheduleLifecycleContext {
        recorded_at: recorded_at.into(),
    }
}

fn intent(policy: MisfirePolicy) -> ScheduleIntent {
    ScheduleIntent::new(
        "schedule-1",
        "revision-1",
        ScheduleSubject::StartTurn,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ScheduleTiming::FixedDelay {
            first_due_at_utc: "2026-08-29T00:00:00Z".into(),
            delay_ms: 1_000,
            max_occurrences: Some(5),
        },
        policy,
        500,
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    )
    .unwrap()
}

fn payload(fact: &FactDraft) -> Value {
    serde_json::from_str(fact.payload.as_json()).unwrap()
}

#[test]
fn every_scheduler_fact_is_exact_and_session_scoped() {
    let at = context("2026-08-29T00:00:00Z");
    let schedule = intent(MisfirePolicy::FireOnce);
    let created = plan_schedule_created(&at, "schedule-create", &schedule).unwrap();
    assert_eq!(created.kind.as_str(), "schedule.created");
    assert!(created.turn_id.is_none() && created.execution_id.is_none());
    assert_eq!(
        payload(&created)["intent_digest"],
        payload(&created)["intent"]["digest"]
    );
    created.validate().unwrap();

    let occurrence = match next_occurrence(&schedule, None, &at.recorded_at).unwrap() {
        ScheduleDecision::Due(value) => value,
        other => panic!("unexpected decision: {other:?}"),
    };
    let claimed = plan_schedule_claimed(&at, &schedule, &occurrence, "lease-1", 1, 7).unwrap();
    assert_eq!(payload(&claimed)["through_position"], 7);
    claimed.validate().unwrap();
    let fired = plan_schedule_fired(
        &at,
        &schedule,
        &occurrence,
        ScheduleDispatchDisposition::Committed,
        8,
    )
    .unwrap();
    assert_eq!(
        payload(&fired)["runtime_command_id"],
        occurrence.runtime_command_id
    );
    fired.validate().unwrap();

    let cancelled = plan_schedule_cancelled(
        &at,
        "schedule-cancel",
        schedule.schedule_id(),
        schedule.revision_id(),
        ScheduleCancelReason::Policy,
    )
    .unwrap();
    assert_eq!(payload(&cancelled)["reason"], "policy");
    cancelled.validate().unwrap();

    let failed = plan_schedule_failed(
        &at,
        &schedule,
        Some(&occurrence),
        ScheduleErrorCode::DispatchConflict,
    )
    .unwrap();
    assert_eq!(payload(&failed)["reason"], "dispatch_conflict");
    failed.validate().unwrap();
}

#[test]
fn skipped_ranges_and_planner_boundaries_are_closed() {
    let at = context("2026-08-29T00:00:03.600Z");
    let schedule = intent(MisfirePolicy::Skip);
    let skipped = match next_occurrence(&schedule, Some(1), &at.recorded_at).unwrap() {
        ScheduleDecision::Skipped(value) => value,
        other => panic!("unexpected decision: {other:?}"),
    };
    let fact = plan_schedule_skipped(&at, &schedule, &skipped, &at.recorded_at).unwrap();
    assert_eq!(payload(&fact)["first_ordinal"], 2);
    assert_eq!(payload(&fact)["last_ordinal"], 4);
    fact.validate().unwrap();

    assert!(plan_schedule_created(&at, "", &schedule).is_err());
    assert!(
        plan_schedule_claimed(&at, &schedule, skipped.next_due.as_ref().unwrap(), "", 1, 0)
            .is_err()
    );
    assert!(plan_schedule_claimed(
        &at,
        &schedule,
        skipped.next_due.as_ref().unwrap(),
        "lease",
        0,
        0
    )
    .is_err());
    assert!(plan_schedule_fired(
        &at,
        &schedule,
        skipped.next_due.as_ref().unwrap(),
        ScheduleDispatchDisposition::Replayed,
        0,
    )
    .is_err());
    assert!(
        plan_schedule_skipped(&at, &schedule, &skipped, "2026-08-29T08:00:03.600+08:00",).is_err()
    );
}
