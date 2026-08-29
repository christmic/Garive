use std::{fmt, time::Duration};

use futures::StreamExt;
use garive_anthropic_messages as messages;
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCancellation, ModelFuture, ModelItem, ModelObserver,
    ModelPort, ModelPortFailure, ModelRequest, ModelStreamEvent, ModelUsage, ObserverDecision,
    TokenCount, UsageSource,
};
use garive_openai_responses as responses;
use garive_provider_anthropic::AnthropicProfile;
use garive_provider_compatible::{
    classify_protocol_error, map_messages_request, map_responses_request, normalize_messages,
    normalize_responses, CompatibleProviderError, ErrorSignature, MessagesDeployment,
    MessagesStreamMapper, ResponsesDeployment, ResponsesStreamMapper, StreamMapping,
};
use garive_provider_openai::OpenAiProfile;

/// Explicit bounds for one Runtime-owned HTTP client and response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeHttpLimits {
    /// Non-zero TCP/TLS connection timeout in milliseconds.
    pub connect_timeout_ms: u64,
    /// Non-zero whole-request timeout in milliseconds.
    pub request_timeout_ms: u64,
    /// Non-zero maximum response bytes, including SSE framing.
    pub max_response_bytes: usize,
}

/// Stable construction failure before a transport can exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeHttpTransportError {
    /// One required bound was zero.
    InvalidLimits,
    /// The explicitly configured HTTP client could not be constructed.
    ClientConfiguration,
}

impl RuntimeHttpTransportError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidLimits => "invalid_limits",
            Self::ClientConfiguration => "client_configuration",
        }
    }
}

/// One explicit no-retry Runtime HTTP transport for a compatible deployment.
pub struct RuntimeModelHttpTransport {
    inner: TransportKind,
}

enum TransportKind {
    Responses {
        deployment: ResponsesDeployment,
        adapter: responses::ResponsesAdapter,
        client: reqwest::Client,
        limits: RuntimeHttpLimits,
    },
    Messages {
        deployment: MessagesDeployment,
        adapter: messages::MessagesAdapter,
        client: reqwest::Client,
        limits: RuntimeHttpLimits,
    },
}

impl RuntimeModelHttpTransport {
    /// Constructs a Responses-compatible transport from validated protocol configuration.
    pub fn responses_compatible(
        deployment: ResponsesDeployment,
        adapter_config: responses::ResponsesAdapterConfig,
        limits: RuntimeHttpLimits,
    ) -> Result<Self, RuntimeHttpTransportError> {
        Ok(Self {
            inner: TransportKind::Responses {
                deployment,
                adapter: responses::ResponsesAdapter::new(adapter_config),
                client: client(limits)?,
                limits,
            },
        })
    }

    /// Constructs a Messages-compatible transport from validated protocol configuration.
    pub fn messages_compatible(
        deployment: MessagesDeployment,
        adapter_config: messages::MessagesAdapterConfig,
        limits: RuntimeHttpLimits,
    ) -> Result<Self, RuntimeHttpTransportError> {
        Ok(Self {
            inner: TransportKind::Messages {
                deployment,
                adapter: messages::MessagesAdapter::new(adapter_config),
                client: client(limits)?,
                limits,
            },
        })
    }

    /// Constructs an official Responses transport without consulting external state.
    pub fn openai(
        mut deployment: ResponsesDeployment,
        profile: OpenAiProfile,
        limits: RuntimeHttpLimits,
    ) -> Result<Self, RuntimeHttpTransportError> {
        deployment.error_policy = profile.error_policy;
        Self::responses_compatible(deployment, profile.adapter_config, limits)
    }

    /// Constructs an official Messages transport without consulting external state.
    pub fn anthropic(
        mut deployment: MessagesDeployment,
        profile: AnthropicProfile,
        limits: RuntimeHttpLimits,
    ) -> Result<Self, RuntimeHttpTransportError> {
        deployment.error_policy = profile.error_policy;
        Self::messages_compatible(deployment, profile.adapter_config, limits)
    }

    fn preflight_responses(
        deployment: &ResponsesDeployment,
        adapter: &responses::ResponsesAdapter,
        request: &ModelRequest,
    ) -> Result<(), ModelPortFailure> {
        let mapped = map_responses_request(deployment, request).map_err(provider_failure)?;
        adapter
            .prepare(&mapped)
            .map(|_| ())
            .map_err(|_| ModelPortFailure::AdapterInvariant)
    }

