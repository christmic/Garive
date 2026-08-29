use std::collections::BTreeSet;

use garive_llm::{
    ModelCapability, ModelInputContent, ModelInputItem, ModelOutputSettings, ModelRequest,
    ModelRequestId, ModelTargetId, TextMode,
};
use garive_provider_anthropic::{
    decode_token_count, project_token_count_request, AnthropicTokenCountProfile,
    TokenCountHttpRequest,
};
use garive_provider_compatible::{map_messages_request, MessagesDeployment, MessagesMediaBinding};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{TokenCounter, TokenCounterDescriptor, TokenCounterFailure};

const COUNTER_ID: &str = "anthropic-messages-count-tokens";
const SECRET_PLACEHOLDER: &str = "<secret>";

/// One explicitly bounded execution boundary for the vendor count exchange.
pub trait TokenCountExchangePort {
    /// Returns the exact transport implementation/policy revision.
    fn transport_revision(&self) -> &str;
    /// Reports whether this exact port is admitted for publication evidence.
    fn publication_eligible(&self) -> bool;
    /// Executes one prepared request without retrying.
    fn execute(&self, request: &TokenCountHttpRequest) -> Result<Vec<u8>, TokenCounterFailure>;
}

/// Explicit provider-count construction values; the profile holds a redacted secret.
pub struct AnthropicProviderCounterConfig {
    /// Exact evidence counter implementation revision.
    pub counter_revision: String,
    /// Frozen normal Messages deployment used for P2-C mapping.
    pub deployment: MessagesDeployment,
    /// Explicit vendor count profile built after secret resolution.
    pub profile: AnthropicTokenCountProfile,
    /// Non-zero create-only output limit removed by count projection.
    pub projection_max_output_tokens: u64,
    /// Whether the resulting descriptor may publish evidence.
    pub publishable: bool,
}

/// Exact provider counter composed from normal mapping and an injected exchange port.
pub struct AnthropicProviderCounter<P> {
    descriptor: TokenCounterDescriptor,
    deployment: MessagesDeployment,
    profile: AnthropicTokenCountProfile,
    projection_max_output_tokens: u64,
    port: P,
}

impl<P: TokenCountExchangePort> AnthropicProviderCounter<P> {
    /// Validates explicit values and binds canonical non-secret configuration.
    pub fn new(
        config: AnthropicProviderCounterConfig,
        port: P,
    ) -> Result<Self, TokenCounterFailure> {
        let transport_revision = port.transport_revision();
        if config.counter_revision.is_empty()
            || config.deployment.target_id.is_empty()
            || config.deployment.model_id.is_empty()
            || config.projection_max_output_tokens == 0
            || transport_revision.is_empty()
            || (config.publishable && !port.publication_eligible())
            || !config
                .deployment
                .capabilities
                .contains(&ModelCapability::Text)
        {
            return Err(TokenCounterFailure);
        }
        let headers = canonical_headers(&config.profile)?;
        let media_bindings = canonical_media(&config.deployment)?;
        let thinking =
            serde_json::to_value(&config.deployment.thinking).map_err(|_| TokenCounterFailure)?;
        let canonical = serde_jcs::to_vec(&CanonicalProviderCounter {
            counter_id: COUNTER_ID,
            counter_revision: &config.counter_revision,
            transport_revision,
            publication_eligible: port.publication_eligible(),
            target_id: &config.deployment.target_id,
            model_id: &config.deployment.model_id,
            capabilities: config
                .deployment
                .capabilities
                .iter()
                .map(capability_name)
                .collect(),
            default_max_output_tokens: config.deployment.default_max_output_tokens,
            projection_max_output_tokens: config.projection_max_output_tokens,
            thinking,
            media_bindings,
            endpoint: config.profile.endpoint(),
            headers,
        })
        .map_err(|_| TokenCounterFailure)?;
        let descriptor = TokenCounterDescriptor::new(
            COUNTER_ID,
            config.counter_revision.clone(),
            format!("{:x}", Sha256::digest(canonical)),
            config.publishable,
        )
        .ok_or(TokenCounterFailure)?;
        Ok(Self {
            descriptor,
            deployment: config.deployment,
            profile: config.profile,
            projection_max_output_tokens: config.projection_max_output_tokens,
            port,
        })
    }
}

