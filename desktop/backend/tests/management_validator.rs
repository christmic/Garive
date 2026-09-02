use garive_desktop::{
    builtin_management_validator, BuiltinManagementValidator, ANTHROPIC_MESSAGES_PROFILE_ID,
    DESKTOP_AGENT_REVISION, DESKTOP_WORKSPACE_AGENT_REVISION, OPENAI_RESPONSES_PROFILE_ID,
};
use garive_runtime::{
    ManagementCommitBody, ManagementConfigError, ManagementValidator,
    MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
};

fn body(profile: &str, definition: &str) -> ManagementCommitBody {
    ManagementCommitBody {
        schema_version: MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
        profile_id: profile.to_owned(),
        endpoint_override: Some("https://api.openai.com/v1".to_owned()),
        model_target_id: "gpt-5.6".to_owned(),
        model_id: "gpt-5.6".to_owned(),
        deployment_id: "tok9-flash".to_owned(),
        definition_id: definition.to_owned(),
        api_key: "sk-test-1234567890".to_owned(),
        runtime_id: "runtime-7e22bcbe-bfa4-4c8f-a0c3-94e07be8f363".to_owned(),
    }
}

#[test]
fn accepts_openai_with_desktop_agent() {
    let validator = BuiltinManagementValidator;
    assert!(validator
        .validate(&body(OPENAI_RESPONSES_PROFILE_ID, DESKTOP_AGENT_REVISION))
        .is_ok());
}

#[test]
fn accepts_anthropic_with_workspace_agent() {
    let validator = BuiltinManagementValidator;
    assert!(validator
        .validate(&body(
            ANTHROPIC_MESSAGES_PROFILE_ID,
            DESKTOP_WORKSPACE_AGENT_REVISION,
        ))
        .is_ok());
}

#[test]
fn rejects_unknown_profile_id() {
    let validator = BuiltinManagementValidator;
    let error = validator
        .validate(&body("openai.responses.v9", DESKTOP_AGENT_REVISION))
        .unwrap_err();
    assert_eq!(error, ManagementConfigError::ProfileUnknown);
}

#[test]
fn rejects_unknown_definition_id() {
    let validator = BuiltinManagementValidator;
    let error = validator
        .validate(&body(OPENAI_RESPONSES_PROFILE_ID, "desktop.agent.v9"))
        .unwrap_err();
    assert_eq!(error, ManagementConfigError::DefinitionUnknown);
}

#[test]
fn builtin_management_validator_factory_returns_same_allowlist() {
    let validator = builtin_management_validator();
    assert!(validator
        .validate(&body(OPENAI_RESPONSES_PROFILE_ID, DESKTOP_AGENT_REVISION))
        .is_ok());
    assert!(matches!(
        validator.validate(&body("unknown.profile", DESKTOP_AGENT_REVISION)),
        Err(ManagementConfigError::ProfileUnknown)
    ));
}
