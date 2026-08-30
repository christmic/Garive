use std::path::PathBuf;

use garive_ledger::{
    validate_runtime_fact, CanonicalPayload, CommitDisposition, ExecutionId, FactDraft, FactId,
    FactKind, LedgerError, RuntimeFactDisposition, SessionId, TurnId,
};
use garive_memory::{
    ContentBinding, DurableFactReference, HypothesisState, MemoryAuthority, MemoryAuthorityBinding,
    MemoryAuthorizedScope, MemoryCommit, MemoryDocumentLimits, MemoryErrorCode, MemoryKind,
    MemoryProposal, MemoryPurpose, MemoryQuery, MemoryRevisionClassification, MemoryRevisionScope,
    MemoryScope, MemoryScopeClass, MemorySensitivity, MemoryState, MemoryStatus, MemoryTombstone,
    MemoryType, MemoryTypeDescriptor, MemoryTypeRegistry,
};
use garive_runtime::{
    authorize_memory_query, authorize_memory_write, plan_classified_memory_write,
    plan_memory_tombstone, plan_memory_write, reconstruct_memory_repository_projection,
    reconstruct_memory_state, verify_memory_evidence, MemoryAccessGrant, MemoryControlAction,
    MemoryControlGrant, MemoryControlRuntimeError, MemoryPrefix, MemoryRepositoryCommitError,
    MemoryRepositoryError, MemoryTombstoneContext, MemoryTombstoneReason, MemoryWriteContext,
    MemoryWriteDecision, MemoryWriteRejection, RuntimeCommandError, SqliteLedger,
    SqliteLedgerError,
};
use serde_json::{json, Value};
use tempfile::tempdir;

fn runtime_payload(kind: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/ledger/runtime-facts-v1.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    fixture["valid_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["kind"].as_str() == Some(kind))
        .map(|value| value["payload"].clone())
        .unwrap_or_else(|| json!({}))
}

fn draft(id: &str, kind: &str, turn: Option<&str>, execution: Option<&str>) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: turn.map(|value| TurnId::try_from(value).unwrap()),
        execution_id: execution.map(|value| ExecutionId::try_from(value).unwrap()),
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&runtime_payload(kind)).unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn initial_facts() -> Vec<FactDraft> {
    vec![
        draft("evidence", "session.opened", None, None),
        draft("turn-fact", "turn.started", Some("turn"), None),
        draft(
            "execution-fact",
            "execution.started",
            Some("turn"),
            Some("execution"),
        ),
    ]
}

fn context(through_position: u64) -> MemoryWriteContext {
    MemoryWriteContext {
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from("execution").unwrap(),
        through_position,
        recorded_at: "2026-08-29T00:00:01Z".into(),
    }
}

fn proposal(expected: Option<&str>, content: &str, payload_digest: &str) -> MemoryProposal {
    MemoryProposal::new(
        if expected.is_some() {
            "proposal-2"
        } else {
            "proposal-1"
        },
        "namespace",
        MemoryScope::session("session").unwrap(),
        MemoryKind::Preference,
        ContentBinding::from_inline(content),
        vec![DurableFactReference::new("session", 1, "evidence", payload_digest).unwrap()],
        MemorySensitivity::Ordinary,
        9_000,
        expected.map(str::to_owned),
    )
    .unwrap()
}

fn commit(record: &str, revision: &str, position: u64, prior: Option<&str>) -> MemoryCommit {
    MemoryCommit::new(
        record,
        revision,
        "a".repeat(64),
        position,
        None,
        prior.map(str::to_owned),
    )
    .unwrap()
}

