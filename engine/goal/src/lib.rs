//! Portable durable Goal definitions, evidence, and pure lifecycle reduction.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod lifecycle;

pub use lifecycle::{GoalEvidenceKind, GoalEvidenceV1, GoalSnapshot, GoalState, GoalTransition};

const DEFINITION_CONTRACT: &str = "garive.goal-definition";
const CONTRACT_VERSION: u8 = 1;

macro_rules! identity {
    ($name:ident, $label:literal) => {
        #[doc = concat!("Non-empty opaque ", $label, " identity.")]
        #[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[doc = concat!("Validates and constructs a ", $label, " identity.")]
            pub fn new(value: impl Into<String>) -> Result<Self, GoalError> {
                let value = value.into();
                if value.is_empty() {
                    Err(GoalError::new(GoalErrorCode::GoalInvalid))
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the opaque identity text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

identity!(GoalId, "Goal");
identity!(GoalCriterionId, "Goal criterion");
identity!(GoalEvidenceId, "Goal evidence");

/// Stable portable Goal failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalErrorCode {
    /// Definition, identity, bound, evidence, or digest is malformed.
    GoalInvalid,
    /// Expected Goal revision does not match.
    GoalRevisionConflict,
    /// Requested lifecycle edge is not admitted.
    GoalTransitionInvalid,
    /// Success does not carry complete verified criterion evidence.
    GoalEvidenceInsufficient,
    /// Child scope, capability, parent identity, or bound exceeds its parent.
    GoalScopeExceeded,
}

/// Typed portable Goal failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GoalError {
    code: GoalErrorCode,
}

impl GoalError {
    const fn new(code: GoalErrorCode) -> Self {
        Self { code }
    }

    /// Returns the stable failure classification.
    pub const fn code(self) -> GoalErrorCode {
        self.code
    }
}

/// Exact capability revision available to one Goal.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalCapabilityReference {
    name: String,
    exact_revision: String,
}

impl GoalCapabilityReference {
    /// Validates one exact non-empty capability reference.
    pub fn new(
        name: impl Into<String>,
        exact_revision: impl Into<String>,
    ) -> Result<Self, GoalError> {
        let value = Self {
            name: name.into(),
            exact_revision: exact_revision.into(),
        };
        if value.name.is_empty() || value.exact_revision.is_empty() {
            Err(GoalError::new(GoalErrorCode::GoalInvalid))
        } else {
            Ok(value)
        }
    }
}

/// Bounded scope references; workspace values are opaque Runtime capabilities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalScopeV1 {
    session_id: Option<String>,
    workspace_capability_ids: BTreeSet<String>,
}

impl GoalScopeV1 {
    /// Requires a Session or at least one unique non-empty workspace capability.
    pub fn new(
        session_id: Option<String>,
        workspace_capability_ids: impl IntoIterator<Item = String>,
    ) -> Result<Self, GoalError> {
        let values: Vec<_> = workspace_capability_ids.into_iter().collect();
        let unique: BTreeSet<_> = values.iter().cloned().collect();
        if session_id.as_deref().is_some_and(str::is_empty)
            || values.iter().any(String::is_empty)
            || values.len() != unique.len()
            || session_id.is_none() && unique.is_empty()
        {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(Self {
            session_id,
            workspace_capability_ids: unique,
        })
    }

    fn is_within(&self, parent: &Self) -> bool {
        (self.session_id.is_none() || self.session_id == parent.session_id)
            && self
                .workspace_capability_ids
                .is_subset(&parent.workspace_capability_ids)
    }
}

/// Explicit non-zero hard bounds for one Goal definition.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalBoundsV1 {
    max_attempts: u32,
    max_plan_revisions: u32,
    max_child_goals: u32,
    token_budget: Option<u64>,
    duration_budget_ms: Option<u64>,
}

