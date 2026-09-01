#[cfg(unix)]
use std::{fs, os::unix::fs::PermissionsExt};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use garive_core::{
    AgentOutcome, ExecutionReport, MissingUsagePolicy, ModelRecoveryPolicy, OutputLimitAction,
    TerminalRecoveryAction, UsageSummary,
};
use garive_desktop::{
    builtin_desktop_agent_installation, builtin_desktop_workspace_agent_installation, DesktopHost,
    DesktopHostConfig, DesktopOperations, DesktopState, DesktopTerminal,
    DesktopWorkspaceContextFile, DesktopWorkspaceExecutionFactory, DesktopWorkspaceGrant,
    DesktopWorkspaceService, DESKTOP_KNOWLEDGE_CAPABILITY_NAME,
    DESKTOP_KNOWLEDGE_CAPABILITY_REVISION, DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST,
    DESKTOP_MEMORY_CAPABILITY_NAME, DESKTOP_MEMORY_CAPABILITY_REVISION,
    DESKTOP_MEMORY_DESCRIPTOR_DIGEST,
};
use garive_goal::{
    GoalBoundsV1, GoalCriterion, GoalCriterionId, GoalDefinitionV1, GoalId, GoalScopeV1, GoalState,
};
use garive_knowledge::{
    CitationScheme, KnowledgeQueryMode, KnowledgeSourceDescriptor, KnowledgeSourceKind,
    KnowledgeTrustClass,
};
use garive_ledger::SessionId;
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelCapability, ModelFuture,
    ModelInputItem, ModelItem, ModelObserver, ModelOutputKind, ModelOutputSettings, ModelPort,
    ModelRequest, ModelStopReason, ModelStreamEvent, ModelUsage, ObserverDecision, RejectionKind,
    TextMode, TokenCount, UsageSource,
};
use garive_memory::{MemoryAuthorizedScope, MemoryDocumentLimits, MemoryScope, MemoryScopeClass};
use garive_plan::{PlanBoundsV1, PlanStepId, PlanStepV1};
use garive_runtime::{
    commit_goal_command, plan_core_terminal, plan_create_goal, reconstruct_goal,
    start_initial_goal_plan_proposal_execution, CatalogueCapabilityPreparationFactory,
    CataloguePlanStepDispatchFactory, CommittedTurn, CoreTerminalContext, GoalCommandContext,
    HostClock, KnowledgeConnector, KnowledgeConnectorFuture, KnowledgeConnectorOutcome,
    LiveHostLimits, LiveOutputEventKind, LocalCapabilityPreparationFactory, LocalExecutionAttempt,
    LocalExecutionPolicy, LocalGovernedExecution, LocalGovernedExecutionFactory,
    LocalKnowledgeSystemBinding, LocalMemorySystemBinding, LocalWorkerError, MemoryControlAction,
    MemoryControlGrant, PlanAdmissionDecision, PlanAdmissionInput, PlanAdmissionPolicy,
    PlanFailureDecision, PlanFailureInput, PlanFailurePolicy, PlanProposalContent,
    PlanProposalFuture, PlanProposalPort, PlanStepDispatchFactory, PlanStepDispatchInput,
    ProcessBackendHostConfig, ProcessExecutable, ProcessLane, ProcessLaneRegistry,
    RuntimeAgentCatalogue, SafetyFuture, SafetyPort, SqliteLedger, T1HostSystemConfig,
    KEYWORD_CURRENT_INPUT_REVISION, USER_DECLARED_PUSH_REVISION,
};
use garive_tools::{T1_APPLY_PATCH, T1_READ_TEXT};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

struct FixedHostClock;
impl HostClock for FixedHostClock {
    fn recorded_at(&self) -> String {
        "2026-08-29T00:00:00Z".into()
    }
}

struct EmptyKnowledgeConnector;
impl KnowledgeConnector for EmptyKnowledgeConnector {
    fn retrieve<'a>(
        &'a self,
        _: &'a KnowledgeSourceDescriptor,
        _: &'a garive_knowledge::KnowledgeRequest,
    ) -> KnowledgeConnectorFuture<'a> {
        Box::pin(async {
            KnowledgeConnectorOutcome::Completed {
                evidence: Vec::new(),
                connector_order_stable: true,
            }
        })
    }
}

fn test_capability_preparation(
    catalogue: Arc<RuntimeAgentCatalogue>,
) -> Arc<dyn LocalCapabilityPreparationFactory> {
    let namespace = "desktop-test-memory";
    let memory = LocalMemorySystemBinding::new(
        DESKTOP_MEMORY_CAPABILITY_NAME,
        DESKTOP_MEMORY_CAPABILITY_REVISION,
        DESKTOP_MEMORY_DESCRIPTOR_DIGEST,
        namespace,
        "desktop-test-retriever-v1",
        USER_DECLARED_PUSH_REVISION,
        MemoryControlGrant::new(
            namespace,
            [MemoryControlAction::Export],
            [MemoryAuthorizedScope {
                scope: MemoryScopeClass::User,
                owner_id: "desktop-test-user".into(),
            }],
        )
        .unwrap(),
        vec![MemoryScope::Namespace],
        MemoryDocumentLimits::new(16_384, 8_192, 128).unwrap(),
        8,
        16_384,
        128,
        2_048,
    )
    .unwrap();
    let source = KnowledgeSourceDescriptor::new(
        "desktop-test-knowledge",
        "desktop-test-knowledge-v1",
        KnowledgeSourceKind::Documentation,
        "desktop.test.knowledge",
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword],
        "a".repeat(64),
        CitationScheme::RecordKey,
        "b".repeat(64),
    )
    .unwrap();
    let knowledge = LocalKnowledgeSystemBinding::new(
        DESKTOP_KNOWLEDGE_CAPABILITY_NAME,
        DESKTOP_KNOWLEDGE_CAPABILITY_REVISION,
        DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST,
        source,
        KEYWORD_CURRENT_INPUT_REVISION,
        4,
        16_384,
        1_000,
        Arc::new(EmptyKnowledgeConnector),
    )
    .unwrap();
    Arc::new(
        CatalogueCapabilityPreparationFactory::new(catalogue, Some(memory))
            .with_knowledge(knowledge),
    )
}

