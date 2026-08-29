//! Explicit endpoint, version, and header construction for one exchange.

use crate::MessagesAdapterError;
use http::{header::HeaderName, uri::Scheme, HeaderValue, Uri};
use std::{collections::BTreeSet, fmt};

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
    ) -> Result<Self, MessagesAdapterError> {
        let name = name.into();
        let value = value.into();
        name.parse::<HeaderName>()
            .map_err(|_| MessagesAdapterError::InvalidHeader)?;
        HeaderValue::from_str(&value).map_err(|_| MessagesAdapterError::InvalidHeader)?;
        Ok(Self {
            name: name.to_ascii_lowercase(),
            value,
            sensitive,
        })
    }

    /// Returns the normalized lower-case name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the value for the Runtime-owned transport.
    pub fn value(&self) -> &str {
        &self.value
    }
    /// Reports whether diagnostics must redact the value.
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

/// Immutable deployment configuration supplied by Garive composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagesAdapterConfig {
    endpoint: String,
    headers: Vec<Header>,
    version_header_name: String,
    protocol_version: String,
}

impl MessagesAdapterConfig {
    /// Validates an endpoint, explicit headers, and protocol version header.
    pub fn new(
        endpoint: impl Into<String>,
        headers: Vec<Header>,
        version_header_name: impl Into<String>,
        protocol_version: impl Into<String>,
    ) -> Result<Self, MessagesAdapterError> {
        let endpoint = endpoint.into();
        let uri = endpoint
            .parse::<Uri>()
            .map_err(|_| MessagesAdapterError::InvalidEndpoint)?;
        let scheme_ok =
            matches!(uri.scheme(), Some(s) if s == &Scheme::HTTP || s == &Scheme::HTTPS);
        if !scheme_ok || uri.authority().is_none() || uri.path().is_empty() {
            return Err(MessagesAdapterError::InvalidEndpoint);
        }
        let version_header_name = version_header_name.into().to_ascii_lowercase();
        version_header_name
            .parse::<HeaderName>()
            .map_err(|_| MessagesAdapterError::InvalidHeader)?;
        let protocol_version = protocol_version.into();
        if protocol_version.is_empty() {
            return Err(MessagesAdapterError::InvalidProtocolVersion);
        }
        HeaderValue::from_str(&protocol_version)
            .map_err(|_| MessagesAdapterError::InvalidHeader)?;
        let mut names = BTreeSet::new();
        if matches!(version_header_name.as_str(), "content-type" | "accept")
            || headers.iter().any(|header| {
                matches!(header.name(), "content-type" | "accept")
                    || header.name() == version_header_name
                    || !names.insert(header.name())
            })
        {
            return Err(MessagesAdapterError::InvalidHeader);
        }
        Ok(Self {
            endpoint,
            headers,
            version_header_name,
            protocol_version,
        })
    }

    /// Returns the configured absolute endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    /// Returns caller-supplied headers in stable order.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }
    /// Returns the configured version header name.
    pub fn version_header_name(&self) -> &str {
        &self.version_header_name
    }
    /// Returns the configured protocol version value.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }
}

/// Stateless protocol adapter bound to explicit deployment configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagesAdapter {
    config: MessagesAdapterConfig,
}

impl MessagesAdapter {
    /// Creates an adapter without consulting process-global configuration.
    pub fn new(config: MessagesAdapterConfig) -> Self {
        Self { config }
    }
    /// Returns the immutable construction configuration.
    pub fn config(&self) -> &MessagesAdapterConfig {
        &self.config
    }

    pub(crate) fn build_request(&self, body: Vec<u8>, stream: bool) -> HttpRequest {
        let mut headers = self.config.headers.clone();
        headers.push(Header {
            name: self.config.version_header_name.clone(),
            value: self.config.protocol_version.clone(),
            sensitive: false,
        });
        headers.push(Header {
            name: "content-type".into(),
            value: "application/json".into(),
            sensitive: false,
        });
        headers.push(Header {
            name: "accept".into(),
            value: if stream {
                "text/event-stream"
            } else {
                "application/json"
            }
            .into(),
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