impl GoalBoundsV1 {
    /// Validates all mandatory and optional bounds as non-zero.
    pub fn new(
        max_attempts: u32,
        max_plan_revisions: u32,
        max_child_goals: u32,
        token_budget: Option<u64>,
        duration_budget_ms: Option<u64>,
    ) -> Result<Self, GoalError> {
        if max_attempts == 0
            || max_plan_revisions == 0
            || max_child_goals == 0
            || token_budget == Some(0)
            || duration_budget_ms == Some(0)
        {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(Self {
            max_attempts,
            max_plan_revisions,
            max_child_goals,
            token_budget,
            duration_budget_ms,
        })
    }

    /// Returns the hard limit on distinct attempts started from Draft.
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    fn is_within(&self, parent: &Self) -> bool {
        self.max_attempts <= parent.max_attempts
            && self.max_plan_revisions <= parent.max_plan_revisions
            && self.max_child_goals <= parent.max_child_goals
            && optional_bound_within(self.token_budget, parent.token_budget)
            && optional_bound_within(self.duration_budget_ms, parent.duration_budget_ms)
    }
}

/// Closed success criterion set; all declared criteria must be satisfied.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalCriterion {
    /// Explicit schema-bound user acceptance.
    UserAcceptance {
        /// Stable criterion identity.
        criterion_id: GoalCriterionId,
        /// Exact portable response schema digest.
        response_schema_digest: String,
    },
    /// Durable Artifact evidence.
    Artifact {
        /// Stable criterion identity.
        criterion_id: GoalCriterionId,
        /// Admitted Artifact kind.
        artifact_kind: String,
        /// Optional exact required Artifact digest.
        required_digest: Option<String>,
    },
    /// One exact durable fact/subject binding.
    DurableFact {
        /// Stable criterion identity.
        criterion_id: GoalCriterionId,
        /// Exact admitted fact kind.
        fact_kind: String,
        /// Exact durable subject digest.
        subject_digest: String,
    },
    /// Completion of a non-empty exact child Goal set.
    ChildGoals {
        /// Stable criterion identity.
        criterion_id: GoalCriterionId,
        /// Canonical unique child Goal identities.
        child_goal_ids: BTreeSet<GoalId>,
    },
}

impl GoalCriterion {
    /// Returns the stable criterion identity.
    pub const fn criterion_id(&self) -> &GoalCriterionId {
        match self {
            Self::UserAcceptance { criterion_id, .. }
            | Self::Artifact { criterion_id, .. }
            | Self::DurableFact { criterion_id, .. }
            | Self::ChildGoals { criterion_id, .. } => criterion_id,
        }
    }

    fn validate(&self) -> bool {
        if self.criterion_id().as_str().is_empty() {
            return false;
        }
        match self {
            Self::UserAcceptance {
                response_schema_digest,
                ..
            } => valid_digest(response_schema_digest),
            Self::Artifact {
                artifact_kind,
                required_digest,
                ..
            } => {
                !artifact_kind.is_empty()
                    && required_digest
                        .as_ref()
                        .is_none_or(|value| valid_digest(value))
            }
            Self::DurableFact {
                fact_kind,
                subject_digest,
                ..
            } => !fact_kind.is_empty() && valid_digest(subject_digest),
            Self::ChildGoals { child_goal_ids, .. } => {
                !child_goal_ids.is_empty()
                    && child_goal_ids
                        .iter()
                        .all(|value| !value.as_str().is_empty())
            }
        }
    }
}

/// Immutable canonical Goal definition revision content.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GoalDefinitionV1 {
    contract: &'static str,
    version: u8,
    goal_id: GoalId,
    objective: String,
    criteria: Vec<GoalCriterion>,
    scope: GoalScopeV1,
    bounds: GoalBoundsV1,
    parent_goal_id: Option<GoalId>,
    capability_references: BTreeSet<GoalCapabilityReference>,
}