struct Operations(AtomicU64);
impl DesktopOperations for Operations {
    fn command_id(
        &self,
        purpose: &'static str,
    ) -> Result<String, garive_desktop::DesktopHostError> {
        Ok(format!(
            "desktop-{purpose}-{}",
            self.0.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn execution_attempt(&self) -> Result<LocalExecutionAttempt, garive_desktop::DesktopHostError> {
        let ordinal = self.0.fetch_add(1, Ordering::SeqCst);
        Ok(LocalExecutionAttempt {
            worker_owner_id: "desktop-worker".into(),
            lease_token: format!("unpredictable-test-token-{ordinal}"),
            now_ms: 1_000 + ordinal,
            clock_revision: "test-monotonic-v1".into(),
            lease_duration_ms: 5_000,
            recorded_at: "2026-08-29T00:00:01Z".into(),
        })
    }
}

struct CompletingModel;
impl ModelPort for CompletingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.target_id.as_str(), "desktop-target");
            assert_eq!(
                observer.observe(&ModelStreamEvent::OutputItemStarted {
                    output_index: 0,
                    kind: ModelOutputKind::Text,
                }),
                ObserverDecision::Continue
            );
            for delta in ["desktop ", "durable answer"] {
                assert_eq!(
                    observer.observe(&ModelStreamEvent::TextDelta {
                        output_index: 0,
                        delta: delta.into(),
                    }),
                    ObserverDecision::Continue
                );
            }
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text {
                    text: "desktop durable answer".into(),
                }],
                usage: ModelUsage {
                    input_tokens: TokenCount::Known(3),
                    output_tokens: TokenCount::Known(4),
                    cache_read_tokens: None,
                    cache_write_tokens: None,
                    source: UsageSource::ProviderReported,
                },
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

#[test]
fn catalogue_plan_preparation_resolves_only_the_session_installation() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("plan-preparation.sqlite3");
    let config = desktop_host_config(&database, Arc::new(CompletingModel));
    let catalogue = config.agent_catalogue.clone();
    let governed = DesktopWorkspaceExecutionFactory::new(
        database.clone(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    let host = DesktopHost::new_governed(config, Arc::new(governed)).unwrap();
    let session = host.create_session("definition-main").unwrap();
    let session = SessionId::try_from(session.as_str()).unwrap();
    let installation = catalogue.get("definition-main").unwrap();
    let snapshot = installation.snapshot();
    let step_id = PlanStepId::new("prepare").unwrap();
    let mut factory = CataloguePlanStepDispatchFactory::new(database, catalogue.clone());
    let prepared = factory
        .prepare(PlanStepDispatchInput {
            session_id: &session,
            goal_id: "goal-1",
            plan_id: "plan-1",
            plan_revision: 1,
            step_id: &step_id,
            objective: "Prepare",
            agent_snapshot_digest: snapshot.snapshot_digest(),
            tool_catalogue_digest: installation.tool_catalogue_digest(),
            safety_policy_revision: &snapshot.governance().exact_revision,
            through_position: 1,
            start_command_id: "start-1",
            recorded_at: "2026-08-29T00:00:01Z",
        })
        .unwrap();
    assert_eq!(prepared.definition_id.as_str(), "definition-main");
    assert_eq!(
        prepared.installed_tool_catalogue_digest,
        installation.tool_catalogue_digest()
    );
    assert_eq!(
        prepared.installed_safety_policy_revision,
        snapshot.governance().exact_revision
    );

    assert!(factory
        .prepare(PlanStepDispatchInput {
            session_id: &session,
            goal_id: "goal-1",
            plan_id: "plan-1",
            plan_revision: 1,
            step_id: &step_id,
            objective: "Prepare",
            agent_snapshot_digest: snapshot.snapshot_digest(),
            tool_catalogue_digest: &"f".repeat(64),
            safety_policy_revision: &snapshot.governance().exact_revision,
            through_position: 1,
            start_command_id: "start-2",
            recorded_at: "2026-08-29T00:00:02Z",
        })
        .is_err());
}

#[tokio::test]
async fn desktop_goal_pump_activates_dispatches_and_closes_one_plan() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("goal-pump.sqlite3");
    let mut config = desktop_host_config(&database, Arc::new(CompletingModel));
    config.plan_admission_policy = Some(Arc::new(AdoptExactProposal));
    config.plan_proposal_port = Some(Arc::new(SingleStepProposal));
    let governed = DesktopWorkspaceExecutionFactory::new(
        database.clone(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    let host = DesktopHost::new_governed(config, Arc::new(governed)).unwrap();
    let session_id = host.create_session("definition-main").unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let opened = ledger.read_facts(&session, 0, 1, None).unwrap().remove(0);
    let criterion_id = GoalCriterionId::new("session-opened").unwrap();
    let goal = GoalDefinitionV1::new(
        GoalId::new("goal-pump").unwrap(),
        "Complete one installed Plan",
        vec![GoalCriterion::DurableFact {
            criterion_id: criterion_id.clone(),
            fact_kind: "session.opened".into(),
            subject_digest: opened.payload.sha256().into(),
        }],
        GoalScopeV1::new(Some(session_id.clone()), []).unwrap(),
        GoalBoundsV1::new(1, 1, 1, None, None).unwrap(),
        None,
        [],
    )
    .unwrap();
    let created = plan_create_goal(
        &ledger,
        &session,
        &GoalCommandContext {
            command_id: "goal-pump-create".into(),
            actor_reference: "user:test".into(),
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        goal,
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    drop(ledger);

    let proposal = host.drive_goal(&session_id, "goal-pump", 1).await.unwrap();
    assert_eq!(proposal.executions, 0);
    assert!(proposal.exhausted);
    let report = host.drive_goal(&session_id, "goal-pump", 8).await.unwrap();
    assert_eq!(report.executions, 1);
    assert!(!report.exhausted);
    let ledger = SqliteLedger::open(database).unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-pump")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
    );
}

#[tokio::test]
async fn desktop_goal_pump_runs_durable_model_planner_before_step_worker() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("model-planner.sqlite3");
    let mut config = desktop_host_config(
        &database,
        Arc::new(PlanningThenCompletingModel(AtomicU64::new(0))),
    );
    config.plan_admission_policy = Some(Arc::new(AdoptExactProposal));
    config.model_plan_proposer_reference = Some("planner:model-test-v1".into());
    let governed = DesktopWorkspaceExecutionFactory::new(
        database.clone(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    let host = DesktopHost::new_governed(config, Arc::new(governed)).unwrap();
    let session_id = host.create_session("definition-main").unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let opened = ledger.read_facts(&session, 0, 1, None).unwrap().remove(0);
    let goal = GoalDefinitionV1::new(
        GoalId::new("goal-pump").unwrap(),
        "Complete one installed Plan",
        vec![GoalCriterion::DurableFact {
            criterion_id: GoalCriterionId::new("session-opened").unwrap(),
            fact_kind: "session.opened".into(),
            subject_digest: opened.payload.sha256().into(),
        }],
        GoalScopeV1::new(Some(session_id.clone()), []).unwrap(),
        GoalBoundsV1::new(1, 1, 1, None, None).unwrap(),
        None,
        [],
    )
    .unwrap();
    let created = plan_create_goal(
        &ledger,
        &session,
        &GoalCommandContext {
            command_id: "model-planner-goal".into(),
            actor_reference: "user:test".into(),
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        goal,
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    drop(ledger);

    let report = host.drive_goal(&session_id, "goal-pump", 12).await.unwrap();
    assert_eq!(report.executions, 2);
    let ledger = SqliteLedger::open(database).unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-pump")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
    );
    let kinds = ledger
        .read_facts(&session, 0, u64::MAX, None)
        .unwrap()
        .into_iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    assert!(kinds
        .windows(2)
        .any(|pair| pair == ["plan.proposal.result_bound", "plan.proposed"]));
}

#[tokio::test]
async fn desktop_goal_pump_recovers_terminal_planner_without_second_model_call() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("model-planner-crash.sqlite3");
    let catalogue = Arc::new(
        RuntimeAgentCatalogue::new([builtin_desktop_agent_installation(
            "definition-main",
            "desktop-main",
        )
        .unwrap()])
        .unwrap(),
    );
    let mut original_config = desktop_host_config(&database, Arc::new(CompletingModel));
    original_config.capability_preparation = test_capability_preparation(catalogue.clone());
    original_config.agent_catalogue = catalogue.clone();
    original_config.model_plan_proposer_reference = Some("planner:model-test-v1".into());
    let original = DesktopHost::new_governed(
        original_config,
        Arc::new(
            DesktopWorkspaceExecutionFactory::new(
                database.clone(),
                DesktopWorkspaceService::default(),
                "main",
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let session_id = original.create_session("definition-main").unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let opened = ledger.read_facts(&session, 0, 1, None).unwrap().remove(0);
    let goal = GoalDefinitionV1::new(
        GoalId::new("goal-pump").unwrap(),
        "Complete one installed Plan",
        vec![GoalCriterion::DurableFact {
            criterion_id: GoalCriterionId::new("session-opened").unwrap(),
            fact_kind: "session.opened".into(),
            subject_digest: opened.payload.sha256().into(),
        }],
        GoalScopeV1::new(Some(session_id.clone()), []).unwrap(),
        GoalBoundsV1::new(1, 1, 1, None, None).unwrap(),
        None,
        [],
    )
    .unwrap();
    let created = plan_create_goal(
        &ledger,
        &session,
        &GoalCommandContext {
            command_id: "model-planner-crash-goal".into(),
            actor_reference: "user:test".into(),
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        goal,
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    drop(ledger);
    let committed = start_initial_goal_plan_proposal_execution(
        &database,
        &session,
        "goal-pump",
        "planner:model-test-v1",
        "2026-08-29T00:00:01Z",
        catalogue,
    )
    .unwrap();
    let usage = UsageSummary {
        input_tokens: TokenCount::Known(3),
        output_tokens: TokenCount::Known(4),
        estimated: false,
    };
    let terminal = plan_core_terminal(
        &CoreTerminalContext {
            turn_id: committed.turn_id.clone(),
            execution_id: committed.execution_id.clone(),
            recorded_at: "2026-08-29T00:00:01Z".into(),
        },
        &ExecutionReport {
            outcome: AgentOutcome::Completed {
                response_items: vec![ModelItem::Text {
                    text: model_plan_topology(),
                }],
                usage,
            },
            completed_iterations: 1,
            usage,
        },
    )
    .unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let version = ledger.session_version(&session).unwrap().unwrap();
    ledger.commit(session.clone(), version, terminal).unwrap();
    assert!(!ledger
        .read_facts(&session, 0, u64::MAX, None)
        .unwrap()
        .iter()
        .any(|fact| fact.kind.as_str() == "plan.proposal.result_bound"));
    drop(ledger);
    drop(original);

    let mut restarted_config = desktop_host_config(&database, Arc::new(CompletingModel));
    restarted_config.plan_admission_policy = Some(Arc::new(AdoptExactProposal));
    restarted_config.model_plan_proposer_reference = Some("planner:model-test-v1".into());
    let restarted = DesktopHost::new_governed(
        restarted_config,
        Arc::new(
            DesktopWorkspaceExecutionFactory::new(
                database.clone(),
                DesktopWorkspaceService::default(),
                "main",
            )
            .unwrap(),
        ),
    )
    .unwrap();
    let report = restarted
        .drive_goal(&session_id, "goal-pump", 12)
        .await
        .unwrap();
    assert_eq!(report.executions, 2);
    let ledger = SqliteLedger::open(database).unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-pump")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Succeeded
    );
}

#[tokio::test]
async fn desktop_goal_pump_requires_policy_before_failed_plan_closes_goal() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("goal-failure.sqlite3");
    let mut config = desktop_host_config(&database, Arc::new(RejectingModel));
    config.plan_admission_policy = Some(Arc::new(AdoptExactProposal));
    config.plan_failure_policy = Some(Arc::new(FailExhaustedPlan));
    config.plan_proposal_port = Some(Arc::new(SingleStepProposal));
    let governed = DesktopWorkspaceExecutionFactory::new(
        database.clone(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    let host = DesktopHost::new_governed(config, Arc::new(governed)).unwrap();
    let session_id = host.create_session("definition-main").unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    let opened = ledger.read_facts(&session, 0, 1, None).unwrap().remove(0);
    let goal = GoalDefinitionV1::new(
        GoalId::new("goal-pump").unwrap(),
        "Complete one installed Plan",
        vec![GoalCriterion::DurableFact {
            criterion_id: GoalCriterionId::new("session-opened").unwrap(),
            fact_kind: "session.opened".into(),
            subject_digest: opened.payload.sha256().into(),
        }],
        GoalScopeV1::new(Some(session_id.clone()), []).unwrap(),
        GoalBoundsV1::new(1, 1, 1, None, None).unwrap(),
        None,
        [],
    )
    .unwrap();
    let created = plan_create_goal(
        &ledger,
        &session,
        &GoalCommandContext {
            command_id: "goal-failure-create".into(),
            actor_reference: "user:test".into(),
            recorded_at: "2026-08-29T00:00:00Z".into(),
        },
        goal,
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &created).unwrap();
    drop(ledger);

    host.drive_goal(&session_id, "goal-pump", 1).await.unwrap();
    let report = host.drive_goal(&session_id, "goal-pump", 12).await.unwrap();
    assert_eq!(report.executions, 1);
    assert!(!report.exhausted);
    let ledger = SqliteLedger::open(database).unwrap();
    assert_eq!(
        reconstruct_goal(&ledger, &session, "goal-pump")
            .unwrap()
            .snapshot
            .state(),
        GoalState::Failed
    );
    let failure = ledger
        .read_facts(&session, 0, u64::MAX, None)
        .unwrap()
        .into_iter()
        .find(|fact| fact.kind.as_str() == "plan.failed")
        .unwrap();
    assert!(failure.payload.as_json().contains("failure-policy:test-v1"));
}

struct AdoptExactProposal;
impl PlanAdmissionPolicy for AdoptExactProposal {
    fn decide(&self, input: &PlanAdmissionInput) -> PlanAdmissionDecision {
        assert_eq!(input.goal_id, "goal-pump");
        assert_eq!(input.goal_revision, 1);
        assert!(input.plan_id.starts_with("g2-plan-"));
        assert_eq!(input.plan_revision, 1);
        PlanAdmissionDecision::Adopt {
            policy_reference: "policy:test-v1".into(),
        }
    }
}

struct FailExhaustedPlan;
impl PlanFailurePolicy for FailExhaustedPlan {
    fn decide(&self, input: &PlanFailureInput) -> PlanFailureDecision {
        assert_eq!(input.goal_id, "goal-pump");
        assert_eq!(input.plan_revision, 1);
        assert_eq!(input.failed_step_ids, ["complete"]);
        PlanFailureDecision::Fail {
            policy_reference: "failure-policy:test-v1".into(),
            reason: "attempts_exhausted".into(),
        }
    }
}

struct SingleStepProposal;
impl PlanProposalPort for SingleStepProposal {
    fn proposer_reference(&self) -> &str {
        "planner:test-v1"
    }

    fn propose<'a>(
        &'a self,
        request: &'a garive_runtime::PlanProposalRequest,
    ) -> PlanProposalFuture<'a> {
        Box::pin(async move {
            assert_eq!(request.goal_id, "goal-pump");
            assert_eq!(request.goal_revision, 1);
            assert_eq!(request.objective, "Complete one installed Plan");
            Ok(PlanProposalContent {
                steps: vec![PlanStepV1::new(
                    PlanStepId::new("complete").unwrap(),
                    request.objective.clone(),
                    [],
                    request.criterion_ids.iter().cloned(),
                    [],
                    Vec::<String>::new(),
                    1,
                )
                .unwrap()],
                bounds: PlanBoundsV1::new(1, 1, 1, None, None).unwrap(),
            })
        })
    }
}

struct RejectingModel;
impl ModelPort for RejectingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async {
            Ok(InvokeOutcome::Rejected {
                kind: RejectionKind::ContentPolicy,
                sanitized_evidence: "planner-test-rejection".into(),
            })
        })
    }
}

struct PlanningThenCompletingModel(AtomicU64);
impl ModelPort for PlanningThenCompletingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.0.fetch_add(1, Ordering::SeqCst);
            let text = if call == 0 {
                assert!(request.tools.is_empty());
                assert!(matches!(
                    request.output.text_mode,
                    TextMode::JsonSchema { .. }
                ));
                model_plan_topology()
            } else {
                "completed".into()
            };
            Ok(InvokeOutcome::Completed {
                items: vec![ModelItem::Text { text }],
                usage: usage(),
                stop_reason: ModelStopReason::EndTurn,
            })
        })
    }
}

fn model_plan_topology() -> String {
    serde_jcs::to_string(&serde_json::json!({
        "contract":"garive.plan-proposal-topology", "version":1,
        "steps":[{"step_id":"complete","objective":"Complete one installed Plan",
            "depends_on":[],"completion_criteria":["session-opened"],
            "required_capabilities":[],"input_bindings":[],"max_attempts":1}],
        "bounds":{"max_steps":1,"max_parallel_ready":1,"max_total_attempts":1,
            "token_budget":null,"duration_budget_ms":null}
    }))
    .unwrap()
}

struct UnavailableSafety;

impl SafetyPort for UnavailableSafety {
    fn decide<'a>(&'a mut self, _: &'a garive_runtime::SafetyRequestV1) -> SafetyFuture<'a> {
        Box::pin(async { Err(garive_runtime::GovernedRuntimePortError::AuthorityUnavailable) })
    }
}

struct SafetyUnavailableFactory(DesktopWorkspaceExecutionFactory);

impl LocalGovernedExecutionFactory for SafetyUnavailableFactory {
    fn create(
        &self,
        committed: &CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        let mut execution = self.0.create(committed)?;
        execution.f0.safety = Box::new(UnavailableSafety);
        Ok(execution)
    }
}

struct MismatchedFactory(DesktopWorkspaceExecutionFactory);

impl LocalGovernedExecutionFactory for MismatchedFactory {
    fn create(
        &self,
        committed: &CommittedTurn,
    ) -> Result<LocalGovernedExecution, LocalWorkerError> {
        let mut execution = self.0.create(committed)?;
        execution.capabilities.definitions.clear();
        Ok(execution)
    }
}

struct SuspendingModel(AtomicU64);
impl ModelPort for SuspendingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                Ok(InvokeOutcome::Interrupted {
                    kind: InterruptionKind::OutputLimit,
                    partial_items: vec![ModelItem::Text {
                        text: "partial".into(),
                    }],
                    usage: usage(),
                })
            } else {
                Ok(InvokeOutcome::Completed {
                    items: vec![ModelItem::Text {
                        text: "resumed answer".into(),
                    }],
                    usage: usage(),
                    stop_reason: ModelStopReason::EndTurn,
                })
            }
        })
    }
}

