//! Explicit Runtime-supplied values shared by vendor connection profiles.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::{collections::BTreeSet, fmt};

use http::{header::HeaderName, uri::Scheme, HeaderValue, Uri};

/// Stable failure while validating explicit vendor connection values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VendorProfileError {
    /// Credential was empty.
    EmptyCredential,
    /// Credential contained a forbidden control character.
    InvalidCredential,
    /// Endpoint was not an absolute HTTP(S) URI with a path.
    InvalidEndpoint,
    /// Extra header name or value was invalid.
    InvalidHeader,
    /// Extra header names were duplicated case-insensitively.
    DuplicateHeader,
    /// Extra header attempted to replace a profile-owned header.
    ReservedHeader,
    /// A profile-owned constant violated a downstream adapter invariant.
    ProfileInvariant,
}

impl VendorProfileError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptyCredential => "empty_credential",
            Self::InvalidCredential => "invalid_credential",
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::InvalidHeader => "invalid_header",
            Self::DuplicateHeader => "duplicate_header",
            Self::ReservedHeader => "reserved_header",
            Self::ProfileInvariant => "profile_invariant",
        }
    }
}

/// Secret connection value with redacted diagnostics and no implicit loaders.
#[derive(Clone, Eq, PartialEq)]
pub struct SecretValue(String);

impl SecretValue {
    /// Validates one Runtime-supplied secret value.
    pub fn new(value: impl Into<String>) -> Result<Self, VendorProfileError> {
        let value = value.into();
        if value.is_empty() {
            return Err(VendorProfileError::EmptyCredential);
        }
        if value
            .chars()
            .any(|character| matches!(character, '\r' | '\n' | '\0'))
        {
            return Err(VendorProfileError::InvalidCredential);
        }
        Ok(Self(value))
    }

    /// Exposes the value only to the profile constructing a sensitive header.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue(<redacted>)")
    }
}

/// Explicit default or Runtime-selected endpoint policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EndpointSelection {
    /// Use the vendor profile's pinned default endpoint.
    Default,
    /// Use this explicit absolute endpoint.
    Explicit(String),
}

/// One caller-supplied extra header with explicit sensitivity.
#[derive(Clone, Eq, PartialEq)]
pub struct ExplicitHeader {
    name: String,
    value: String,
    sensitive: bool,
}

impl ExplicitHeader {
    /// Validates an extra header without applying vendor reservation policy.
    pub fn new(
        name: impl Into<String>,
        value: impl Into<String>,
        sensitive: bool,
    ) -> Result<Self, VendorProfileError> {
        let name = name.into();
        let value = value.into();
        name.parse::<HeaderName>()
            .map_err(|_| VendorProfileError::InvalidHeader)?;
        HeaderValue::from_str(&value).map_err(|_| VendorProfileError::InvalidHeader)?;
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

    /// Returns the exact header value.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Reports whether diagnostics must redact the value.
    pub const fn is_sensitive(&self) -> bool {
        self.sensitive
    }
}

impl fmt::Debug for ExplicitHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExplicitHeader")
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

/// Complete explicit connection input passed from Runtime to one profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionInput {
    endpoint: EndpointSelection,
    credential: SecretValue,
    extra_headers: Vec<ExplicitHeader>,
}

impl ConnectionInput {
    /// Creates one connection input without consulting external state.
    pub fn new(
        endpoint: EndpointSelection,
        credential: SecretValue,
        extra_headers: Vec<ExplicitHeader>,
    ) -> Self {
        Self {
            endpoint,
            credential,
            extra_headers,
        }
    }

    /// Resolves and validates this input against one vendor's constants.
    pub fn resolve(
        &self,
        default_endpoint: &str,
        reserved_headers: &[&str],
    ) -> Result<ResolvedConnection<'_>, VendorProfileError> {
        let endpoint = match &self.endpoint {
            EndpointSelection::Default => default_endpoint,
            EndpointSelection::Explicit(value) => value,
        };
        validate_endpoint(endpoint)?;
        let reserved: BTreeSet<_> = reserved_headers
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        let mut names = BTreeSet::new();
        for header in &self.extra_headers {
            if reserved.contains(header.name()) {
                return Err(VendorProfileError::ReservedHeader);
            }
            if !names.insert(header.name()) {
                return Err(VendorProfileError::DuplicateHeader);
            }
        }
        Ok(ResolvedConnection {
            endpoint: endpoint.to_owned(),
            credential: &self.credential,
            extra_headers: &self.extra_headers,
        })
    }
}

/// Validated borrowed connection values used during profile construction.
#[derive(Clone, Debug)]
pub struct ResolvedConnection<'a> {
    endpoint: String,
    credential: &'a SecretValue,
    extra_headers: &'a [ExplicitHeader],
}

impl<'a> ResolvedConnection<'a> {
    /// Returns the validated absolute endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the validated secret wrapper.
    pub const fn credential(&self) -> &'a SecretValue {
        self.credential
    }

    /// Returns validated, non-reserved, uniquely named extra headers.
    pub const fn extra_headers(&self) -> &'a [ExplicitHeader] {
        self.extra_headers
    }
}

fn validate_endpoint(endpoint: &str) -> Result<(), VendorProfileError> {
    let uri = endpoint
        .parse::<Uri>()
        .map_err(|_| VendorProfileError::InvalidEndpoint)?;
    let scheme_ok =
        matches!(uri.scheme(), Some(value) if value == &Scheme::HTTP || value == &Scheme::HTTPS);
    if !scheme_ok || uri.authority().is_none() || uri.path().is_empty() {
        Err(VendorProfileError::InvalidEndpoint)
    } else {
        Ok(())
    }
}
