use garive_provider_profile::{
    ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue, VendorProfileError,
};

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
