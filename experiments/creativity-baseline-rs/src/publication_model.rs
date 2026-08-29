use std::collections::{BTreeMap, BTreeSet};

use garive_anthropic_messages::{Header as MessagesHeader, MessagesAdapterConfig};
use garive_llm::ModelCapability;
use garive_openai_responses::{Header as ResponsesHeader, ResponsesAdapterConfig};
use garive_provider_compatible::{MessagesDeployment, ProtocolErrorPolicy, ResponsesDeployment};
use garive_provider_profile::SecretValue;
use garive_runtime::{RuntimeHttpLimits, RuntimeModelHttpTransport};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    CreativityBaselineError, CreativityBaselineErrorCode, ExperimentPortDescriptor,
    ModelCreativityConfig, ModelCreativityEvaluator, ModelCreativityGenerator,
    EVALUATOR_TEMPLATE_REVISION, GENERATOR_TEMPLATE_REVISION,
};

/// Portable compatible protocol selected for one external model endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProtocol {
    /// Responses-compatible portable protocol.
    ResponsesCompatible,
    /// Messages-compatible portable protocol.
    MessagesCompatible,
}

impl ModelProtocol {
    /// Returns the stable evidence name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::ResponsesCompatible => "responses_compatible",
            Self::MessagesCompatible => "messages_compatible",
        }
    }
}

/// Explicit non-secret additional protocol header.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NonSecretHeader {
    /// Header name.
    pub name: String,
    /// Exact non-secret header value.
    pub value: String,
}

/// Strict explicit external model endpoint configuration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelEndpointConfig {
    /// Compatible protocol dialect.
    pub protocol: ModelProtocol,
    /// Neutral target identity.
    pub target_id: String,
    /// Protocol model identifier.
    pub model_id: String,
    /// Asserted exact model/deployment revision for review.
    pub model_revision: String,
    /// Absolute explicit protocol endpoint.
    pub endpoint: String,
    /// Opaque OS credential-store account/reference.
    pub credential_ref: String,
    /// Header receiving the resolved credential.
    pub credential_header_name: String,
    /// Non-secret prefix prepended to the credential, such as `Bearer `.
    pub credential_header_prefix: String,
    /// Ordered additional non-secret headers.
    pub non_secret_headers: Vec<NonSecretHeader>,
    /// Version header name required only by Messages-compatible endpoints.
    pub messages_version_header_name: Option<String>,
    /// Protocol version required only by Messages-compatible endpoints.
    pub messages_protocol_version: Option<String>,
    /// Non-zero generated-token bound per call.
    pub max_output_tokens: u64,
    /// Non-zero connection timeout.
    pub connect_timeout_ms: u64,
    /// Non-zero whole-request timeout.
    pub request_timeout_ms: u64,
    /// Non-zero maximum response bytes.
    pub max_response_bytes: usize,
}

/// Transparent non-secret coordinate bound into CR-B evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicationModelCoordinate {
    /// Compatible protocol dialect.
    pub protocol: ModelProtocol,
    /// Neutral target identity.
    pub target_id: String,
    /// Protocol model identifier.
    pub model_id: String,
    /// Asserted model/deployment revision.
    pub model_revision: String,
    /// Exact template and configuration binding.
    pub port: ExperimentPortDescriptor,
}

/// Resolves one opaque credential reference without exposing ambient config.
pub trait CredentialReferenceResolver {
    /// Returns a redacted validated secret or a content-free failure.
    fn resolve(&self, credential_ref: &str) -> Result<SecretValue, CredentialResolutionFailure>;
}

/// Content-free credential resolution failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialResolutionFailure;

/// Builds one publication-capable generator after strict pre-secret validation.
pub fn build_publication_generator(
    config: ModelEndpointConfig,
    resolver: &dyn CredentialReferenceResolver,
) -> Result<(ModelCreativityGenerator, PublicationModelCoordinate), CreativityBaselineError> {
    let built = build(config, resolver, "generator", GENERATOR_TEMPLATE_REVISION)?;
    let coordinate = built.coordinate.clone();
    let port = ModelCreativityGenerator::new(built.creativity, built.transport)?;
    Ok((port, coordinate))
}

