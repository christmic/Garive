use garive_anthropic_messages::CreateMessageRequest;
use garive_provider_anthropic::{
    build_token_count_profile, decode_token_count, project_token_count_request,
    AnthropicTokenCountError,
};
use garive_provider_profile::{
    ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue, VendorProfileError,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/providers/anthropic-token-count-v1.json"
    ))
    .unwrap()
}

#[test]
fn shared_projection_preserves_only_counted_request_fields() {
    let fixture = fixture();
    let cases = fixture["projection_cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let request: CreateMessageRequest =
            serde_json::from_value(case["create_request"].clone()).unwrap();
        let actual = project_token_count_request(&request).unwrap();
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["expected_count_request"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn shared_response_and_failure_shapes_are_exact() {
    let fixture = fixture();
    for case in fixture["response_cases"].as_array().unwrap() {
        let body = serde_json::to_vec(&case["body"]).unwrap();
        assert_eq!(
            decode_token_count(&body).unwrap().input_tokens(),
            case["expected"].as_u64().unwrap()
        );
    }

    let failures = fixture["failure_cases"].as_array().unwrap();
    for case in failures.iter().filter(|case| case.get("body").is_some()) {
        let body = serde_json::to_vec(&case["body"]).unwrap();
        let error = decode_token_count(&body).unwrap_err();
        assert_eq!(error.code(), case["code"].as_str().unwrap());
    }
    assert_eq!(
        decode_token_count(br#"{"input_tokens":1,"input_tokens":2}"#).unwrap_err(),
        AnthropicTokenCountError::InvalidResponse
    );
}

#[test]
fn request_extensions_and_invalid_native_requests_fail_before_projection() {
    let fixture = fixture();
    let case = &fixture["projection_cases"][0];
    let mut request: CreateMessageRequest =
        serde_json::from_value(case["create_request"].clone()).unwrap();
    request.extensions.insert("hosted".into(), json!(true));
    assert_eq!(
        project_token_count_request(&request).unwrap_err(),
        AnthropicTokenCountError::UnsupportedExtension
    );

    request.extensions.clear();
    request.messages.clear();
    assert_eq!(
        project_token_count_request(&request).unwrap_err(),
        AnthropicTokenCountError::InvalidRequest
    );
}

#[test]
fn shared_profile_prepares_the_exact_redacted_exchange() {
    let fixture = fixture();
    let input = ConnectionInput::new(
        EndpointSelection::Default,
        SecretValue::new("fixture-secret").unwrap(),
        vec![ExplicitHeader::new("x-trace", "trace-1", false).unwrap()],
    );
    let profile = build_token_count_profile(&input).unwrap();
    assert_eq!(
        profile.endpoint(),
        fixture["profile"]["default_endpoint"].as_str().unwrap()
    );
    assert_eq!(
        profile
            .headers()
            .iter()
            .find(|header| header.name() == "anthropic-version")
            .unwrap()
            .value(),
        fixture["profile"]["protocol_version"].as_str().unwrap()
    );
    assert!(!format!("{profile:?}").contains("fixture-secret"));

    let request: CreateMessageRequest =
        serde_json::from_value(fixture["projection_cases"][0]["create_request"].clone()).unwrap();
    let projected = project_token_count_request(&request).unwrap();
    let exchange = profile.prepare(&projected).unwrap();
    assert_eq!(exchange.method(), "POST");
    assert_eq!(exchange.uri(), profile.endpoint());
    assert_eq!(
        serde_json::from_slice::<Value>(exchange.body()).unwrap(),
        fixture["projection_cases"][0]["expected_count_request"]
    );
    assert!(!format!("{exchange:?}").contains("fixture-secret"));
}

#[test]
fn shared_explicit_profile_failures_are_stable() {
    let secret = || SecretValue::new("fixture-secret").unwrap();
    let relative = ConnectionInput::new(
        EndpointSelection::Explicit("/v1/messages/count_tokens".into()),
        secret(),
        vec![],
    );
    assert_eq!(
        build_token_count_profile(&relative).unwrap_err(),
        VendorProfileError::InvalidEndpoint
    );

    let reserved = ConnectionInput::new(
        EndpointSelection::Default,
        secret(),
        vec![ExplicitHeader::new("Anthropic-Version", "caller", false).unwrap()],
    );
    assert_eq!(
        build_token_count_profile(&reserved).unwrap_err(),
        VendorProfileError::ReservedHeader
    );
}
