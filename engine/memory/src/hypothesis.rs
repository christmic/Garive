use serde::Serialize;

use crate::{
    values::{valid_digest, valid_text, MAX_REFERENCE_BYTES},
    MemoryError, MemoryErrorCode, MemoryKind,
};

/// Cognitive lifecycle class, independent of content role.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Stable facts, preferences, constraints, and decisions.
    Semantic,
    /// Bounded index of one past episode or Session.
    Episodic,
    /// Negative or corrective knowledge derived from outcomes.
    Lesson,
    /// Versioned reusable workflow or playbook.
    Procedural,
}

/// M1 content role preserving M0 meaning.
pub type MemoryRole = MemoryKind;

/// Provenance authority of a memory hypothesis.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryAuthority {
    /// Law backed by an explicit user-command receipt.
    UserDeclared,
    /// Correctable hypothesis proposed by the Agent.
    AgentLearned,
    /// Published organisational truth backed by a receipt.
    OrganisationPublished,
}

/// Receipt-bound authority claim; Runtime still verifies the receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryAuthorityBinding {
    authority: MemoryAuthority,
    receipt_digest: Option<String>,
}

impl MemoryAuthorityBinding {
    /// Constructs an authority binding with exact receipt-shape rules.
    pub fn new(
        authority: MemoryAuthority,
        receipt_digest: Option<String>,
    ) -> Result<Self, MemoryError> {
        let requires = authority != MemoryAuthority::AgentLearned;
        if requires != receipt_digest.is_some()
            || receipt_digest.as_deref().is_some_and(|v| !valid_digest(v))
        {
            return Err(MemoryError::new(if requires && receipt_digest.is_none() {
                MemoryErrorCode::AuthorityReceiptRequired
            } else {
                MemoryErrorCode::InvalidMemory
            }));
        }
        Ok(Self {
            authority,
            receipt_digest,
        })
    }
    /// Returns the declared authority.
    pub const fn authority(&self) -> MemoryAuthority {
        self.authority
    }
    /// Returns the frozen receipt digest when required.
    pub fn receipt_digest(&self) -> Option<&str> {
        self.receipt_digest.as_deref()
    }
}

/// Privacy and ownership class; identifiers remain Runtime-opaque.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeClass {
    /// One Session only.
    Session,
    /// One installed Agent instance.
    AgentInstance,
    /// One opaque authorized user namespace.
    User,
    /// One opaque authorized project namespace.
    Project,
    /// Aggregated platform namespace requiring extra policy.
    Platform,
}

/// Scope class with the additional policy binding required for Platform recall.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryScopeBinding {
    scope: MemoryScopeClass,
    aggregation_policy_digest: Option<String>,
}

impl MemoryScopeBinding {
    /// Validates a scope binding. Only Platform requires and permits a policy digest.
    pub fn new(
        scope: MemoryScopeClass,
        aggregation_policy_digest: Option<String>,
    ) -> Result<Self, MemoryError> {
        let platform = scope == MemoryScopeClass::Platform;
        if platform != aggregation_policy_digest.is_some()
            || aggregation_policy_digest
                .as_deref()
                .is_some_and(|v| !valid_digest(v))
        {
            return Err(MemoryError::new(
                if platform && aggregation_policy_digest.is_none() {
                    MemoryErrorCode::ScopePolicyDenied
                } else {
                    MemoryErrorCode::InvalidMemory
                },
            ));
        }
        Ok(Self {
            scope,
            aggregation_policy_digest,
        })
    }
    /// Returns the scope class.
    pub const fn scope(&self) -> MemoryScopeClass {
        self.scope
    }
}

/// One immutable M1 registry row selecting admitted policy revisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryTypeDescriptor {
    memory_type: MemoryType,
    roles: Vec<MemoryRole>,
    authorities: Vec<MemoryAuthority>,
    lifecycle: String,
    recall: String,
    retention: String,
    surface_kind: String,
}

impl MemoryTypeDescriptor {
    /// Constructs one canonical, non-empty descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory_type: MemoryType,
        roles: Vec<MemoryRole>,
        authorities: Vec<MemoryAuthority>,
        lifecycle: impl Into<String>,
        recall: impl Into<String>,
        retention: impl Into<String>,
        surface_kind: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            memory_type,
            roles,
            authorities,
            lifecycle: lifecycle.into(),
            recall: recall.into(),
            retention: retention.into(),
            surface_kind: surface_kind.into(),
        };
        if value.roles.is_empty()
            || !ordered_unique(&value.roles)
            || value.authorities.is_empty()
            || !ordered_unique(&value.authorities)
            || [
                &value.lifecycle,
                &value.recall,
                &value.retention,
                &value.surface_kind,
            ]
            .iter()
            .any(|v| !valid_text(v, MAX_REFERENCE_BYTES))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the memory type.
    pub const fn memory_type(&self) -> MemoryType {
        self.memory_type
    }
    /// Returns admitted roles.
    pub fn roles(&self) -> &[MemoryRole] {
        &self.roles
    }
    /// Returns admitted authorities.
    pub fn authorities(&self) -> &[MemoryAuthority] {
        &self.authorities
    }
    /// Tests an exact type/role/authority combination.
    pub fn admits(&self, role: MemoryRole, authority: MemoryAuthority) -> bool {
        self.roles.contains(&role) && self.authorities.contains(&authority)
    }
}

/// Frozen complete M1 type registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryTypeRegistry {
    revision: String,
    descriptors: Vec<MemoryTypeDescriptor>,
}

impl MemoryTypeRegistry {
    /// Requires canonical rows covering every M1 type exactly once.
    pub fn new(
        revision: impl Into<String>,
        descriptors: Vec<MemoryTypeDescriptor>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            revision: revision.into(),
            descriptors,
        };
        let types: Vec<_> = value
            .descriptors
            .iter()
            .map(MemoryTypeDescriptor::memory_type)
            .collect();
        if !valid_text(&value.revision, MAX_REFERENCE_BYTES)
            || types
                != [
                    MemoryType::Semantic,
                    MemoryType::Episodic,
                    MemoryType::Lesson,
                    MemoryType::Procedural,
                ]
        {
            return Err(MemoryError::new(MemoryErrorCode::UnknownMemoryType));
        }
        Ok(value)
    }
    /// Tests admission under the exact registry.
    pub fn admits(
        &self,
        memory_type: MemoryType,
        role: MemoryRole,
        authority: MemoryAuthority,
    ) -> bool {
        self.descriptors
            .iter()
            .find(|v| v.memory_type == memory_type)
            .is_some_and(|v| v.admits(role, authority))
    }
}

/// Explicit result of importing one M0 role into M1.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedMemoryClassification {
    /// Assigned cognitive type.
    pub memory_type: MemoryType,
    /// Preserved M0 content role.
    pub role: MemoryRole,
    /// Explicit authority binding.
    pub authority: MemoryAuthorityBinding,
}

/// Maps M0 meaning without inspecting content or inferring authority.
pub fn import_m0_classification(
    kind: MemoryKind,
    authority: MemoryAuthorityBinding,
) -> ImportedMemoryClassification {
    ImportedMemoryClassification {
        memory_type: if kind == MemoryKind::Summary {
            MemoryType::Episodic
        } else {
            MemoryType::Semantic
        },
        role: kind,
        authority,
    }
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
