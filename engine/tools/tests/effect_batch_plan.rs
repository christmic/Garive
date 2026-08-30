use garive_tools::{
    plan_effect_batch, AccessMode, AccessNamespace, AccessPolicyEntry, EffectBatchLimitsV1,
    EffectBatchStep, ExecutionCapability, ExecutionRequirements, InvocationAccessSet,
    PreparationError, ReplayClass, ResourceAccess, ToolAccessPolicyV1, ToolAccessResolver,
    ToolCatalog, ToolDefinition, ToolIntent,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/deterministic-effect-batches-v1.json"
    ))
    .unwrap()
}

fn assert_keys(value: &Value, expected: &[&str]) {
    let mut actual: Vec<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

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

#[test]
fn shared_c5b_fixture_matches_canonical_rust_semantics() {
    let fixture = fixture();
    assert_keys(
        &fixture,
        &[
            "schema_version",
            "definition_template",
            "normalization_cases",
            "policy_cases",
            "plan_case",
            "failure_cases",
        ],
    );
    assert_eq!(fixture["schema_version"], 1);

    for case in fixture["normalization_cases"].as_array().unwrap() {
        assert_keys(
            case,
            &["name", "namespace", "resource_key", "mode", "valid"],
        );
        let namespace = match case["namespace"].as_str().unwrap() {
            "filesystem" => AccessNamespace::Filesystem,
            "network" => AccessNamespace::Network,
            value => panic!("unknown fixture namespace {value}"),
        };
        let result = ResourceAccess::new(
            namespace,
            case["resource_key"].as_str().unwrap(),
            AccessMode::Read,
        );
        assert_eq!(result.is_ok(), case["valid"].as_bool().unwrap());
    }

    let policy = ToolAccessPolicyV1::new(
        "policy-v1",
        [AccessPolicyEntry::new("src", [AccessMode::Read]).unwrap()],
        [],
        [],
        [],
        1,
        512,
    )
    .unwrap();
    for case in fixture["policy_cases"].as_array().unwrap() {
        assert_keys(case, &["name", "resource_key", "covered"]);
        let accesses = InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            case["resource_key"].as_str().unwrap(),
            AccessMode::Read,
        )
        .unwrap()])
        .unwrap();
        assert_eq!(policy.covers(&accesses), case["covered"].as_bool().unwrap());
    }

    let case = &fixture["plan_case"];
    assert_keys(
        case,
        &[
            "name",
            "calls",
            "limits",
            "conflict_graph_bytes",
            "conflict_graph_digest",
            "steps",
            "plan_digest",
        ],
    );
    let calls = case["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            assert_keys(call, &["path", "mode", "replay_class", "prepared_digest"]);
            let mode = if call["mode"] == "read" {
                AccessMode::Read
            } else {
                AccessMode::Write
            };
            let replay = if call["replay_class"] == "read_only" {
                ReplayClass::ReadOnly
            } else {
                ReplayClass::Idempotent
            };
            let prepared = prepared(call["path"].as_str().unwrap(), mode, replay);
            assert_eq!(prepared.input_digest(), call["prepared_digest"]);
            prepared
        })
        .collect::<Vec<_>>();
    let limits_value = &case["limits"];
    assert_keys(
        limits_value,
        &[
            "max_intents",
            "max_accesses_per_intent",
            "max_total_accesses",
            "max_parallel_reads",
            "max_buffered_result_bytes",
        ],
    );
    let fixture_limits = EffectBatchLimitsV1::new(
        limits_value["max_intents"].as_u64().unwrap() as usize,
        limits_value["max_accesses_per_intent"].as_u64().unwrap() as usize,
        limits_value["max_total_accesses"].as_u64().unwrap() as usize,
        limits_value["max_parallel_reads"].as_u64().unwrap() as usize,
        limits_value["max_buffered_result_bytes"].as_u64().unwrap(),
    )
    .unwrap();
    let plan = plan_effect_batch(&calls, &fixture_limits).unwrap();
    let expected_graph = case["conflict_graph_bytes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_u64().unwrap() as u8)
        .collect::<Vec<_>>();
    assert_eq!(plan.conflict_graph_bytes(), expected_graph);
    assert_eq!(plan.conflict_graph_digest(), case["conflict_graph_digest"]);
    assert_eq!(plan.plan_digest(), case["plan_digest"]);
    assert_eq!(
        fixture["failure_cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|failure| failure["expected_code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "effect_batch_bound_exceeded",
            "effect_access_invalid",
            "effect_access_invalid"
        ]
    );
}