impl GoalDefinitionV1 {
    /// Validates required text, unique criteria/capabilities and self-parenting.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        goal_id: GoalId,
        objective: impl Into<String>,
        criteria: Vec<GoalCriterion>,
        scope: GoalScopeV1,
        bounds: GoalBoundsV1,
        parent_goal_id: Option<GoalId>,
        capability_references: impl IntoIterator<Item = GoalCapabilityReference>,
    ) -> Result<Self, GoalError> {
        let objective = objective.into();
        let ids: BTreeSet<_> = criteria.iter().map(GoalCriterion::criterion_id).collect();
        let capability_values: Vec<_> = capability_references.into_iter().collect();
        let capabilities: BTreeSet<_> = capability_values.iter().cloned().collect();
        if goal_id.as_str().is_empty()
            || objective.is_empty()
            || criteria.is_empty()
            || ids.len() != criteria.len()
            || criteria.iter().any(|criterion| !criterion.validate())
            || capability_values.len() != capabilities.len()
            || capability_values
                .iter()
                .any(|value| value.name.is_empty() || value.exact_revision.is_empty())
            || parent_goal_id
                .as_ref()
                .is_some_and(|value| value.as_str().is_empty())
            || parent_goal_id.as_ref() == Some(&goal_id)
        {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(Self {
            contract: DEFINITION_CONTRACT,
            version: CONTRACT_VERSION,
            goal_id,
            objective,
            criteria,
            scope,
            bounds,
            parent_goal_id,
            capability_references: capabilities,
        })
    }

    /// Returns the exact Goal identity.
    pub const fn goal_id(&self) -> &GoalId {
        &self.goal_id
    }

    /// Returns criteria in semantic declaration order.
    pub fn criteria(&self) -> &[GoalCriterion] {
        &self.criteria
    }

    /// Returns the immutable hard bounds for this revision.
    pub const fn bounds(&self) -> &GoalBoundsV1 {
        &self.bounds
    }

    /// Proves that this child definition only narrows one exact parent grant.
    pub fn validate_child_of(&self, parent: &Self) -> Result<(), GoalError> {
        if self.parent_goal_id.as_ref() != Some(parent.goal_id())
            || !self.scope.is_within(&parent.scope)
            || !self.bounds.is_within(&parent.bounds)
            || !self
                .capability_references
                .is_subset(&parent.capability_references)
        {
            Err(GoalError::new(GoalErrorCode::GoalScopeExceeded))
        } else {
            Ok(())
        }
    }

    /// Reconstructs and revalidates one exact canonical definition document.
    pub fn from_canonical_json(json: &str) -> Result<Self, GoalError> {
        let raw: RawGoalDefinitionV1 =
            serde_json::from_str(json).map_err(|_| GoalError::new(GoalErrorCode::GoalInvalid))?;
        if raw.contract != DEFINITION_CONTRACT || raw.version != CONTRACT_VERSION {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        let scope = GoalScopeV1::new(raw.scope.session_id, raw.scope.workspace_capability_ids)?;
        let bounds = GoalBoundsV1::new(
            raw.bounds.max_attempts,
            raw.bounds.max_plan_revisions,
            raw.bounds.max_child_goals,
            raw.bounds.token_budget,
            raw.bounds.duration_budget_ms,
        )?;
        let value = Self::new(
            raw.goal_id,
            raw.objective,
            raw.criteria,
            scope,
            bounds,
            raw.parent_goal_id,
            raw.capability_references,
        )?;
        if value.canonical_json()? != json {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(value)
    }

    /// Returns the exact RFC 8785 definition document stored by Runtime.
    pub fn canonical_json(&self) -> Result<String, GoalError> {
        serde_jcs::to_string(self).map_err(|_| GoalError::new(GoalErrorCode::GoalInvalid))
    }

    /// Returns lowercase SHA-256 over the RFC 8785 canonical definition.
    pub fn digest(&self) -> Result<String, GoalError> {
        let bytes =
            serde_jcs::to_vec(self).map_err(|_| GoalError::new(GoalErrorCode::GoalInvalid))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGoalDefinitionV1 {
    contract: String,
    version: u8,
    goal_id: GoalId,
    objective: String,
    criteria: Vec<GoalCriterion>,
    scope: GoalScopeV1,
    bounds: GoalBoundsV1,
    parent_goal_id: Option<GoalId>,
    capability_references: Vec<GoalCapabilityReference>,
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

const fn optional_bound_within(child: Option<u64>, parent: Option<u64>) -> bool {
    match (child, parent) {
        (_, None) => true,
        (Some(child), Some(parent)) => child <= parent,
        (None, Some(_)) => false,
    }
}