struct WorkspaceWritingModel {
    calls: AtomicU64,
    arguments: String,
}

struct WorkspaceReadingModel(AtomicU64);

impl ModelPort for WorkspaceReadingModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.0.fetch_add(1, Ordering::SeqCst);
            if call > 0 {
                assert!(request.input_items.iter().any(|item| matches!(
                    item,
                    ModelInputItem::ToolObservation { model_call_id, .. }
                        if model_call_id == "read-call-1"
                )));
            }
            Ok(InvokeOutcome::Completed {
                items: if call == 0 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "read-call-1".into(),
                        tool_name: T1_READ_TEXT.into(),
                        arguments_json: serde_json::json!({
                            "path":"note.txt",
                            "max_bytes":4096
                        })
                        .to_string(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "workspace read completed".into(),
                    }]
                },
                usage: usage(),
                stop_reason: if call == 0 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

struct WorkspacePatchingModel {
    calls: AtomicU64,
    arguments: String,
}

impl ModelPort for WorkspacePatchingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(InvokeOutcome::Completed {
                items: if call < 2 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: "patch-call-1".into(),
                        tool_name: T1_APPLY_PATCH.into(),
                        arguments_json: self.arguments.clone(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "workspace patch completed".into(),
                    }]
                },
                usage: usage(),
                stop_reason: if call < 2 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

