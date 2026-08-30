//! Exact C5b resource declarations and policy coverage.

use std::net::{Ipv4Addr, Ipv6Addr};

use serde::Serialize;
use serde_json::Value;

use crate::prepared::{PreparationError, PreparationErrorCode};

/// Pure trusted resolver from schema-validated arguments to exact resources.
pub trait ToolAccessResolver {
    /// Returns the immutable resolver implementation revision.
    fn revision(&self) -> &str;

    /// Resolves a bounded exact access set without authority or I/O.
    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError>;
}

/// Closed namespace order used by canonical access sets and conflict graphs.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessNamespace {
    /// Workspace-relative filesystem identity.
    Filesystem,
    /// Admitted process executor lane.
    Process,
    /// Canonical HTTP origin identity.
    Network,
    /// Runtime-owned logical lane.
    Runtime,
}

/// Access strength in canonical read, write, exclusive order.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    /// Non-mutating access to one exact resource.
    Read,
    /// Mutation of one exact resource.
    Write,
    /// Namespace-wide exclusion for one planner step.
    Exclusive,
}

/// One canonical exact resource access derived from validated arguments.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceAccess {
    namespace: AccessNamespace,
    resource_key: String,
    mode: AccessMode,
}

impl ResourceAccess {
    /// Validates and constructs one namespace-specific canonical key.
    pub fn new(
        namespace: AccessNamespace,
        resource_key: impl Into<String>,
        mode: AccessMode,
    ) -> Result<Self, PreparationError> {
        let resource_key = resource_key.into();
        if !valid_key(namespace, &resource_key) {
            return Err(access_error());
        }
        Ok(Self {
            namespace,
            resource_key,
            mode,
        })
    }

    /// Returns the closed resource namespace.
    pub const fn namespace(&self) -> AccessNamespace {
        self.namespace
    }

    /// Returns the canonical opaque identity.
    pub fn resource_key(&self) -> &str {
        &self.resource_key
    }

    /// Returns the exact requested access mode.
    pub const fn mode(&self) -> AccessMode {
        self.mode
    }
}

/// Non-empty canonical ordered unique invocation accesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct InvocationAccessSet(Vec<ResourceAccess>);

impl InvocationAccessSet {
    /// Sorts exact accesses and rejects empty or duplicate resource identities.
    pub fn new(
        accesses: impl IntoIterator<Item = ResourceAccess>,
    ) -> Result<Self, PreparationError> {
        let mut accesses: Vec<_> = accesses.into_iter().collect();
        accesses.sort();
        if accesses.is_empty()
            || accesses.windows(2).any(|pair| {
                pair[0].namespace == pair[1].namespace
                    && pair[0].resource_key == pair[1].resource_key
            })
        {
            return Err(access_error());
        }
        Ok(Self(accesses))
    }

    /// Returns the canonical exact accesses.
    pub fn values(&self) -> &[ResourceAccess] {
        &self.0
    }
}

/// One policy ceiling entry used by a namespace-specific list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AccessPolicyEntry {
    resource: String,
    allowed_modes: Vec<AccessMode>,
}

impl AccessPolicyEntry {
    /// Sorts a non-empty unique mode set for one non-empty resource ceiling.
    pub fn new(
        resource: impl Into<String>,
        modes: impl IntoIterator<Item = AccessMode>,
    ) -> Result<Self, PreparationError> {
        let resource = resource.into();
        let mut allowed_modes: Vec<_> = modes.into_iter().collect();
        allowed_modes.sort();
        if resource.is_empty()
            || allowed_modes.is_empty()
            || allowed_modes.windows(2).any(|pair| pair[0] == pair[1])
        {
            return Err(access_error());
        }
        Ok(Self {
            resource,
            allowed_modes,
        })
    }
}

/// Frozen v1 maximum access surface and result charge for one Tool revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ToolAccessPolicyV1 {
    policy_revision: String,
    filesystem_roots: Vec<AccessPolicyEntry>,
    process_lanes: Vec<AccessPolicyEntry>,
    network_origins: Vec<AccessPolicyEntry>,
    runtime_lanes: Vec<AccessPolicyEntry>,
    max_accesses: usize,
    max_result_bytes: u64,
}

