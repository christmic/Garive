use garive_llm::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings, ModelRequest,
    ModelRequestId, ModelRole, ModelTargetId, TextMode,
};
use garive_runtime::{canonical_model_request_digest, RuntimeCommandError};

fn request() -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request"),
        target_id: ModelTargetId::new("target"),
        required_capabilities: vec![ModelCapability::Text],
        input_items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("hello".into())],
        }],
        tools: vec![],
        output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        trace_metadata: vec![("turn_id".into(), "turn".into())],
    }
}

#[test]
fn neutral_request_digest_matches_the_c6_canonical_form() {
    assert_eq!(
        canonical_model_request_digest(&request()).unwrap(),
        "b742b706c189ab2682b520c5ef0fc0168990653e588598f3e48696579fc43fb4"
    );
    let mut same_request_new_identity = request();
    same_request_new_identity.request_id = ModelRequestId::new("retry");
    assert_eq!(
        canonical_model_request_digest(&same_request_new_identity),
        canonical_model_request_digest(&request())
    );
}

#[test]
fn invalid_or_changed_requests_do_not_share_the_digest() {
    let mut changed = request();
    changed.input_items = vec![ModelInputItem::Message {
        role: ModelRole::User,
        content: vec![ModelInputContent::Text("changed".into())],
    }];
    assert_ne!(
        canonical_model_request_digest(&changed),
        canonical_model_request_digest(&request())
    );
    changed.target_id = ModelTargetId::new("");
    assert_eq!(
        canonical_model_request_digest(&changed),
        Err(RuntimeCommandError::InvalidCommand)
    );
}
