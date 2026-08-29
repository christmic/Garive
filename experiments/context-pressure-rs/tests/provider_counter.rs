use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use garive_adapter_anthropic_messages::ThinkingConfig;
use garive_context_pressure::{
    load_corpus, measure_context_pressure, AnthropicProviderCounter,
    AnthropicProviderCounterConfig, TokenCountExchangePort, TokenCounter, TokenCounterFailure,
};
use garive_llm::{ModelCapability, ModelInputContent, ModelInputItem, ModelRole};
use garive_provider_anthropic::{build_token_count_profile, TokenCountHttpRequest};
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue};
use serde_json::Value;

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/context-pressure-corpus-v1.json"
));

type RecordedRequests = Arc<Mutex<Vec<Value>>>;
type TestCounter = AnthropicProviderCounter<RecordingPort>;

#[derive(Clone)]
struct RecordingPort {
    endpoint: String,
    requests: RecordedRequests,
    eligible: bool,
    revision: &'static str,
    response: &'static [u8],
}

impl TokenCountExchangePort for RecordingPort {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn transport_revision(&self) -> &str {
        self.revision
    }

    fn publication_eligible(&self) -> bool {
        self.eligible
    }

    fn execute(&self, request: &TokenCountHttpRequest) -> Result<Vec<u8>, TokenCounterFailure> {
        let body = serde_json::from_slice(request.body()).map_err(|_| TokenCounterFailure)?;
        self.requests.lock().unwrap().push(body);
        Ok(self.response.to_vec())
    }
}

fn counter(
    secret: &str,
    publishable: bool,
) -> Result<(TestCounter, RecordedRequests), TokenCounterFailure> {
    custom_counter(
        secret,
        publishable,
        BTreeSet::from([ModelCapability::Text, ModelCapability::Tools]),
        br#"{"input_tokens":32}"#,
    )
}

fn custom_counter(
    secret: &str,
    publishable: bool,
    capabilities: BTreeSet<ModelCapability>,
    response: &'static [u8],
) -> Result<(TestCounter, RecordedRequests), TokenCounterFailure> {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let profile = build_token_count_profile(&ConnectionInput::new(
        EndpointSelection::Default,
        SecretValue::new(secret).unwrap(),
        vec![],
    ))
    .unwrap();
    let deployment = MessagesDeployment {
        target_id: "evidence-target".into(),
        model_id: "evidence-model".into(),
        capabilities,
        default_max_output_tokens: None,
        media_bindings: BTreeMap::new(),
        thinking: None,
        error_policy: ProtocolErrorPolicy::default(),
    };
    let endpoint = profile.endpoint().to_owned();
    let value = AnthropicProviderCounter::new(
        AnthropicProviderCounterConfig {
            counter_revision: "composition-v1".into(),
            deployment,
            profile,
            projection_max_output_tokens: 1,
            publishable,
        },
        RecordingPort {
            endpoint,
            requests: Arc::clone(&requests),
            eligible: false,
            revision: "recording-v1",
            response,
        },
    )?;
    Ok((value, requests))
}

#[test]
fn corpus_uses_normal_mapping_before_exact_count_projection() {
    let corpus = load_corpus(CORPUS.as_bytes()).unwrap();
    let (counter, requests) = counter("first-secret", false).unwrap();
    let run = measure_context_pressure(&corpus, &counter).unwrap();
    assert_eq!(run.summary.ordered_cases.len(), 4);

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert!(requests
        .iter()
        .all(|body| body.get("max_tokens").is_none() && body.get("stream").is_none()));
    let long_running = &requests[3];
    assert!(long_running["system"]
        .to_string()
        .contains("resumed from durable position 41"));
    assert!(long_running["messages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|message| message["role"] == "user" || message["role"] == "assistant"));
}

#[test]
fn secret_is_not_bound_and_fake_port_cannot_publish() {
    let (first, _) = counter("first-secret", false).unwrap();
    let (second, _) = counter("different-secret", false).unwrap();
    assert_eq!(
        first.descriptor().config_digest,
        second.descriptor().config_digest
    );
    assert!(counter("first-secret", true).is_err());
}

#[test]
fn unsupported_input_and_malformed_response_fail_closed() {
    let (counter, _) = counter("secret", false).unwrap();
    assert!(counter
        .count_input_tokens(&[ModelInputItem::ReasoningReference {
            reference: "opaque".into(),
        }])
        .is_err());

    let (counter, _) = custom_counter(
        "secret",
        false,
        BTreeSet::from([ModelCapability::Text]),
        br#"{"input_tokens":0}"#,
    )
    .unwrap();
    assert!(counter
        .count_input_tokens(&[ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("count this".into())],
        }])
        .is_err());

    let (counter, _) = custom_counter(
        "secret",
        false,
        BTreeSet::from([ModelCapability::Text]),
        br#"{"input_tokens":1}"#,
    )
    .unwrap();
    assert!(counter
        .count_input_tokens(&[ModelInputItem::ToolObservation {
            model_call_id: "call-1".into(),
            result_json: "{}".into(),
        }])
        .is_err());
}

#[test]
fn every_nonsecret_route_value_is_bound_to_the_digest() {
    let digest = |endpoint: EndpointSelection,
                  model: &str,
                  trace: &str,
                  revision: &'static str,
                  thinking: Option<ThinkingConfig>| {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let profile = build_token_count_profile(&ConnectionInput::new(
            endpoint,
            SecretValue::new("secret").unwrap(),
            vec![ExplicitHeader::new("x-trace", trace, false).unwrap()],
        ))
        .unwrap();
        let deployment = MessagesDeployment {
            target_id: "target".into(),
            model_id: model.into(),
            capabilities: BTreeSet::from([ModelCapability::Text]),
            default_max_output_tokens: None,
            media_bindings: BTreeMap::new(),
            thinking,
            error_policy: ProtocolErrorPolicy::default(),
        };
        let endpoint = profile.endpoint().to_owned();
        AnthropicProviderCounter::new(
            AnthropicProviderCounterConfig {
                counter_revision: "composition-v1".into(),
                deployment,
                profile,
                projection_max_output_tokens: 1,
                publishable: false,
            },
            RecordingPort {
                endpoint,
                requests,
                eligible: false,
                revision,
                response: br#"{"input_tokens":1}"#,
            },
        )
        .unwrap()
        .descriptor()
        .config_digest
        .clone()
    };
    let base = digest(
        EndpointSelection::Default,
        "model-a",
        "trace-a",
        "tx-a",
        None,
    );
    for changed in [
        digest(
            EndpointSelection::Explicit("https://example.test/count".into()),
            "model-a",
            "trace-a",
            "tx-a",
            None,
        ),
        digest(
            EndpointSelection::Default,
            "model-b",
            "trace-a",
            "tx-a",
            None,
        ),
        digest(
            EndpointSelection::Default,
            "model-a",
            "trace-b",
            "tx-a",
            None,
        ),
        digest(
            EndpointSelection::Default,
            "model-a",
            "trace-a",
            "tx-b",
            None,
        ),
        digest(
            EndpointSelection::Default,
            "model-a",
            "trace-a",
            "tx-a",
            Some(ThinkingConfig::Disabled),
        ),
    ] {
        assert_ne!(base, changed);
    }
}