impl ToolAccessPolicyV1 {
    /// Validates namespace keys, canonical ordering, duplicates, and bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        policy_revision: impl Into<String>,
        filesystem_roots: impl IntoIterator<Item = AccessPolicyEntry>,
        process_lanes: impl IntoIterator<Item = AccessPolicyEntry>,
        network_origins: impl IntoIterator<Item = AccessPolicyEntry>,
        runtime_lanes: impl IntoIterator<Item = AccessPolicyEntry>,
        max_accesses: usize,
        max_result_bytes: u64,
    ) -> Result<Self, PreparationError> {
        let policy_revision = policy_revision.into();
        let filesystem_roots = policy_entries(AccessNamespace::Filesystem, filesystem_roots)?;
        let process_lanes = policy_entries(AccessNamespace::Process, process_lanes)?;
        let network_origins = policy_entries(AccessNamespace::Network, network_origins)?;
        let runtime_lanes = policy_entries(AccessNamespace::Runtime, runtime_lanes)?;
        if policy_revision.is_empty() || max_accesses == 0 || max_result_bytes == 0 {
            return Err(access_error());
        }
        Ok(Self {
            policy_revision,
            filesystem_roots,
            process_lanes,
            network_origins,
            runtime_lanes,
            max_accesses,
            max_result_bytes,
        })
    }

    /// Returns whether every exact access is inside this authority ceiling.
    pub fn covers(&self, accesses: &InvocationAccessSet) -> bool {
        accesses.0.len() <= self.max_accesses
            && accesses.0.iter().all(|access| {
                self.entries(access.namespace).iter().any(|entry| {
                    entry.allowed_modes.contains(&access.mode)
                        && match access.namespace {
                            AccessNamespace::Filesystem => {
                                access.resource_key == entry.resource
                                    || access
                                        .resource_key
                                        .starts_with(&format!("{}/", entry.resource))
                            }
                            _ => access.resource_key == entry.resource,
                        }
                })
            })
    }

    /// Returns the immutable policy revision.
    pub fn policy_revision(&self) -> &str {
        &self.policy_revision
    }

    /// Returns the maximum exact accesses for one invocation.
    pub const fn max_accesses(&self) -> usize {
        self.max_accesses
    }

    /// Returns the maximum buffered result charge.
    pub const fn max_result_bytes(&self) -> u64 {
        self.max_result_bytes
    }

    fn entries(&self, namespace: AccessNamespace) -> &[AccessPolicyEntry] {
        match namespace {
            AccessNamespace::Filesystem => &self.filesystem_roots,
            AccessNamespace::Process => &self.process_lanes,
            AccessNamespace::Network => &self.network_origins,
            AccessNamespace::Runtime => &self.runtime_lanes,
        }
    }
}

fn policy_entries(
    namespace: AccessNamespace,
    entries: impl IntoIterator<Item = AccessPolicyEntry>,
) -> Result<Vec<AccessPolicyEntry>, PreparationError> {
    let mut entries: Vec<_> = entries.into_iter().collect();
    entries.sort_by(|left, right| left.resource.cmp(&right.resource));
    if entries
        .iter()
        .any(|entry| !valid_key(namespace, &entry.resource))
        || entries
            .windows(2)
            .any(|pair| pair[0].resource == pair[1].resource)
    {
        return Err(access_error());
    }
    Ok(entries)
}

fn valid_key(namespace: AccessNamespace, key: &str) -> bool {
    match namespace {
        AccessNamespace::Filesystem => {
            !key.starts_with('/')
                && !key.contains('\0')
                && !key.contains('\\')
                && key
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
        }
        AccessNamespace::Network => canonical_origin(key),
        AccessNamespace::Process | AccessNamespace::Runtime => {
            !key.is_empty()
                && key.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
                })
        }
    }
}

fn canonical_origin(origin: &str) -> bool {
    let authority = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"));
    let Some(authority) = authority else {
        return false;
    };
    if authority.contains(['/', '?', '#', '@']) {
        return false;
    }
    let (host, port) = if authority.starts_with('[') {
        let Some(end) = authority.find(']') else {
            return false;
        };
        let host = &authority[1..end];
        if host.contains('.')
            || host
                .parse::<Ipv6Addr>()
                .map(|value| value.to_string())
                .ok()
                .as_deref()
                != Some(host)
        {
            return false;
        }
        (
            host,
            authority
                .get(end + 2..)
                .filter(|_| authority.as_bytes().get(end + 1) == Some(&b':')),
        )
    } else {
        let Some((host, port)) = authority.rsplit_once(':') else {
            return false;
        };
        let canonical_ip = host
            .parse::<Ipv4Addr>()
            .map(|value| value.to_string())
            .ok()
            .as_deref()
            == Some(host);
        let canonical_dns = host.len() <= 253
            && host.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
            })
            && !host.starts_with('.')
            && !host.ends_with('.')
            && !host.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
            });
        if !canonical_ip && !canonical_dns {
            return false;
        }
        (host, Some(port))
    };
    !host.is_empty()
        && port.is_some_and(|value| {
            value
                .parse::<u16>()
                .is_ok_and(|number| number != 0 && number.to_string() == value)
        })
}

fn access_error() -> PreparationError {
    PreparationError::new(PreparationErrorCode::EffectAccessInvalid)
}
