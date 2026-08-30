use std::collections::BTreeSet;

use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload,
    CanonicalPayloadError, CommitDisposition, ExecutionId, FactDraft, FactId, FactKind,
    LedgerError, LedgerState, ModelRequestId, SessionId, ToolInvocationId, TurnId,
};

mod common;

fn fact(id: &str, kind: &str) -> FactDraft {
    let lifecycle = kind == "tool.preparation_rejected"
        || kind.starts_with("turn.")
        || kind.starts_with("execution.")
        || kind.starts_with("model.")
        || kind.starts_with("effect.")
        || kind.starts_with("knowledge.");
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: lifecycle.then(|| TurnId::try_from("turn").unwrap()),
        execution_id: (kind == "tool.preparation_rejected"
            || kind.starts_with("execution.")
            || kind.starts_with("model.")
            || kind.starts_with("effect.")
            || kind.starts_with("knowledge."))
        .then(|| ExecutionId::try_from("execution").unwrap()),
        model_request_id: (kind == "tool.preparation_rejected" || kind.starts_with("model."))
            .then(|| ModelRequestId::try_from("request").unwrap()),
        tool_invocation_id: kind
            .starts_with("effect.")
            .then(|| ToolInvocationId::try_from("tool").unwrap()),
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&common::runtime_payload(kind)).unwrap(),
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
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "execution.iteration_started",
        "execution.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "execution.iteration_started",
        "execution.iteration_started",
    ]);
    for terminal in ["turn.completed", "turn.stopped", "turn.failed"] {
        let execution_terminal = terminal.replacen("turn.", "execution.", 1);
        assert_valid(&[
            "session.opened",
            "turn.started",
            "execution.started",
            &execution_terminal,
            terminal,
        ]);
        assert_transition_error(&[
            "session.opened",
            "turn.started",
            "execution.started",
            &execution_terminal,
            terminal,
            terminal,
        ]);
    }
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "execution.completed",
        "turn.completed",
        "session.closed",
    ]);
    for terminal in [
        "execution.abandoned",
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
fn c6_control_facts_require_exact_lifecycle_owners() {
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "execution.abandoned",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "model.prepared",
        "model.started",
        "tool.preparation_rejected",
    ]);
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.denied",
        "effect.observation",
        "execution.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.observation",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "turn.completed",
        "turn.cancel_requested",
    ]);
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
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.authorized",
        "effect.failed",
        "execution.completed",
    ]);
    for prefix in [
        vec!["effect.prepared", "effect.started"],
        vec!["effect.prepared", "effect.authorized", "effect.started"],
    ] {
        for terminal in ["effect.failed", "effect.uncertain"] {
            let mut kinds = vec!["session.opened", "turn.started", "execution.started"];
            kinds.extend(prefix.iter().copied());
            kinds.extend([terminal, "execution.completed"]);
            assert_valid(&kinds);
        }
    }
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.started",
        "effect.receipt",
        "effect.completed",
        "execution.completed",
    ]);
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
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.started",
        "effect.uncertain",
        "execution.suspended",
        "turn.suspended",
        "effect.reconciled",
        "effect.observation",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "effect.prepared",
        "effect.started",
        "effect.uncertain",
        "effect.observation",
    ]);
}

#[test]
fn every_knowledge_terminal_requires_requested_and_exact_dispatch_state() {
    assert_valid(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "knowledge.requested",
        "knowledge.dispatched",
        "knowledge.completed",
        "execution.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "knowledge.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "knowledge.requested",
        "knowledge.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "knowledge.requested",
        "knowledge.dispatched",
        "knowledge.completed",
        "knowledge.completed",
    ]);
    assert_transition_error(&[
        "session.opened",
        "turn.started",
        "execution.started",
        "knowledge.requested",
        "execution.completed",
    ]);
}

