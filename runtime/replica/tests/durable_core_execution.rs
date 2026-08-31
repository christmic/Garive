use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU32,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use futures::executor::block_on;
use garive_core::{
    AgentCursor, AgentDefinitionId, AgentDefinitionRevision, AgentEntry, AgentEvent,
    AgentInstanceId, AgentOutcome, AgentToolCapabilities, AgentTurnRequest, CandidateKind,
    ClockPort, ContextCandidate, ContextPort, ContextPortError, ContextPurpose, ContextRequest,
    EventSink, ExecutionId as CoreExecutionId, ExecutionLimits, FactRef, MissingUsagePolicy,
    ModelOnlyLimits, ModelRecoveryPolicy, OutputLimitAction, PortFailure, Retention,
    SessionId as CoreSessionId, TerminalRecoveryAction, ToolPreparationPort, TurnId as CoreTurnId,
    Visibility,
};
use garive_knowledge::{
    Citation, CitationScheme, ContentBinding as KnowledgeContent, FreshnessRequirement,
    KnowledgeEvidence, KnowledgeFreshness, KnowledgeQueryMode, KnowledgeRequest,
    KnowledgeSourceDescriptor, KnowledgeSourceKind, KnowledgeTrustClass,
};
use garive_ledger::{
    AgentDefinitionId as LedgerDefinitionId, AgentDefinitionRevision as LedgerRevision,
    AgentInstanceId as LedgerAgentId, CanonicalPayload, CommitDisposition, FactDraft, FactId,
    FactKind, LedgerError, SessionId,
};
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture,
    ModelInputContent, ModelInputItem, ModelItem, ModelObserver, ModelOutputSettings, ModelPort,
    ModelPortFailure, ModelRequest, ModelStopReason, ModelTargetId, ModelUsage, TextMode,
    TokenCount, UsageSource,
};
use garive_memory::{
    ContentBinding as MemoryContent, DurableFactReference, MemoryKind, MemoryPurpose, MemoryQuery,
    MemoryRecord, MemoryScope, MemoryScore, MemorySensitivity, MemoryStatus,
};
use garive_provider_anthropic::build_profile as build_anthropic_profile;
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use garive_runtime::{
    commit_planned_turn, execute_durable_agent_with_f0, execute_durable_model_only,
    execute_durable_model_only_with_capabilities, plan_cancel_turn, plan_memory_retrieval,
    plan_skill_activation, plan_start_turn, AuthorityDecision, AuthorityFuture, AuthorityPort,
    AuthorityRequest, CancelReason, CancelTurnCommand, DurableExecutionConfig,
    DurableExecutionError, EffectiveRuntimeLimits, ExecutionLeaseRequest, ExecutorDispatch,
    ExecutorFuture, ExecutorPort, F0ExecutionGovernance, F0GovernanceContext, KnowledgeAccessGrant,
    KnowledgeConnector, KnowledgeConnectorFuture, KnowledgeConnectorOutcome, LiveOutputEventKind,
    LiveOutputHub, LiveOutputLimits, MemoryRetrievalContext, ModelLifecycleContext,
    PreparedAgentCapabilities, PreparedExecution, PreparedKnowledgeCapability, RuntimeCommandError,
    RuntimeCommandId, RuntimeHttpLimits, RuntimeModelHttpTransport, SafetyDecisionV1,
    SafetyDisposition, SafetyEvaluation, SafetyFuture, SafetyPort, SandboxAdmission,
    SandboxAdmissionPort, SandboxAdmissionRequest, SandboxBindingV1, SkillActivationContext,
    SqliteLedger, StartTurnCommand, TerminalPublicationError, TerminalPublisher,
};
use garive_skill::{
    ActivationMode, ActivationPolicy, ContentBinding, SkillActivationRequest, SkillDefinition,
};
use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, EffectReceipt, ExecutionCapability,
    ExecutionFact, ExecutionRequirements, InvocationAccessSet, PreparationError, PreparedToolCall,
    ReceiptId, ReplayClass, ResourceAccess, SandboxControl, SandboxRequirementsV1,
    TerminalClassification, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition,
    ToolIntent,
};
use tempfile::tempdir;

struct Context {
    positions: Vec<u64>,
}

struct LiveMemoryContext {
    session_id: String,
    prompt: String,
}