impl ModelPort for WorkspaceWritingModel {
    fn invoke<'a>(
        &'a self,
        _: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(InvokeOutcome::Completed {
                items: if call < 2 {
                    vec![ModelItem::ToolIntent {
                        model_call_id: format!("write-call-{call}"),
                        tool_name: "write_file".into(),
                        arguments_json: self.arguments.clone(),
                    }]
                } else {
                    vec![ModelItem::Text {
                        text: "artifact committed".into(),
                    }]
                },
                usage: usage(),
                stop_reason: if call < 2 {
                    ModelStopReason::ToolUse
                } else {
                    ModelStopReason::EndTurn
                },
            })
        })
    }
}

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(3),
        output_tokens: TokenCount::Known(4),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn desktop_host(database: &Path, model: Arc<dyn ModelPort>) -> DesktopHost {
    desktop_host_with_ordinal(database, model, 1)
}

fn desktop_host_with_ordinal(
    database: &Path,
    model: Arc<dyn ModelPort>,
    first_operation: u64,
) -> DesktopHost {
    let mut config = desktop_host_config(database, model);
    config.operations = Arc::new(Operations(AtomicU64::new(first_operation)));
    let factory = DesktopWorkspaceExecutionFactory::new(
        database.to_owned(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    DesktopHost::new_governed(config, Arc::new(factory)).expect("Desktop Host composition")
}

fn desktop_host_config(database: &Path, model: Arc<dyn ModelPort>) -> DesktopHostConfig {
    let agent_catalogue = Arc::new(
        RuntimeAgentCatalogue::new([builtin_desktop_agent_installation(
            "definition-main",
            "desktop-main",
        )
        .unwrap()])
        .unwrap(),
    );
    DesktopHostConfig {
        database_path: database.to_owned(),
        capability_preparation: test_capability_preparation(agent_catalogue.clone()),
        agent_catalogue,
        default_agent_definition_id: "definition-main".into(),
        t1_host_system_config: None,
        host_limits: LiveHostLimits {
            max_command_bytes: 4_096,
            event_batch_size: 64,
            event_poll_interval_ms: 10,
            activity: None,
        },
        execution_policy: LocalExecutionPolicy {
            model_target_id: "desktop-target".into(),
            deployment_id: "desktop-deployment".into(),
            recovery_policy_revision: "recovery-1".into(),
            required_capabilities: vec![ModelCapability::Text],
            model_output: ModelOutputSettings {
                max_output_tokens: Some(8_192),
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
            max_context_items: 8,
            max_context_utf8_bytes: 2_048,
            max_model_attempts: 1,
        },
        dispatch_capacity: 2,
        host_clock: Arc::new(FixedHostClock),
        model,
        plan_admission_policy: None,
        plan_failure_policy: None,
        plan_proposal_port: None,
        model_plan_proposer_reference: None,
        operations: Arc::new(Operations(AtomicU64::new(1))),
    }
}

fn install_agent_catalogue(config: &mut DesktopHostConfig, catalogue: RuntimeAgentCatalogue) {
    let catalogue = Arc::new(catalogue);
    config.capability_preparation = test_capability_preparation(catalogue.clone());
    config.agent_catalogue = catalogue;
}

#[tokio::test]
async fn installed_snapshot_rejects_a_different_executor_catalogue() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("mismatched.db");
    let inner = DesktopWorkspaceExecutionFactory::new(
        database.clone(),
        DesktopWorkspaceService::default(),
        "main",
    )
    .unwrap();
    let host = DesktopHost::new_governed(
        desktop_host_config(&database, Arc::new(CompletingModel)),
        Arc::new(MismatchedFactory(inner)),
    )
    .unwrap();
    let state = DesktopState::default();
    state.install(host).unwrap();
    assert_eq!(
        state
            .run_turn_isolated("definition-main".into(), "hello".into())
            .await
            .unwrap_err(),
        garive_desktop::DesktopHostError::ExecutionFailure
    );
}

#[tokio::test]
async fn restart_blocks_new_execution_until_durable_startup_work_is_recovered() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("startup-recovery.db");
    let original = desktop_host(&database, Arc::new(CompletingModel));
    let session_id = original.create_session("definition-main").unwrap();
    let committed = original
        .live_host()
        .start_turn("lost-turn", &session_id, "recover me")
        .unwrap();
    drop(original);

    let restarted = desktop_host_with_ordinal(&database, Arc::new(CompletingModel), 100);
    assert_eq!(
        restarted
            .start_turn_command("must-wait", &session_id, "new work")
            .unwrap_err(),
        garive_desktop::DesktopHostError::StartupRecoveryRequired
    );
    assert_eq!(restarted.recover_startup().await.unwrap(), 1);

    let timeline = restarted.session_timeline(&session_id, 0, 8).unwrap();
    assert_eq!(timeline.items[0].turn_id, committed.turn_id);
    assert_eq!(timeline.items[0].state, "completed");
    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    assert!(ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap()
        .iter()
        .any(|fact| fact.kind.as_str() == "execution.abandoned"));
    let next = restarted
        .run_turn_in_session("definition-main", Some(&session_id), "new work")
        .await
        .unwrap();
    assert_eq!(next.terminal, DesktopTerminal::Completed);
}

#[cfg(unix)]
#[tokio::test]
async fn desktop_restart_resumes_the_same_prepared_v3_workspace_invocation() {
    let directory = tempdir().unwrap();
    let workspace_path = directory.path().join("Workspace");
    fs::create_dir(&workspace_path).unwrap();
    fs::write(workspace_path.join("note.txt"), "restart-safe content").unwrap();
    let workspaces = DesktopWorkspaceService::default();
    let selected = workspaces.admit_selected(&workspace_path, "main").unwrap();
    let writable = workspaces
        .authorize_writes(&selected.workspace_id, &workspace_path, "main")
        .unwrap();
    let t1 = t1_host(directory.path());
    let database = directory.path().join("desktop-f0-recovery.db");
    let mut config = desktop_host_config(
        &database,
        Arc::new(WorkspaceReadingModel(AtomicU64::new(0))),
    );
    install_agent_catalogue(
        &mut config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            builtin_desktop_workspace_agent_installation(
                "definition-workspace",
                "desktop-workspace",
                &t1.tool_capabilities().unwrap(),
            )
            .unwrap(),
        ])
        .unwrap(),
    );
    config.t1_host_system_config = Some(t1.clone());
    let failing_factory =
        DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces.clone(), "main")
            .unwrap()
            .with_t1_host_system_config(t1.clone());
    let original =
        DesktopHost::new_governed(config, Arc::new(SafetyUnavailableFactory(failing_factory)))
            .unwrap();
    let session_id = original.create_session("definition-workspace").unwrap();
    original.attach_workspace(&session_id, &writable).unwrap();
    assert_eq!(
        original
            .run_turn_in_session("definition-workspace", Some(&session_id), "read note.txt",)
            .await
            .unwrap_err(),
        garive_desktop::DesktopHostError::ExecutionFailure
    );
    assert_eq!(
        original
            .start_turn_command("must-recover", &session_id, "new work")
            .unwrap_err(),
        garive_desktop::DesktopHostError::StartupRecoveryRequired
    );
    drop(original);

    let mut restart_config = desktop_host_config(
        &database,
        Arc::new(WorkspaceReadingModel(AtomicU64::new(1))),
    );
    install_agent_catalogue(
        &mut restart_config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            builtin_desktop_workspace_agent_installation(
                "definition-workspace",
                "desktop-workspace",
                &t1.tool_capabilities().unwrap(),
            )
            .unwrap(),
        ])
        .unwrap(),
    );
    restart_config.t1_host_system_config = Some(t1.clone());
    restart_config.operations = Arc::new(Operations(AtomicU64::new(100)));
    let restart_factory =
        DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces, "main")
            .unwrap()
            .with_t1_host_system_config(t1);
    let restarted = DesktopHost::new_governed(restart_config, Arc::new(restart_factory)).unwrap();
    assert_eq!(restarted.recover_startup().await.unwrap(), 1);

    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap();
    for kind in [
        "effect.prepared",
        "safety.decided",
        "effect.authorized",
        "sandbox.bound",
        "sandbox.preflighted",
        "effect.started",
    ] {
        assert_eq!(
            facts
                .iter()
                .filter(|fact| fact.kind.as_str() == kind)
                .count(),
            1,
            "{kind}"
        );
    }
    assert_eq!(
        restarted.session_timeline(&session_id, 0, 8).unwrap().items[0].state,
        "completed"
    );
}