#[test]
fn scheduler_claims_ranges_terminals_and_updates_are_exact() {
    assert_valid(&[
        "session.opened",
        "schedule.created",
        "schedule.claimed",
        "schedule.fired",
        "schedule.skipped",
        "schedule.cancelled",
    ]);
    let mut exhausted = fact("exhausted", "schedule.exhausted");
    exhausted.payload = CanonicalPayload::from_value(&serde_json::json!({
        "schedule_id":"schedule-1","revision_id":"revision-1","last_handled_ordinal":4
    }))
    .unwrap();
    let mut completed = LedgerState::default();
    assert!(completed
        .commit(
            SessionId::try_from("exhausted-session").unwrap(),
            0,
            vec![
                fact("exhausted-open", "session.opened"),
                fact("exhausted-create", "schedule.created"),
                fact("exhausted-claim", "schedule.claimed"),
                fact("exhausted-fired", "schedule.fired"),
                fact("exhausted-skipped", "schedule.skipped"),
                exhausted,
                fact("exhausted-close", "session.closed"),
            ],
        )
        .is_ok());
    assert_transition_error(&["session.opened", "schedule.claimed"]);
    assert_transition_error(&["session.opened", "schedule.created", "schedule.fired"]);
    assert_transition_error(&[
        "session.opened",
        "schedule.created",
        "schedule.claimed",
        "schedule.cancelled",
    ]);
    assert_transition_error(&[
        "session.opened",
        "schedule.created",
        "schedule.claimed",
        "schedule.fired",
        "schedule.fired",
    ]);
    let mut fail_due = fact("fail-due", "schedule.failed");
    fail_due.payload = CanonicalPayload::from_value(&serde_json::json!({
        "schedule_id":"schedule-1","revision_id":"revision-1",
        "occurrence_id":"occurrence-1","ordinal":1,"reason":"misfire_limit_exceeded"
    }))
    .unwrap();
    let mut failed = LedgerState::default();
    assert!(failed
        .commit(
            SessionId::try_from("failed-session").unwrap(),
            0,
            vec![
                fact("failed-open", "session.opened"),
                fact("failed-create", "schedule.created"),
                fail_due,
            ],
        )
        .is_ok());

    let mut superseded = fact("supersede", "schedule.cancelled");
    superseded.payload = CanonicalPayload::from_value(&serde_json::json!({
        "command_id":"supersede","schedule_id":"schedule-1",
        "expected_revision_id":"revision-1","reason":"superseded"
    }))
    .unwrap();
    let mut next = fact("create-next", "schedule.created");
    let mut payload = common::runtime_payload("schedule.created");
    payload["revision_id"] = serde_json::json!("revision-2");
    next.payload = CanonicalPayload::from_value(&payload).unwrap();
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = LedgerState::default();
    assert_eq!(
        ledger
            .commit(
                session.clone(),
                0,
                vec![
                    fact("open", "session.opened"),
                    fact("create", "schedule.created"),
                    superseded.clone(),
                    next
                ],
            )
            .unwrap()
            .disposition,
        CommitDisposition::Committed,
    );
    let mut invalid = LedgerState::default();
    assert_eq!(
        invalid.commit(
            session,
            0,
            vec![
                fact("open", "session.opened"),
                fact("create", "schedule.created"),
                superseded
            ],
        ),
        Err(LedgerError::InvalidTransition),
    );
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

#[test]
fn prepared_v3_requires_the_exact_durable_f0_chain_before_start() {
    let prefix = [
        fact("open-f0", "session.opened"),
        fact("turn-f0", "turn.started"),
        fact("execution-f0", "execution.started"),
    ];
    let chain = [
        f0_fact("prepared-f0", "effect.prepared"),
        f0_fact("safety-f0", "safety.decided"),
        f0_fact("authorized-f0", "effect.authorized"),
        f0_fact("bound-f0", "sandbox.bound"),
        f0_fact("preflight-f0", "sandbox.preflighted"),
        f0_fact("started-f0", "effect.started"),
        f0_fact("failed-f0", "effect.failed"),
    ];
    let mut ledger = LedgerState::default();
    assert!(ledger
        .commit(
            SessionId::try_from("f0-valid").unwrap(),
            0,
            prefix.clone().into_iter().chain(chain).collect(),
        )
        .is_ok());

    for omitted in ["safety.decided", "sandbox.bound", "sandbox.preflighted"] {
        let mut ledger = LedgerState::default();
        let facts = prefix
            .clone()
            .into_iter()
            .chain(
                [
                    "effect.prepared",
                    "safety.decided",
                    "effect.authorized",
                    "sandbox.bound",
                    "sandbox.preflighted",
                    "effect.started",
                ]
                .into_iter()
                .filter(|kind| *kind != omitted)
                .enumerate()
                .map(|(index, kind)| f0_fact(&format!("{omitted}-{index}"), kind)),
            )
            .collect();
        assert_eq!(
            ledger.commit(SessionId::try_from(omitted).unwrap(), 0, facts),
            Err(LedgerError::InvalidTransition),
            "{omitted}"
        );
    }

    let mut mixed = f0_fact("mixed-preflight", "sandbox.preflighted");
    let mut payload = serde_json::from_str::<serde_json::Value>(mixed.payload.as_json()).unwrap();
    payload["decision_id"] = serde_json::json!("different-decision");
    mixed.payload = CanonicalPayload::from_value(&payload).unwrap();
    let mut ledger = LedgerState::default();
    assert_eq!(
        ledger.commit(
            SessionId::try_from("f0-mixed").unwrap(),
            0,
            prefix
                .into_iter()
                .chain([
                    f0_fact("mixed-prepared", "effect.prepared"),
                    f0_fact("mixed-safety", "safety.decided"),
                    f0_fact("mixed-authorized", "effect.authorized"),
                    f0_fact("mixed-bound", "sandbox.bound"),
                    mixed,
                ])
                .collect(),
        ),
        Err(LedgerError::InvalidTransition)
    );
}

fn f0_fact(id: &str, kind: &str) -> FactDraft {
    let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    let mut value = fact(id, "effect.prepared");
    value.kind = FactKind::new(kind).unwrap();
    value.schema_version = match kind {
        "effect.prepared" => 3,
        "effect.authorized" => 2,
        _ => 1,
    };
    value.payload = CanonicalPayload::from_value(&match kind {
        "effect.prepared" => serde_json::json!({
            "prepared_contract_version":3,"prepared_digest":empty,"tool_name":"tool",
            "tool_revision":"revision-v3","replay_class":"read_only","model_call_id":"call",
            "access_policy_revision":"access-v1","access_resolver_revision":"resolver-v1",
            "invocation_accesses":{"digest":empty,"inline_utf8":""},"max_result_bytes":512,
            "sandbox_requirements":{"digest":empty,"inline_utf8":""},
            "sandbox_requirements_digest":empty
        }),
        "safety.decided" => serde_json::json!({
            "request_id":"request","decision_id":"decision","disposition":"allow",
            "prepared_digest":empty,"tool_name":"tool","tool_revision":"revision-v3",
            "actor_authority_reference":"actor","exact_access_digest":empty,
            "sandbox_requirements_digest":empty,"policy_revision":"policy-v1",
            "constraints_digest":empty
        }),
        "effect.authorized" => serde_json::json!({
            "prepared_contract_version":3,"prepared_digest":empty,"grant_id":"grant",
            "authority_revision":"policy-v1","constraints_digest":empty,
            "granted_requirements":{"digest":empty,"inline_utf8":""}
        }),
        "sandbox.bound" => serde_json::json!({
            "binding_id":"binding","decision_id":"decision","prepared_digest":empty,
            "workspace_capability_id":"workspace","executor_id":"executor",
            "executor_revision":"executor-v1","policy_revision":"policy-v1",
            "access_scope_digest":empty,"enforcement_digest":empty,"effective_limits_digest":empty
        }),
        "sandbox.preflighted" => serde_json::json!({
            "preflight_id":"preflight","binding_id":"binding","decision_id":"decision",
            "prepared_digest":empty,"grant_id":"grant","executor_id":"executor",
            "executor_revision":"executor-v1","dispatch_attempt_id":"attempt"
        }),
        "effect.started" => serde_json::json!({
            "prepared_digest":empty,"grant_id":"grant","executor_id":"executor",
            "executor_revision":"executor-v1","dispatch_attempt_id":"attempt"
        }),
        "effect.failed" => serde_json::json!({
            "prepared_digest":empty,"code":"tool_failure"
        }),
        _ => unreachable!(),
    })
    .unwrap();
    value
}