    fn preflight_messages(
        deployment: &MessagesDeployment,
        adapter: &messages::MessagesAdapter,
        request: &ModelRequest,
    ) -> Result<(), ModelPortFailure> {
        let mapped = map_messages_request(deployment, request).map_err(provider_failure)?;
        adapter
            .prepare(&mapped)
            .map(|_| ())
            .map_err(|_| ModelPortFailure::AdapterInvariant)
    }
}

impl fmt::Debug for RuntimeModelHttpTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            TransportKind::Responses {
                deployment,
                adapter,
                limits,
                ..
            } => formatter
                .debug_struct("RuntimeModelHttpTransport::Responses")
                .field("target_id", &deployment.target_id)
                .field("adapter", adapter)
                .field("limits", limits)
                .finish(),
            TransportKind::Messages {
                deployment,
                adapter,
                limits,
                ..
            } => formatter
                .debug_struct("RuntimeModelHttpTransport::Messages")
                .field("target_id", &deployment.target_id)
                .field("adapter", adapter)
                .field("limits", limits)
                .finish(),
        }
    }
}

impl ModelPort for RuntimeModelHttpTransport {
    fn preflight(&self, request: &ModelRequest) -> Result<(), ModelPortFailure> {
        request
            .validate()
            .map_err(|_| ModelPortFailure::InvalidRequest)?;
        match &self.inner {
            TransportKind::Responses {
                deployment,
                adapter,
                ..
            } => Self::preflight_responses(deployment, adapter, request),
            TransportKind::Messages {
                deployment,
                adapter,
                ..
            } => Self::preflight_messages(deployment, adapter, request),
        }
    }

    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        observer: &'a mut dyn ModelObserver,
        cancellation: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        Box::pin(async move {
            self.preflight(request)?;
            if cancellation.is_cancelled() {
                return Ok(cancelled(Vec::new(), unknown_usage()));
            }
            match &self.inner {
                TransportKind::Responses {
                    deployment,
                    adapter,
                    client,
                    limits,
                } => {
                    let mapped =
                        map_responses_request(deployment, request).map_err(provider_failure)?;
                    let wire = adapter
                        .prepare(&mapped)
                        .map_err(|_| ModelPortFailure::AdapterInvariant)?;
                    let response = send(client, &wire).await?;
                    if mapped.stream && response.status().is_success() {
                        stream_responses(
                            response,
                            adapter,
                            request,
                            observer,
                            cancellation,
                            *limits,
                        )
                        .await
                    } else {
                        buffered_responses(response, adapter, deployment, *limits).await
                    }
                }
                TransportKind::Messages {
                    deployment,
                    adapter,
                    client,
                    limits,
                } => {
                    let mapped =
                        map_messages_request(deployment, request).map_err(provider_failure)?;
                    let wire = adapter
                        .prepare(&mapped)
                        .map_err(|_| ModelPortFailure::AdapterInvariant)?;
                    let response = send(client, &wire).await?;
                    if mapped.stream && response.status().is_success() {
                        stream_messages(response, request, observer, cancellation, *limits).await
                    } else {
                        buffered_messages(response, adapter, deployment, *limits).await
                    }
                }
            }
        })
    }
}

fn client(limits: RuntimeHttpLimits) -> Result<reqwest::Client, RuntimeHttpTransportError> {
    if limits.connect_timeout_ms == 0
        || limits.request_timeout_ms == 0
        || limits.max_response_bytes == 0
    {
        return Err(RuntimeHttpTransportError::InvalidLimits);
    }
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_millis(limits.connect_timeout_ms))
        .timeout(Duration::from_millis(limits.request_timeout_ms))
        .build()
        .map_err(|_| RuntimeHttpTransportError::ClientConfiguration)
}

async fn send(
    client: &reqwest::Client,
    request: &impl WireRequest,
) -> Result<reqwest::Response, ModelPortFailure> {
    let mut outgoing = client.post(request.uri()).body(request.body().to_vec());
    for header in request.headers() {
        outgoing = outgoing.header(header.0, header.1);
    }
    outgoing
        .send()
        .await
        .map_err(|_| ModelPortFailure::RequiredPortFailure)
}

trait WireRequest {
    fn uri(&self) -> &str;
    fn body(&self) -> &[u8];
    fn headers(&self) -> Vec<(&str, &str)>;
}

