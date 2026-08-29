use std::{fs, path::PathBuf};

use garive_memory::{
    advance_distillation, audit_memory, decide_candidate, AdmissionAssessment, CandidateStability,
    ContentBinding, DistillationWatermark, DurableFactReference, HypothesisState,
    MaintenanceNoopCode, MemoryAuditAction, MemoryAuditEntry, MemoryAuditPolicy, MemoryAuthority,
    MemoryAuthorityBinding, MemoryCandidate, MemoryCandidateIntent, MemoryCandidateSource,
    MemoryContradiction, MemoryKind, MemoryMaintenanceDecision, MemoryScopeBinding,
    MemoryScopeClass, MemoryType, WatermarkDisposition,
};
use serde_json::Value;

#[test]
fn shared_candidates_reduce_to_exact_four_way_decisions() {
    let root = fixture();
    for case in root["candidate_cases"].as_array().unwrap() {
        let result = candidate(case, &root["evidence"]).and_then(|candidate| {
            let assessment = assessment(case)?;
            decide_candidate(&candidate, assessment.as_ref(), "maintenance-decision")
        });
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            assert_eq!(
                decision_name(result.unwrap()),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn shared_distillation_watermarks_are_monotonic_and_replayable() {
    let root = fixture();
    for case in root["watermark_cases"].as_array().unwrap() {
        let prior = case.get("prior").map(watermark).transpose().unwrap();
        let next = watermark(&case["next"]).unwrap();
        let result = advance_distillation(prior.as_ref(), &next);
        if let Some(failure) = case.get("failure") {
            assert_eq!(
                result.unwrap_err().code().wire_name(),
                failure.as_str().unwrap(),
                "{}",
                case["name"]
            );
        } else {
            let name = match result.unwrap() {
                WatermarkDisposition::Advanced => "advanced",
                WatermarkDisposition::Replayed => "replayed",
            };
            assert_eq!(name, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }
}

#[test]
fn shared_memory_audit_is_bounded_deterministic_and_read_only() {
    let root = fixture();
    let audit = &root["audit"];
    let entries = audit["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(audit_entry)
        .collect::<Vec<_>>();
    let contradictions = audit["contradictions"]
        .as_array()
        .unwrap()
        .iter()
        .map(contradiction)
        .collect::<Vec<_>>();
    let policy = &audit["policy"];
    let report = audit_memory(
        &entries,
        &contradictions,
        number(audit, "current_position"),
        MemoryAuditPolicy {
            max_active_records: number(policy, "max_active_records").try_into().unwrap(),
            max_active_bytes: number(policy, "max_active_bytes"),
            stale_after_positions: number(policy, "stale_after_positions"),
            low_use_threshold: number(policy, "low_use_threshold"),
            max_report_items: number(policy, "max_report_items").try_into().unwrap(),
        },
    )
    .unwrap();
    let expected = &audit["expected"];
    assert_eq!(
        report
            .duplicate_groups
            .iter()
            .map(|group| group.iter().map(identity_name).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        strings_2d(&expected["duplicate_groups"])
    );
    assert_eq!(
        report.stale.iter().map(identity_name).collect::<Vec<_>>(),
        strings(&expected["stale"])
    );
    assert_eq!(
        report.low_use.iter().map(identity_name).collect::<Vec<_>>(),
        strings(&expected["low_use"])
    );
    assert_eq!(
        report.actions.iter().map(action_name).collect::<Vec<_>>(),
        strings(&expected["actions"])
    );
    assert_eq!(report.truncated, expected["truncated"].as_bool().unwrap());
    assert_eq!(report.contradictions, contradictions);

    let second = audit_memory(
        &entries,
        &contradictions,
        number(audit, "current_position"),
        MemoryAuditPolicy {
            max_active_records: 2,
            max_active_bytes: 70,
            stale_after_positions: 50,
            low_use_threshold: 2,
            max_report_items: 20,
        },
    )
    .unwrap();
    assert_eq!(report, second);
    assert_eq!(
        audit_memory(
            &entries,
            &contradictions,
            number(audit, "current_position"),
            MemoryAuditPolicy {
                max_active_records: 2,
                max_active_bytes: 70,
                stale_after_positions: 50,
                low_use_threshold: 2,
                max_report_items: 3,
            },
        )
        .unwrap_err()
        .code()
        .wire_name(),
        "limit_exceeded",
    );
}

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/memory-maintenance-v1.json");
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn candidate(
    case: &Value,
    evidence: &Value,
) -> Result<MemoryCandidate, garive_memory::MemoryError> {
    let authority = MemoryAuthorityBinding::new(
        authority(case["authority"].as_str().unwrap()),
        case.get("receipt_digest")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )?;
    let intent = match case["intent"].as_str().unwrap() {
        "learn" => MemoryCandidateIntent::Learn {
            memory_type: MemoryType::Lesson,
            role: MemoryKind::LearnedFact,
            authority,
            scope: MemoryScopeBinding::new(MemoryScopeClass::User, None).unwrap(),
            content: ContentBinding::from_inline("memory"),
            content_bytes: 6,
            evidence: vec![fact_reference(evidence)],
        },
        "forget" => MemoryCandidateIntent::Forget {
            record_id: case["target_record_id"].as_str().unwrap().to_owned(),
            revision_id: case["target_revision_id"].as_str().unwrap().to_owned(),
            authority,
        },
        other => panic!("unknown candidate intent: {other}"),
    };
    MemoryCandidate::new(
        case["name"].as_str().unwrap(),
        "namespace-maintenance",
        "extractor-v1",
        source(case["source"].as_str().unwrap()),
        intent,
    )
}

fn assessment(case: &Value) -> Result<Option<AdmissionAssessment>, garive_memory::MemoryError> {
    if case["intent"] == "forget" {
        return Ok(None);
    }
    AdmissionAssessment::new(
        case["generalizable"].as_bool().unwrap(),
        match case["stability"].as_str().unwrap() {
            "confirmed" => CandidateStability::Confirmed,
            "uncertain" => CandidateStability::Uncertain,
            other => panic!("unknown stability: {other}"),
        },
        case.get("duplicate_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        case.get("conflicting_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .map(Some)
}

fn decision_name(decision: MemoryMaintenanceDecision) -> String {
    match decision {
        MemoryMaintenanceDecision::Add { .. } => "add".to_owned(),
        MemoryMaintenanceDecision::Update {
            expected_active_revision_id,
            ..
        } => format!("update:{expected_active_revision_id}"),
        MemoryMaintenanceDecision::Delete {
            record_id,
            revision_id,
            ..
        } => format!("delete:{record_id}:{revision_id}"),
        MemoryMaintenanceDecision::Noop { code } => match code {
            MaintenanceNoopCode::NotGeneralizable => "noop_not_generalizable".to_owned(),
            MaintenanceNoopCode::UnstableDeferred => "noop_unstable_deferred".to_owned(),
            MaintenanceNoopCode::Duplicate => "noop_duplicate".to_owned(),
        },
    }
}

fn watermark(value: &Value) -> Result<DistillationWatermark, garive_memory::MemoryError> {
    DistillationWatermark::new(
        value["extractor_revision"].as_str().unwrap(),
        value["session_id"].as_str().unwrap(),
        number(value, "through_position"),
        value["batch_digest"].as_str().unwrap(),
    )
}

fn audit_entry(value: &Value) -> MemoryAuditEntry {
    MemoryAuditEntry {
        record_id: value["record_id"].as_str().unwrap().to_owned(),
        revision_id: value["revision_id"].as_str().unwrap().to_owned(),
        memory_type: memory_type(value["type"].as_str().unwrap()),
        state: state(value["state"].as_str().unwrap()),
        content_digest: value["content_digest"].as_str().unwrap().to_owned(),
        content_bytes: number(value, "content_bytes"),
        use_count: number(value, "use_count"),
        last_verified_position: number(value, "last_verified_position"),
        retention_score_basis_points: number(value, "retention_score").try_into().unwrap(),
    }
}

fn contradiction(value: &Value) -> MemoryContradiction {
    MemoryContradiction {
        left: (
            value["left_record_id"].as_str().unwrap().to_owned(),
            value["left_revision_id"].as_str().unwrap().to_owned(),
        ),
        right: (
            value["right_record_id"].as_str().unwrap().to_owned(),
            value["right_revision_id"].as_str().unwrap().to_owned(),
        ),
    }
}

fn fact_reference(value: &Value) -> DurableFactReference {
    DurableFactReference::new(
        value["session_id"].as_str().unwrap(),
        number(value, "position"),
        value["fact_id"].as_str().unwrap(),
        value["payload_digest"].as_str().unwrap(),
    )
    .unwrap()
}

fn source(value: &str) -> MemoryCandidateSource {
    match value {
        "explicit_user_command" => MemoryCandidateSource::ExplicitUserCommand,
        "session_end" => MemoryCandidateSource::SessionEnd,
        "exit_summary" => MemoryCandidateSource::ExitSummary,
        "scheduled_distillation" => MemoryCandidateSource::ScheduledDistillation,
        other => panic!("unknown source: {other}"),
    }
}

fn authority(value: &str) -> MemoryAuthority {
    match value {
        "user_declared" => MemoryAuthority::UserDeclared,
        "agent_learned" => MemoryAuthority::AgentLearned,
        "organisation_published" => MemoryAuthority::OrganisationPublished,
        other => panic!("unknown authority: {other}"),
    }
}

fn memory_type(value: &str) -> MemoryType {
    match value {
        "semantic" => MemoryType::Semantic,
        "episodic" => MemoryType::Episodic,
        "lesson" => MemoryType::Lesson,
        "procedural" => MemoryType::Procedural,
        other => panic!("unknown memory type: {other}"),
    }
}

fn state(value: &str) -> HypothesisState {
    match value {
        "candidate" => HypothesisState::Candidate,
        "active" => HypothesisState::Active,
        "cold" => HypothesisState::Cold,
        "archived" => HypothesisState::Archived,
        "promoted" => HypothesisState::Promoted,
        other => panic!("unknown state: {other}"),
    }
}

fn action_name(action: &MemoryAuditAction) -> String {
    match action {
        MemoryAuditAction::Cool { identity } => format!("cool:{}", identity_name(identity)),
        MemoryAuditAction::Archive { identity } => format!("archive:{}", identity_name(identity)),
    }
}

fn identity_name(identity: &(String, String)) -> String {
    format!("{}:{}", identity.0, identity.1)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect()
}

fn strings_2d(value: &Value) -> Vec<Vec<String>> {
    value.as_array().unwrap().iter().map(strings).collect()
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_str().unwrap().parse().unwrap()
}
