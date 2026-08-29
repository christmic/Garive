use garive_scheduler::{
    next_occurrence, schedule_occurrence, MisfirePolicy, ScheduleDecision, ScheduleErrorCode,
    ScheduleIntent, ScheduleSubject, ScheduleTiming,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/scheduler-v1.json"
    ))
    .unwrap()
}

fn policy(value: &str) -> MisfirePolicy {
    match value {
        "fire_once" => MisfirePolicy::FireOnce,
        "skip" => MisfirePolicy::Skip,
        "fail" => MisfirePolicy::Fail,
        _ => panic!("unknown fixture policy"),
    }
}

fn intent(root: &Value, policy: MisfirePolicy) -> ScheduleIntent {
    let value = &root["intent"];
    let timing = &value["timing"];
    ScheduleIntent::new(
        value["schedule_id"].as_str().unwrap(),
        value["revision_id"].as_str().unwrap(),
        ScheduleSubject::StartTurn,
        value["subject_binding_digest"].as_str().unwrap(),
        ScheduleTiming::FixedDelay {
            first_due_at_utc: timing["first_due_at_utc"].as_str().unwrap().into(),
            delay_ms: timing["delay_ms"].as_u64().unwrap(),
            max_occurrences: timing["max_occurrences"].as_u64(),
        },
        policy,
        value["max_lateness_ms"].as_u64().unwrap(),
        value["effective_limits_digest"].as_str().unwrap(),
    )
    .unwrap()
}

#[test]
fn shared_intent_and_occurrence_digests_are_frozen() {
    let root = fixture();
    let intent = intent(&root, MisfirePolicy::FireOnce);
    assert_eq!(
        intent.intent_digest().unwrap(),
        root["intent"]["expected_intent_digest"]
    );
    let binding = intent.intent_binding().unwrap();
    assert_eq!(
        ScheduleIntent::from_binding("schedule-1", "revision-1", &binding).unwrap(),
        intent
    );
    let mut corrupt = binding;
    corrupt.digest = "0".repeat(64);
    assert_eq!(
        ScheduleIntent::from_binding("schedule-1", "revision-1", &corrupt)
            .unwrap_err()
            .code(),
        ScheduleErrorCode::CorruptScheduleState
    );
    let decision = next_occurrence(&intent, None, "2026-08-29T00:00:00Z").unwrap();
    let ScheduleDecision::Due(first) = decision else {
        panic!("expected due")
    };
    assert_eq!(
        first.occurrence_id,
        root["first_occurrence"]["expected_occurrence_id"]
    );
    assert_eq!(
        first.runtime_command_id,
        root["first_occurrence"]["expected_runtime_command_id"]
    );
}

#[test]
fn shared_recurrence_and_misfire_cases_are_bounded() {
    let root = fixture();
    for case in root["decision_cases"].as_array().unwrap() {
        let decision = next_occurrence(
            &intent(&root, policy(case["policy"].as_str().unwrap())),
            case["last_handled"].as_u64(),
            case["now"].as_str().unwrap(),
        )
        .unwrap();
        let actual = match &decision {
            ScheduleDecision::NotDue(_) => "not_due",
            ScheduleDecision::Due(_) => "due",
            ScheduleDecision::Skipped(value) => {
                assert_eq!(value.first_ordinal, case["first_skipped"].as_u64().unwrap());
                assert_eq!(value.last_ordinal, case["last_skipped"].as_u64().unwrap());
                assert_eq!(
                    value.next_due.as_ref().map(|item| item.ordinal),
                    case["next_ordinal"].as_u64()
                );
                "skipped"
            }
            ScheduleDecision::Exhausted => "exhausted",
            ScheduleDecision::FailMisfire(_) => "fail_misfire",
        };
        assert_eq!(actual, case["expected"], "{}", case["name"]);
    }
}

