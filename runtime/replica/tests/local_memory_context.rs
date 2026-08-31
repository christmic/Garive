use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use garive_config::{
    resolve_definition, AgentDefinition, CapabilityDescriptor, CapabilityKind, CapabilityReference,
    ContextPolicyCandidate, ContextPolicyReference, DefaultLimits, GovernancePolicy,
    GovernancePolicyCandidate, ProductPolicy, ResolutionRegistry,
};
use garive_core::{
    MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction, TerminalRecoveryAction,
};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelInputContent, ModelItem,
    ModelObserver, ModelOutputKind, ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason,
    ModelStreamEvent, ModelUsage, ObserverDecision, TextMode, TokenCount, UsageSource,
};
use garive_memory::{
    ContentBinding, DurableFactReference, HypothesisState, MemoryAuthority, MemoryAuthorityBinding,
    MemoryCommit, MemoryDocumentLimits, MemoryKind, MemoryProposal, MemoryRevisionClassification,
    MemoryRevisionScope, MemoryScope, MemoryScopeClass, MemorySensitivity, MemoryState, MemoryType,
    MemoryTypeDescriptor, MemoryTypeRegistry,
};
use garive_runtime::{
    local_dispatch_queue, plan_classified_memory_write, CatalogueCapabilityPreparationFactory,
    HostClock, LiveHost, LiveHostLimits, LocalExecutionAttempt, LocalExecutionPolicy,
    LocalExecutionWorker, LocalMemorySystemBinding, LocalWorkerDisposition, LocalWorkerError,
    MemoryControlAction, MemoryControlGrant, MemoryWriteContext, RuntimeAgentCatalogue,
    RuntimeAgentInstallation, SqliteLedger, USER_DECLARED_PUSH_REVISION,
};
use tempfile::tempdir;

const DESCRIPTOR_DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Clock;
impl HostClock for Clock {
    fn recorded_at(&self) -> String {
        "2026-09-01T00:00:00Z".into()
    }
}

struct CompletingModel(AtomicUsize);
impl ModelPort for CompletingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            assert_eq!(
                observer.observe(&ModelStreamEvent::OutputItemStarted {
                    output_index: 0,
                    kind: ModelOutputKind::Text,
                }),
                ObserverDecision::Continue
            );
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text { text: "ok".into() }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

struct MemoryCheckingModel;
impl ModelPort for MemoryCheckingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        assert!(request.input_items.iter().any(|item| matches!(
            item,
            garive_llm::ModelInputItem::Message { content, .. }
                if content.iter().any(|part| matches!(
                    part,
                    ModelInputContent::Text(text)
                        if text.contains("garive.memory") && text.contains("dark mode")
                ))
        )));
        Box::pin(async move {
            assert_eq!(
                observer.observe(&ModelStreamEvent::OutputItemStarted {
                    output_index: 0,
                    kind: ModelOutputKind::Text,
                }),
                ObserverDecision::Continue
            );
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text { text: "ok".into() }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

#[tokio::test]
async fn configured_empty_repository_commits_retrieval_before_model() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("memory.db");
    let installation = installation(true);
    let installed = installation.clone_installed_agent();
    let catalogue = Arc::new(RuntimeAgentCatalogue::new([installation]).unwrap());
    let binding = binding(DESCRIPTOR_DIGEST);
    let factory = Arc::new(CatalogueCapabilityPreparationFactory::new(
        catalogue,
        Some(binding),
    ));
    let (dispatcher, mut queue) = local_dispatch_queue(1).unwrap();
    let host = LiveHost::new(
        &database,
        installed,
        host_limits(),
        Arc::new(Clock),
        dispatcher,
    )
    .unwrap();
    let session = host.create_session("create", "agent").unwrap();
    let turn = host
        .start_turn("start", &session.session_id, "hello")
        .unwrap();
    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model.clone())
        .unwrap()
        .with_capability_preparation(factory);
    assert!(matches!(
        queue.try_run_next(&worker, &attempt()).await,
        Ok(LocalWorkerDisposition::TerminalCommitted { .. })
    ));
    assert_eq!(model.0.load(Ordering::SeqCst), 1);
    let ledger = SqliteLedger::open(&database).unwrap();
    let kinds = ledger
        .load_turn(&garive_ledger::TurnId::try_from(turn.turn_id.as_str()).unwrap())
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    let retrieval = kinds
        .iter()
        .position(|kind| kind == "memory.retrieval_recorded")
        .unwrap();
    let model_started = kinds
        .iter()
        .position(|kind| kind == "model.started")
        .unwrap();
    assert!(retrieval < model_started);
}

