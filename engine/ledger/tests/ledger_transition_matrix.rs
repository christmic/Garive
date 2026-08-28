use std::collections::BTreeSet;

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload,
    CanonicalPayloadError, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind,
    LedgerError, LedgerState, ModelRequestId, SessionId, ToolInvocationId, TurnId,
};

fn fact(id: &str, kind: &str) -> FactDraft {
    let lifecycle = kind.starts_with("turn.")
        || kind.starts_with("execution.")
        || kind.starts_with("model.")
        || kind.starts_with("effect.");
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: lifecycle.then(|| TurnId::try_from("turn").unwrap()),
        execution_id: (kind.starts_with("execution.")
            || kind.starts_with("model.")
            || kind.starts_with("effect."))
        .then(|| ExecutionId::try_from("execution").unwrap()),
        model_request_id: kind
            .starts_with("model.")
            .then(|| ModelRequestId::try_from("request").unwrap()),
        tool_invocation_id: kind
            .starts_with("effect.")
            .then(|| ToolInvocationId::try_from("tool").unwrap()),
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&serde_json::json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn commit_kinds(kinds: &[&str]) -> Result<garive_ledger::CommitResult, LedgerError> {
    let mut ledger = LedgerState::default();
    let drafts = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| fact(&format!("fact-{index}"), kind))
        .collect();
    ledger.commit(SessionId::try_from("session").unwrap(), 0, drafts)
}

fn assert_valid(kinds: &[&str]) {
    assert_eq!(
        commit_kinds(kinds).unwrap().disposition,
        CommitDisposition::Committed
    );
}

fn assert_transition_error(kinds: &[&str]) {
    assert_eq!(commit_kinds(kinds), Err(LedgerError::InvalidTransition));
}

#[test]
fn every_turn_and_execution_terminal_is_admitted_once() {
    for terminal in ["turn.completed", "turn.stopped", "turn.failed"] {
        assert_valid(&["session.opened", "turn.started", terminal]);
        assert_transition_error(&["session.opened", "turn.started", terminal, terminal]);
    }
    assert_valid(&[
        "session.opened",
        "turn.started",
        "turn.suspended",
        "turn.started",
        "turn.completed",
        "session.closed",
    ]);
    for terminal in [
        "execution.completed",
        "execution.suspended",
        "execution.stopped",
        "execution.failed",
    ] {
        assert_valid(&[
            "session.opened",
            "turn.started",
            "execution.started",
            terminal,
        ]);
        assert_transition_error(&[
            "session.opened",
            "turn.started",
            "execution.started",
            terminal,
            terminal,
        ]);
    }
}

#[test]
fn every_model_terminal_requires_prepared_started_and_same_execution() {
    for terminal in [
        "model.completed",
        "model.rejected",
        "model.interrupted",
        "model.unavailable",
        "model.uncertain",
    ] {
        assert_valid(&[
            "session.opened",
            "turn.started",
            "execution.started",
            "model.prepared",
            "model.started",
            terminal,
            "execution.completed",
        ]);
        assert_transition_error(&[
            "session.opened",
            "turn.started",
            "execution.started",
            "model.prepared",
            terminal,
        ]);
    }

    let session = SessionId::try_from("session").unwrap();
    let mut ledger = LedgerState::default();
    let mut second_execution = fact("e2", "execution.started");
    second_execution.execution_id = Some(ExecutionId::try_from("execution-2").unwrap());
    let mut wrong_owner = fact("wrong-owner", "model.started");
    wrong_owner.execution_id = second_execution.execution_id.clone();
    assert_eq!(
        ledger.commit(
            session,
            0,
            vec![
                fact("open", "session.opened"),
                fact("turn", "turn.started"),
                fact("e1", "execution.started"),
                second_execution,
                fact("prepared", "model.prepared"),
                wrong_owner,
            ],
        ),
        Err(LedgerError::InvalidTransition)
    );
}

