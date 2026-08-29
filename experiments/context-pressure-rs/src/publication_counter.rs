use std::collections::{BTreeMap, BTreeSet};

use garive_llm::ModelCapability;
use garive_provider_anthropic::build_token_count_profile;
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy};
use garive_provider_profile::{ConnectionInput, EndpointSelection, ExplicitHeader, SecretValue};
use serde::Deserialize;

use crate::{
    AnthropicProviderCounter, AnthropicProviderCounterConfig, ReqwestTokenCountExchangePort,
    TokenCountHttpLimits,
};

/// Content-free failure while resolving one opaque credential reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialResolutionFailure;

/// Injected secret boundary used by publication composition.
pub trait CredentialReferenceResolver {
    /// Resolves exactly one opaque reference without fallback discovery.
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, CredentialResolutionFailure>;
}

/// Stable failure before a publication provider counter exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCounterBuildError {
    /// The strict non-secret configuration violated an invariant.
    InvalidConfiguration,
    /// The injected resolver could not return the referenced secret.
    CredentialUnavailable,
}

impl ProviderCounterBuildError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "invalid_configuration",
            Self::CredentialUnavailable => "credential_unavailable",
        }
    }
}

/// Strict non-secret provider-counter portion of one C7-C run document.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCounterRunConfig {
    counter_revision: String,
    publishable: bool,
    credential_ref: String,
    endpoint: Option<String>,
    target_id: String,
    model_id: String,
    capabilities: Vec<String>,
    projection_max_output_tokens: u64,
    #[serde(default)]
    extra_headers: Vec<NonSecretHeaderConfig>,
    http: HttpConfig,
}

impl ProviderCounterRunConfig {
    /// Reports whether this exact run requests publication eligibility.
    pub const fn publication_requested(&self) -> bool {
        self.publishable
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NonSecretHeaderConfig {
    name: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HttpConfig {
    connect_timeout_ms: u64,
    request_timeout_ms: u64,
    max_response_bytes: usize,
}

/// Concrete exact provider counter used by the publication runner.
pub type PublicationProviderCounter = AnthropicProviderCounter<ReqwestTokenCountExchangePort>;

/// Resolves one secret and constructs the complete bounded provider route.
pub fn build_publication_provider_counter(
    config: ProviderCounterRunConfig,
    resolver: &dyn CredentialReferenceResolver,
) -> Result<PublicationProviderCounter, ProviderCounterBuildError> {
    if !identity(&config.counter_revision)
        || !identity(&config.credential_ref)
        || !identity(&config.target_id)
        || !identity(&config.model_id)
        || config.projection_max_output_tokens == 0
        || config.http.connect_timeout_ms == 0
        || config.http.request_timeout_ms == 0
        || config.http.max_response_bytes == 0
    {
        return Err(ProviderCounterBuildError::InvalidConfiguration);
    }
    let capabilities = capabilities(&config.capabilities)?;
    let headers = config
        .extra_headers
        .into_iter()
        .map(|header| ExplicitHeader::new(header.name, header.value, false))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)?;
    let endpoint = config.endpoint.map_or(EndpointSelection::Default, |value| {
        EndpointSelection::Explicit(value)
    });
    let validation_profile = build_token_count_profile(&ConnectionInput::new(
        endpoint.clone(),
        SecretValue::new("configuration-validation")
            .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)?,
        headers.clone(),
    ))
    .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)?;
    let port = ReqwestTokenCountExchangePort::new(
        validation_profile.endpoint(),
        TokenCountHttpLimits {
            connect_timeout_ms: config.http.connect_timeout_ms,
            request_timeout_ms: config.http.request_timeout_ms,
            max_response_bytes: config.http.max_response_bytes,
        },
    )
    .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)?;
    if config.publishable && !crate::TokenCountExchangePort::publication_eligible(&port) {
        return Err(ProviderCounterBuildError::InvalidConfiguration);
    }
    let secret = resolver
        .resolve(&config.credential_ref)
        .map_err(|_| ProviderCounterBuildError::CredentialUnavailable)?;
    let profile = build_token_count_profile(&ConnectionInput::new(endpoint, secret, headers))
        .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)?;
    AnthropicProviderCounter::new(
        AnthropicProviderCounterConfig {
            counter_revision: config.counter_revision,
            deployment: MessagesDeployment {
                target_id: config.target_id,
                model_id: config.model_id,
                capabilities,
                default_max_output_tokens: None,
                media_bindings: BTreeMap::new(),
                thinking: None,
                error_policy: ProtocolErrorPolicy::default(),
            },
            profile,
            projection_max_output_tokens: config.projection_max_output_tokens,
            publishable: config.publishable,
        },
        port,
    )
    .map_err(|_| ProviderCounterBuildError::InvalidConfiguration)
}

fn capabilities(values: &[String]) -> Result<BTreeSet<ModelCapability>, ProviderCounterBuildError> {
    let mut result = BTreeSet::new();
    for value in values {
        let capability = match value.as_str() {
            "text" => ModelCapability::Text,
            "vision" => ModelCapability::Vision,
            "reasoning" => ModelCapability::Reasoning,
            "tools" => ModelCapability::Tools,
            "json_output" => ModelCapability::JsonOutput,
            "streaming" => ModelCapability::Streaming,
            _ => return Err(ProviderCounterBuildError::InvalidConfiguration),
        };
        if !result.insert(capability) {
            return Err(ProviderCounterBuildError::InvalidConfiguration);
        }
    }
    if result.is_empty() {
        return Err(ProviderCounterBuildError::InvalidConfiguration);
    }
    Ok(result)
}

fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}