impl ContextPort for LiveMemoryContext {
    fn read_candidates(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<Vec<ContextCandidate>, ContextPortError> {
        if request.session_id != self.session_id {
            return Err(ContextPortError::PortFailure);
        }
        Ok(vec![ContextCandidate {
            fact_ref: FactRef {
                session_id: self.session_id.clone(),
                position: 3,
            },
            kind: CandidateKind::UserInput,
            retention: Retention::Required,
            visibility: Visibility::Visible,
            items: vec![ModelInputItem::Message {
                role: garive_llm::ModelRole::User,
                content: vec![ModelInputContent::Text(self.prompt.clone())],
            }],
        }])
    }
}

#[derive(Clone, Copy)]
enum LiveMemoryCase {
    None,
    Correct,
    SupersededConflict,
}

/// Explicit live acceptance: opt in with GARIVE_LIVE_API=1.
#[tokio::test]
#[ignore = "calls a real model through the configured loopback gateway"]
async fn live_memory_ledger_improves_factual_work_and_rejects_stale_revision() {
    assert_eq!(std::env::var("GARIVE_LIVE_API").as_deref(), Ok("1"));
    let endpoint = std::env::var("GARIVE_LIVE_API_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:9527/v1/messages".into());
    let credential =
        std::env::var("GARIVE_LIVE_API_KEY").unwrap_or_else(|_| "token9-loopback".into());
    let transport = RuntimeModelHttpTransport::anthropic(
        MessagesDeployment {
            target_id: "target".into(),
            model_id: "deepseek-v4-pro".into(),
            capabilities: BTreeSet::from([ModelCapability::Text, ModelCapability::Streaming]),
            default_max_output_tokens: Some(2_048),
            media_bindings: BTreeMap::new(),
            thinking: None,
            error_policy: ProtocolErrorPolicy::default(),
        },
        build_anthropic_profile(&ConnectionInput::new(
            EndpointSelection::Explicit(endpoint),
            SecretValue::new(credential).unwrap(),
            vec![],
        ))
        .unwrap(),
        RuntimeHttpLimits {
            connect_timeout_ms: 5_000,
            request_timeout_ms: 120_000,
            max_response_bytes: 1_048_576,
        },
    )
    .unwrap();

    let baseline = run_live_memory_case(&transport, LiveMemoryCase::None).await;
    let correct = run_live_memory_case(&transport, LiveMemoryCase::Correct).await;
    let conflict = run_live_memory_case(&transport, LiveMemoryCase::SupersededConflict).await;

    let baseline_value = parse_live_json(&baseline.0);
    let correct_value = parse_live_json(&correct.0);
    let conflict_value = parse_live_json(&conflict.0);
    assert_ne!(
        baseline_value,
        serde_json::json!({
            "codename":"Amber Heron", "deploy_day":"Tuesday", "region":"Qingdao",
            "evidence_record_ids":["client-brief"]
        })
    );
    let expected = serde_json::json!({
        "codename":"Amber Heron", "deploy_day":"Tuesday", "region":"Qingdao",
        "evidence_record_ids":["client-brief"]
    });
    assert_eq!(correct_value, expected);
    assert_eq!(conflict_value, expected);
    assert!(!conflict.0.contains("Friday"));

    println!(
        "{}",
        serde_json::json!({
            "baseline":{"output":baseline.0,"input_tokens":baseline.1,"output_tokens":baseline.2},
            "correct_memory":{"output":correct.0,"input_tokens":correct.1,"output_tokens":correct.2},
            "superseded_conflict":{"output":conflict.0,"input_tokens":conflict.1,"output_tokens":conflict.2}
        })
    );
}

async fn run_live_memory_case(
    transport: &RuntimeModelHttpTransport,
    case: LiveMemoryCase,
) -> (String, u64, u64) {
    let directory = tempdir().unwrap();
    let suffix = match case {
        LiveMemoryCase::None => "none",
        LiveMemoryCase::Correct => "correct",
        LiveMemoryCase::SupersededConflict => "conflict",
    };
    let path = directory.path().join(format!("memory-{suffix}.sqlite3"));
    let session = SessionId::try_from(format!("live-memory-{suffix}").as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new(format!("start-{suffix}")).unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "memory effectiveness acceptance".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 1,
            max_input_tokens: None,
            max_output_tokens: Some(2_048),
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-31T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, plan.facts).unwrap();
    let evidence = ledger.read_facts(&session, 2, 3, None).unwrap().remove(0);
    let mut request = core_request(&session, &plan.turn_id, &execution);
    request.required_capabilities = vec![ModelCapability::Text, ModelCapability::Streaming];
    request.context_request.max_utf8_bytes = 16_384;
    request.model_output.max_output_tokens = Some(2_048);
    request.limits.max_total_tokens = None;
    request.limits.execution = ExecutionLimits::new(NonZeroU32::new(1).unwrap());
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "token9-deepseek-pro".into(),
            recovery_policy_revision: "live-memory-v1".into(),
            max_attempts: 1,
            recorded_at: "2026-08-31T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            owner_id: "live-memory-worker".into(),
            lease_token: format!("live-memory-lease-{suffix}"),
            now_ms: 1,
            duration_ms: 180_000,
        },
    };
    let reference = DurableFactReference::new(
        session.as_str(),
        evidence.position,
        evidence.fact_id.as_str(),
        evidence.payload.sha256(),
    )
    .unwrap();
    let scope = MemoryScope::session(session.as_str()).unwrap();
    let active_text = r#"{"codename":"Amber Heron","deploy_day":"Tuesday","region":"Qingdao"}"#;
    let active = MemoryRecord::new(
        "client-brief",
        "revision-active",
        "workspace",
        scope.clone(),
        MemoryKind::LearnedFact,
        MemoryContent::from_inline(active_text),
        vec![reference.clone()],
        MemoryStatus::Active,
        MemorySensitivity::Ordinary,
        10_000,
        4,
        matches!(case, LiveMemoryCase::SupersededConflict).then(|| "revision-stale".into()),
        None,
    )
    .unwrap();
    let stale_text = r#"{"codename":"Amber Heron","deploy_day":"Friday","region":"Qingdao"}"#;
    let stale = MemoryRecord::new(
        "client-brief",
        "revision-stale",
        "workspace",
        scope.clone(),
        MemoryKind::LearnedFact,
        MemoryContent::from_inline(stale_text),
        vec![reference],
        MemoryStatus::Superseded,
        MemorySensitivity::Ordinary,
        10_000,
        3,
        None,
        None,
    )
    .unwrap();
    let records = match case {
        LiveMemoryCase::None => vec![],
        LiveMemoryCase::Correct => vec![active],
        LiveMemoryCase::SupersededConflict => vec![stale, active],
    };
    let scores = records
        .iter()
        .map(|record| {
            MemoryScore::new(
                record.record_id(),
                record.revision_id(),
                10_000,
                record.content().inline_utf8().unwrap().len() as u64,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let retrieval = (!records.is_empty()).then(|| {
        let query = MemoryQuery::new(
            format!("query-{suffix}"),
            "workspace",
            vec![scope],
            MemoryPurpose::Context,
            "exact-live-v1",
            MemoryContent::from_inline("client brief"),
            4,
            "2026-08-31T00:00:01Z",
            4,
            4_096,
            false,
            None,
        )
        .unwrap();
        plan_memory_retrieval(
            &MemoryRetrievalContext {
                turn_id: plan.turn_id.clone(),
                execution_id: execution.clone(),
                recorded_at: "2026-08-31T00:00:01Z".into(),
            },
            &records,
            &scores,
            &query,
        )
        .unwrap()
    });
    if matches!(case, LiveMemoryCase::SupersededConflict) {
        let selected = retrieval.as_ref().unwrap().retrieval.matches.as_slice();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].revision_id(), "revision-active");
    }
    let prompt = "Return exactly one compact JSON object with keys codename, deploy_day, region, evidence_record_ids. Use only committed garive.memory evidence. If absent, use null and an empty evidence_record_ids array. Never infer values. evidence_record_ids must contain the record_id values actually used.";
    let mut context = LiveMemoryContext {
        session_id: session.as_str().into(),
        prompt: prompt.into(),
    };
    let signals = Signals;
    let live_output = LiveOutputHub::new(LiveOutputLimits {
        max_active_executions: 1,
        max_preview_bytes: 64 * 1024,
        max_event_bytes: 16 * 1024,
        broadcast_capacity: 64,
        max_subscribers_per_session: 1,
    })
    .unwrap();
    let mut events = live_output.event_sink();
    let mut publisher = Publisher {
        path: path.clone(),
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.completed", "turn.completed"],
    };
    let result = execute_durable_model_only_with_capabilities(
        &mut ledger,
        &config,
        &request,
        PreparedAgentCapabilities {
            skill_activation: None,
            memory_retrieval: retrieval,
            knowledge_retrieval: None,
        },
        &mut context,
        transport,
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    )
    .await
    .unwrap();
    drop(ledger);
    let restarted = SqliteLedger::open(&path).unwrap();
    let facts = restarted.load_turn(&plan.turn_id).unwrap().facts;
    assert_eq!(
        facts
            .iter()
            .filter(|fact| fact.kind.as_str() == "memory.retrieval_recorded")
            .count(),
        usize::from(!records.is_empty())
    );
    if !records.is_empty() {
        let memory = facts
            .iter()
            .find(|fact| fact.kind.as_str() == "memory.retrieval_recorded")
            .unwrap();
        let started = facts
            .iter()
            .find(|fact| fact.kind.as_str() == "model.started")
            .unwrap();
        assert!(memory.position < started.position);
        if matches!(case, LiveMemoryCase::SupersededConflict) {
            assert!(!memory.payload.as_json().contains("Friday"));
            assert!(!memory.payload.as_json().contains("revision-stale"));
        }
    }
    let (items, usage) = match result.report.outcome {
        AgentOutcome::Completed {
            response_items,
            usage,
        } => (response_items, usage),
        outcome => panic!("unexpected live outcome: {outcome:?}"),
    };
    let text = items
        .into_iter()
        .find_map(|item| match item {
            ModelItem::Text { text } => Some(text),
            _ => None,
        })
        .unwrap();
    let mut subscriber = live_output.subscribe(session.as_str()).unwrap();
    let preview = tokio::time::timeout(std::time::Duration::from_secs(1), subscriber.recv())
        .await
        .unwrap()
        .unwrap();
    match preview.kind {
        LiveOutputEventKind::Snapshot {
            text: streamed,
            through_sequence,
        } => {
            assert!(through_sequence > 0);
            assert_eq!(streamed, text);
        }
        other => panic!("unexpected real live output: {other:?}"),
    }
    (
        text,
        known_tokens(usage.input_tokens),
        known_tokens(usage.output_tokens),
    )
}

