use garive_adapter_anthropic_messages::{
    Header, MessagesAdapter, MessagesAdapterConfig, MessagesAdapterError,
};

#[test]
fn construction_requires_every_deployment_value() {
    for endpoint in ["", "/v1/messages", "ftp://example.test/messages"] {
        assert_eq!(
            MessagesAdapterConfig::new(endpoint, vec![], "x-protocol-version", "2026-01-01"),
            Err(MessagesAdapterError::InvalidEndpoint)
        );
    }
    assert_eq!(
        MessagesAdapterConfig::new(
            "https://example.test/messages",
            vec![],
            "x-protocol-version",
            ""
        ),
        Err(MessagesAdapterError::InvalidProtocolVersion)
    );
}

#[test]
fn version_and_sensitive_headers_are_explicit() {
    let secret = Header::new("authorization", "Bearer fixture-secret", true).unwrap();
    let config = MessagesAdapterConfig::new(
        "https://example.test/messages",
        vec![secret],
        "x-protocol-version",
        "2026-01-01",
    )
    .unwrap();
    assert_eq!(config.version_header_name(), "x-protocol-version");
    assert_eq!(config.protocol_version(), "2026-01-01");
    let debug = format!("{:?}", MessagesAdapter::new(config));
    assert!(debug.contains("<redacted>"));
    assert!(!debug.contains("fixture-secret"));
}

#[test]
fn reserved_duplicate_and_version_headers_are_rejected() {
    let version = Header::new("x-protocol-version", "duplicate", false).unwrap();
    let accept = Header::new("accept", "application/json", false).unwrap();
    for headers in [vec![version], vec![accept]] {
        assert_eq!(
            MessagesAdapterConfig::new(
                "https://example.test/messages",
                headers,
                "x-protocol-version",
                "2026-01-01"
            ),
            Err(MessagesAdapterError::InvalidHeader)
        );
    }
}
