use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use garive_config::{
    resolve_definition, AgentDefinition, ContextPolicyCandidate, ContextPolicyReference,
    DefaultLimits, GovernancePolicy, GovernancePolicyCandidate, ProductPolicy, ResolutionRegistry,
};
use garive_goal::{
    GoalBoundsV1, GoalCriterion, GoalCriterionId, GoalDefinitionV1, GoalId, GoalScopeV1,
};
use garive_ledger::{CanonicalPayload, FactDraft, FactId, FactKind, SessionId};
use garive_runtime::{
    commit_goal_command, plan_create_goal, plan_proposal_output_schema,
    start_initial_goal_plan_proposal_execution, GoalCommandContext, PlanProposalRuntimeError,
    RuntimeAgentCatalogue, RuntimeAgentInstallation, SqliteLedger,
};
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn fixed_goal_prefix_starts_and_replays_one_planner_execution() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("planner-start.db");
    let session = SessionId::try_from("session-1").unwrap();
    let installation = installation();
    let installed = installation.installed_agent().clone();
    let catalogue = Arc::new(RuntimeAgentCatalogue::new([installation]).unwrap());
    let opened = fact(
        "session-open",
        "session.opened",
        json!({"command_id":"session-open","definition_id":installed.definition_id,
            "definition_revision":installed.definition_revision,
            "snapshot_digest":installed.snapshot_digest,"agent_instance_id":"agent-instance-1"}),
    );
    let opened_digest = opened.payload.sha256().to_owned();
    let mut ledger = SqliteLedger::open(&database).unwrap();
    ledger.commit(session.clone(), 0, vec![opened]).unwrap();
    let goal = GoalDefinitionV1::new(
        GoalId::new("goal-1").unwrap(),
        "Deliver one verified result",
        vec![GoalCriterion::DurableFact {
            criterion_id: GoalCriterionId::new("accepted").unwrap(),
            fact_kind: "session.opened".into(),
            subject_digest: opened_digest,
        }],
        GoalScopeV1::new(Some(session.as_str().into()), []).unwrap(),
        GoalBoundsV1::new(2, 2, 1, None, None).unwrap(),
        None,
        [],
    )
    .unwrap();
    let planned = plan_create_goal(
        &ledger,
        &session,
        &GoalCommandContext {
            command_id: "goal-create".into(),
            actor_reference: "user:test".into(),
            recorded_at: "2026-09-01T00:00:01Z".into(),
        },
        goal,
    )
    .unwrap();
    commit_goal_command(&mut ledger, session.clone(), 1, &planned).unwrap();
    drop(ledger);

    let committed = start_initial_goal_plan_proposal_execution(
        &database,
        &session,
        "goal-1",
        "planner-v1",
        "2026-09-01T00:00:02Z",
        catalogue.clone(),
    )
    .unwrap();
    assert_eq!(committed.session_version, 3);
    assert_eq!(committed.committed_position, 6);
    let replay = start_initial_goal_plan_proposal_execution(
        &database,
        &session,
        "goal-1",
        "planner-v1",
        "2026-09-01T00:00:02Z",
        catalogue.clone(),
    )
    .unwrap();
    assert_eq!(replay, committed);

    let ledger = SqliteLedger::open(&database).unwrap();
    let watermark = ledger.session_watermark(&session).unwrap().unwrap();
    assert_eq!((watermark.session_version, watermark.max_position), (3, 6));
    let facts = ledger.read_facts(&session, 0, 6, None).unwrap();
    let request: Value = serde_json::from_str(facts[2].payload.as_json()).unwrap();
    assert_eq!(
        request["output_schema_digest"],
        plan_proposal_output_schema().digest
    );
    let input: Value = serde_json::from_str(facts[4].payload.as_json()).unwrap();
    let prompt: Value =
        serde_json::from_str(input["content"]["inline_utf8"].as_str().unwrap()).unwrap();
    assert_eq!(prompt["goal"]["objective"], "Deliver one verified result");
    assert_eq!(
        prompt["output"]["schema_digest"],
        request["output_schema_digest"]
    );
    drop(ledger);
    assert_eq!(
        start_initial_goal_plan_proposal_execution(
            &database,
            &session,
            "goal-1",
            "planner-v2",
            "2026-09-01T00:00:02Z",
            catalogue,
        ),
        Err(PlanProposalRuntimeError::CorruptState)
    );
}

fn fact(id: &str, kind: &str, payload: Value) -> FactDraft {
    FactDraft {
        fact_id: FactId::try_from(id).unwrap(),
        turn_id: None,
        execution_id: None,
        model_request_id: None,
        tool_invocation_id: None,
        kind: FactKind::new(kind).unwrap(),
        schema_version: 1,
        payload: CanonicalPayload::from_value(&payload).unwrap(),
        recorded_at: "2026-09-01T00:00:00Z".into(),
    }
}

fn installation() -> RuntimeAgentInstallation {
    let limits = DefaultLimits::new(2, Some(1024), Some(512), Some(30_000)).unwrap();
    let definition = AgentDefinition::new(
        "planner",
        "1",
        Vec::new(),
        Vec::new(),
        Vec::new(),
        GovernancePolicy::new("governance", "1", BTreeSet::new(), []).unwrap(),
        ContextPolicyReference::new("context", "1").unwrap(),
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
            capability_descriptors: Vec::new(),
            governance_policies: vec![GovernancePolicyCandidate {
                policy_id: "governance".into(),
                exact_revision: "1".into(),
                allowed_requirement_capabilities: BTreeSet::new(),
                interaction_modes: BTreeSet::new(),
            }],
            context_policies: vec![ContextPolicyCandidate {
                policy_id: "context".into(),
                exact_revision: "1".into(),
                descriptor_digest: "a".repeat(64),
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
    RuntimeAgentInstallation::new(snapshot, "planner-instance", Vec::new()).unwrap()
}
