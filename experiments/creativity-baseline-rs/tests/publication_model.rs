use std::sync::atomic::{AtomicUsize, Ordering};

use garive_creativity_baseline::{
    build_publication_evaluator, build_publication_generator, CredentialReferenceResolver,
    CredentialResolutionFailure, ModelEndpointConfig, ModelProtocol, NonSecretHeader,
};
use garive_provider_profile::SecretValue;

#[test]
fn invalid_nonsecret_configuration_fails_before_credential_resolution() {
    let resolver = Resolver::new("secret-one");
    let mut cases = Vec::new();

    let mut zero = responses();
    zero.request_timeout_ms = 0;
    cases.push(zero);
    let mut query = responses();
    query.endpoint = "https://models.example/v1/responses?unsafe=true".into();
    cases.push(query);
    let mut contradiction = responses();
    contradiction.messages_protocol_version = Some("v1".into());
    cases.push(contradiction);
    let mut duplicate = responses();
    duplicate.non_secret_headers.push(NonSecretHeader {
        name: "Authorization".into(),
        value: "not-secret".into(),
    });
    cases.push(duplicate);
    let mut reserved = responses();
    reserved.credential_header_name = "content-type".into();
    cases.push(reserved);
    let mut bad_messages = messages();
    bad_messages.messages_version_header_name = None;
    cases.push(bad_messages);

    for config in cases {
        assert!(build_publication_generator(config, &resolver).is_err());
    }
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn secrets_do_not_change_digest_and_every_nonsecret_coordinate_is_bound() {
    let first = build_publication_generator(responses(), &Resolver::new("secret-one"))
        .unwrap()
        .1;
    let second = build_publication_generator(responses(), &Resolver::new("secret-two"))
        .unwrap()
        .1;
    assert_eq!(first.port.config_digest, second.port.config_digest);
    assert!(!first.port.publishable);

    let base = responses();
    let base_digest = digest(base.clone());
    let mut variants = Vec::new();
    variants.push(with(base.clone(), |v| v.target_id.push('x')));
    variants.push(with(base.clone(), |v| v.model_id.push('x')));
    variants.push(with(base.clone(), |v| v.model_revision.push('x')));
    variants.push(with(base.clone(), |v| v.endpoint.push_str("-other")));
    variants.push(with(base.clone(), |v| v.credential_ref.push('x')));
    variants.push(with(base.clone(), |v| {
        v.credential_header_name = "x-token".into()
    }));
    variants.push(with(base.clone(), |v| v.credential_header_prefix.push('x')));
    variants.push(with(base.clone(), |v| {
        v.non_secret_headers[0].value.push('x')
    }));
    variants.push(with(base.clone(), |v| {
        v.non_secret_headers[0].name = "x-other-route".into()
    }));
    variants.push(with(base.clone(), |v| v.max_output_tokens += 1));
    variants.push(with(base.clone(), |v| v.connect_timeout_ms += 1));
    variants.push(with(base.clone(), |v| v.request_timeout_ms += 1));
    variants.push(with(base, |v| v.max_response_bytes += 1));
    for variant in variants {
        assert_ne!(digest(variant), base_digest);
    }

    let messages_base = messages();
    let messages_digest = digest(messages_base.clone());
    assert_ne!(messages_digest, base_digest);
    let mut version_name = messages_base.clone();
    version_name.messages_version_header_name = Some("other-version".into());
    assert_ne!(digest(version_name), messages_digest);
    let mut version_value = messages_base;
    version_value.messages_protocol_version = Some("2026-02-02".into());
    assert_ne!(digest(version_value), messages_digest);

    let evaluator = build_publication_evaluator(responses(), &Resolver::new("secret"))
        .unwrap()
        .1;
    assert_ne!(evaluator.port.config_digest, first.port.config_digest);
}

#[test]
fn both_public_https_compatible_dialects_are_publication_eligible() {
    let resolver = Resolver::new("secret");
    let mut responses = responses();
    responses.endpoint = "https://responses.example/v1/responses".into();
    let responses = build_publication_generator(responses, &resolver).unwrap().1;
    assert!(responses.port.publishable);
    assert_eq!(responses.protocol, ModelProtocol::ResponsesCompatible);

    let mut messages = messages();
    messages.endpoint = "https://messages.example/v1/messages".into();
    let messages = build_publication_evaluator(messages, &resolver).unwrap().1;
    assert!(messages.port.publishable);
    assert_eq!(messages.protocol, ModelProtocol::MessagesCompatible);
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 2);
}

fn digest(config: ModelEndpointConfig) -> String {
    build_publication_generator(config, &Resolver::new("secret"))
        .unwrap()
        .1
        .port
        .config_digest
}

fn with(
    mut value: ModelEndpointConfig,
    change: impl FnOnce(&mut ModelEndpointConfig),
) -> ModelEndpointConfig {
    change(&mut value);
    value
}

fn responses() -> ModelEndpointConfig {
    ModelEndpointConfig {
        protocol: ModelProtocol::ResponsesCompatible,
        target_id: "generator-target".into(),
        model_id: "model-a".into(),
        model_revision: "model-a-2026-08-30".into(),
        endpoint: "http://127.0.0.1:9/v1/responses".into(),
        credential_ref: "test-account".into(),
        credential_header_name: "authorization".into(),
        credential_header_prefix: "Bearer ".into(),
        non_secret_headers: vec![NonSecretHeader {
            name: "x-routing-key".into(),
            value: "route-a".into(),
        }],
        messages_version_header_name: None,
        messages_protocol_version: None,
        max_output_tokens: 1024,
        connect_timeout_ms: 100,
        request_timeout_ms: 200,
        max_response_bytes: 65536,
    }
}

fn messages() -> ModelEndpointConfig {
    ModelEndpointConfig {
        protocol: ModelProtocol::MessagesCompatible,
        target_id: "evaluator-target".into(),
        model_id: "model-b".into(),
        model_revision: "model-b-2026-08-30".into(),
        endpoint: "http://127.0.0.1:9/v1/messages".into(),
        credential_ref: "test-account".into(),
        credential_header_name: "x-api-key".into(),
        credential_header_prefix: String::new(),
        non_secret_headers: Vec::new(),
        messages_version_header_name: Some("protocol-version".into()),
        messages_protocol_version: Some("2026-01-01".into()),
        max_output_tokens: 1024,
        connect_timeout_ms: 100,
        request_timeout_ms: 200,
        max_response_bytes: 65536,
    }
}

struct Resolver {
    secret: String,
    calls: AtomicUsize,
}

impl Resolver {
    fn new(secret: &str) -> Self {
        Self {
            secret: secret.into(),
            calls: AtomicUsize::new(0),
        }
    }
}

impl CredentialReferenceResolver for Resolver {
    fn resolve(&self, _: &str) -> Result<SecretValue, CredentialResolutionFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        SecretValue::new(self.secret.clone()).map_err(|_| CredentialResolutionFailure)
    }
}