fn parse_live_json(text: &str) -> serde_json::Value {
    serde_json::from_str(
        text.trim()
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim(),
    )
    .unwrap_or_else(|error| panic!("invalid live JSON {error}: {text}"))
}

fn known_tokens(value: TokenCount) -> u64 {
    match value {
        TokenCount::Known(value) => value,
        TokenCount::Unknown => 0,
    }
}
impl ContextPort for Context {
    fn read_candidates(
        &mut self,
        request: &ContextRequest,
        _: u32,
    ) -> Result<Vec<ContextCandidate>, ContextPortError> {
        self.positions.push(request.through_position);
        let fact = FactRef {
            session_id: request.session_id.clone(),
            position: 3,
        };
        Ok(vec![ContextCandidate {
            fact_ref: fact,
            kind: CandidateKind::UserInput,
            retention: Retention::Required,
            visibility: Visibility::Visible,
            items: vec![ModelInputItem::Message {
                role: garive_llm::ModelRole::User,
                content: vec![ModelInputContent::Text("hello".into())],
            }],
        }])
    }
}

struct Model {
    path: PathBuf,
    session: SessionId,
    tool_first: bool,
    calls: AtomicUsize,
}
impl ModelPort for Model {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let active = ledger.list_uncertain_model_requests(&self.session).unwrap();
            assert_eq!(active[0].as_str(), request.request_id.as_str());
            Ok(InvokeOutcome::Completed {
                items: if self.tool_first && call == 0 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "call".into(),
                        tool_name: "read_file".into(),
                        arguments_json: r#"{"path":"a"}"#.into(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "done".into(),
                    }]
                },
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(1),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: if self.tool_first && call == 0 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

