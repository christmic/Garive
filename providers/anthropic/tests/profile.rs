use garive_llm::{RejectionKind, UnavailableKind};
use garive_provider_anthropic::build_profile;
use garive_provider_compatible::{ErrorDisposition, ErrorSignature};
use garive_provider_profile::{ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/providers/vendor-connection-profiles-v1.json"
    ))
    .unwrap()
}

#[test]
fn shared_anthropic_profiles_construct_exact_redacted_adapter_config() {
    for case in fixture()["profiles"].as_array().unwrap() {
        if case["vendor"] != "anthropic" {
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
                .find(|header| header.name() == "x-api-key")
                .unwrap();
            assert_eq!(auth.value(), case["expected"]["credential_value"]);
            assert!(auth.is_sensitive());
            assert_eq!(
                profile.adapter_config.version_header_name(),
                case["expected"]["version_header"]
            );
            assert_eq!(
                profile.adapter_config.protocol_version(),
                case["expected"]["protocol_version"]
            );
            assert!(!format!("{profile:?}").contains(case["credential"].as_str().unwrap()));
        }
    }
}

#[test]
fn shared_anthropic_error_rules_are_exact_and_context_is_unclassified() {
    let policy = garive_provider_anthropic::default_error_policy().unwrap();
    for rule in fixture()["error_rules"]["anthropic"].as_array().unwrap() {
        let disposition = policy.classify(&ErrorSignature {
            status: rule["status"].as_u64().unwrap() as u16,
            protocol_type: rule["type"].as_str().unwrap().into(),
            code: rule["code"].as_str().map(str::to_owned),
        });
        match rule["expected"].as_str().unwrap() {
            "authentication" => assert_eq!(
                disposition,
                Some(ErrorDisposition::Rejected(RejectionKind::Authentication))
            ),
            "rate_limited" => assert_eq!(
                disposition,
                Some(ErrorDisposition::Unavailable(UnavailableKind::RateLimited))
            ),
            "model_unavailable" => assert_eq!(
                disposition,
                Some(ErrorDisposition::Unavailable(
                    UnavailableKind::ModelUnavailable
                ))
            ),
            "unclassified_protocol_error" => assert_eq!(disposition, None),
            other => panic!("unknown expectation {other}"),
        }
    }
}