#[tokio::test]
async fn desktop_routes_each_session_through_its_exact_installed_agent() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("catalogue.db");
    let mut config = desktop_host_config(&database, Arc::new(CompletingModel));
    install_agent_catalogue(
        &mut config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            builtin_desktop_agent_installation("definition-work", "desktop-work").unwrap(),
        ])
        .unwrap(),
    );
    let factory =
        DesktopWorkspaceExecutionFactory::new(database, DesktopWorkspaceService::default(), "main")
            .unwrap();
    let host = DesktopHost::new_governed(config, Arc::new(factory)).unwrap();
    let state = DesktopState::default();
    state.install(host).unwrap();

    assert_eq!(state.definitions().unwrap().definitions.len(), 2);
    let result = state
        .run_turn_isolated("definition-work".into(), "workspace agent".into())
        .await
        .unwrap();
    let session = state
        .sessions(10, None)
        .unwrap()
        .sessions
        .into_iter()
        .find(|value| value.session_id == result.session_id)
        .unwrap();
    assert_eq!(session.definition_id, "definition-work");
}

#[tokio::test]
async fn typed_ipc_core_runs_an_embedded_durable_agent() {
    let directory = tempdir().expect("temp directory");
    let host = desktop_host(
        &directory.path().join("desktop.db"),
        Arc::new(CompletingModel),
    );
    let state = DesktopState::default();
    state.install(host).expect("one install");
    assert_eq!(
        state.capabilities().agent_definition_id.as_deref(),
        Some("definition-main")
    );
    assert!(state.capabilities().durable_navigation);
    assert!(state.capabilities().workspaces);
    assert!(!state.capabilities().updater);
    let result = state
        .run_turn_isolated("definition-main".into(), "hello desktop".into())
        .await
        .expect("durable turn");
    assert_eq!(result.terminal, DesktopTerminal::Completed);
    assert_eq!(result.text, "desktop durable answer");
    assert!(result.cursor > 2);
    assert!(!result.session_id.is_empty());
    assert!(!result.turn_id.is_empty());
    assert!(!result.execution_id.is_empty());

    let continued = state
        .run_turn_in_session_isolated(
            "definition-main".into(),
            Some(result.session_id.clone()),
            "follow-up desktop".into(),
        )
        .await
        .expect("durable follow-up Turn");
    assert_eq!(continued.session_id, result.session_id);
    assert_ne!(continued.turn_id, result.turn_id);
    assert!(continued.cursor > result.cursor);

    let recents = state.recent_sessions(8).expect("durable recents");
    assert_eq!(recents.len(), 1);
    assert_eq!(recents[0].session_id, result.session_id);
    assert_eq!(recents[0].turn_count, 2);
    let timeline = state
        .session_timeline(&result.session_id, 0, 8)
        .expect("durable timeline");
    assert_eq!(timeline.items.len(), 2);
    assert_eq!(timeline.items[0].user_text, "hello desktop");
    assert_eq!(
        timeline.items[1].completion_text.as_deref(),
        Some("desktop durable answer")
    );
}