fn classification_registry() -> MemoryTypeRegistry {
    let descriptor = |memory_type, roles, authorities, name: &str| {
        MemoryTypeDescriptor::new(
            memory_type,
            roles,
            authorities,
            format!("{name}-v1"),
            format!("{name}-v1"),
            format!("{name}-v1"),
            format!("memory.{name}"),
        )
        .unwrap()
    };
    MemoryTypeRegistry::new(
        "registry-v1",
        vec![
            descriptor(
                MemoryType::Semantic,
                vec![
                    MemoryKind::Preference,
                    MemoryKind::Constraint,
                    MemoryKind::Decision,
                    MemoryKind::LearnedFact,
                ],
                vec![
                    MemoryAuthority::UserDeclared,
                    MemoryAuthority::AgentLearned,
                    MemoryAuthority::OrganisationPublished,
                ],
                "semantic",
            ),
            descriptor(
                MemoryType::Episodic,
                vec![MemoryKind::Summary],
                vec![MemoryAuthority::AgentLearned],
                "episodic",
            ),
            descriptor(
                MemoryType::Lesson,
                vec![MemoryKind::LearnedFact],
                vec![
                    MemoryAuthority::AgentLearned,
                    MemoryAuthority::OrganisationPublished,
                ],
                "lesson",
            ),
            descriptor(
                MemoryType::Procedural,
                vec![MemoryKind::LearnedFact, MemoryKind::Summary],
                vec![
                    MemoryAuthority::AgentLearned,
                    MemoryAuthority::OrganisationPublished,
                ],
                "procedural",
            ),
        ],
    )
    .unwrap()
}

