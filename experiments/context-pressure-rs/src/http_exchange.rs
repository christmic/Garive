use std::{fmt, io::Read, net::IpAddr, time::Duration};

use garive_provider_anthropic::TokenCountHttpRequest;

use crate::{TokenCountExchangePort, TokenCounterFailure};

const TRANSPORT_REVISION: &str = "reqwest-0.12.no-proxy.no-redirect.single-attempt.v1";

/// Explicit non-zero bounds for one exact count-token HTTP attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenCountHttpLimits {
    /// TCP/TLS connection timeout in milliseconds.
    pub connect_timeout_ms: u64,
    /// Whole-request timeout in milliseconds.
    pub request_timeout_ms: u64,
    /// Maximum accepted success-body bytes.
    pub max_response_bytes: usize,
}

/// Shipping bounded, no-retry HTTP exchange for exact token counting.
pub struct ReqwestTokenCountExchangePort {
    endpoint: String,
    client: reqwest::blocking::Client,
    limits: TokenCountHttpLimits,
    publication_eligible: bool,
}

impl ReqwestTokenCountExchangePort {
    /// Constructs a client without ambient proxy discovery or redirects.
    pub fn new(
        endpoint: impl Into<String>,
        limits: TokenCountHttpLimits,
    ) -> Result<Self, TokenCounterFailure> {
        if limits.connect_timeout_ms == 0
            || limits.request_timeout_ms == 0
            || limits.max_response_bytes == 0
        {
            return Err(TokenCounterFailure);
        }
        let endpoint = endpoint.into();
        let url = reqwest::Url::parse(&endpoint).map_err(|_| TokenCounterFailure)?;
        if !matches!(url.scheme(), "http" | "https") || url.path().is_empty() {
            return Err(TokenCounterFailure);
        }
        let publication_eligible = publication_endpoint(&url);
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_millis(limits.connect_timeout_ms))
            .timeout(Duration::from_millis(limits.request_timeout_ms))
            .build()
            .map_err(|_| TokenCounterFailure)?;
        Ok(Self {
            endpoint,
            client,
            limits,
            publication_eligible,
        })
    }
}

impl TokenCountExchangePort for ReqwestTokenCountExchangePort {
    fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn transport_revision(&self) -> &str {
        TRANSPORT_REVISION
    }

    fn publication_eligible(&self) -> bool {
        self.publication_eligible
    }

    fn execute(&self, request: &TokenCountHttpRequest) -> Result<Vec<u8>, TokenCounterFailure> {
        if request.uri() != self.endpoint || request.method() != "POST" {
            return Err(TokenCounterFailure);
        }
        let mut outgoing = self
            .client
            .post(request.uri())
            .body(request.body().to_vec());
        for header in request.headers() {
            outgoing = outgoing.header(header.name(), header.value());
        }
        let response = outgoing.send().map_err(|_| TokenCounterFailure)?;
        if !response.status().is_success()
            || response
                .content_length()
                .is_some_and(|length| length > self.limits.max_response_bytes as u64)
        {
            return Err(TokenCounterFailure);
        }
        let limit = self
            .limits
            .max_response_bytes
            .checked_add(1)
            .ok_or(TokenCounterFailure)?;
        let mut body = Vec::new();
        response
            .take(limit as u64)
            .read_to_end(&mut body)
            .map_err(|_| TokenCounterFailure)?;
        if body.len() > self.limits.max_response_bytes {
            return Err(TokenCounterFailure);
        }
        Ok(body)
    }
}

impl fmt::Debug for ReqwestTokenCountExchangePort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReqwestTokenCountExchangePort")
            .field("endpoint", &self.endpoint)
            .field("limits", &self.limits)
            .field("publication_eligible", &self.publication_eligible)
            .finish_non_exhaustive()
    }
}

fn publication_endpoint(url: &reqwest::Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") || host.to_ascii_lowercase().ends_with(".localhost") {
        return false;
    }
    host.parse::<IpAddr>()
        .map_or(true, |value| !value.is_loopback())
}