#[tokio::test]
async fn product_commands_acknowledge_exact_commits_before_bounded_follow() {
    let directory = tempdir().expect("temp directory");
    let state = DesktopState::default();
    state
        .install(desktop_host(
            &directory.path().join("product-commands.db"),
            Arc::new(CompletingModel),
        ))
        .expect("install");

    let definitions = state.definitions().expect("definitions");
    assert_eq!(definitions.definitions[0].definition_id, "definition-main");
    let created = state
        .create_session_command("client-create-1", "definition-main")
        .expect("create commit");
    assert_eq!(
        state
            .create_session_command("client-create-1", "definition-main")
            .expect("exact create replay"),
        created
    );

    let started = state
        .start_turn_detached(
            "client-turn-1".into(),
            created.session_id.clone(),
            "hello product".into(),
        )
        .await
        .expect("start commit");
    assert!(started.committed_position > created.committed_position);
    let mut cursor = started.committed_position;
    let mut events = Vec::new();
    for _ in 0..100 {
        let page = match state.event_page(&created.session_id, cursor) {
            Ok(page) => page,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                continue;
            }
        };
        cursor = page.scanned_through_position;
        events.extend(page.events);
        if events.iter().any(|event| event.event == "turn.completed") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(events.iter().any(|event| event.event == "turn.completed"));
    assert_eq!(
        state
            .start_turn_detached(
                "client-turn-1".into(),
                created.session_id,
                "hello product".into(),
            )
            .await
            .expect("exact Turn replay"),
        started
    );
}

#[tokio::test]
async fn embedded_desktop_publishes_real_model_deltas_before_durable_terminal() {
    let directory = tempdir().expect("temp directory");
    let host = desktop_host(
        &directory.path().join("live-output.db"),
        Arc::new(CompletingModel),
    );
    let created = host
        .create_session_command("create-live", "definition-main")
        .expect("create");
    let mut subscriber = host
        .subscribe_live_output(&created.session_id)
        .expect("subscribe");
    host.start_turn_command("turn-live", &created.session_id, "stream this")
        .expect("start");
    assert!(host.drive_pending().await.expect("drive"));
    let mut text = String::new();
    let mut ended = false;
    for _ in 0..12 {
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), subscriber.recv())
            .await
            .expect("live deadline")
            .expect("live event");
        match event.kind {
            LiveOutputEventKind::TextDelta { text: delta } => text.push_str(&delta),
            LiveOutputEventKind::Ended { .. } => {
                ended = true;
                break;
            }
            _ => {}
        }
    }
    assert_eq!(text, "desktop durable answer");
    assert!(ended);
}

#[test]
fn cancellation_binds_the_exact_observed_prefix() {
    let directory = tempdir().expect("temp directory");
    let host = desktop_host(
        &directory.path().join("product-cancel.db"),
        Arc::new(CompletingModel),
    );
    let created = host
        .create_session_command("client-create-cancel", "definition-main")
        .expect("create");
    let started = host
        .start_turn_command("client-turn-cancel", &created.session_id, "cancel me")
        .expect("start");
    let cancelled = host
        .cancel_turn_command(
            "client-cancel-1",
            &created.session_id,
            &started.turn_id,
            started.committed_position,
        )
        .expect("cancel");
    assert!(cancelled.committed_position > started.committed_position);
    assert!(host
        .cancel_turn_command(
            "client-cancel-1",
            &created.session_id,
            &started.turn_id,
            started.committed_position - 1,
        )
        .is_err());
}

#[tokio::test]
async fn product_reopens_after_process_restart_and_commits_a_second_turn() {
    let directory = tempdir().expect("temp directory");
    let database = directory.path().join("product-restart.db");
    let first_process = DesktopState::default();
    first_process
        .install(desktop_host_with_ordinal(
            &database,
            Arc::new(CompletingModel),
            1,
        ))
        .expect("first process installs");
    let first = first_process
        .run_turn_isolated("definition-main".into(), "first durable request".into())
        .await
        .expect("first Turn completes");
    assert_eq!(first.terminal, DesktopTerminal::Completed);
    drop(first_process);

    let restarted = DesktopState::default();
    restarted
        .install(desktop_host_with_ordinal(
            &database,
            Arc::new(CompletingModel),
            100,
        ))
        .expect("restarted process installs");
    let reopened = restarted
        .session_timeline(&first.session_id, 0, 8)
        .expect("Session reopens from Ledger");
    assert_eq!(reopened.items.len(), 1);
    assert_eq!(reopened.items[0].user_text, "first durable request");

    let second = restarted
        .run_turn_in_session_isolated(
            "definition-main".into(),
            Some(first.session_id.clone()),
            "second durable request".into(),
        )
        .await
        .expect("second Turn completes");
    assert_eq!(second.session_id, first.session_id);
    assert_ne!(second.turn_id, first.turn_id);
    let final_timeline = restarted
        .session_timeline(&first.session_id, 0, 8)
        .expect("two-Turn timeline restores");
    assert_eq!(final_timeline.items.len(), 2);
    assert_eq!(final_timeline.items[1].user_text, "second durable request");
    assert!(final_timeline.items[1].latest_position > reopened.items[0].latest_position);
}

#[tokio::test]
async fn restart_safe_partial_output_can_resume_the_same_turn() {
    let directory = tempdir().expect("temp directory");
    let state = DesktopState::default();
    state
        .install(desktop_host(
            &directory.path().join("suspended.db"),
            Arc::new(SuspendingModel(AtomicU64::new(0))),
        ))
        .unwrap();
    let first = state
        .run_turn_isolated("definition-main".into(), "long outcome".into())
        .await
        .unwrap();
    assert_eq!(first.terminal, DesktopTerminal::Suspended);
    let timeline = state.session_timeline(&first.session_id, 0, 8).unwrap();
    let suspension = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(suspension.kind, "partial_output");

    let resumed = state
        .continue_turn_isolated(
            first.session_id.clone(),
            first.turn_id.clone(),
            suspension.suspension_id.clone(),
            suspension.session_version,
            "continue".into(),
        )
        .await
        .unwrap();
    assert_eq!(resumed.turn_id, first.turn_id);
    assert_eq!(resumed.terminal, DesktopTerminal::Completed);
    let restored = state.session_timeline(&first.session_id, 0, 8).unwrap();
    assert_eq!(restored.items.len(), 1);
    assert_eq!(
        restored.items[0].completion_text.as_deref(),
        Some("resumed answer")
    );
    assert!(restored.items[0].suspension.is_none());
}

#[tokio::test]
async fn unconfigured_state_is_stable_and_secret_free() {
    let state = DesktopState::default();
    assert!(!state.capabilities().configured);
    assert!(state.capabilities().agent_definition_id.is_none());
    assert!(!state.capabilities().updater);
    let error = state
        .run_turn("definition", "private input")
        .await
        .expect_err("configuration is required");
    assert_eq!(error.code(), "not_configured");
    assert!(!format!("{error:?}").contains("private input"));
}

#[test]
fn workspace_attachment_survives_desktop_host_restart_without_paths() {
    let directory = tempdir().expect("temp directory");
    let database = directory.path().join("workspace.db");
    let host = desktop_host(&database, Arc::new(CompletingModel));
    let session_id = host.create_session("definition-main").unwrap();
    let grant = DesktopWorkspaceGrant {
        schema_version: 1,
        workspace_id: "workspace-opaque".into(),
        display_name: "Briefs".into(),
        access: "enumerate",
        grant_revision: 1,
        state: "active",
        expires_at: "2026-08-30T12:00:00Z".into(),
    };
    let attached = host.attach_workspace(&session_id, &grant).unwrap();
    assert_eq!(attached.workspace_id, "workspace-opaque");

    let restarted = desktop_host(&database, Arc::new(CompletingModel));
    let restored = restarted.session_workspaces(&session_id).unwrap();
    assert_eq!(restored, vec![attached]);
    let public = serde_json::to_string(&restored).unwrap();
    assert!(!public.contains(directory.path().to_string_lossy().as_ref()));
    assert!(!public.contains("path"));
}