struct CancelDuringModel {
    path: PathBuf,
    session: SessionId,
    turn: garive_ledger::TurnId,
}

struct RejectPreflight;

struct SkillCheckingModel {
    path: PathBuf,
    turn: garive_ledger::TurnId,
}

struct CheckingKnowledgeConnector {
    path: PathBuf,
    turn: garive_ledger::TurnId,
}

impl KnowledgeConnector for CheckingKnowledgeConnector {
    fn retrieve<'a>(
        &'a self,
        _: &'a KnowledgeSourceDescriptor,
        _: &'a KnowledgeRequest,
    ) -> KnowledgeConnectorFuture<'a> {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let kinds = ledger
                .load_turn(&self.turn)
                .unwrap()
                .facts
                .into_iter()
                .map(|fact| fact.kind.as_str().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(
                &kinds[kinds.len() - 2..],
                ["knowledge.requested", "knowledge.dispatched"]
            );
            assert!(!kinds.iter().any(|kind| kind == "knowledge.completed"));
            KnowledgeConnectorOutcome::Completed {
                evidence: vec![knowledge_evidence()],
                connector_order_stable: true,
            }
        })
    }
}

impl ModelPort for SkillCheckingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            let kinds: Vec<_> = ledger
                .load_turn(&self.turn)
                .unwrap()
                .facts
                .iter()
                .map(|fact| fact.kind.as_str().to_owned())
                .collect();
            let skill = kinds
                .iter()
                .position(|kind| kind == "skill.activated")
                .unwrap();
            let memory = kinds
                .iter()
                .position(|kind| kind == "memory.retrieval_recorded")
                .unwrap();
            let started = kinds
                .iter()
                .position(|kind| kind == "model.started")
                .unwrap();
            let knowledge = kinds
                .iter()
                .position(|kind| kind == "knowledge.completed")
                .unwrap();
            assert!(skill < memory && memory < knowledge && knowledge < started);
            assert!(matches!(
                &request.input_items[0],
                ModelInputItem::Message {
                    role: garive_llm::ModelRole::Developer,
                    content,
                } if content == &vec![ModelInputContent::Text("Check facts.".into())]
            ));
            assert!(matches!(
                &request.input_items[1],
                ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    content,
                } if matches!(&content[0], ModelInputContent::Text(text) if text.contains("garive.memory"))
            ));
            assert!(matches!(
                &request.input_items[2],
                ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    content,
                } if matches!(&content[0], ModelInputContent::Text(text) if text.contains("garive.knowledge"))
            ));
            assert!(matches!(
                &request.input_items[3],
                ModelInputItem::Message {
                    role: garive_llm::ModelRole::User,
                    ..
                }
            ));
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "done".into(),
                }],
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

impl ModelPort for RejectPreflight {
    fn preflight(&self, _: &ModelRequest) -> Result<(), ModelPortFailure> {
        Err(ModelPortFailure::UnsupportedCapability)
    }

    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        panic!("invoke must not run after rejected preflight")
    }
}

impl ModelPort for CancelDuringModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let mut ledger = SqliteLedger::open(&self.path).unwrap();
            let snapshot = ledger.load_turn(&self.turn).unwrap();
            let cancel = plan_cancel_turn(&CancelTurnCommand {
                command_id: RuntimeCommandId::new("cancel-during-model").unwrap(),
                session_id: self.session.clone(),
                turn_id: self.turn.clone(),
                reason: CancelReason::User,
                requested_through_position: snapshot.through_position,
                recorded_at: "2026-08-29T00:00:02Z".into(),
            })
            .unwrap();
            commit_planned_turn(
                &mut ledger,
                self.session.clone(),
                snapshot.session_version,
                &cancel,
            )
            .unwrap();
            assert!(cancellation.is_cancelled());
            Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::Cancelled,
                partial_items: vec![],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(1),
                    output_tokens: TokenCount::Known(0),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
            })
        })
    }
}

struct Authority;
impl AuthorityPort for Authority {
    fn authorize<'a>(&'a mut self, request: AuthorityRequest<'a>) -> AuthorityFuture<'a> {
        Box::pin(async move {
            Ok(AuthorityDecision::Approve {
                granted_requirements: request.prepared.requirements().clone(),
                constraints_digest: "a".repeat(64),
                authority_revision: "policy-1".into(),
            })
        })
    }
}

struct ReadAccessResolver;

impl ToolAccessResolver for ReadAccessResolver {
    fn revision(&self) -> &str {
        "read-resolver-v1"
    }

    fn resolve(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap_or_default(),
            AccessMode::Read,
        )?])
    }
}

struct ReadPreparation(ToolCatalog);

impl ToolPreparationPort for ReadPreparation {
    fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.0.prepare_v3(intent, &ReadAccessResolver)
    }
}

struct AllowReadSafety(ExecutionRequirements);

