use std::collections::BTreeSet;

use garive_skill::{
    activate_skills, ActivationMode, ActivationPolicy, CapabilityReference, ContentBinding,
    ExactToolReference, SkillActivationRequest, SkillActivationResult, SkillDefinition,
    SkillErrorCode,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/skill-activation-v1.json"
    ))
    .expect("valid shared fixture")
}

fn text(value: &Value, field: &str) -> String {
    value[field].as_str().expect("string field").to_owned()
}

fn capability(value: &Value) -> CapabilityReference {
    CapabilityReference::new(
        text(value, "kind"),
        text(value, "name"),
        text(value, "exact_revision"),
        text(value, "contract_version"),
    )
    .expect("valid capability")
}

fn tool(value: &Value) -> ExactToolReference {
    ExactToolReference::new(text(value, "name"), text(value, "exact_revision")).expect("valid tool")
}

fn definition(value: &Value) -> SkillDefinition {
    let activation = match value["activation"]["kind"].as_str().expect("kind") {
        "explicit_only" => ActivationPolicy::ExplicitOnly,
        "tagged" => ActivationPolicy::tagged(
            value["activation"]["tags"]
                .as_array()
                .expect("tags")
                .iter()
                .map(|tag| tag.as_str().expect("tag").to_owned()),
        )
        .expect("valid tags"),
        other => panic!("unexpected policy {other}"),
    };
    SkillDefinition::new(
        text(value, "skill_id"),
        text(value, "skill_revision"),
        text(value, "name"),
        text(value, "description"),
        ContentBinding::new(
            text(&value["instructions"], "digest"),
            text(&value["instructions"], "inline_utf8"),
        )
        .expect("valid content"),
        activation,
        value["required_capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .map(capability)
            .collect(),
        value["allowed_tool_references"]
            .as_array()
            .expect("tools")
            .iter()
            .map(tool)
            .collect(),
        value["max_instruction_bytes"].as_u64().expect("bound"),
        text(value, "contract_version"),
    )
    .expect("valid definition")
}

fn request(value: &Value) -> SkillActivationRequest {
    SkillActivationRequest::new(
        text(value, "activation_id"),
        text(value, "turn_id"),
        text(value, "execution_id"),
        value["iteration"].as_u64().expect("iteration"),
        ActivationMode::from_wire(value["mode"].as_str().expect("mode")).expect("valid mode"),
        value
            .get("requested_skill_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        value["trusted_tags"]
            .as_array()
            .expect("tags")
            .iter()
            .map(|tag| tag.as_str().expect("tag").to_owned())
            .collect(),
        value["through_position"].as_u64().expect("position"),
        value["max_active_skills"].as_u64().expect("count") as u32,
        value["max_total_instruction_bytes"]
            .as_u64()
            .expect("bytes"),
    )
    .expect("valid request")
}

#[test]
fn shared_vectors_cover_digests_order_bounds_and_failures() {
    let root = fixture();
    let definitions: Vec<_> = root["definitions"]
        .as_array()
        .expect("definitions")
        .iter()
        .map(definition)
        .collect();
    for (source, parsed) in root["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(&definitions)
    {
        assert_eq!(
            parsed.definition_digest().unwrap(),
            text(source, "expected_definition_digest")
        );
    }
    let capabilities: BTreeSet<_> = root["available_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(capability)
        .collect();
    let tools: BTreeSet<_> = root["available_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(tool)
        .collect();

    for case in root["cases"].as_array().expect("cases") {
        let request = request(&case["request"]);
        if let Some(expected_digest) = case.get("expected_request_digest") {
            assert_eq!(
                request.request_digest().unwrap(),
                expected_digest.as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
        let expected = &case["expected"];
        match (
            expected["status"].as_str().unwrap(),
            activate_skills(&definitions, &capabilities, &tools, &request),
        ) {
            ("none", Ok(SkillActivationResult::None)) => {}
            (
                "activated",
                Ok(SkillActivationResult::Activated {
                    ordered_skills,
                    truncated,
                }),
            ) => {
                let ids: Vec<_> = ordered_skills
                    .iter()
                    .map(|skill| skill.skill_id())
                    .collect();
                let expected_ids: Vec<_> = expected["skill_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| id.as_str().unwrap())
                    .collect();
                assert_eq!(ids, expected_ids, "{}", case["name"]);
                assert_eq!(
                    truncated,
                    expected["truncated"].as_bool().unwrap(),
                    "{}",
                    case["name"]
                );
            }
            ("error", Err(error)) => assert_eq!(
                error.code().wire_name(),
                expected["code"].as_str().unwrap(),
                "{}",
                case["name"]
            ),
            (_, actual) => panic!("unexpected result for {}: {actual:?}", case["name"]),
        }
    }
}

#[test]
fn rejects_unsupported_or_snapshot_widening_inputs() {
    assert_eq!(
        ActivationMode::from_wire("semantic").unwrap_err().code(),
        SkillErrorCode::ActivationModeUnsupported
    );
    assert_eq!(
        ContentBinding::new("0".repeat(64), "wrong")
            .unwrap_err()
            .code(),
        SkillErrorCode::InstructionDigestMismatch
    );
    let root = fixture();
    let code = definition(&root["definitions"][1]);
    let tagged = request(&root["cases"][1]["request"]);
    assert_eq!(
        activate_skills(
            std::slice::from_ref(&code),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &tagged
        )
        .unwrap_err()
        .code(),
        SkillErrorCode::RequiredCapabilityUnavailable
    );
    let capabilities = BTreeSet::from([capability(&root["available_capabilities"][0])]);
    assert_eq!(
        activate_skills(
            std::slice::from_ref(&code),
            &capabilities,
            &BTreeSet::new(),
            &tagged
        )
        .unwrap_err()
        .code(),
        SkillErrorCode::SkillNotEnabled
    );

    let conflict = SkillDefinition::new(
        "code",
        "2",
        "Other",
        "Conflicting definition.",
        ContentBinding::from_inline("Other."),
        ActivationPolicy::tagged(["code".to_owned(), "rust".to_owned()]).unwrap(),
        vec![],
        vec![],
        64,
        "1",
    )
    .unwrap();
    assert_eq!(
        activate_skills(&[code, conflict], &capabilities, &BTreeSet::new(), &tagged)
            .unwrap_err()
            .code(),
        SkillErrorCode::ActivationConflict
    );
}

#[test]
fn activation_identity_is_not_request_semantics() {
    let root = fixture();
    let source = &root["cases"][0]["request"];
    let first = request(source);
    let mut changed = source.clone();
    changed["activation_id"] = Value::String("activation-other".to_owned());
    assert_eq!(
        first.request_digest().unwrap(),
        request(&changed).request_digest().unwrap()
    );
}
