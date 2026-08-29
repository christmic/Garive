use garive_adapter_openai_responses::{
    Header, ResponsesAdapter, ResponsesAdapterConfig, ResponsesAdapterError,
};

#[test]
fn construction_requires_explicit_absolute_endpoint() {
    for endpoint in ["", "/v1/responses", "ftp://example.test/responses"] {
        assert_eq!(
            ResponsesAdapterConfig::new(endpoint, vec![]),
            Err(ResponsesAdapterError::InvalidEndpoint)
        );
    }
    let config = ResponsesAdapterConfig::new("https://example.test/inference", vec![]).unwrap();
    assert_eq!(config.endpoint(), "https://example.test/inference");
}

#[test]
fn headers_are_validated_deduplicated_and_redacted() {
    assert_eq!(
        Header::new("accept", "application/json", false),
        Err(ResponsesAdapterError::InvalidHeader)
    );
    assert_eq!(
        Header::new("bad name", "value", false),
        Err(ResponsesAdapterError::InvalidHeader)
    );
    let secret = Header::new("authorization", "Bearer fixture-secret", true).unwrap();
    assert!(!format!("{secret:?}").contains("fixture-secret"));
    assert_eq!(secret.value(), "Bearer fixture-secret");
    assert!(secret.is_sensitive());
    assert_eq!(
        ResponsesAdapterConfig::new(
            "https://example.test/inference",
            vec![secret.clone(), secret]
        ),
        Err(ResponsesAdapterError::InvalidHeader)
    );
}

#[test]
fn adapter_debug_never_exposes_sensitive_headers() {
    let config = ResponsesAdapterConfig::new(
        "https://example.test/inference",
        vec![Header::new("x-token", "fixture-secret", true).unwrap()],
    )
    .unwrap();
    let debug = format!("{:?}", ResponsesAdapter::new(config));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("fixture-secret"));
}
