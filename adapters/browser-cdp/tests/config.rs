use garive_adapter_browser_cdp::{CdpAdapterConfig, CdpAdapterConfigError, CdpLimits};

#[test]
fn endpoint_is_explicit_loopback_without_credentials_or_discovery() {
    let limits = CdpLimits::new(1_048_576, 1, 128, 30_000).expect("limits");
    assert!(CdpAdapterConfig::new(
        "ws://127.0.0.1:9222/devtools/browser/capability-token",
        limits
    )
    .is_ok());
    for endpoint in [
        "wss://127.0.0.1:9222/devtools/browser/token",
        "ws://example.test:9222/devtools/browser/token",
        "ws://127.0.0.1/devtools/browser/token",
        "ws://user:secret@127.0.0.1:9222/devtools/browser/token",
    ] {
        assert_eq!(
            CdpAdapterConfig::new(endpoint, limits),
            Err(CdpAdapterConfigError::InvalidEndpoint)
        );
    }
}

#[test]
fn limits_are_nonzero_and_hard_bounded() {
    assert_eq!(
        CdpLimits::new(16_777_217, 1, 1, 1),
        Err(CdpAdapterConfigError::InvalidLimits)
    );
    assert_eq!(
        CdpLimits::new(1, 2, 1, 1),
        Err(CdpAdapterConfigError::InvalidLimits)
    );
}
