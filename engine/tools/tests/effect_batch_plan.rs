use garive_tools::{
    plan_effect_batch, AccessMode, AccessNamespace, AccessPolicyEntry, EffectBatchLimitsV1,
    EffectBatchStep, ExecutionCapability, ExecutionRequirements, InvocationAccessSet,
    PreparationError, ReplayClass, ResourceAccess, ToolAccessPolicyV1, ToolAccessResolver,
    ToolCatalog, ToolDefinition, ToolIntent,
};
use serde_json::{json, Value};

struct ExactResolver {
    revision: &'static str,
    mode: AccessMode,
}

impl ToolAccessResolver for ExactResolver {
    fn revision(&self) -> &str {
        self.revision
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            self.mode,
        )?])
    }
}

fn prepared(path: &str, mode: AccessMode, replay: ReplayClass) -> garive_tools::PreparedToolCall {
    let capability = if mode == AccessMode::Read {
        ExecutionCapability::FilesystemRead
    } else {
        ExecutionCapability::FilesystemWrite
    };
    let definition = ToolDefinition::new_v2(
        format!("tool_{path}"),
        "revision-v2",
        "Exact fixture tool",
        json!({
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
            "additionalProperties": false
        }),
        ExecutionRequirements::new([capability], 1000, 1024).unwrap(),
        replay,
        ToolAccessPolicyV1::new(
            "policy-v1",
            [AccessPolicyEntry::new("src", [mode]).unwrap()],
            [],
            [],
            [],
            1,
            512,
        )
        .unwrap(),
        "exact-v1",
    )
    .unwrap();
    let catalog = ToolCatalog::new([definition]).unwrap();
    catalog
        .prepare_v2(
            &ToolIntent::new(
                format!("call-{path}"),
                format!("tool_{path}"),
                format!(r#"{{"path":"{path}"}}"#),
            ),
            &ExactResolver {
                revision: "exact-v1",
                mode,
            },
        )
        .unwrap()
}

fn limits() -> EffectBatchLimitsV1 {
    EffectBatchLimitsV1::new(8, 2, 16, 3, 2_048).unwrap()
}

#[test]
fn graph_and_plan_follow_original_intent_order() {
    let calls = vec![
        prepared("src/a", AccessMode::Read, ReplayClass::ReadOnly),
        prepared("src/b", AccessMode::Read, ReplayClass::ReadOnly),
        prepared("src/a", AccessMode::Read, ReplayClass::ReadOnly),
        prepared("src/a", AccessMode::Write, ReplayClass::Idempotent),
    ];
    let plan = plan_effect_batch(&calls, &limits()).unwrap();

    assert_eq!(plan.conflict_graph_bytes(), &[0, 0, 1, 0, 0, 1]);
    assert_eq!(
        plan.steps(),
        &[
            EffectBatchStep::ParallelReadGroup {
                intent_indexes: vec![0, 1, 2]
            },
            EffectBatchStep::SequentialStep { intent_index: 3 }
        ]
    );
    assert_eq!(plan.conflict_graph_digest().len(), 64);
    assert_eq!(plan.plan_digest().len(), 64);
}

#[test]
fn buffer_bound_splits_read_groups_without_reordering() {
    let calls = vec![
        prepared("src/a", AccessMode::Read, ReplayClass::ReadOnly),
        prepared("src/b", AccessMode::Read, ReplayClass::ReadOnly),
        prepared("src/c", AccessMode::Read, ReplayClass::ReadOnly),
    ];
    let limits = EffectBatchLimitsV1::new(3, 1, 3, 3, 1_024).unwrap();
    let plan = plan_effect_batch(&calls, &limits).unwrap();

    assert_eq!(
        plan.steps(),
        &[
            EffectBatchStep::ParallelReadGroup {
                intent_indexes: vec![0, 1]
            },
            EffectBatchStep::ParallelReadGroup {
                intent_indexes: vec![2]
            }
        ]
    );
}

#[test]
fn invalid_limits_and_v1_calls_fail_before_planning() {
    assert!(EffectBatchLimitsV1::new(0, 1, 1, 1, 1).is_err());

    let requirements =
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1, 1).unwrap();
    let v1 = ToolDefinition::new(
        "legacy",
        "v1",
        "Legacy",
        json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        }),
        requirements,
        ReplayClass::ReadOnly,
    )
    .unwrap();
    let call = ToolCatalog::new([v1])
        .unwrap()
        .prepare(&ToolIntent::new("call", "legacy", "{}"))
        .unwrap();
    assert!(plan_effect_batch(&[call], &limits()).is_err());
}