impl<P: TokenCountExchangePort> TokenCounter for AnthropicProviderCounter<P> {
    fn descriptor(&self) -> &TokenCounterDescriptor {
        &self.descriptor
    }

    fn count_input_tokens(&self, items: &[ModelInputItem]) -> Result<u64, TokenCounterFailure> {
        let request = ModelRequest {
            request_id: ModelRequestId::new("context-pressure-token-count"),
            target_id: ModelTargetId::new(self.deployment.target_id.clone()),
            required_capabilities: required_capabilities(items),
            input_items: items.to_vec(),
            tools: vec![],
            output: ModelOutputSettings {
                max_output_tokens: Some(self.projection_max_output_tokens),
                text_mode: TextMode::Plain,
                reasoning_visibility: false,
            },
            trace_metadata: vec![],
        };
        let mapped =
            map_messages_request(&self.deployment, &request).map_err(|_| TokenCounterFailure)?;
        let count = project_token_count_request(&mapped).map_err(|_| TokenCounterFailure)?;
        let exchange = self
            .profile
            .prepare(&count)
            .map_err(|_| TokenCounterFailure)?;
        let response = self.port.execute(&exchange)?;
        decode_token_count(&response)
            .map(|value| value.input_tokens())
            .map_err(|_| TokenCounterFailure)
    }
}

fn required_capabilities(items: &[ModelInputItem]) -> Vec<ModelCapability> {
    let mut values = BTreeSet::from([ModelCapability::Text]);
    for item in items {
        match item {
            ModelInputItem::Message { content, .. }
                if content
                    .iter()
                    .any(|part| matches!(part, ModelInputContent::MediaReference { .. })) =>
            {
                values.insert(ModelCapability::Vision);
            }
            ModelInputItem::ToolObservation { .. } => {
                values.insert(ModelCapability::Tools);
            }
            ModelInputItem::ReasoningReference { .. } => {
                values.insert(ModelCapability::Reasoning);
            }
            _ => {}
        }
    }
    values.into_iter().collect()
}

fn capability_name(value: &ModelCapability) -> &'static str {
    match value {
        ModelCapability::Text => "text",
        ModelCapability::Vision => "vision",
        ModelCapability::Reasoning => "reasoning",
        ModelCapability::Tools => "tools",
        ModelCapability::JsonOutput => "json_output",
        ModelCapability::Streaming => "streaming",
    }
}

fn canonical_headers(
    profile: &AnthropicTokenCountProfile,
) -> Result<Vec<CanonicalHeader>, TokenCounterFailure> {
    let mut api_keys = 0;
    let mut values = Vec::new();
    for header in profile.headers() {
        let value = if header.name() == "x-api-key" {
            if !header.is_sensitive() {
                return Err(TokenCounterFailure);
            }
            api_keys += 1;
            SECRET_PLACEHOLDER
        } else {
            if header.is_sensitive() {
                return Err(TokenCounterFailure);
            }
            header.value()
        };
        values.push(CanonicalHeader {
            name: header.name().to_owned(),
            value: value.to_owned(),
        });
    }
    if api_keys != 1 {
        return Err(TokenCounterFailure);
    }
    Ok(values)
}

fn canonical_media(deployment: &MessagesDeployment) -> Result<Value, TokenCounterFailure> {
    let values = deployment
        .media_bindings
        .iter()
        .map(|(reference, binding)| {
            let value = match binding {
                MessagesMediaBinding::Image(source) => json!({"kind":"image","source":source}),
                MessagesMediaBinding::Document(source) => {
                    json!({"kind":"document","source":source})
                }
            };
            (reference.clone(), value)
        })
        .collect();
    Ok(Value::Object(values))
}

#[derive(Serialize)]
struct CanonicalProviderCounter<'a> {
    counter_id: &'static str,
    counter_revision: &'a str,
    transport_revision: &'a str,
    publication_eligible: bool,
    target_id: &'a str,
    model_id: &'a str,
    capabilities: Vec<&'static str>,
    default_max_output_tokens: Option<u64>,
    projection_max_output_tokens: u64,
    thinking: Value,
    media_bindings: Value,
    endpoint: &'a str,
    headers: Vec<CanonicalHeader>,
}

#[derive(Serialize)]
struct CanonicalHeader {
    name: String,
    value: String,
}