#[test]
fn monotonicity_and_invalid_matrix_fail_closed() {
    let root = fixture();
    let intent = intent(&root, MisfirePolicy::FireOnce);
    let mut prior = String::new();
    for handled in 0..5 {
        let decision = next_occurrence(&intent, Some(handled), "2026-08-28T00:00:00Z").unwrap();
        let ScheduleDecision::NotDue(value) = decision else {
            panic!("expected future")
        };
        assert!(value.due_at_utc > prior);
        prior = value.due_at_utc;
    }
    assert_eq!(
        next_occurrence(&intent, None, "not-a-clock")
            .unwrap_err()
            .code(),
        ScheduleErrorCode::ClockInvalid,
    );
    let overflow = ScheduleIntent::new(
        "schedule",
        "revision",
        ScheduleSubject::StartTurn,
        "a".repeat(64),
        ScheduleTiming::FixedDelay {
            first_due_at_utc: "2026-08-29T00:00:00Z".into(),
            delay_ms: u64::MAX,
            max_occurrences: None,
        },
        MisfirePolicy::FireOnce,
        1,
        "b".repeat(64),
    )
    .unwrap();
    assert_eq!(
        next_occurrence(&overflow, Some(1), "2026-08-29T00:00:00Z")
            .unwrap_err()
            .code(),
        ScheduleErrorCode::OccurrenceOverflow,
    );
    assert_eq!(root["invalid_cases"].as_array().unwrap().len(), 4);
    let codes = [
        ScheduleErrorCode::InvalidSchedule,
        ScheduleErrorCode::ScheduleNotFound,
        ScheduleErrorCode::RevisionConflict,
        ScheduleErrorCode::SubjectNotResumable,
        ScheduleErrorCode::AuthorityDenied,
        ScheduleErrorCode::ClockInvalid,
        ScheduleErrorCode::OccurrenceOverflow,
        ScheduleErrorCode::MisfireLimitExceeded,
        ScheduleErrorCode::LeaseLost,
        ScheduleErrorCode::DispatchConflict,
        ScheduleErrorCode::DurabilityFailure,
        ScheduleErrorCode::CorruptScheduleState,
    ];
    assert_eq!(
        codes.map(ScheduleErrorCode::wire_name).as_slice(),
        root["failure_codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn recurrence_properties_hold_across_delays_and_large_misfires() {
    for delay_ms in [1, 7, 1_000, u32::MAX as u64] {
        let value = ScheduleIntent::new(
            format!("schedule-{delay_ms}"),
            "revision",
            ScheduleSubject::StartTurn,
            "a".repeat(64),
            ScheduleTiming::FixedDelay {
                first_due_at_utc: "2026-08-29T00:00:00Z".into(),
                delay_ms,
                max_occurrences: Some(64),
            },
            MisfirePolicy::FireOnce,
            1,
            "b".repeat(64),
        )
        .unwrap();
        let mut previous = None;
        let mut identities = std::collections::BTreeSet::new();
        for ordinal in 1..=64 {
            let occurrence = schedule_occurrence(&value, ordinal).unwrap().unwrap();
            let due = chrono::DateTime::parse_from_rfc3339(&occurrence.due_at_utc).unwrap();
            assert!(previous.as_ref().is_none_or(|prior| &due > prior));
            assert!(identities.insert(occurrence.occurrence_id));
            previous = Some(due);
        }
        assert!(schedule_occurrence(&value, 65).unwrap().is_none());
    }

    let unbounded = ScheduleIntent::new(
        "large-misfire",
        "revision",
        ScheduleSubject::StartTurn,
        "a".repeat(64),
        ScheduleTiming::FixedDelay {
            first_due_at_utc: "2026-08-29T00:00:00Z".into(),
            delay_ms: 1,
            max_occurrences: None,
        },
        MisfirePolicy::Skip,
        1,
        "b".repeat(64),
    )
    .unwrap();
    let ScheduleDecision::Skipped(range) =
        next_occurrence(&unbounded, None, "2026-08-30T00:00:00Z").unwrap()
    else {
        panic!("expected one bounded skip range")
    };
    assert_eq!(range.first_ordinal, 1);
    assert!(range.last_ordinal > 80_000_000);
    assert_eq!(range.next_due.unwrap().ordinal, range.last_ordinal + 1);
}
