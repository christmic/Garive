//! Pure revisioned Goal lifecycle and success-evidence reduction.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    valid_digest, valid_reference, GoalCriterion, GoalCriterionId, GoalDefinitionV1, GoalError,
    GoalErrorCode, GoalEvidenceId, MAX_COLLECTION_ITEMS,
};

/// Closed evidence family matching the four G1 criterion variants.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalEvidenceKind {
    /// Schema-validated user response evidence.
    UserAcceptance,
    /// Durable Artifact evidence.
    Artifact,
    /// Durable Ledger fact evidence.
    DurableFact,
    /// Verified child-Goal terminal evidence.
    ChildGoals,
}

/// Exact evidence reference evaluated at a frozen commit version.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GoalEvidenceV1 {
    evidence_id: GoalEvidenceId,
    criterion_id: GoalCriterionId,
    kind: GoalEvidenceKind,
    durable_reference: String,
    evidence_digest: String,
    observed_at_commit_version: u64,
}

impl GoalEvidenceV1 {
    /// Validates a non-empty reference, canonical digest, and positive position.
    pub fn new(
        evidence_id: GoalEvidenceId,
        criterion_id: GoalCriterionId,
        kind: GoalEvidenceKind,
        durable_reference: impl Into<String>,
        evidence_digest: impl Into<String>,
        observed_at_commit_version: u64,
    ) -> Result<Self, GoalError> {
        let value = Self {
            evidence_id,
            criterion_id,
            kind,
            durable_reference: durable_reference.into(),
            evidence_digest: evidence_digest.into(),
            observed_at_commit_version,
        };
        if !valid_reference(&value.durable_reference)
            || !valid_digest(&value.evidence_digest)
            || value.observed_at_commit_version == 0
        {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(value)
    }

    /// Returns the criterion satisfied by this evidence.
    pub const fn criterion_id(&self) -> &GoalCriterionId {
        &self.criterion_id
    }

    /// Returns the declared evidence family.
    pub const fn kind(&self) -> GoalEvidenceKind {
        self.kind
    }

    /// Serializes an ordered evidence set for one durable content binding.
    pub fn canonical_json(values: &[Self]) -> Result<String, GoalError> {
        if values.len() > MAX_COLLECTION_ITEMS {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        serde_jcs::to_string(values).map_err(|_| GoalError::new(GoalErrorCode::GoalInvalid))
    }

    /// Reconstructs and revalidates an exact canonical evidence set document.
    pub fn list_from_canonical_json(json: &str) -> Result<Vec<Self>, GoalError> {
        let raw: Vec<Self> =
            serde_json::from_str(json).map_err(|_| GoalError::new(GoalErrorCode::GoalInvalid))?;
        let mut values = Vec::with_capacity(raw.len());
        for value in raw {
            values.push(Self::new(
                value.evidence_id,
                value.criterion_id,
                value.kind,
                value.durable_reference,
                value.evidence_digest,
                value.observed_at_commit_version,
            )?);
        }
        if Self::canonical_json(&values)? != json {
            return Err(GoalError::new(GoalErrorCode::GoalInvalid));
        }
        Ok(values)
    }
}

/// Closed durable Goal lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalState {
    /// Definition may still be revised before work starts.
    Draft,
    /// Work is currently admitted.
    Active,
    /// Typed external input or reconciliation is required.
    Suspended,
    /// Every success criterion has verified evidence.
    Succeeded,
    /// Work ended unsuccessfully under an explicit terminal command.
    Failed,
    /// Authenticated actor cancelled the Goal.
    Cancelled,
}

impl GoalState {
    /// Returns whether no later lifecycle transition is permitted.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// One requested pure lifecycle transition after Runtime command validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GoalTransition {
    /// Start or resume work.
    Activate,
    /// Pause for one stable non-empty reason code.
    Suspend(String),
    /// Close successfully with complete exact evidence.
    Succeed(Vec<GoalEvidenceV1>),
    /// Close unsuccessfully with one stable non-empty reason code.
    Fail(String),
    /// Cancel with one stable non-empty reason code.
    Cancel(String),
    /// Replace definition content and return to Draft.
    Revise(Box<GoalDefinitionV1>),
}

/// Immutable Goal projection after one contiguous durable prefix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoalSnapshot {
    definition: GoalDefinitionV1,
    revision: u64,
    state: GoalState,
    terminal_evidence: Vec<GoalEvidenceV1>,
}