impl WireRequest for responses::HttpRequest {
    fn uri(&self) -> &str {
        self.uri()
    }
    fn body(&self) -> &[u8] {
        self.body()
    }
    fn headers(&self) -> Vec<(&str, &str)> {
        self.headers()
            .iter()
            .map(|header| (header.name(), header.value()))
            .collect()
    }
}

impl WireRequest for messages::HttpRequest {
    fn uri(&self) -> &str {
        self.uri()
    }
    fn body(&self) -> &[u8] {
        self.body()
    }
    fn headers(&self) -> Vec<(&str, &str)> {
        self.headers()
            .iter()
            .map(|header| (header.name(), header.value()))
            .collect()
    }
}

async fn buffered_responses(
    response: reqwest::Response,
    adapter: &responses::ResponsesAdapter,
    deployment: &ResponsesDeployment,
    limits: RuntimeHttpLimits,
) -> Result<InvokeOutcome, ModelPortFailure> {
    let status = response.status().as_u16();
    let retry_after = retry_after(response.headers());
    let headers = response_headers(response.headers(), responses::Header::new)?;
    let body = collect_body(response, limits).await?;
    match adapter
        .decode_response(status, &headers, &body)
        .map_err(|_| ModelPortFailure::AdapterInvariant)?
    {
        responses::DecodedResponse::Response { response, .. } => {
            normalize_responses(&response, deployment.reasoning.is_some()).map_err(provider_failure)
        }
        responses::DecodedResponse::Error { error, .. } => classify_protocol_error(
            &deployment.error_policy,
            ErrorSignature {
                status,
                protocol_type: error.error.r#type,
                code: error.error.code,
            },
            retry_after,
        )
        .map_err(provider_failure),
    }
}

async fn buffered_messages(
    response: reqwest::Response,
    adapter: &messages::MessagesAdapter,
    deployment: &MessagesDeployment,
    limits: RuntimeHttpLimits,
) -> Result<InvokeOutcome, ModelPortFailure> {
    let status = response.status().as_u16();
    let retry_after = retry_after(response.headers());
    let headers = response_headers(response.headers(), messages::Header::new)?;
    let body = collect_body(response, limits).await?;
    match adapter
        .decode_response(status, &headers, &body)
        .map_err(|_| ModelPortFailure::AdapterInvariant)?
    {
        messages::DecodedResponse::Message { message, .. } => {
            normalize_messages(&message, deployment.thinking.is_some()).map_err(provider_failure)
        }
        messages::DecodedResponse::Error { error, .. } => classify_protocol_error(
            &deployment.error_policy,
            ErrorSignature {
                status,
                protocol_type: error.error.r#type,
                code: None,
            },
            retry_after,
        )
        .map_err(provider_failure),
    }
}

async fn stream_responses(
    response: reqwest::Response,
    adapter: &responses::ResponsesAdapter,
    request: &ModelRequest,
    observer: &mut dyn ModelObserver,
    cancellation: &dyn ModelCancellation,
    limits: RuntimeHttpLimits,
) -> Result<InvokeOutcome, ModelPortFailure> {
    if !response.status().is_success() {
        return Err(ModelPortFailure::AdapterInvariant);
    }
    let mut decoder = adapter.stream_decoder();
    let mut mapper = ResponsesStreamMapper::new(request.output.reasoning_visibility);
    let mut partial = PartialStream::default();
    let mut terminal = None;
    let mut received = 0usize;
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| ModelPortFailure::RequiredPortFailure)?;
        received = bounded_add(received, chunk.len(), limits.max_response_bytes)?;
        for event in decoder
            .push(&chunk)
            .map_err(|_| ModelPortFailure::AdapterInvariant)?
        {
            let mapping = mapper
                .accept(&event)
                .map_err(|_| ModelPortFailure::AdapterInvariant)?;
            if apply_mapping(mapping, observer, cancellation, &mut partial, &mut terminal)? {
                return Ok(cancelled(partial.items, partial.usage));
            }
        }
    }
    decoder
        .finish()
        .map_err(|_| ModelPortFailure::AdapterInvariant)?;
    terminal.ok_or(ModelPortFailure::AdapterInvariant)
}