impl SafetyPort for AllowReadSafety {
    fn decide<'a>(&'a mut self, request: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async move {
            Ok(SafetyEvaluation {
                decision: SafetyDecisionV1::new(
                    "read-safety",
                    SafetyDisposition::Allow,
                    request.invocation_id().clone(),
                    request.prepared_digest(),
                    Some("a".repeat(64)),
                    "policy-1",
                    None,
                )
                .unwrap(),
                granted_requirements: Some(self.0.clone()),
                interaction: None,
            })
        })
    }
}

struct ReadSandbox;

impl SandboxAdmissionPort for ReadSandbox {
    fn admit(
        &mut self,
        _: SandboxAdmissionRequest<'_>,
    ) -> Result<SandboxAdmission, garive_runtime::GovernedRuntimePortError> {
        Ok(SandboxAdmission {
            binding: SandboxBindingV1::new(
                "read-binding",
                "workspace",
                "local.read",
                "1",
                "policy-1",
                read_access_policy(),
                read_sandbox_requirements(),
            )
            .unwrap(),
            effective_limits_digest: "b".repeat(64),
            preflight_id: "read-preflight".into(),
            dispatch_attempt_id: "tool-dispatch".into(),
        })
    }
}

fn read_access_policy() -> ToolAccessPolicyV1 {
    ToolAccessPolicyV1::new(
        "read-policy-v1",
        [AccessPolicyEntry::new("a", [AccessMode::Read]).unwrap()],
        [],
        [],
        [],
        1,
        1_024,
    )
    .unwrap()
}

fn read_sandbox_requirements() -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        8,
    )
    .unwrap()
}

struct Executor {
    path: PathBuf,
    session: SessionId,
}
impl ExecutorPort for Executor {
    fn prepare(
        &mut self,
        _: &garive_tools::ToolInvocationId,
        _: &garive_tools::PreparedToolCall,
        _: &garive_tools::InvocationGrant,
    ) -> Result<PreparedExecution, String> {
        Ok(PreparedExecution {
            executor_id: "local.read".into(),
            executor_revision: "1".into(),
            dispatch_attempt_id: "tool-dispatch".into(),
        })
    }

    fn dispatch<'a>(&'a mut self, command: ExecutorDispatch<'a>) -> ExecutorFuture<'a> {
        Box::pin(async move {
            let ledger = SqliteLedger::open(&self.path).unwrap();
            assert_eq!(
                ledger
                    .list_uncertain_tool_invocations(&self.session)
                    .unwrap()
                    .len(),
                1
            );
            let content = serde_json::json!({"text":"ok"});
            let result_digest = CanonicalPayload::from_value(&content)
                .unwrap()
                .sha256()
                .to_owned();
            Ok(ExecutionFact::Completed {
                receipt: Some(EffectReceipt {
                    receipt_id: ReceiptId::new(command.receipt_id).unwrap(),
                    invocation_id: command.invocation_id.clone(),
                    prepared_digest: command.prepared.input_digest().into(),
                    grant_id: command.grant.grant_id.clone(),
                    executor_id: command.execution.executor_id.clone(),
                    executor_revision: command.execution.executor_revision.clone(),
                    terminal_classification: TerminalClassification::Completed,
                    result_digest,
                }),
                content,
                truncated: false,
            })
        })
    }
}

struct Signals;
impl ModelCancellation for Signals {
    fn is_cancelled(&self) -> bool {
        false
    }
}
impl ClockPort for Signals {
    fn now_tick(&self) -> Result<u64, PortFailure> {
        Ok(0)
    }
}
impl EventSink for Signals {
    fn emit(&mut self, _: AgentEvent) -> Result<(), PortFailure> {
        Ok(())
    }
}

