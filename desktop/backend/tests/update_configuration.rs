use garive_desktop::desktop_updater_configured;

#[test]
fn updater_requires_one_bounded_https_channel_and_public_key() {
    let admitted = serde_json::json!({
        "endpoints": ["https://releases.example.com/garive/{{target}}/{{arch}}/{{current_version}}"],
        "pubkey": "untrusted comment: minisign public key\nRWQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    });
    assert!(desktop_updater_configured(Some(&admitted)));
    assert!(!desktop_updater_configured(None));
    assert!(!desktop_updater_configured(Some(&serde_json::json!({}))));
}

#[test]
fn updater_rejects_insecure_ambiguous_or_unbounded_configuration() {
    let cases = [
        serde_json::json!({"endpoints": ["http://releases.example.com/latest.json"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://user:secret@releases.example.com/latest.json"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json#fragment"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://127.0.0.1/latest.json"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://localhost/latest.json"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://a.example/latest", "https://b.example/latest", "https://c.example/latest"], "pubkey": "key"}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json"], "pubkey": "  "}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json"], "pubkey": "k".repeat(16 * 1024 + 1)}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json"], "pubkey": "key", "dangerousInsecureTransportProtocol": true}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json"], "pubkey": "key", "dangerousAcceptInvalidCerts": true}),
        serde_json::json!({"endpoints": ["https://releases.example.com/latest.json"], "pubkey": "key", "dangerousAcceptInvalidHostnames": true}),
    ];
    for case in cases {
        assert!(!desktop_updater_configured(Some(&case)), "{case}");
    }
}
