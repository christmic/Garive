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
    InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture, ModelItem, ModelObserver,
    ModelOutputKind, ModelOutputSettings, ModelPort, ModelRequest, ModelStopReason,
    ModelStreamEvent, ModelUsage, ObserverDecision, TextMode, TokenCount, UsageSource,
};
use garive_memory::{MemoryDocumentLimits, MemoryScope};
use garive_runtime::{
    local_dispatch_queue, CatalogueCapabilityPreparationFactory, HostClock, LiveHost,
    LiveHostLimits, LocalExecutionAttempt, LocalExecutionPolicy, LocalExecutionWorker,
    LocalMemorySystemBinding, LocalWorkerDisposition, LocalWorkerError, MemoryControlAction,
    MemoryControlGrant, RuntimeAgentCatalogue, RuntimeAgentInstallation, SqliteLedger,
    USER_DECLARED_PUSH_REVISION,
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
    LocalMemorySystemBinding::new(
        "memory.local",
        "memory.local.v1",
        digest,
        "memory-local",
        USER_DECLARED_PUSH_REVISION,
        USER_DECLARED_PUSH_REVISION,
        MemoryControlGrant::new("memory-local", [MemoryControlAction::Export], []).unwrap(),
        vec![MemoryScope::Namespace],
        MemoryDocumentLimits::new(16_384, 8_192, 128).unwrap(),
        8,
        8_192,
        128,
        1_024,
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
