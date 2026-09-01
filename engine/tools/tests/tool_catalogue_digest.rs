use garive_tools::{
    tool_catalogue_digest, ExecutionCapability, ExecutionRequirements, PreparationErrorCode,
    ReplayClass, ToolDefinition,
};
use serde_json::json;

#[test]
fn catalogue_digest_is_order_independent_and_meaning_sensitive() {
    let alpha = definition("alpha", "Read alpha");
    let beta = definition("beta", "Read beta");
    let forward = tool_catalogue_digest(&[alpha.clone(), beta.clone()]).unwrap();
    let reverse = tool_catalogue_digest(&[beta, alpha.clone()]).unwrap();
    assert_eq!(forward, reverse);
    assert_ne!(
        forward,
        tool_catalogue_digest(&[alpha, definition("beta", "Changed meaning")]).unwrap()
    );
    assert_eq!(
        tool_catalogue_digest(&[
            definition("duplicate", "One"),
            definition("duplicate", "Two")
        ])
        .unwrap_err()
        .code(),
        PreparationErrorCode::InvalidToolDefinition
    );
}

fn definition(name: &str, description: &str) -> ToolDefinition {
    ToolDefinition::new(
        name,
        "v1",
        description,
        json!({
            "type":"object",
            "properties":{},
            "required":[],
            "additionalProperties":false
        }),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 1_000).unwrap(),
        ReplayClass::ReadOnly,
    )
    .unwrap()
}
