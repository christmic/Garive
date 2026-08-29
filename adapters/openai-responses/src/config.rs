//! Explicit endpoint and header construction for one protocol exchange.

use crate::ResponsesAdapterError;
use http::{header::HeaderName, uri::Scheme, HeaderValue, Uri};
use std::fmt;

/// One caller-supplied HTTP header with an explicit redaction policy.
#[derive(Clone, Eq, PartialEq)]
pub struct Header {
    name: String,
    value: String,
    sensitive: bool,
}

impl Header {
    /// Validates and creates a header.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<String>,
        sensitive: bool,
    ) -> Result<Self, ResponsesAdapterError> {
        let name = name.into();
        let value = value.into();
        name.parse::<HeaderName>()
            .map_err(|_| ResponsesAdapterError::InvalidHeader)?;
        HeaderValue::from_str(&value).map_err(|_| ResponsesAdapterError::InvalidHeader)?;
        if matches!(
            name.to_ascii_lowercase().as_str(),
            "content-type" | "accept"
        ) {
            return Err(ResponsesAdapterError::InvalidHeader);
        }
        Ok(Self {
            name: name.to_ascii_lowercase(),
            value,
            sensitive,
        })
    }

    /// Returns the normalized lower-case header name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the value for the Runtime-owned HTTP transport.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Reports whether diagnostics must redact this value.
    pub fn is_sensitive(&self) -> bool {
        self.sensitive
    }
}

impl fmt::Debug for Header {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Header")
            .field("name", &self.name)
            .field(
                "value",
                if self.sensitive {
                    &"<redacted>" as &dyn fmt::Debug
                } else {
                    &self.value
                },
            )
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

/// Immutable configuration supplied by Garive Provider composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponsesAdapterConfig {
    endpoint: String,
    headers: Vec<Header>,
}

impl ResponsesAdapterConfig {
    /// Validates a deployment endpoint and explicit headers.
    pub fn new(
        endpoint: impl Into<String>,
        headers: Vec<Header>,
    ) -> Result<Self, ResponsesAdapterError> {
        let endpoint = endpoint.into();
        let uri = endpoint
            .parse::<Uri>()
            .map_err(|_| ResponsesAdapterError::InvalidEndpoint)?;
        let supported_scheme = match uri.scheme() {
            Some(scheme) => scheme == &Scheme::HTTP || scheme == &Scheme::HTTPS,
            None => false,
        };
        if !supported_scheme || uri.authority().is_none() || uri.path().is_empty() {
            return Err(ResponsesAdapterError::InvalidEndpoint);
        }
        let mut names = std::collections::BTreeSet::new();
        if headers.iter().any(|header| !names.insert(header.name())) {
            return Err(ResponsesAdapterError::InvalidHeader);
        }
        Ok(Self { endpoint, headers })
    }

    /// Returns the configured absolute endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns caller-supplied headers in stable order.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }
}

/// Stateless protocol adapter bound to explicit deployment configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponsesAdapter {
    config: ResponsesAdapterConfig,
}

impl ResponsesAdapter {
    /// Creates an adapter without consulting process-global configuration.
    pub fn new(config: ResponsesAdapterConfig) -> Self {
        Self { config }
    }

    /// Returns the immutable construction configuration.
    pub fn config(&self) -> &ResponsesAdapterConfig {
        &self.config
    }

    pub(crate) fn build_request(&self, body: Vec<u8>, stream: bool) -> HttpRequest {
        let mut headers = self.config.headers.clone();
        headers.push(Header {
            name: "content-type".into(),
            value: "application/json".into(),
            sensitive: false,
        });
        headers.push(Header {
            name: "accept".into(),
            value: if stream {
                "text/event-stream".into()
            } else {
                "application/json".into()
            },
            sensitive: false,
        });
        HttpRequest {
            uri: self.config.endpoint.clone(),
            headers,
            body,
        }
    }
}

/// Fully described single HTTP request for a Runtime-owned transport.
#[derive(Clone, Eq, PartialEq)]
pub struct HttpRequest {
    uri: String,
    headers: Vec<Header>,
    body: Vec<u8>,
}

impl HttpRequest {
    /// Returns the required HTTP method.
    pub fn method(&self) -> &'static str {
        "POST"
    }

    /// Returns the configured absolute URI.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Returns explicit and protocol-required headers.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// Returns the encoded JSON request body.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("uri", &self.uri)
            .field("headers", &self.headers)
            .field("body_length", &self.body.len())
            .finish()
    }
}