impl GoalSnapshot {
    /// Creates revision 1 in Draft from a validated definition.
    pub const fn new(definition: GoalDefinitionV1) -> Self {
        Self {
            definition,
            revision: 1,
            state: GoalState::Draft,
            terminal_evidence: Vec::new(),
        }
    }

    /// Returns the positive contiguous Goal revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the current durable lifecycle state.
    pub const fn state(&self) -> GoalState {
        self.state
    }

    /// Returns the exact current definition revision content.
    pub const fn definition(&self) -> &GoalDefinitionV1 {
        &self.definition
    }

    /// Applies one transition only at the caller's exact expected revision.
    pub fn apply(
        &self,
        expected_revision: u64,
        transition: GoalTransition,
    ) -> Result<Self, GoalError> {
        if expected_revision != self.revision {
            return Err(GoalError::new(GoalErrorCode::GoalRevisionConflict));
        }
        if self.state.is_terminal() {
            return Err(GoalError::new(GoalErrorCode::GoalTransitionInvalid));
        }
        let mut next = self.clone();
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or_else(|| GoalError::new(GoalErrorCode::GoalInvalid))?;
        match transition {
            GoalTransition::Activate
                if matches!(self.state, GoalState::Draft | GoalState::Suspended) =>
            {
                next.state = GoalState::Active;
            }
            GoalTransition::Suspend(reason)
                if self.state == GoalState::Active && valid_reference(&reason) =>
            {
                next.state = GoalState::Suspended;
            }
            GoalTransition::Succeed(evidence) if self.state == GoalState::Active => {
                validate_evidence(self.definition.criteria(), &evidence)?;
                next.state = GoalState::Succeeded;
                next.terminal_evidence = evidence;
            }
            GoalTransition::Fail(reason)
                if matches!(self.state, GoalState::Active | GoalState::Suspended)
                    && valid_reference(&reason) =>
            {
                next.state = GoalState::Failed;
            }
            GoalTransition::Cancel(reason) if valid_reference(&reason) => {
                next.state = GoalState::Cancelled;
            }
            GoalTransition::Revise(definition)
                if definition.goal_id() == self.definition.goal_id() =>
            {
                next.definition = *definition;
                next.state = GoalState::Draft;
                next.terminal_evidence.clear();
            }
            _ => return Err(GoalError::new(GoalErrorCode::GoalTransitionInvalid)),
        }
        Ok(next)
    }
}

fn validate_evidence(
    criteria: &[GoalCriterion],
    evidence: &[GoalEvidenceV1],
) -> Result<(), GoalError> {
    let by_criterion: BTreeMap<_, _> = evidence
        .iter()
        .map(|value| (value.criterion_id(), value))
        .collect();
    let evidence_ids: BTreeMap<_, _> = evidence
        .iter()
        .map(|value| (&value.evidence_id, value))
        .collect();
    if by_criterion.len() != evidence.len()
        || evidence_ids.len() != evidence.len()
        || criteria.len() != evidence.len()
        || criteria.iter().any(|criterion| {
            by_criterion
                .get(criterion.criterion_id())
                .is_none_or(|value| value.kind() != criterion_kind(criterion))
        })
    {
        return Err(GoalError::new(GoalErrorCode::GoalEvidenceInsufficient));
    }
    Ok(())
}

const fn criterion_kind(criterion: &GoalCriterion) -> GoalEvidenceKind {
    match criterion {
        GoalCriterion::UserAcceptance { .. } => GoalEvidenceKind::UserAcceptance,
        GoalCriterion::Artifact { .. } => GoalEvidenceKind::Artifact,
        GoalCriterion::DurableFact { .. } => GoalEvidenceKind::DurableFact,
        GoalCriterion::ChildGoals { .. } => GoalEvidenceKind::ChildGoals,
    }
}
