use garive_knowledge::{
    Citation, CitationScheme, ContentBinding, FreshnessRequirement, KnowledgeEvidence,
    KnowledgeFreshness, KnowledgeQueryMode, KnowledgeRequest, KnowledgeSourceDescriptor,
    KnowledgeSourceKind, KnowledgeTrustClass,
};
use garive_ledger::{
    AgentDefinitionId, AgentDefinitionRevision, AgentInstanceId, CanonicalPayload, FactDraft,
    FactId, FactKind, SessionId,
};
use garive_runtime::{
    derive_knowledge_recovery, plan_knowledge_completed, plan_knowledge_dispatched,
    plan_knowledge_requested, plan_start_turn, EffectiveRuntimeLimits, KnowledgeLifecycleContext,
    KnowledgeRecoveryAction, KnowledgeRecoveryContext, RuntimeCommandId, SqliteLedger,
    StartTurnCommand,
};
use tempfile::tempdir;

#[test]
fn sqlite_restart_distinguishes_requested_dispatched_and_terminal_positions() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("knowledge-recovery.sqlite3");
    let session = SessionId::try_from("knowledge-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = plan_start_turn(
        &StartTurnCommand {
            command_id: RuntimeCommandId::new("knowledge-start").unwrap(),
            session_id: session.clone(),
            agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
            definition_id: AgentDefinitionId::try_from("definition").unwrap(),
            definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
            snapshot_digest: "a".repeat(64),
            trusted_input: "hello".into(),
            limits: EffectiveRuntimeLimits {
                max_iterations: 1,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        1,
    )
    .unwrap();
    let execution = start.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, start.facts).unwrap();
    let lifecycle = KnowledgeLifecycleContext {
        turn_id: start.turn_id.clone(),
        execution_id: execution.clone(),
        recorded_at: "2026-08-29T00:00:01Z".into(),
    };
    let request = request();
    let source = source();
    let prepared = plan_knowledge_requested(&lifecycle, &request).unwrap();
    let request_digest = prepared.request_digest.clone();
    ledger
        .commit(session.clone(), 2, vec![prepared.fact.clone()])
        .unwrap();
    drop(ledger);

    let mut restarted = SqliteLedger::open(&path).unwrap();
    let mut recovery = KnowledgeRecoveryContext {
        session_id: session.clone(),
        turn_id: start.turn_id.clone(),
        execution_id: execution,
        through_position: 5,
        request_id: "knowledge-request".into(),
    };
    assert_eq!(
        derive_knowledge_recovery(&restarted, &recovery).unwrap(),
        KnowledgeRecoveryAction::RedispatchSameRequest {
            request_digest: request_digest.clone(),
        }
    );

    let dispatched = plan_knowledge_dispatched(&lifecycle, &prepared, "attempt-1").unwrap();
    restarted
        .commit(session.clone(), 3, vec![dispatched])
        .unwrap();
    drop(restarted);
    let mut restarted = SqliteLedger::open(&path).unwrap();
    recovery.through_position = 6;
    assert_eq!(
        derive_knowledge_recovery(&restarted, &recovery).unwrap(),
        KnowledgeRecoveryAction::ClassifyUncertain {
            request_digest: request_digest.clone(),
            dispatch_attempt_id: "attempt-1".into(),
        }
    );

    let completed = plan_knowledge_completed(
        &lifecycle,
        &prepared,
        &source,
        &request,
        vec![evidence()],
        true,
    )
    .unwrap();
    restarted.commit(session, 4, vec![completed.fact]).unwrap();
    drop(restarted);
    let restarted = SqliteLedger::open(&path).unwrap();
    recovery.through_position = 7;
    assert_eq!(
        derive_knowledge_recovery(&restarted, &recovery).unwrap(),
        KnowledgeRecoveryAction::ReturnTerminal {
            request_digest,
            terminal_position: 7,
            completed: true,
        }
    );
}

fn request() -> KnowledgeRequest {
    KnowledgeRequest::new(
        "knowledge-request",
        "docs",
        "1",
        KnowledgeQueryMode::Keyword,
        ContentBinding::from_inline("garive"),
        vec![],
        4,
        1,
        64,
        1_000,
        FreshnessRequirement::CachedAllowed,
    )
    .unwrap()
}

fn source() -> KnowledgeSourceDescriptor {
    KnowledgeSourceDescriptor::new(
        "docs",
        "1",
        KnowledgeSourceKind::Documentation,
        "product-docs",
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword],
        "b".repeat(64),
        CitationScheme::UriFragment,
        "c".repeat(64),
    )
    .unwrap()
}

fn evidence() -> KnowledgeEvidence {
    let content = ContentBinding::from_inline("knowledge");
    KnowledgeEvidence::new(
        "evidence",
        "docs",
        "1",
        None,
        content.clone(),
        9,
        Citation::new(
            CitationScheme::UriFragment,
            "intro",
            None,
            None,
            content.digest(),
        )
        .unwrap(),
        "2026-08-29T00:00:01Z",
        KnowledgeFreshness::Fresh,
        KnowledgeTrustClass::Curated,
        9000,
    )
    .unwrap()
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
        payload: CanonicalPayload::from_value(&serde_json::json!({})).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}