async fn stream_messages(
    response: reqwest::Response,
    request: &ModelRequest,
    observer: &mut dyn ModelObserver,
    cancellation: &dyn ModelCancellation,
    limits: RuntimeHttpLimits,
) -> Result<InvokeOutcome, ModelPortFailure> {
    if !response.status().is_success() {
        return Err(ModelPortFailure::AdapterInvariant);
    }
    let mut decoder = messages::MessagesStreamDecoder::new();
    let mut mapper = MessagesStreamMapper::new(request.output.reasoning_visibility);
    let mut partial = PartialStream::default();
    let mut terminal = None;
    let mut received = 0usize;
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| ModelPortFailure::RequiredPortFailure)?;
        received = bounded_add(received, chunk.len(), limits.max_response_bytes)?;
        for event in decoder
            .push(&chunk)
            .map_err(|_| ModelPortFailure::AdapterInvariant)?
        {
            let mapping = mapper
                .accept(&event)
                .map_err(|_| ModelPortFailure::AdapterInvariant)?;
            if apply_mapping(mapping, observer, cancellation, &mut partial, &mut terminal)? {
                return Ok(cancelled(partial.items, partial.usage));
            }
        }
    }
    decoder
        .finish()
        .map_err(|_| ModelPortFailure::AdapterInvariant)?;
    terminal.ok_or(ModelPortFailure::AdapterInvariant)
}

struct PartialStream {
    items: Vec<ModelItem>,
    usage: ModelUsage,
}

impl Default for PartialStream {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            usage: unknown_usage(),
        }
    }
}

fn apply_mapping(
    mapping: StreamMapping,
    observer: &mut dyn ModelObserver,
    cancellation: &dyn ModelCancellation,
    partial: &mut PartialStream,
    terminal: &mut Option<InvokeOutcome>,
) -> Result<bool, ModelPortFailure> {
    for event in mapping.events {
        match &event {
            ModelStreamEvent::OutputItemCompleted { item, .. } => partial.items.push(item.clone()),
            ModelStreamEvent::UsageUpdated { usage } => partial.usage = *usage,
            _ => {}
        }
        if cancellation.is_cancelled() || observer.observe(&event) == ObserverDecision::Cancel {
            return Ok(true);
        }
    }
    if let Some(outcome) = mapping.terminal {
        if terminal.replace(outcome).is_some() {
            return Err(ModelPortFailure::AdapterInvariant);
        }
    }
    Ok(cancellation.is_cancelled())
}

async fn collect_body(
    response: reqwest::Response,
    limits: RuntimeHttpLimits,
) -> Result<Vec<u8>, ModelPortFailure> {
    let mut output = Vec::new();
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|_| ModelPortFailure::RequiredPortFailure)?;
        bounded_add(output.len(), chunk.len(), limits.max_response_bytes)?;
        output.extend_from_slice(&chunk);
    }
    Ok(output)
}

fn bounded_add(current: usize, next: usize, maximum: usize) -> Result<usize, ModelPortFailure> {
    current
        .checked_add(next)
        .filter(|total| *total <= maximum)
        .ok_or(ModelPortFailure::RequiredPortFailure)
}

fn response_headers<T, E>(
    values: &reqwest::header::HeaderMap,
    constructor: impl Fn(String, String, bool) -> Result<T, E>,
) -> Result<Vec<T>, ModelPortFailure> {
    values
        .iter()
        .map(|(name, value)| {
            let value = value
                .to_str()
                .map_err(|_| ModelPortFailure::AdapterInvariant)?;
            constructor(name.as_str().to_owned(), value.to_owned(), false)
                .map_err(|_| ModelPortFailure::AdapterInvariant)
        })
        .collect()
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn provider_failure(error: CompatibleProviderError) -> ModelPortFailure {
    match error {
        CompatibleProviderError::InvalidRequest(_) => ModelPortFailure::InvalidRequest,
        CompatibleProviderError::TargetMismatch
        | CompatibleProviderError::UnsupportedCapability(_) => {
            ModelPortFailure::UnsupportedCapability
        }
        _ => ModelPortFailure::AdapterInvariant,
    }
}

fn cancelled(partial_items: Vec<ModelItem>, usage: ModelUsage) -> InvokeOutcome {
    InvokeOutcome::Interrupted {
        kind: InterruptionKind::Cancelled,
        partial_items,
        usage,
    }
}

fn unknown_usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Unknown,
        output_tokens: TokenCount::Unknown,
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::Estimated,
    }
}