#[tokio::test]
async fn required_binding_fails_closed_before_model_dispatch() {
    for (binding, expected) in [
        (None, LocalWorkerError::CapabilityBindingMissing),
        (
            Some(binding(&"b".repeat(64))),
            LocalWorkerError::CapabilityBindingMismatch,
        ),
    ] {
        let directory = tempdir().unwrap();
        let database = directory.path().join("memory.db");
        let installation = installation(true);
        let installed = installation.clone_installed_agent();
        let catalogue = Arc::new(RuntimeAgentCatalogue::new([installation]).unwrap());
        let (dispatcher, mut queue) = local_dispatch_queue(1).unwrap();
        let host = LiveHost::new(
            &database,
            installed,
            host_limits(),
            Arc::new(Clock),
            dispatcher,
        )
        .unwrap();
        let session = host.create_session("create", "agent").unwrap();
        host.start_turn("start", &session.session_id, "hello")
            .unwrap();
        let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
        let worker = LocalExecutionWorker::new(&database, policy(), model.clone())
            .unwrap()
            .with_capability_preparation(Arc::new(CatalogueCapabilityPreparationFactory::new(
                catalogue, binding,
            )));
        assert_eq!(queue.try_run_next(&worker, &attempt()).await, Err(expected));
        assert_eq!(model.0.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn extra_binding_cannot_add_memory_to_snapshot() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("memory.db");
    let installation = installation(false);
    let installed = installation.clone_installed_agent();
    let catalogue = Arc::new(RuntimeAgentCatalogue::new([installation]).unwrap());
    let (dispatcher, mut queue) = local_dispatch_queue(1).unwrap();
    let host = LiveHost::new(
        &database,
        installed,
        host_limits(),
        Arc::new(Clock),
        dispatcher,
    )
    .unwrap();
    let session = host.create_session("create", "agent").unwrap();
    let turn = host
        .start_turn("start", &session.session_id, "hello")
        .unwrap();
    let model = Arc::new(CompletingModel(AtomicUsize::new(0)));
    let worker = LocalExecutionWorker::new(&database, policy(), model)
        .unwrap()
        .with_capability_preparation(Arc::new(CatalogueCapabilityPreparationFactory::new(
            catalogue,
            Some(binding(DESCRIPTOR_DIGEST)),
        )));
    queue.try_run_next(&worker, &attempt()).await.unwrap();
    let ledger = SqliteLedger::open(&database).unwrap();
    assert!(ledger
        .load_turn(&garive_ledger::TurnId::try_from(turn.turn_id.as_str()).unwrap())
        .unwrap()
        .facts
        .iter()
        .all(|fact| fact.kind.as_str() != "memory.retrieval_recorded"));
}

#[tokio::test]
async fn user_memory_written_in_one_session_reaches_another_sessions_model() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("cross-session.db");
    let installation = installation(true);
    let installed = installation.clone_installed_agent();
    let (discard_dispatcher, discard_queue) = local_dispatch_queue(1).unwrap();
    drop(discard_queue);
    let source_host = LiveHost::new(
        &database,
        installed.clone(),
        host_limits(),
        Arc::new(Clock),
        discard_dispatcher,
    )
    .unwrap();
    let source = source_host
        .create_session("source-create", "agent")
        .unwrap();
    let source_turn = source_host
        .start_turn("source-start", &source.session_id, "remember this")
        .unwrap();
    let source_session = garive_ledger::SessionId::try_from(source.session_id.as_str()).unwrap();
    let source_turn_id = garive_ledger::TurnId::try_from(source_turn.turn_id.as_str()).unwrap();
    let source_execution =
        garive_ledger::ExecutionId::try_from(source_turn.execution_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let evidence = ledger.read_facts(&source_session, 0, 1, None).unwrap()[0].clone();
    let proposal = MemoryProposal::new(
        "proposal-dark-mode",
        "memory-local",
        MemoryScope::Namespace,
        MemoryKind::Preference,
        ContentBinding::from_inline("dark mode"),
        vec![DurableFactReference::new(
            source.session_id,
            evidence.position,
            evidence.fact_id.as_str(),
            evidence.payload.sha256(),
        )
        .unwrap()],
        MemorySensitivity::Ordinary,
        10_000,
        None,
    )
    .unwrap();
    let classification = MemoryRevisionClassification::new(
        MemoryKind::Preference,
        MemoryAuthorityBinding::new(MemoryAuthority::UserDeclared, Some("e".repeat(64))).unwrap(),
        MemoryRevisionScope::new(MemoryScopeClass::User, "user-local", None).unwrap(),
        HypothesisState::Active,
        "classification-v1",
        &classification_registry(),
    )
    .unwrap();
    let context = MemoryWriteContext {
        turn_id: source_turn_id,
        execution_id: source_execution,
        through_position: source_turn.committed_position,
        recorded_at: "2026-09-01T00:00:01Z".into(),
    };
    let commit = MemoryCommit::new(
        "record-dark-mode",
        "revision-dark-mode-v1",
        "d".repeat(64),
        source_turn.committed_position + 2,
        None,
        None,
    )
    .unwrap();
    let planned = plan_classified_memory_write(
        &context,
        &MemoryState::default(),
        &proposal,
        commit,
        &source_session,
        "classification-dark-mode",
        &classification,
    )
    .unwrap();
    let source_version = ledger.session_version(&source_session).unwrap().unwrap();
    ledger
        .commit_classified_memory_write(
            source_session,
            source_version,
            planned,
            MemoryDocumentLimits::new(16_384, 8_192, 128).unwrap(),
        )
        .unwrap();
    drop(ledger);

    let catalogue = Arc::new(RuntimeAgentCatalogue::new([installation]).unwrap());
    let (dispatcher, mut queue) = local_dispatch_queue(1).unwrap();
    let target_host = LiveHost::new(
        &database,
        installed,
        host_limits(),
        Arc::new(Clock),
        dispatcher,
    )
    .unwrap();
    let target = target_host
        .create_session("target-create", "agent")
        .unwrap();
    let target_turn = target_host
        .start_turn("target-start", &target.session_id, "what do I prefer?")
        .unwrap();
    let worker = LocalExecutionWorker::new(&database, policy(), Arc::new(MemoryCheckingModel))
        .unwrap()
        .with_capability_preparation(Arc::new(CatalogueCapabilityPreparationFactory::new(
            catalogue,
            Some(binding_with_scope(
                DESCRIPTOR_DIGEST,
                MemoryScope::Namespace,
                MemoryScopeClass::User,
                "user-local",
            )),
        )));
    queue.try_run_next(&worker, &attempt()).await.unwrap();
    let ledger = SqliteLedger::open(&database).unwrap();
    let facts = ledger
        .load_turn(&garive_ledger::TurnId::try_from(target_turn.turn_id.as_str()).unwrap())
        .unwrap()
        .facts;
    let retrieval = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "memory.retrieval_recorded")
        .unwrap();
    let started = facts
        .iter()
        .position(|fact| fact.kind.as_str() == "model.started")
        .unwrap();
    assert!(retrieval < started);
}

fn installation(memory: bool) -> RuntimeAgentInstallation {
    let descriptor = CapabilityDescriptor {
        kind: CapabilityKind::Memory,
        name: "memory.local".into(),
        exact_revision: "memory.local.v1".into(),
        contract_version: 1,
        descriptor_digest: DESCRIPTOR_DIGEST.into(),
    };
    let capabilities = memory.then(|| {
        CapabilityReference::new(
            CapabilityKind::Memory,
            &descriptor.name,
            &descriptor.exact_revision,
            descriptor.contract_version,
            true,
        )
        .unwrap()
    });
    let limits = DefaultLimits::new(2, Some(100), Some(20), Some(1_000)).unwrap();
    let definition = AgentDefinition::new(
        "agent",
        "agent.v1",
        Vec::new(),
        Vec::new(),
        capabilities.into_iter().collect(),
        GovernancePolicy::new("governance", "governance.v1", [], []).unwrap(),
        ContextPolicyReference::new("context", "context.v1").unwrap(),
        limits.clone(),
        BTreeMap::from([("effective_snapshot".into(), 1)]),
    )
    .unwrap();
    let snapshot = resolve_definition(
        &definition,
        &ResolutionRegistry {
            instructions: Vec::new(),
            model_roles: Vec::new(),
            tools: Vec::new(),
            capability_descriptors: memory.then_some(descriptor).into_iter().collect(),
            governance_policies: vec![GovernancePolicyCandidate {
                policy_id: "governance".into(),
                exact_revision: "governance.v1".into(),
                allowed_requirement_capabilities: BTreeSet::new(),
                interaction_modes: BTreeSet::new(),
            }],
            context_policies: vec![ContextPolicyCandidate {
                policy_id: "context".into(),
                exact_revision: "context.v1".into(),
                descriptor_digest: "c".repeat(64),
            }],
            public_tool_activity_catalogue: None,
        },
        &ProductPolicy {
            allowed_requirement_capabilities: BTreeSet::new(),
            interaction_modes: BTreeSet::new(),
            limit_caps: limits,
            admitted_contract_versions: BTreeMap::from([(
                "effective_snapshot".into(),
                BTreeSet::from([1]),
            )]),
        },
    )
    .unwrap();
    RuntimeAgentInstallation::new(snapshot, "local-agent", Vec::new()).unwrap()
}

fn binding(digest: &str) -> LocalMemorySystemBinding {
    binding_with_scope(
        digest,
        MemoryScope::Namespace,
        MemoryScopeClass::User,
        "user-local",
    )
}

fn binding_with_scope(
    digest: &str,
    query_scope: MemoryScope,
    control_scope: MemoryScopeClass,
    owner_id: &str,
) -> LocalMemorySystemBinding {
    LocalMemorySystemBinding::new(
        "memory.local",
        "memory.local.v1",
        digest,
        "memory-local",
        USER_DECLARED_PUSH_REVISION,
        USER_DECLARED_PUSH_REVISION,
        MemoryControlGrant::new(
            "memory-local",
            [MemoryControlAction::Export],
            [garive_memory::MemoryAuthorizedScope {
                scope: control_scope,
                owner_id: owner_id.into(),
            }],
        )
        .unwrap(),
        vec![query_scope],
        MemoryDocumentLimits::new(16_384, 8_192, 128).unwrap(),
        8,
        8_192,
        128,
        1_024,
    )
    .unwrap()
}

fn classification_registry() -> MemoryTypeRegistry {
    let descriptor = |memory_type, roles, authorities, name: &str| {
        MemoryTypeDescriptor::new(
            memory_type,
            roles,
            authorities,
            format!("{name}-retention-v1"),
            format!("{name}-evidence-v1"),
            format!("{name}-promotion-v1"),
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

fn host_limits() -> LiveHostLimits {
    LiveHostLimits {
        max_command_bytes: 4_096,
        event_batch_size: 64,
        event_poll_interval_ms: 10,
        activity: None,
    }
}

fn policy() -> LocalExecutionPolicy {
    LocalExecutionPolicy {
        model_target_id: "target".into(),
        deployment_id: "deployment".into(),
        recovery_policy_revision: "recovery.v1".into(),
        required_capabilities: vec![ModelCapability::Text],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(20),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Stop,
            transport: TerminalRecoveryAction::Stop,
            unavailable: TerminalRecoveryAction::Stop,
            missing_usage: MissingUsagePolicy::Stop,
        },
        max_context_items: 8,
        max_context_utf8_bytes: 8_192,
        max_model_attempts: 1,
    }
}

fn attempt() -> LocalExecutionAttempt {
    LocalExecutionAttempt {
        worker_owner_id: "worker".into(),
        lease_token: "lease-token".into(),
        now_ms: 1_000,
        lease_duration_ms: 5_000,
        recorded_at: "2026-09-01T00:00:01Z".into(),
    }
}