/// Builds one publication-capable blind evaluator after strict pre-secret validation.
pub fn build_publication_evaluator(
    config: ModelEndpointConfig,
    resolver: &dyn CredentialReferenceResolver,
) -> Result<(ModelCreativityEvaluator, PublicationModelCoordinate), CreativityBaselineError> {
    let built = build(config, resolver, "evaluator", EVALUATOR_TEMPLATE_REVISION)?;
    let coordinate = built.coordinate.clone();
    let port = ModelCreativityEvaluator::new(built.creativity, built.transport)?;
    Ok((port, coordinate))
}

struct Built {
    creativity: ModelCreativityConfig,
    coordinate: PublicationModelCoordinate,
    transport: Box<dyn garive_llm::ModelPort>,
}

fn build(
    config: ModelEndpointConfig,
    resolver: &dyn CredentialReferenceResolver,
    role: &'static str,
    template_revision: &'static str,
) -> Result<Built, CreativityBaselineError> {
    let publication_eligible = validate(&config)?;
    let canonical = serde_jcs::to_vec(&CanonicalConfig {
        role,
        template_revision,
        config: &config,
        transport_revision: "runtime-http.no-proxy.no-redirect.single-attempt.v1",
    })
    .map_err(|_| error())?;
    let descriptor = ExperimentPortDescriptor::new(
        format!("garive-compatible-creativity-{role}"),
        template_revision,
        format!("{:x}", Sha256::digest(canonical)),
        publication_eligible,
    )
    .ok_or_else(error)?;
    let coordinate = PublicationModelCoordinate {
        protocol: config.protocol,
        target_id: config.target_id.clone(),
        model_id: config.model_id.clone(),
        model_revision: config.model_revision.clone(),
        port: descriptor.clone(),
    };
    let secret = resolver
        .resolve(&config.credential_ref)
        .map_err(|_| error())?;
    let transport = transport(&config, secret)?;
    Ok(Built {
        creativity: ModelCreativityConfig {
            target_id: config.target_id,
            max_output_tokens: config.max_output_tokens,
            descriptor,
        },
        coordinate,
        transport,
    })
}

fn validate(config: &ModelEndpointConfig) -> Result<bool, CreativityBaselineError> {
    if !identity(&config.target_id)
        || !identity(&config.model_id)
        || !identity(&config.model_revision)
        || !identity(&config.credential_ref)
        || config.max_output_tokens == 0
        || config.connect_timeout_ms == 0
        || config.request_timeout_ms == 0
        || config.max_response_bytes == 0
        || config.credential_header_prefix.contains(['\r', '\n', '\0'])
    {
        return Err(error());
    }
    let url = reqwest::Url::parse(&config.endpoint).map_err(|_| error())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.path().is_empty()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(error());
    }
    let messages_shape = matches!(config.protocol, ModelProtocol::MessagesCompatible);
    if messages_shape
        != (config.messages_version_header_name.is_some()
            && config.messages_protocol_version.is_some())
    {
        return Err(error());
    }
    validate_headers(config)?;
    Ok(publication_endpoint(&url))
}

fn validate_headers(config: &ModelEndpointConfig) -> Result<(), CreativityBaselineError> {
    let mut names = BTreeSet::new();
    if !names.insert(config.credential_header_name.to_ascii_lowercase())
        || config.non_secret_headers.iter().any(|header| {
            !names.insert(header.name.to_ascii_lowercase())
                || header.value.contains(['\r', '\n', '\0'])
        })
    {
        return Err(error());
    }
    let placeholder = format!("{}credential", config.credential_header_prefix);
    match config.protocol {
        ModelProtocol::ResponsesCompatible => {
            let mut headers = response_headers(config, &placeholder)?;
            ResponsesAdapterConfig::new(config.endpoint.clone(), std::mem::take(&mut headers))
                .map_err(|_| error())?;
        }
        ModelProtocol::MessagesCompatible => {
            let mut headers = message_headers(config, &placeholder)?;
            MessagesAdapterConfig::new(
                config.endpoint.clone(),
                std::mem::take(&mut headers),
                config
                    .messages_version_header_name
                    .clone()
                    .ok_or_else(error)?,
                config.messages_protocol_version.clone().ok_or_else(error)?,
            )
            .map_err(|_| error())?;
        }
    }
    Ok(())
}