struct Publisher {
    path: PathBuf,
    turn: garive_ledger::TurnId,
    fail: bool,
    calls: usize,
    expected_terminal: [&'static str; 2],
}
impl TerminalPublisher for Publisher {
    fn publish_terminal(
        &mut self,
        _: &garive_core::ExecutionReport,
        positions: &[u64],
    ) -> Result<(), TerminalPublicationError> {
        let ledger = SqliteLedger::open(&self.path).unwrap();
        let snapshot = ledger.load_turn(&self.turn).unwrap();
        let kinds: Vec<_> = snapshot
            .facts
            .iter()
            .map(|fact| fact.kind.as_str())
            .collect();
        assert_eq!(&kinds[kinds.len() - 2..], self.expected_terminal);
        assert_eq!(positions.len(), 2);
        self.calls += 1;
        if self.fail {
            Err(TerminalPublicationError)
        } else {
            Ok(())
        }
    }
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

#[test]
fn sqlite_dispatch_and_publication_cross_only_after_their_commits() {
    for fail_publication in [false, true] {
        let directory = tempdir().unwrap();
        let path = directory.path().join("runtime.sqlite3");
        let session = SessionId::try_from(if fail_publication {
            "session-fail"
        } else {
            "session"
        })
        .unwrap();
        let mut ledger = SqliteLedger::open(&path).unwrap();
        ledger
            .commit(session.clone(), 0, vec![open_session()])
            .unwrap();
        let start = StartTurnCommand {
            command_id: RuntimeCommandId::new("start").unwrap(),
            session_id: session.clone(),
            agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
            definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
            definition_revision: LedgerRevision::try_from("revision").unwrap(),
            snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .into(),
            trusted_input: "hello".into(),
            limits: EffectiveRuntimeLimits {
                max_iterations: 2,
                max_input_tokens: None,
                max_output_tokens: None,
                deadline_budget_ms: None,
            },
            recorded_at: "2026-08-29T00:00:00Z".into(),
        };
        let plan = plan_start_turn(&start, 1).unwrap();
        let execution = plan.execution_id.clone().unwrap();
        ledger.commit(session.clone(), 1, plan.facts).unwrap();
        let request = core_request(&session, &plan.turn_id, &execution);
        let config = DurableExecutionConfig {
            session_id: session.clone(),
            expected_session_version: 2,
            model: ModelLifecycleContext {
                turn_id: plan.turn_id.clone(),
                execution_id: execution.clone(),
                deployment_id: "deployment".into(),
                recovery_policy_revision: "policy".into(),
                max_attempts: 1,
                recorded_at: "2026-08-29T00:00:01Z".into(),
            },
            lease: ExecutionLeaseRequest {
                turn_id: plan.turn_id.clone(),
                execution_id: execution,
                owner_id: "test-worker".into(),
                lease_token: format!("lease-{fail_publication}"),
                now_ms: 1,
                duration_ms: 10_000,
            },
        };
        let model = Model {
            path: path.clone(),
            session,
            tool_first: false,
            calls: AtomicUsize::new(0),
        };
        let mut context = Context { positions: vec![] };
        let signals = Signals;
        let mut events = Signals;
        let mut publisher = Publisher {
            path: path.clone(),
            turn: plan.turn_id.clone(),
            fail: fail_publication,
            calls: 0,
            expected_terminal: ["execution.completed", "turn.completed"],
        };
        let mut stale = request.clone();
        stale.context_request.through_position -= 1;
        assert!(matches!(
            block_on(execute_durable_model_only(
                &mut ledger,
                &config,
                &stale,
                &mut context,
                &model,
                &mut events,
                &signals,
                &signals,
                &mut publisher,
            )),
            Err(DurableExecutionError::Command(
                RuntimeCommandError::ConcurrentModification
            ))
        ));
        let result = block_on(execute_durable_model_only(
            &mut ledger,
            &config,
            &request,
            &mut context,
            &model,
            &mut events,
            &signals,
            &signals,
            &mut publisher,
        ))
        .unwrap();
        assert_eq!(result.publication.is_err(), fail_publication);
        assert_eq!(publisher.calls, 1);
        assert_eq!(ledger.load_turn(&plan.turn_id).unwrap().facts.len(), 9);
    }
}

#[test]
fn complete_agent_loop_coordinates_model_effect_context_and_terminal_commits() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("agent.sqlite3");
    let session = SessionId::try_from("agent-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("agent-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 3,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, plan.facts).unwrap();
    let mut request = core_request(&session, &plan.turn_id, &execution);
    request.required_capabilities = vec![ModelCapability::Tools];
    request.limits.execution = ExecutionLimits::new(NonZeroU32::new(3).unwrap());
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution,
            owner_id: "agent-worker".into(),
            lease_token: "agent-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let model = Model {
        path: path.clone(),
        session: session.clone(),
        tool_first: true,
        calls: AtomicUsize::new(0),
    };
    let mut authority = Authority;
    let mut executor = Executor {
        path: path.clone(),
        session: session.clone(),
    };
    let requirements =
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_024).unwrap();
    let definition = ToolDefinition::new_v3(
        "read_file",
        "1",
        "Read one file.",
        serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        requirements.clone(),
        ReplayClass::ReadOnly,
        read_access_policy(),
        "read-resolver-v1",
        read_sandbox_requirements(),
    )
    .unwrap();
    let capabilities = AgentToolCapabilities {
        definitions: vec![definition.clone()],
    };
    let preparation = ReadPreparation(ToolCatalog::new([definition]).unwrap());
    let mut safety = AllowReadSafety(requirements);
    let mut sandbox = ReadSandbox;
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path: path.clone(),
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.completed", "turn.completed"],
    };
    let result = block_on(execute_durable_agent_with_f0(
        &mut ledger,
        &config,
        &request,
        &capabilities,
        &mut context,
        &model,
        &mut authority,
        &mut executor,
        F0ExecutionGovernance {
            preparation: &preparation,
            safety: &mut safety,
            sandbox: &mut sandbox,
            context: F0GovernanceContext {
                actor_authority_reference: "actor".into(),
                goal_reference: None,
                plan_reference: None,
                effective_policy_revision: "policy-1".into(),
            },
        },
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert!(matches!(
        result.report.outcome,
        AgentOutcome::Completed { .. }
    ));
    assert_eq!(context.positions, [4, 17]);
    assert_eq!(publisher.calls, 1);
    let kinds = ledger
        .load_turn(&plan.turn_id)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            "turn.started",
            "turn.input",
            "execution.started",
            "execution.iteration_started",
            "model.prepared",
            "model.started",
            "model.completed",
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.observation",
            "execution.iteration_started",
            "model.prepared",
            "model.started",
            "model.completed",
            "execution.completed",
            "turn.completed",
        ]
    );
}

