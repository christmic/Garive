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
fn secrets_are_validated_and_redacted() {
    assert_eq!(
        SecretValue::new(""),
        Err(VendorProfileError::EmptyCredential)
    );
    assert_eq!(
        SecretValue::new("secret\nvalue"),
        Err(VendorProfileError::InvalidCredential)
    );
    let secret = SecretValue::new("fixture-secret").unwrap();
    assert!(!format!("{secret:?}").contains("fixture-secret"));
}

#[test]
fn endpoint_duplicate_and_reserved_headers_fail_before_profile_construction() {
    let secret = SecretValue::new("fixture-secret").unwrap();
    let relative = ConnectionInput::new(
        EndpointSelection::Explicit("/responses".into()),
        secret.clone(),
        vec![],
    );
    assert!(matches!(
        relative.resolve("https://default.test/responses", &[]),
        Err(VendorProfileError::InvalidEndpoint)
    ));

    let header = ExplicitHeader::new("x-extra", "one", false).unwrap();
    let duplicate = ConnectionInput::new(
        EndpointSelection::Default,
        secret.clone(),
        vec![
            header.clone(),
            ExplicitHeader::new("X-Extra", "two", false).unwrap(),
        ],
    );
    assert!(matches!(
        duplicate.resolve("https://default.test/responses", &[]),
        Err(VendorProfileError::DuplicateHeader)
    ));

    let reserved = ConnectionInput::new(EndpointSelection::Default, secret, vec![header]);
    assert!(matches!(
        reserved.resolve("https://default.test/responses", &["x-extra"]),
        Err(VendorProfileError::ReservedHeader)
    ));
}

#[test]
fn shared_generic_failure_cases_return_stable_codes() {
    for case in fixture()["failure_cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let error = match name {
            "empty-credential" => SecretValue::new("").unwrap_err(),
            "credential-line-break" => SecretValue::new("secret\nvalue").unwrap_err(),
            "relative-endpoint" => ConnectionInput::new(
                EndpointSelection::Explicit("/responses".into()),
                SecretValue::new("secret").unwrap(),
                vec![],
            )
            .resolve("https://default.test/responses", &[])
            .unwrap_err(),
            "invalid-header-name" => ExplicitHeader::new("bad header", "value", false).unwrap_err(),
            "duplicate-header" => ConnectionInput::new(
                EndpointSelection::Default,
                SecretValue::new("secret").unwrap(),
                vec![
                    ExplicitHeader::new("x-extra", "one", false).unwrap(),
                    ExplicitHeader::new("X-Extra", "two", false).unwrap(),
                ],
            )
            .resolve("https://default.test/responses", &[])
            .unwrap_err(),
            _ => continue,
        };
        assert_eq!(error.code(), case["code"].as_str().unwrap(), "{name}");
    }
}