#[tokio::test]
async fn selected_workspace_text_reaches_the_embedded_runtime_without_frontend_content() {
    let directory = tempdir().unwrap();
    let state = DesktopState::default();
    state
        .install(desktop_host(
            &directory.path().join("context.db"),
            Arc::new(CompletingModel),
        ))
        .unwrap();
    let session_id = state.create_session("definition-main").unwrap();
    let grant = DesktopWorkspaceGrant {
        schema_version: 1,
        workspace_id: "workspace-opaque".into(),
        display_name: "Briefs".into(),
        access: "enumerate",
        grant_revision: 1,
        state: "active",
        expires_at: "2026-08-30T12:00:00Z".into(),
    };
    state.attach_workspace(&session_id, &grant).unwrap();
    let result = state
        .start_turn_with_context_detached(
            "client-context-1".into(),
            session_id.clone(),
            "summarize".into(),
            vec![DesktopWorkspaceContextFile {
                workspace_id: grant.workspace_id,
                grant_revision: 1,
                entry_id: "entry-opaque".into(),
                display_name: "brief.md".into(),
                kind: "text",
                content_digest: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    .into(),
                content_utf8: "hello".into(),
            }],
        )
        .await
        .unwrap();
    assert_eq!(result.session_id, session_id);
    let mut completed = false;
    for _ in 0..100 {
        if let Ok(page) = state.event_page(&session_id, result.committed_position) {
            if page
                .events
                .iter()
                .any(|event| event.event == "turn.completed")
            {
                completed = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(completed, "contextual Turn did not complete");
}

#[tokio::test]
async fn approved_workspace_write_commits_receipt_and_creates_an_atomic_artifact() {
    let directory = tempdir().unwrap();
    let workspace_path = directory.path().join("Workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let workspaces = DesktopWorkspaceService::default();
    let selected = workspaces.admit_selected(&workspace_path, "main").unwrap();
    let writable = workspaces
        .authorize_writes(&selected.workspace_id, &workspace_path, "main")
        .unwrap();
    let database = directory.path().join("governed.db");
    let arguments = serde_json::json!({
        "workspace_id":writable.workspace_id,
        "artifact_name":"result.md",
        "content_utf8":"durable artifact"
    })
    .to_string();
    let model = Arc::new(WorkspaceWritingModel {
        calls: AtomicU64::new(0),
        arguments,
    });
    let factory = Arc::new(
        DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces.clone(), "main")
            .unwrap(),
    );
    let state = DesktopState::default();
    state
        .install(DesktopHost::new_governed(desktop_host_config(&database, model), factory).unwrap())
        .unwrap();
    let session_id = state.create_session("definition-main").unwrap();
    state.attach_workspace(&session_id, &writable).unwrap();

    let suspended = state
        .run_turn_in_session_isolated(
            "definition-main".into(),
            Some(session_id.clone()),
            "create the result".into(),
        )
        .await
        .unwrap();
    assert_eq!(
        suspended.terminal,
        DesktopTerminal::Suspended,
        "{suspended:?}"
    );
    let timeline = state.session_timeline(&session_id, 0, 8).unwrap();
    let approval = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(approval.kind, "approval_required");

    let continued = state
        .continue_approval_detached(
            "client-approval-1".into(),
            session_id.clone(),
            suspended.turn_id.clone(),
            approval.suspension_id.clone(),
            approval.session_version,
            true,
        )
        .await
        .unwrap();
    assert_eq!(continued.session_id, session_id);
    assert_eq!(continued.turn_id, suspended.turn_id);
    let mut completed = false;
    for _ in 0..100 {
        let timeline = match state.session_timeline(&session_id, 0, 8) {
            Ok(timeline) => timeline,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
                continue;
            }
        };
        if timeline.items[0].state == "completed" {
            assert_eq!(
                timeline.items[0].completion_text.as_deref(),
                Some("artifact committed")
            );
            completed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert!(completed, "approval continuation did not complete");
    assert_eq!(
        std::fs::read_to_string(workspace_path.join("result.md")).unwrap(),
        "durable artifact"
    );
    let artifacts = state.artifacts(&session_id, 0, 8).unwrap();
    assert_eq!(artifacts.items.len(), 1);
    let artifact_view = &artifacts.items[0];
    assert_eq!(artifact_view.display_name, "result.md");
    assert_eq!(artifact_view.mime_type, "text/markdown");
    assert_eq!(artifact_view.byte_size, 16);
    assert_eq!(artifact_view.preview, "text");
    assert_eq!(
        artifact_view.workspace_id.as_deref(),
        Some(writable.workspace_id.as_str())
    );
    assert!(!serde_json::to_string(&artifacts)
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
    let preview = workspaces
        .preview_text_artifact(
            &artifact_view.artifact_id,
            artifact_view.revision,
            artifact_view.workspace_id.as_deref().unwrap(),
            &artifact_view.display_name,
            &artifact_view.content_digest,
            "main",
        )
        .unwrap();
    assert_eq!(preview.content_utf8, "durable artifact");
    assert!(!serde_json::to_string(&preview)
        .unwrap()
        .contains(directory.path().to_string_lossy().as_ref()));
    std::fs::write(workspace_path.join("result.md"), "tampered").unwrap();
    assert_eq!(
        workspaces
            .preview_text_artifact(
                &artifact_view.artifact_id,
                artifact_view.revision,
                artifact_view.workspace_id.as_deref().unwrap(),
                &artifact_view.display_name,
                &artifact_view.content_digest,
                "main",
            )
            .unwrap_err(),
        garive_desktop::DesktopWorkspaceError::Unavailable
    );

    let restarted = desktop_host(&database, Arc::new(CompletingModel));
    assert_eq!(restarted.artifacts(&session_id, 0, 8).unwrap(), artifacts);

    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap();
    let kinds = facts
        .iter()
        .map(|fact| fact.kind.as_str().to_owned())
        .collect::<Vec<_>>();
    for required in [
        "interaction.requested",
        "interaction.resolved",
        "effect.authorized",
        "effect.started",
        "effect.receipt",
        "effect.completed",
        "artifact.committed",
        "turn.completed",
    ] {
        assert!(
            kinds.iter().any(|kind| kind == required),
            "missing {required}"
        );
    }
    let completed_position = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "effect.completed")
        .unwrap()
        .position;
    let artifact = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "artifact.committed")
        .unwrap();
    assert_eq!(artifact.position, completed_position + 1);
    assert!(artifact.payload.as_json().contains("artifact-"));

    let started = facts
        .iter()
        .find(|fact| fact.kind.as_str() == "effect.started")
        .unwrap();
    let invocation = started.tool_invocation_id.as_ref().unwrap();
    let governed_chain = facts
        .iter()
        .filter(|fact| fact.tool_invocation_id.as_ref() == Some(invocation))
        .map(|fact| fact.kind.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        governed_chain,
        [
            "effect.prepared",
            "safety.decided",
            "effect.authorized",
            "sandbox.bound",
            "sandbox.preflighted",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "artifact.committed",
            "effect.observation",
        ]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn workspace_agent_runs_t1_read_through_the_complete_f0_chain() {
    let directory = tempdir().unwrap();
    let workspace_path = directory.path().join("Workspace");
    fs::create_dir(&workspace_path).unwrap();
    fs::write(workspace_path.join("note.txt"), "bound workspace content").unwrap();
    let workspaces = DesktopWorkspaceService::default();
    let selected = workspaces.admit_selected(&workspace_path, "main").unwrap();
    let writable = workspaces
        .authorize_writes(&selected.workspace_id, &workspace_path, "main")
        .unwrap();
    let t1 = t1_host(directory.path());
    let database = directory.path().join("workspace-t1.db");
    let mut config = desktop_host_config(
        &database,
        Arc::new(WorkspaceReadingModel(AtomicU64::new(0))),
    );
    let workspace_agent = builtin_desktop_workspace_agent_installation(
        "definition-workspace",
        "desktop-workspace",
        &t1.tool_capabilities().unwrap(),
    )
    .unwrap();
    install_agent_catalogue(
        &mut config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            workspace_agent,
        ])
        .unwrap(),
    );
    config.t1_host_system_config = Some(t1.clone());
    let factory = DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces, "main")
        .unwrap()
        .with_t1_host_system_config(t1);
    let state = DesktopState::default();
    state
        .install(DesktopHost::new_governed(config, Arc::new(factory)).unwrap())
        .unwrap();
    let session_id = state.create_session("definition-workspace").unwrap();
    state.attach_workspace(&session_id, &writable).unwrap();

    let result = state
        .run_turn_in_session_isolated(
            "definition-workspace".into(),
            Some(session_id.clone()),
            "read note.txt".into(),
        )
        .await
        .unwrap();
    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap();
    let audit = facts
        .iter()
        .map(|fact| (fact.kind.as_str(), fact.payload.as_json()))
        .collect::<Vec<_>>();
    assert_eq!(
        result.terminal,
        DesktopTerminal::Completed,
        "result={result:?} timeline={:?} audit={audit:?}",
        state.session_timeline(&session_id, 0, 8).unwrap(),
    );
    assert_eq!(result.text, "workspace read completed");
    for required in [
        "effect.prepared",
        "safety.decided",
        "effect.authorized",
        "sandbox.bound",
        "sandbox.preflighted",
        "effect.started",
        "effect.receipt",
        "effect.observation",
    ] {
        assert!(
            facts.iter().any(|fact| fact.kind.as_str() == required),
            "{required}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn workspace_agent_patch_requires_durable_approval_then_acknowledges_receipt() {
    let directory = tempdir().unwrap();
    let workspace_path = directory.path().join("Workspace");
    fs::create_dir(&workspace_path).unwrap();
    fs::write(workspace_path.join("note.txt"), "before\n").unwrap();
    let before_digest = format!("{:x}", Sha256::digest(b"before\n"));
    let arguments = serde_json::json!({
        "patch":"*** Begin Patch\n*** Update File: note.txt\n@@\n-before\n+after\n*** End Patch",
        "expected_files":[{"path":"note.txt","before_digest":before_digest}]
    })
    .to_string();
    let workspaces = DesktopWorkspaceService::default();
    let selected = workspaces.admit_selected(&workspace_path, "main").unwrap();
    let writable = workspaces
        .authorize_writes(&selected.workspace_id, &workspace_path, "main")
        .unwrap();
    let t1 = t1_host(directory.path());
    let restart_t1 = t1.clone();
    let restart_workspaces = workspaces.clone();
    let database = directory.path().join("workspace-patch.db");
    let mut config = desktop_host_config(
        &database,
        Arc::new(WorkspacePatchingModel {
            calls: AtomicU64::new(0),
            arguments,
        }),
    );
    install_agent_catalogue(
        &mut config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            builtin_desktop_workspace_agent_installation(
                "definition-workspace",
                "desktop-workspace",
                &t1.tool_capabilities().unwrap(),
            )
            .unwrap(),
        ])
        .unwrap(),
    );
    config.t1_host_system_config = Some(t1.clone());
    let factory = DesktopWorkspaceExecutionFactory::new(database.clone(), workspaces, "main")
        .unwrap()
        .with_t1_host_system_config(t1);
    let state = DesktopState::default();
    state
        .install(DesktopHost::new_governed(config, Arc::new(factory)).unwrap())
        .unwrap();
    let session_id = state.create_session("definition-workspace").unwrap();
    state.attach_workspace(&session_id, &writable).unwrap();

    let suspended = state
        .run_turn_in_session_isolated(
            "definition-workspace".into(),
            Some(session_id.clone()),
            "patch note.txt".into(),
        )
        .await
        .unwrap();
    assert_eq!(suspended.terminal, DesktopTerminal::Suspended);
    assert_eq!(
        fs::read_to_string(workspace_path.join("note.txt")).unwrap(),
        "before\n"
    );
    let timeline = state.session_timeline(&session_id, 0, 8).unwrap();
    let approval = timeline.items[0].suspension.as_ref().unwrap();
    assert_eq!(approval.kind, "approval_required");
    state
        .continue_approval_detached(
            "approve-patch-1".into(),
            session_id.clone(),
            suspended.turn_id,
            approval.suspension_id.clone(),
            approval.session_version,
            true,
        )
        .await
        .unwrap();
    let mut final_state = String::new();
    for _ in 0..500 {
        final_state = state.session_timeline(&session_id, 0, 8).unwrap().items[0]
            .state
            .clone();
        if final_state == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }
    assert_eq!(
        final_state,
        "completed",
        "{:?}",
        state.session_timeline(&session_id, 0, 8).unwrap()
    );
    let ledger = SqliteLedger::open(&database).unwrap();
    let session = SessionId::try_from(session_id.as_str()).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    let facts = ledger
        .read_facts(&session, 0, watermark.max_position, None)
        .unwrap();
    let audit = facts
        .iter()
        .map(|fact| (fact.kind.as_str(), fact.payload.as_json()))
        .collect::<Vec<_>>();
    assert_eq!(
        fs::read_to_string(workspace_path.join("note.txt")).unwrap(),
        "after\n",
        "{audit:?}"
    );
    assert_eq!(
        fs::read_dir(directory.path().join("patch-recovery"))
            .unwrap()
            .count(),
        0
    );
    for required in ["interaction.resolved", "effect.started", "effect.receipt"] {
        assert!(facts.iter().any(|fact| fact.kind.as_str() == required));
    }
    drop(state);

    let mut restart_config = desktop_host_config(
        &database,
        Arc::new(WorkspaceReadingModel(AtomicU64::new(0))),
    );
    install_agent_catalogue(
        &mut restart_config,
        RuntimeAgentCatalogue::new([
            builtin_desktop_agent_installation("definition-main", "desktop-main").unwrap(),
            builtin_desktop_workspace_agent_installation(
                "definition-workspace",
                "desktop-workspace",
                &restart_t1.tool_capabilities().unwrap(),
            )
            .unwrap(),
        ])
        .unwrap(),
    );
    restart_config.t1_host_system_config = Some(restart_t1.clone());
    restart_config.operations = Arc::new(Operations(AtomicU64::new(100)));
    let restart_factory =
        DesktopWorkspaceExecutionFactory::new(database, restart_workspaces, "main")
            .unwrap()
            .with_t1_host_system_config(restart_t1);
    let restarted = DesktopState::default();
    restarted
        .install(DesktopHost::new_governed(restart_config, Arc::new(restart_factory)).unwrap())
        .unwrap();
    assert_eq!(
        restarted.session_timeline(&session_id, 0, 8).unwrap().items[0].state,
        "completed"
    );
    let second = restarted
        .run_turn_in_session_isolated(
            "definition-workspace".into(),
            Some(session_id),
            "read after restart".into(),
        )
        .await
        .unwrap();
    assert_eq!(second.terminal, DesktopTerminal::Completed);
    assert_eq!(second.text, "workspace read completed");
}

#[cfg(unix)]
fn t1_host(root: &Path) -> T1HostSystemConfig {
    let patch_recovery = root.join("patch-recovery");
    let process_recovery = root.join("process-recovery");
    for directory in [&patch_recovery, &process_recovery] {
        fs::create_dir(directory).unwrap();
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let lanes = ProcessLaneRegistry::new([ProcessLane::new(
        "rust",
        [ProcessExecutable::new("cargo", "/opt/garive/bin/cargo").unwrap()],
        Vec::new(),
    )
    .unwrap()])
    .unwrap();
    T1HostSystemConfig::new(
        "t1.policy.v1",
        "t1.executor.v1",
        patch_recovery,
        lanes,
        ProcessBackendHostConfig::podman(
            "/opt/garive/bin/podman",
            "unix:///var/run/garive-podman.sock",
            format!("localhost/garive-runner@sha256:{}", "a".repeat(64)),
            process_recovery,
            5_000,
        )
        .unwrap(),
    )
    .unwrap()
}