#[test]
fn every_effect_terminal_and_receipt_path_is_explicit() {
    for prefix in [
        vec!["effect.prepared", "effect.started"],
        vec!["effect.prepared", "effect.authorized", "effect.started"],
    ] {
        for terminal in ["effect.completed", "effect.failed", "effect.uncertain"] {
            let mut kinds = vec!["session.opened", "turn.started", "execution.started"];
            kinds.extend(prefix.iter().copied());
            kinds.extend([terminal, "execution.completed"]);
            assert_valid(&kinds);
        }
    }
    for terminal in ["effect.completed", "effect.failed"] {
        assert_valid(&[
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.started",
            "effect.receipt",
            terminal,
            "execution.completed",
        ]);
    }
    for prefix in [
        &["effect.prepared", "effect.denied"][..],
        &["effect.prepared", "effect.authorized", "effect.denied"][..],
    ] {
        let mut kinds = vec!["session.opened", "turn.started", "execution.started"];
        kinds.extend(prefix);
        assert_valid(&kinds);
    }
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.completed",
    ]);
}

#[test]
fn parents_cannot_close_before_active_or_recovery_pending_children() {
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "turn.completed",
    ]);
    for pending in ["model.started", "effect.started", "effect.receipt"] {
        let prepared = if pending.starts_with("model") {
            "model.prepared"
        } else {
            "effect.prepared"
        };
        let mut kinds = vec![
            "session.opened",
            "turn.started",
            "execution.started",
            prepared,
        ];
        if pending == "effect.receipt" {
            kinds.push("effect.started");
        }
        kinds.extend([pending, "execution.completed"]);
        assert_transition_error(&kinds);
    }
    assert_transition_error(&["session.opened", "turn.started", "session.closed"]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "session.closed",
    ]);
}

#[test]
fn commit_validation_and_idempotency_fail_closed_without_partial_state() {
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = LedgerState::default();
    assert_eq!(
        ledger.commit(session.clone(), 0, vec![]),
        Err(LedgerError::EmptyBatch)
    );
    let duplicate = fact("duplicate", "session.opened");
    assert_eq!(
        ledger.commit(session.clone(), 0, vec![duplicate.clone(), duplicate]),
        Err(LedgerError::InvalidFact)
    );
    assert_eq!(ledger.fact_count(&session), 0);

    ledger
        .commit(session.clone(), 0, vec![fact("open", "session.opened")])
        .unwrap();
    assert_eq!(
        ledger.commit(
            session.clone(),
            1,
            vec![
                fact("open", "session.opened"),
                fact("new", "privacy.redacted")
            ],
        ),
        Err(LedgerError::IncompleteReplay)
    );
    assert_eq!(ledger.session_version(&session), Some(1));
    assert_eq!(ledger.fact_count(&session), 1);
}

#[test]
fn lifecycle_identities_cannot_be_reowned_by_another_session() {
    for shared_kind in [
        "turn.started",
        "execution.started",
        "model.prepared",
        "effect.prepared",
    ] {
        let first = SessionId::try_from("first").unwrap();
        let second = SessionId::try_from("second").unwrap();
        let mut ledger = LedgerState::default();
        let prefix: Vec<_> = ["session.opened", "turn.started", "execution.started"]
            .into_iter()
            .take_while(|kind| *kind != shared_kind)
            .chain([shared_kind])
            .enumerate()
            .map(|(index, kind)| fact(&format!("first-{index}"), kind))
            .collect();
        ledger.commit(first, 0, prefix).unwrap();

        let mut second_batch = vec![fact("second-open", "session.opened")];
        second_batch.push(fact("reowned", shared_kind));
        assert_eq!(
            ledger.commit(second.clone(), 0, second_batch),
            Err(LedgerError::InvalidTransition),
            "{shared_kind}"
        );
        assert_eq!(ledger.fact_count(&second), 0, "{shared_kind}");
    }
}