#[test]
fn model_preflight_failure_precedes_every_model_lifecycle_fact() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("preflight.sqlite3");
    let session = SessionId::try_from("preflight-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("preflight-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger
        .commit(session.clone(), 1, plan.facts.clone())
        .unwrap();
    let request = core_request(&session, &plan.turn_id, &execution);
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution,
            owner_id: "preflight-worker".into(),
            lease_token: "preflight-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path,
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.failed", "turn.failed"],
    };
    let result = block_on(execute_durable_model_only(
        &mut ledger,
        &config,
        &request,
        &mut context,
        &RejectPreflight,
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert!(matches!(
        result.report.outcome,
        AgentOutcome::Failed {
            reason: garive_core::AgentFailureReason::RequiredCapabilityUnavailable
        }
    ));
    let kinds = ledger
        .load_turn(&plan.turn_id)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert!(!kinds.iter().any(|kind| kind.starts_with("model.")));
    assert_eq!(
        &kinds[kinds.len() - 2..],
        ["execution.failed", "turn.failed"]
    );
}

#[test]
fn durable_cancel_request_reaches_the_frozen_core_signal() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("cancel.sqlite3");
    let session = SessionId::try_from("cancel-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("cancel-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger
        .commit(session.clone(), 1, plan.facts.clone())
        .unwrap();
    let request = core_request(&session, &plan.turn_id, &execution);
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution,
            owner_id: "cancel-worker".into(),
            lease_token: "cancel-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let model = CancelDuringModel {
        path: path.clone(),
        session,
        turn: plan.turn_id.clone(),
    };
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path,
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.stopped", "turn.stopped"],
    };
    let result = block_on(execute_durable_model_only(
        &mut ledger,
        &config,
        &request,
        &mut context,
        &model,
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert!(matches!(
        result.report.outcome,
        AgentOutcome::Stopped {
            reason: garive_core::StopReason::Cancelled
        }
    ));
    let kinds = ledger
        .load_turn(&plan.turn_id)
        .unwrap()
        .facts
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert!(kinds
        .windows(2)
        .any(|pair| pair == ["model.started", "turn.cancel_requested"]));
    assert_eq!(
        &kinds[kinds.len() - 2..],
        ["execution.stopped", "turn.stopped"]
    );
}

#[test]
fn skill_activation_commits_before_model_and_replays_exactly_after_restart() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("skill.sqlite3");
    let session = SessionId::try_from("skill-session").unwrap();
    let mut ledger = SqliteLedger::open(&path).unwrap();
    ledger
        .commit(session.clone(), 0, vec![open_session()])
        .unwrap();
    let start = StartTurnCommand {
        command_id: RuntimeCommandId::new("skill-start").unwrap(),
        session_id: session.clone(),
        agent_instance_id: LedgerAgentId::try_from("agent").unwrap(),
        definition_id: LedgerDefinitionId::try_from("definition").unwrap(),
        definition_revision: LedgerRevision::try_from("revision").unwrap(),
        snapshot_digest: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        trusted_input: "hello".into(),
        limits: EffectiveRuntimeLimits {
            max_iterations: 2,
            max_input_tokens: None,
            max_output_tokens: None,
            deadline_budget_ms: None,
        },
        recorded_at: "2026-08-29T00:00:00Z".into(),
    };
    let plan = plan_start_turn(&start, 1).unwrap();
    let execution = plan.execution_id.clone().unwrap();
    ledger.commit(session.clone(), 1, plan.facts).unwrap();
    let mut request = core_request(&session, &plan.turn_id, &execution);
    request.context_request.max_utf8_bytes = 4_096;
    let activation_request = SkillActivationRequest::new(
        "activation-1",
        "turn",
        "execution",
        1,
        ActivationMode::Explicit,
        Some("review".into()),
        vec![],
        request.context_request.through_position,
        1,
        64,
    )
    .unwrap();
    let definition = skill_definition("Check facts.");
    let activation = plan_skill_activation(
        &SkillActivationContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        std::slice::from_ref(&definition),
        &BTreeSet::new(),
        &BTreeSet::new(),
        &activation_request,
    )
    .unwrap();
    let replay_fact = activation.fact.clone();
    let evidence_fact = ledger.read_facts(&session, 2, 3, None).unwrap().remove(0);
    let scope = MemoryScope::session(session.as_str()).unwrap();
    let content = MemoryContent::from_inline("remember");
    let record = MemoryRecord::new(
        "record",
        "revision",
        "namespace",
        scope.clone(),
        MemoryKind::LearnedFact,
        content.clone(),
        vec![DurableFactReference::new(
            session.as_str(),
            evidence_fact.position,
            evidence_fact.fact_id.as_str(),
            evidence_fact.payload.sha256(),
        )
        .unwrap()],
        MemoryStatus::Active,
        MemorySensitivity::Ordinary,
        8000,
        4,
        None,
        None,
    )
    .unwrap();
    let query = MemoryQuery::new(
        "query",
        "namespace",
        vec![scope],
        MemoryPurpose::Context,
        "retriever-1",
        MemoryContent::from_inline("remember"),
        4,
        "2026-08-29T00:00:01Z",
        1,
        64,
        false,
        None,
    )
    .unwrap();
    let retrieval = plan_memory_retrieval(
        &MemoryRetrievalContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        &[record],
        &[MemoryScore::new("record", "revision", 9000, 8).unwrap()],
        &query,
    )
    .unwrap();
    let replay_memory_fact = retrieval.fact.clone();
    let knowledge_request = KnowledgeRequest::new(
        "knowledge-request",
        "docs",
        "1",
        KnowledgeQueryMode::Keyword,
        KnowledgeContent::from_inline("garive"),
        vec![],
        4,
        1,
        64,
        1_000,
        FreshnessRequirement::CachedAllowed,
    )
    .unwrap();
    let knowledge_source = knowledge_source();
    let knowledge = PreparedKnowledgeCapability::new(
        knowledge_source,
        knowledge_request,
        KnowledgeAccessGrant::new("docs", "1").unwrap(),
        "knowledge-attempt",
        Arc::new(CheckingKnowledgeConnector {
            path: path.clone(),
            turn: plan.turn_id.clone(),
        }),
    )
    .unwrap();
    let config = DurableExecutionConfig {
        session_id: session.clone(),
        expected_session_version: 2,
        model: ModelLifecycleContext {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            deployment_id: "deployment".into(),
            recovery_policy_revision: "policy".into(),
            max_attempts: 1,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        lease: ExecutionLeaseRequest {
            turn_id: plan.turn_id.clone(),
            execution_id: execution.clone(),
            owner_id: "skill-worker".into(),
            lease_token: "skill-lease".into(),
            now_ms: 1,
            duration_ms: 10_000,
        },
    };
    let mut context = Context { positions: vec![] };
    let signals = Signals;
    let mut events = Signals;
    let mut publisher = Publisher {
        path: path.clone(),
        turn: plan.turn_id.clone(),
        fail: false,
        calls: 0,
        expected_terminal: ["execution.completed", "turn.completed"],
    };
    block_on(execute_durable_model_only_with_capabilities(
        &mut ledger,
        &config,
        &request,
        PreparedAgentCapabilities {
            skill_activation: Some(activation),
            memory_retrieval: Some(retrieval),
            knowledge_retrieval: Some(knowledge),
        },
        &mut context,
        &SkillCheckingModel {
            path: path.clone(),
            turn: plan.turn_id.clone(),
        },
        &mut events,
        &signals,
        &signals,
        &mut publisher,
    ))
    .unwrap();
    assert_eq!(context.positions, [9]);

    drop(ledger);
    let mut restarted = SqliteLedger::open(&path).unwrap();
    let replay = restarted
        .commit(session.clone(), 0, vec![replay_fact])
        .unwrap();
    assert_eq!(replay.disposition, CommitDisposition::Replayed);
    let replay_memory = restarted
        .commit(session.clone(), 0, vec![replay_memory_fact])
        .unwrap();
    assert_eq!(replay_memory.disposition, CommitDisposition::Replayed);
    let changed = plan_skill_activation(
        &SkillActivationContext {
            turn_id: plan.turn_id,
            execution_id: execution,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        &[skill_definition("Changed instructions.")],
        &BTreeSet::new(),
        &BTreeSet::new(),
        &activation_request,
    )
    .unwrap();
    assert!(matches!(
        restarted.commit(session, replay.session_version, vec![changed.fact]),
        Err(garive_runtime::SqliteLedgerError::Domain(
            LedgerError::IdempotencyCollision
        ))
    ));
}

fn skill_definition(instructions: &str) -> SkillDefinition {
    SkillDefinition::new(
        "review",
        "1",
        "Review",
        "Review exact facts.",
        ContentBinding::from_inline(instructions),
        ActivationPolicy::ExplicitOnly,
        vec![],
        vec![],
        64,
        "1",
    )
    .unwrap()
}

fn knowledge_source() -> KnowledgeSourceDescriptor {
    KnowledgeSourceDescriptor::new(
        "docs",
        "1",
        KnowledgeSourceKind::Documentation,
        "product-docs",
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword],
        "a".repeat(64),
        CitationScheme::UriFragment,
        "b".repeat(64),
    )
    .unwrap()
}

fn knowledge_evidence() -> KnowledgeEvidence {
    let content = KnowledgeContent::from_inline("knowledge");
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
            Some("https://example.test/docs#intro".into()),
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

fn core_request(
    session: &SessionId,
    turn: &garive_ledger::TurnId,
    execution: &garive_ledger::ExecutionId,
) -> AgentTurnRequest {
    AgentTurnRequest {
        session_id: CoreSessionId::try_from(session.as_str()).unwrap(),
        turn_id: CoreTurnId::try_from(turn.as_str()).unwrap(),
        execution_id: CoreExecutionId::try_from(execution.as_str()).unwrap(),
        agent_instance_id: AgentInstanceId::try_from("agent").unwrap(),
        definition_id: AgentDefinitionId::try_from("definition").unwrap(),
        definition_revision: AgentDefinitionRevision::try_from("revision").unwrap(),
        entry: AgentEntry::Start {
            trusted_input: "hello".into(),
        },
        cursor: AgentCursor {
            completed_iterations: 0,
            last_durable_position: 0,
        },
        context_request: ContextRequest {
            session_id: session.as_str().into(),
            turn_id: turn.as_str().into(),
            purpose: ContextPurpose::Inference,
            after_position: None,
            through_position: 4,
            max_items: 10,
            max_utf8_bytes: 100,
        },
        activated_skills: vec![],
        capability_context_candidates: vec![],
        model_targets: vec![ModelTargetId::new("target")],
        required_capabilities: vec![ModelCapability::Text],
        model_output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        recovery_policy: ModelRecoveryPolicy {
            max_context_rebuilds: 0,
            output_limit: OutputLimitAction::Suspend,
            transport: TerminalRecoveryAction::Suspend,
            unavailable: TerminalRecoveryAction::Suspend,
            missing_usage: MissingUsagePolicy::Stop,
        },
        limits: ModelOnlyLimits {
            execution: ExecutionLimits::new(NonZeroU32::new(2).unwrap()),
            max_total_tokens: Some(10),
            deadline_tick: None,
        },
    }
}
