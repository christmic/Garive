use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

use garive_context_pressure::{
    load_corpus, measure_context_pressure, AnthropicProviderCounter,
    AnthropicProviderCounterConfig, TokenCountExchangePort, TokenCounter, TokenCounterFailure,
};
use garive_llm::ModelCapability;
use garive_provider_anthropic::{build_token_count_profile, TokenCountHttpRequest};
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, EndpointSelection, SecretValue};
use serde_json::Value;

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/context-pressure-corpus-v1.json"
));

type RecordedRequests = Arc<Mutex<Vec<Value>>>;
type TestCounter = AnthropicProviderCounter<RecordingPort>;

#[derive(Clone)]
struct RecordingPort {
    requests: RecordedRequests,
    eligible: bool,
}

impl TokenCountExchangePort for RecordingPort {
    fn transport_revision(&self) -> &str {
        "recording-v1"
    }

    fn publication_eligible(&self) -> bool {
        self.eligible
    }

    fn execute(&self, request: &TokenCountHttpRequest) -> Result<Vec<u8>, TokenCounterFailure> {
        let body = serde_json::from_slice(request.body()).map_err(|_| TokenCounterFailure)?;
        self.requests.lock().unwrap().push(body);
        Ok(br#"{"input_tokens":32}"#.to_vec())
    }
}

fn counter(
    secret: &str,
    publishable: bool,
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
        capabilities: BTreeSet::from([ModelCapability::Text, ModelCapability::Tools]),
        default_max_output_tokens: None,
        media_bindings: BTreeMap::new(),
        thinking: None,
        error_policy: ProtocolErrorPolicy::default(),
    };
    let value = AnthropicProviderCounter::new(
        AnthropicProviderCounterConfig {
            counter_revision: "composition-v1".into(),
            deployment,
            profile,
            projection_max_output_tokens: 1,
            publishable,
        },
        RecordingPort {
            requests: Arc::clone(&requests),
            eligible: false,
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