fn transport(
    config: &ModelEndpointConfig,
    secret: SecretValue,
) -> Result<Box<dyn garive_llm::ModelPort>, CreativityBaselineError> {
    let credential = format!(
        "{}{}",
        config.credential_header_prefix,
        secret.expose_secret()
    );
    let capabilities = BTreeSet::from([ModelCapability::Text, ModelCapability::JsonOutput]);
    let limits = RuntimeHttpLimits {
        connect_timeout_ms: config.connect_timeout_ms,
        request_timeout_ms: config.request_timeout_ms,
        max_response_bytes: config.max_response_bytes,
    };
    match config.protocol {
        ModelProtocol::ResponsesCompatible => Ok(Box::new(
            RuntimeModelHttpTransport::responses_compatible(
                ResponsesDeployment {
                    target_id: config.target_id.clone(),
                    model_id: config.model_id.clone(),
                    capabilities,
                    default_max_output_tokens: None,
                    media_bindings: BTreeMap::new(),
                    reasoning: None,
                    error_policy: ProtocolErrorPolicy::default(),
                },
                ResponsesAdapterConfig::new(
                    config.endpoint.clone(),
                    response_headers(config, &credential)?,
                )
                .map_err(|_| error())?,
                limits,
            )
            .map_err(|_| error())?,
        )),
        ModelProtocol::MessagesCompatible => Ok(Box::new(
            RuntimeModelHttpTransport::messages_compatible(
                MessagesDeployment {
                    target_id: config.target_id.clone(),
                    model_id: config.model_id.clone(),
                    capabilities,
                    default_max_output_tokens: None,
                    media_bindings: BTreeMap::new(),
                    thinking: None,
                    error_policy: ProtocolErrorPolicy::default(),
                },
                MessagesAdapterConfig::new(
                    config.endpoint.clone(),
                    message_headers(config, &credential)?,
                    config
                        .messages_version_header_name
                        .clone()
                        .ok_or_else(error)?,
                    config.messages_protocol_version.clone().ok_or_else(error)?,
                )
                .map_err(|_| error())?,
                limits,
            )
            .map_err(|_| error())?,
        )),
    }
}

fn response_headers(
    config: &ModelEndpointConfig,
    credential: &str,
) -> Result<Vec<ResponsesHeader>, CreativityBaselineError> {
    let mut headers = vec![
        ResponsesHeader::new(&config.credential_header_name, credential, true)
            .map_err(|_| error())?,
    ];
    for header in &config.non_secret_headers {
        headers
            .push(ResponsesHeader::new(&header.name, &header.value, false).map_err(|_| error())?);
    }
    Ok(headers)
}

fn message_headers(
    config: &ModelEndpointConfig,
    credential: &str,
) -> Result<Vec<MessagesHeader>, CreativityBaselineError> {
    let mut headers = vec![
        MessagesHeader::new(&config.credential_header_name, credential, true)
            .map_err(|_| error())?,
    ];
    for header in &config.non_secret_headers {
        headers.push(MessagesHeader::new(&header.name, &header.value, false).map_err(|_| error())?);
    }
    Ok(headers)
}

fn publication_endpoint(url: &reqwest::Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return false;
    }
    host.parse::<std::net::IpAddr>()
        .map_or(true, |value| !value.is_loopback())
}

fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}

#[derive(Serialize)]
struct CanonicalConfig<'a> {
    role: &'static str,
    template_revision: &'static str,
    config: &'a ModelEndpointConfig,
    transport_revision: &'static str,
}

fn error() -> CreativityBaselineError {
    CreativityBaselineError::new(CreativityBaselineErrorCode::InvalidPort)
}