#[test]
fn classified_write_commits_source_and_projection_metadata_as_one_fact_batch() {
    let directory = tempdir().unwrap();
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(directory.path().join("classified.sqlite3")).unwrap();
    ledger.commit(session.clone(), 0, initial_facts()).unwrap();
    let evidence_digest = ledger.read_facts(&session, 0, 1, None).unwrap()[0]
        .payload
        .sha256()
        .to_owned();
    let initial_proposal = proposal(None, "dark mode", &evidence_digest);
    let classification = MemoryRevisionClassification::new(
        MemoryKind::Preference,
        MemoryAuthorityBinding::new(MemoryAuthority::AgentLearned, None).unwrap(),
        MemoryRevisionScope::new(MemoryScopeClass::Session, "session", None).unwrap(),
        HypothesisState::Candidate,
        "classification-v1",
        &classification_registry(),
    )
    .unwrap();
    let planned = plan_classified_memory_write(
        &context(3),
        &MemoryState::default(),
        &initial_proposal,
        commit("record", "revision-1", 5, None),
        &session,
        "classification",
        &classification,
    )
    .unwrap();
    assert_eq!(
        planned
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "memory.proposed",
            "memory.committed",
            "memory.revision_classified"
        ]
    );
    assert_eq!(
        validate_runtime_fact(&planned.facts[2]),
        Ok(RuntimeFactDisposition::AppliedV1)
    );
    let classification_payload: Value =
        serde_json::from_str(planned.facts[2].payload.as_json()).unwrap();
    assert_eq!(classification_payload["source_commit"]["position"], 5);
    assert_eq!(classification_payload["scope_owner_id"], "session");
    ledger
        .connection_for_test()
        .execute_batch(
            "CREATE TRIGGER fail_memory_source BEFORE INSERT ON memory_control_sources \
             BEGIN SELECT RAISE(ABORT, 'injected'); END;",
        )
        .unwrap();
    assert!(matches!(
        ledger.commit_classified_memory_write(
            session.clone(),
            1,
            planned,
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        ),
        Err(MemoryRepositoryCommitError::Repository(
            MemoryRepositoryError::Unavailable
        )),
    ));
    assert_eq!(
        ledger
            .session_watermark(&session)
            .unwrap()
            .unwrap()
            .max_position,
        3
    );
    ledger
        .connection_for_test()
        .execute_batch("DROP TRIGGER fail_memory_source;")
        .unwrap();

    let planned = plan_classified_memory_write(
        &context(3),
        &MemoryState::default(),
        &initial_proposal,
        commit("record", "revision-1", 5, None),
        &session,
        "classification",
        &classification,
    )
    .unwrap();
    let result = ledger
        .commit_classified_memory_write(
            session.clone(),
            1,
            planned,
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(result.ledger.positions, vec![4, 5, 6]);
    assert_eq!(
        (
            result.previous_repository_revision,
            result.committed_repository_revision
        ),
        (0, 1)
    );
    let grant = MemoryControlGrant::new(
        "namespace",
        [MemoryControlAction::Export],
        [MemoryAuthorizedScope {
            scope: MemoryScopeClass::Session,
            owner_id: "session".into(),
        }],
    )
    .unwrap();
    let projection = ledger
        .read_memory_control_projection(
            &grant,
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(projection.repository_revision, 1);
    assert_eq!(projection.documents[0].content(), "dark mode\n");
    assert_eq!(
        projection.documents[0].authority(),
        MemoryAuthority::AgentLearned
    );

    let replay = plan_classified_memory_write(
        &context(3),
        &MemoryState::default(),
        &initial_proposal,
        commit("record", "revision-1", 5, None),
        &session,
        "classification",
        &classification,
    )
    .unwrap();
    let replay = ledger
        .commit_classified_memory_write(
            session.clone(),
            1,
            replay,
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(replay.ledger.disposition, CommitDisposition::Replayed);
    assert_eq!(
        (
            replay.previous_repository_revision,
            replay.committed_repository_revision
        ),
        (0, 1)
    );
    drop(ledger);
    let mut ledger = SqliteLedger::open(directory.path().join("classified.sqlite3")).unwrap();
    let facts = ledger.read_facts(&session, 0, 6, None).unwrap();
    assert_eq!(facts[5].kind.as_str(), "memory.revision_classified");
    let recovered = reconstruct_memory_state(
        &ledger,
        &[MemoryPrefix {
            session_id: session.clone(),
            through_position: 6,
        }],
    )
    .unwrap();
    let changed = proposal(Some("revision-1"), "light mode", &evidence_digest);
    let superseding = plan_classified_memory_write(
        &context(6),
        &recovered,
        &changed,
        commit("record", "revision-2", 8, Some("revision-1")),
        &session,
        "classification-2",
        &classification,
    )
    .unwrap();
    let superseding = ledger
        .commit_classified_memory_write(
            session.clone(),
            2,
            superseding,
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(superseding.ledger.positions, vec![7, 8, 9, 10]);
    assert_eq!(superseding.committed_repository_revision, 2);
    let projection = ledger
        .read_memory_control_projection(
            &grant,
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(projection.repository_revision, 2);
    assert_eq!(
        projection.documents[0].record_ref().revision_id(),
        Some("revision-2")
    );
    assert_eq!(projection.documents[0].content(), "light mode\n");
    let rebuilt = reconstruct_memory_repository_projection(
        &ledger,
        &[MemoryPrefix {
            session_id: session.clone(),
            through_position: 10,
        }],
        "namespace",
        MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
    )
    .unwrap();
    assert_eq!(rebuilt, projection);
    assert_eq!(
        reconstruct_memory_repository_projection(
            &ledger,
            &[MemoryPrefix {
                session_id: session.clone(),
                through_position: 8,
            }],
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        ),
        Err(MemoryErrorCode::CorruptMemoryState),
    );

    let mismatched = MemoryRevisionClassification::new(
        MemoryKind::Preference,
        MemoryAuthorityBinding::new(MemoryAuthority::AgentLearned, None).unwrap(),
        MemoryRevisionScope::new(MemoryScopeClass::Project, "project", None).unwrap(),
        HypothesisState::Candidate,
        "classification-v1",
        &classification_registry(),
    )
    .unwrap();
    assert_eq!(
        plan_classified_memory_write(
            &context(3),
            &MemoryState::default(),
            &initial_proposal,
            commit("record", "revision-1", 5, None),
            &session,
            "classification",
            &mismatched
        )
        .err()
        .unwrap(),
        RuntimeCommandError::InvalidCommand,
    );
    let current = reconstruct_memory_state(
        &ledger,
        &[MemoryPrefix {
            session_id: session.clone(),
            through_position: 10,
        }],
    )
    .unwrap();
    let tombstone = plan_memory_tombstone(
        &MemoryTombstoneContext {
            command_id: "forget-projection".into(),
            recorded_at: "2026-08-29T00:00:03Z".into(),
        },
        &current,
        &MemoryTombstone {
            record_id: "record".into(),
            revision_id: "revision-2".into(),
        },
        MemoryTombstoneReason::Policy,
    )
    .unwrap();
    let tombstone = ledger
        .commit_memory_tombstone(session.clone(), 3, tombstone)
        .unwrap();
    assert_eq!(tombstone.ledger.positions, vec![11]);
    assert_eq!(tombstone.committed_repository_revision, 3);
    let projection = ledger
        .read_memory_control_projection(
            &grant,
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap();
    assert_eq!(projection.repository_revision, 3);
    assert!(projection.documents.is_empty());
    assert_eq!(
        reconstruct_memory_repository_projection(
            &ledger,
            &[MemoryPrefix {
                session_id: session.clone(),
                through_position: 11,
            }],
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        )
        .unwrap(),
        projection,
    );
    ledger
        .connection_for_test()
        .execute(
            "UPDATE memory_control_sources SET source_payload_digest=?1 WHERE revision_id='revision-2'",
            ["b".repeat(64)],
        )
        .unwrap();
    assert_eq!(
        ledger.read_memory_control_projection(
            &grant,
            "namespace",
            MemoryDocumentLimits::new(4096, 2048, 128).unwrap(),
        ),
        Err(MemoryControlRuntimeError::PersistenceFailed),
    );
}

#[test]
fn sqlite_write_batches_are_atomic_replayable_and_restart_safe() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("memory.sqlite3");
    let session = SessionId::try_from("session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger.commit(session.clone(), 0, initial_facts()).unwrap();
    let evidence_digest = ledger.read_facts(&session, 0, 1, None).unwrap()[0]
        .payload
        .sha256()
        .to_owned();
    let first_proposal = proposal(None, "dark mode", &evidence_digest);
    let first = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &first_proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-1", 5, None)),
    )
    .unwrap();
    let result = ledger
        .commit(session.clone(), 1, first.facts.clone())
        .unwrap();
    assert_eq!(result.positions, vec![4, 5]);
    assert_eq!(
        ledger
            .commit(session.clone(), 1, first.facts.clone())
            .unwrap()
            .disposition,
        CommitDisposition::Replayed
    );
    drop(ledger);

    let mut ledger = SqliteLedger::open(&path).unwrap();
    let facts = ledger.read_facts(&session, 0, 5, None).unwrap();
    assert_eq!(facts[3].kind.as_str(), "memory.proposed");
    assert_eq!(facts[4].kind.as_str(), "memory.committed");

    let changed = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &first_proposal,
        MemoryWriteDecision::Reject(MemoryWriteRejection::NamespaceDenied),
    )
    .unwrap();
    assert!(matches!(
        ledger.commit(session.clone(), 2, changed.facts),
        Err(SqliteLedgerError::Domain(LedgerError::IncompleteReplay))
    ));
    assert_eq!(
        ledger
            .session_watermark(&session)
            .unwrap()
            .unwrap()
            .max_position,
        5
    );

    let prefixes = vec![MemoryPrefix {
        session_id: session.clone(),
        through_position: 5,
    }];
    let recovered = reconstruct_memory_state(&ledger, &prefixes).unwrap();
    assert_eq!(recovered.revisions().len(), 1);
    assert_eq!(recovered.revisions()[0].status(), MemoryStatus::Active);
    verify_memory_evidence(&ledger, &prefixes, &first_proposal).unwrap();
    let scope = MemoryScope::session("session").unwrap();
    let ordinary_grant =
        MemoryAccessGrant::new("namespace", vec![scope.clone()], prefixes.clone(), None).unwrap();
    authorize_memory_write(&ledger, &ordinary_grant, &first_proposal).unwrap();
    let mismatch = proposal(None, "dark mode", &"b".repeat(64));
    assert_eq!(
        verify_memory_evidence(&ledger, &prefixes, &mismatch),
        Err(MemoryErrorCode::EvidenceMismatch)
    );
    let foreign = MemoryProposal::new(
        "foreign",
        "namespace",
        MemoryScope::Namespace,
        MemoryKind::LearnedFact,
        ContentBinding::from_inline("foreign"),
        vec![DurableFactReference::new("foreign-session", 1, "fact", "a".repeat(64)).unwrap()],
        MemorySensitivity::Ordinary,
        5_000,
        None,
    )
    .unwrap();
    assert_eq!(
        verify_memory_evidence(&ledger, &prefixes, &foreign),
        Err(MemoryErrorCode::NamespaceDenied)
    );
    assert_eq!(
        authorize_memory_write(&ledger, &ordinary_grant, &foreign),
        Err(MemoryErrorCode::NamespaceDenied)
    );
    let restricted_query = MemoryQuery::new(
        "restricted-query",
        "namespace",
        vec![scope.clone()],
        MemoryPurpose::Context,
        "retriever-1",
        ContentBinding::from_inline("dark"),
        5,
        "2026-08-29T00:00:02Z",
        1,
        64,
        true,
        Some("c".repeat(64)),
    )
    .unwrap();
    assert_eq!(
        authorize_memory_query(&ordinary_grant, &restricted_query),
        Err(MemoryErrorCode::SensitivityDenied)
    );
    let restricted_grant = MemoryAccessGrant::new(
        "namespace",
        vec![scope],
        prefixes.clone(),
        Some("c".repeat(64)),
    )
    .unwrap();
    authorize_memory_query(&restricted_grant, &restricted_query).unwrap();

    let tombstone = plan_memory_tombstone(
        &MemoryTombstoneContext {
            command_id: "forget-restart".into(),
            recorded_at: "2026-08-29T00:00:02Z".into(),
        },
        &recovered,
        &MemoryTombstone {
            record_id: "record".into(),
            revision_id: "revision-1".into(),
        },
        MemoryTombstoneReason::Policy,
    )
    .unwrap();
    ledger
        .commit(session.clone(), 2, vec![tombstone.fact])
        .unwrap();
    let after_tombstone = reconstruct_memory_state(
        &ledger,
        &[MemoryPrefix {
            session_id: session,
            through_position: 6,
        }],
    )
    .unwrap();
    assert_eq!(
        after_tombstone.revisions()[0].status(),
        MemoryStatus::Tombstoned
    );
}

#[test]
fn supersession_and_tombstone_require_the_exact_active_revision() {
    let evidence = "b".repeat(64);
    let first_proposal = proposal(None, "one", &evidence);
    let first = plan_memory_write(
        &context(3),
        &MemoryState::default(),
        &first_proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-1", 5, None)),
    )
    .unwrap();
    let second_proposal = proposal(Some("revision-1"), "two", &evidence);
    let second = plan_memory_write(
        &context(5),
        &first.next_state,
        &second_proposal,
        MemoryWriteDecision::Commit(commit("record", "revision-2", 7, Some("revision-1"))),
    )
    .unwrap();
    assert_eq!(second.facts.len(), 3);
    assert_eq!(second.facts[2].kind.as_str(), "memory.superseded");
    assert_eq!(
        plan_memory_write(
            &context(5),
            &first.next_state,
            &second_proposal,
            MemoryWriteDecision::Commit(commit("record", "revision-2", 8, Some("revision-1"))),
        )
        .err()
        .unwrap(),
        RuntimeCommandError::InvalidCommand
    );

    assert_eq!(
        plan_memory_tombstone(
            &MemoryTombstoneContext {
                command_id: "forget".into(),
                recorded_at: "2026-08-29T00:00:02Z".into(),
            },
            &second.next_state,
            &MemoryTombstone {
                record_id: "record".into(),
                revision_id: "revision-2".into(),
            },
            MemoryTombstoneReason::UserRequest,
        )
        .err()
        .unwrap(),
        RuntimeCommandError::InvalidCommand,
    );
    let tombstone = plan_memory_tombstone(
        &MemoryTombstoneContext {
            command_id: "forget".into(),
            recorded_at: "2026-08-29T00:00:02Z".into(),
        },
        &second.next_state,
        &MemoryTombstone {
            record_id: "record".into(),
            revision_id: "revision-2".into(),
        },
        MemoryTombstoneReason::Policy,
    )
    .unwrap();
    assert!(tombstone.fact.turn_id.is_none() && tombstone.fact.execution_id.is_none());
    assert_eq!(
        plan_memory_tombstone(
            &MemoryTombstoneContext {
                command_id: "stale".into(),
                recorded_at: "2026-08-29T00:00:02Z".into(),
            },
            &tombstone.next_state,
            &MemoryTombstone {
                record_id: "record".into(),
                revision_id: "revision-2".into(),
            },
            MemoryTombstoneReason::Policy,
        )
        .err()
        .unwrap(),
        RuntimeCommandError::CommandConflict
    );
}
