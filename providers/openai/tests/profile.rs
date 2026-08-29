use garive_llm::{RejectionKind, UnavailableKind};
use garive_provider_compatible::{ErrorDisposition, ErrorSignature};
use garive_provider_openai::build_profile;
use garive_provider_profile::{
    ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue, VendorProfileError,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/providers/vendor-connection-profiles-v1.json"
    ))
    .unwrap()
}

#[test]
fn shared_openai_profiles_construct_exact_redacted_adapter_config() {
    for case in fixture()["profiles"].as_array().unwrap() {
        if case["vendor"] != "openai" {
            continue;
        }
        let endpoint = if case["endpoint"]["kind"] == "default" {
            EndpointSelection::Default
        } else {
            EndpointSelection::Explicit(case["endpoint"]["value"].as_str().unwrap().into())
        };
        let headers = case["extra_headers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|header| {
                ExplicitHeader::new(
                    header["name"].as_str().unwrap(),
                    header["value"].as_str().unwrap(),
                    header["sensitive"].as_bool().unwrap(),
                )
                .unwrap()
            })
            .collect();
        let input = ConnectionInput::new(
            endpoint,
            SecretValue::new(case["credential"].as_str().unwrap()).unwrap(),
            headers,
        );
        let profile = build_profile(&input).unwrap();
        assert_eq!(
            profile.adapter_config.endpoint(),
            case["expected"]["endpoint"].as_str().unwrap()
        );
        if case["endpoint"]["kind"] == "default" {
            let auth = profile
                .adapter_config
                .headers()
                .iter()
                .find(|header| header.name() == "authorization")
                .unwrap();
            assert_eq!(auth.value(), case["expected"]["credential_value"]);
            assert!(auth.is_sensitive());
            assert!(!format!("{profile:?}").contains(case["credential"].as_str().unwrap()));
        }
    }
}

#[test]
fn shared_openai_error_rules_are_exact() {
    let policy = garive_provider_openai::default_error_policy().unwrap();
    for rule in fixture()["error_rules"]["openai"].as_array().unwrap() {
        let disposition = policy
            .classify(&ErrorSignature {
                status: rule["status"].as_u64().unwrap() as u16,
                protocol_type: rule["type"].as_str().unwrap().into(),
                code: rule["code"].as_str().map(str::to_owned),
            })
            .unwrap();
        match rule["expected"].as_str().unwrap() {
            "context_overflow" => assert_eq!(
                disposition,
                ErrorDisposition::Rejected(RejectionKind::ContextOverflow)
            ),
            "authentication" => assert_eq!(
                disposition,
                ErrorDisposition::Rejected(RejectionKind::Authentication)
            ),
            "rate_limited" => assert_eq!(
                disposition,
                ErrorDisposition::Unavailable(UnavailableKind::RateLimited)
            ),
            "model_unavailable" => assert_eq!(
                disposition,
                ErrorDisposition::Unavailable(UnavailableKind::ModelUnavailable)
            ),
            other => panic!("unknown expectation {other}"),
        }
    }
}

#[test]
fn shared_openai_reserved_header_case_returns_stable_code() {
    let fixture = fixture();
    let case = fixture["failure_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "openai-reserved-auth")
        .unwrap();
    let input = ConnectionInput::new(
        EndpointSelection::Default,
        SecretValue::new("secret").unwrap(),
        vec![ExplicitHeader::new("Authorization", "caller", true).unwrap()],
    );
    let error = build_profile(&input).unwrap_err();
    assert_eq!(error, VendorProfileError::ReservedHeader);
    assert_eq!(error.code(), case["code"].as_str().unwrap());
}