#[test]
fn read_and_recovery_queries_cover_ranges_filters_and_missing_references() {
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = LedgerState::default();
    ledger
        .commit(
            session.clone(),
            0,
            vec![
                fact("open", "session.opened"),
                fact("turn", "turn.started"),
                fact("execution", "execution.started"),
                fact("model", "model.prepared"),
                fact("effect", "effect.prepared"),
            ],
        )
        .unwrap();
    assert_eq!(
        ledger.read_facts(&session, 0, 0, None),
        Err(LedgerError::InvalidReadRange)
    );
    assert_eq!(
        ledger.read_facts(&session, 3, 3, None),
        Err(LedgerError::InvalidReadRange)
    );
    let missing = SessionId::try_from("missing").unwrap();
    assert_eq!(
        ledger.read_facts(&missing, 0, 1, None),
        Err(LedgerError::MissingReference)
    );

    let filter = BTreeSet::from([FactKind::new("turn.started").unwrap()]);
    let facts = ledger.read_facts(&session, 0, 5, Some(&filter)).unwrap();
    assert_eq!(
        facts.iter().map(|value| value.position).collect::<Vec<_>>(),
        vec![2]
    );
    let snapshot = ledger
        .load_turn(&TurnId::try_from("turn").unwrap())
        .unwrap();
    assert_eq!(
        (snapshot.session_version, snapshot.through_position),
        (1, 5)
    );
    assert_eq!(snapshot.facts.len(), 4);
    assert_eq!(
        ledger
            .find_model_request(&ModelRequestId::try_from("request").unwrap())
            .len(),
        1
    );
    assert_eq!(
        ledger
            .find_tool_invocation(&ToolInvocationId::try_from("tool").unwrap())
            .len(),
        1
    );
    assert_eq!(
        ledger.load_turn(&TurnId::try_from("missing").unwrap()),
        Err(LedgerError::MissingReference)
    );
    assert_eq!(
        ledger.list_uncertain_model_requests(&missing),
        Err(LedgerError::MissingReference)
    );
    assert_eq!(
        ledger.list_uncertain_tool_invocations(&missing),
        Err(LedgerError::MissingReference)
    );
}

#[test]
fn envelope_identity_and_canonical_payload_boundaries_are_typed() {
    assert!(SessionId::try_from("").is_err());
    assert!(TurnId::try_from("").is_err());
    assert!(ExecutionId::try_from("").is_err());
    assert!(FactId::try_from("").is_err());
    assert!(ModelRequestId::try_from("").is_err());
    assert!(ToolInvocationId::try_from("").is_err());
    assert!(AgentInstanceId::try_from("").is_err());
    assert!(AgentDefinitionId::try_from("").is_err());
    assert!(AgentDefinitionRevision::try_from("").is_err());
    assert_eq!(FactKind::new(""), Err(LedgerError::InvalidFact));

    assert_eq!(
        CanonicalPayload::from_canonical_parts("{".into(), "00".into()),
        Err(CanonicalPayloadError::InvalidJson)
    );
    assert_eq!(
        CanonicalPayload::from_canonical_parts("{\"b\":1,\"a\":2}".into(), "00".into()),
        Err(CanonicalPayloadError::NonCanonical)
    );
    assert_eq!(
        CanonicalPayload::from_canonical_parts("{}".into(), "00".into()),
        Err(CanonicalPayloadError::DigestMismatch)
    );
    assert_eq!(
        CanonicalPayload::from_value(&serde_json::json!(1.5)),
        Err(CanonicalPayloadError::UnsupportedNumber)
    );

    let mut invalid = fact("invalid", "session.opened");
    invalid.schema_version = 0;
    assert_eq!(invalid.validate(), Err(LedgerError::InvalidFact));
    invalid.schema_version = 1;
    invalid.recorded_at = "not-a-time".into();
    assert_eq!(invalid.validate(), Err(LedgerError::InvalidFact));
}
