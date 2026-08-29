use std::collections::BTreeSet;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;

use crate::{
    values::{valid_digest, valid_id},
    ContentBinding, DurableFactReference, MemoryError, MemoryErrorCode, MemoryKind, MemoryScope,
    MemorySensitivity, MemoryStatus,
};

const MAX_CONFIDENCE_BASIS_POINTS: u16 = 10_000;

/// Untrusted candidate write carrying evidence but no authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryProposal {
    proposal_id: String,
    namespace_id: String,
    scope: MemoryScope,
    kind: MemoryKind,
    content: ContentBinding,
    evidence: Vec<DurableFactReference>,
    sensitivity: MemorySensitivity,
    confidence_basis_points: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_active_revision_id: Option<String>,
}

impl MemoryProposal {
    /// Validates one proposal and its ordered non-empty evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        proposal_id: impl Into<String>,
        namespace_id: impl Into<String>,
        scope: MemoryScope,
        kind: MemoryKind,
        content: ContentBinding,
        evidence: Vec<DurableFactReference>,
        sensitivity: MemorySensitivity,
        confidence_basis_points: u16,
        expected_active_revision_id: Option<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            proposal_id: proposal_id.into(),
            namespace_id: namespace_id.into(),
            scope,
            kind,
            content,
            evidence,
            sensitivity,
            confidence_basis_points,
            expected_active_revision_id,
        };
        if !valid_id(&value.proposal_id)
            || !valid_id(&value.namespace_id)
            || value.evidence.is_empty()
            || !ordered_unique(&value.evidence)
            || value.confidence_basis_points > MAX_CONFIDENCE_BASIS_POINTS
            || value
                .expected_active_revision_id
                .as_deref()
                .is_some_and(|revision| !valid_id(revision))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the proposal identity.
    pub fn proposal_id(&self) -> &str {
        &self.proposal_id
    }
    /// Returns the authorized namespace requested by the proposal.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the requested record scope.
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    /// Returns the proposed record kind.
    pub const fn kind(&self) -> MemoryKind {
        self.kind
    }
    /// Returns the exact proposed content binding.
    pub const fn content(&self) -> &ContentBinding {
        &self.content
    }
    /// Returns ordered durable evidence references.
    pub fn evidence(&self) -> &[DurableFactReference] {
        &self.evidence
    }
    /// Returns the proposed sensitivity class.
    pub const fn sensitivity(&self) -> MemorySensitivity {
        self.sensitivity
    }
    /// Returns the provenance confidence metadata.
    pub const fn confidence_basis_points(&self) -> u16 {
        self.confidence_basis_points
    }
    /// Returns the expected active revision, when updating.
    pub fn expected_active_revision_id(&self) -> Option<&str> {
        self.expected_active_revision_id.as_deref()
    }
}

/// Runtime-authorized immutable revision coordinates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCommit {
    record_id: String,
    revision_id: String,
    retention_policy_digest: String,
    valid_from_position: u64,
    expires_at_utc: Option<String>,
    supersedes_revision_id: Option<String>,
}

impl MemoryCommit {
    /// Validates exact record, revision, retention, time and supersession bindings.
    pub fn new(
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        retention_policy_digest: impl Into<String>,
        valid_from_position: u64,
        expires_at_utc: Option<String>,
        supersedes_revision_id: Option<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            retention_policy_digest: retention_policy_digest.into(),
            valid_from_position,
            expires_at_utc,
            supersedes_revision_id,
        };
        if !valid_id(&value.record_id)
            || !valid_id(&value.revision_id)
            || !valid_digest(&value.retention_policy_digest)
            || value.valid_from_position == 0
            || value
                .expires_at_utc
                .as_deref()
                .is_some_and(|time| !canonical_utc(time))
            || value
                .supersedes_revision_id
                .as_deref()
                .is_some_and(|revision| !valid_id(revision) || revision == value.revision_id)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the stable logical record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the new immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the frozen retention-policy binding.
    pub fn retention_policy_digest(&self) -> &str {
        &self.retention_policy_digest
    }
    /// Returns the first durable position at which the revision is visible.
    pub const fn valid_from_position(&self) -> u64 {
        self.valid_from_position
    }
    /// Returns the canonical expiry time, when any.
    pub fn expires_at_utc(&self) -> Option<&str> {
        self.expires_at_utc.as_deref()
    }
    /// Returns the exact prior revision replaced by this commit.
    pub fn supersedes_revision_id(&self) -> Option<&str> {
        self.supersedes_revision_id.as_deref()
    }
}

/// One immutable governed memory revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MemoryRecord {
    record_id: String,
    revision_id: String,
    namespace_id: String,
    scope: MemoryScope,
    kind: MemoryKind,
    content: ContentBinding,
    evidence: Vec<DurableFactReference>,
    status: MemoryStatus,
    sensitivity: MemorySensitivity,
    confidence_basis_points: u16,
    valid_from_position: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    supersedes_revision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at_utc: Option<String>,
}

impl MemoryRecord {
    /// Validates a complete immutable record revision.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        namespace_id: impl Into<String>,
        scope: MemoryScope,
        kind: MemoryKind,
        content: ContentBinding,
        evidence: Vec<DurableFactReference>,
        status: MemoryStatus,
        sensitivity: MemorySensitivity,
        confidence_basis_points: u16,
        valid_from_position: u64,
        supersedes_revision_id: Option<String>,
        expires_at_utc: Option<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            namespace_id: namespace_id.into(),
            scope,
            kind,
            content,
            evidence,
            status,
            sensitivity,
            confidence_basis_points,
            valid_from_position,
            supersedes_revision_id,
            expires_at_utc,
        };
        if !valid_id(&value.record_id)
            || !valid_id(&value.revision_id)
            || !valid_id(&value.namespace_id)
            || value.evidence.is_empty()
            || !ordered_unique(&value.evidence)
            || value.confidence_basis_points > MAX_CONFIDENCE_BASIS_POINTS
            || value.valid_from_position == 0
            || value
                .supersedes_revision_id
                .as_deref()
                .is_some_and(|revision| !valid_id(revision))
            || value
                .expires_at_utc
                .as_deref()
                .is_some_and(|time| !canonical_utc(time))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }

    /// Returns the stable logical record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the namespace boundary.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the authorized scope.
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    /// Returns the portable record kind.
    pub const fn kind(&self) -> MemoryKind {
        self.kind
    }
    /// Returns the exact content binding.
    pub const fn content(&self) -> &ContentBinding {
        &self.content
    }
    /// Returns ordered durable evidence.
    pub fn evidence(&self) -> &[DurableFactReference] {
        &self.evidence
    }
    /// Returns the revision lifecycle status.
    pub const fn status(&self) -> MemoryStatus {
        self.status
    }
    /// Returns the sensitivity class.
    pub const fn sensitivity(&self) -> MemorySensitivity {
        self.sensitivity
    }
    /// Returns provenance confidence metadata.
    pub const fn confidence_basis_points(&self) -> u16 {
        self.confidence_basis_points
    }
    /// Returns the durable visibility position.
    pub const fn valid_from_position(&self) -> u64 {
        self.valid_from_position
    }
    /// Returns the canonical expiry time, when any.
    pub fn expires_at_utc(&self) -> Option<&str> {
        self.expires_at_utc.as_deref()
    }
    /// Returns the exact prior revision named by this revision.
    pub fn supersedes_revision_id(&self) -> Option<&str> {
        self.supersedes_revision_id.as_deref()
    }
}

/// Exact old/new binding emitted with an accepted supersession.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySupersession {
    /// Stable logical record identity.
    pub record_id: String,
    /// Previously active immutable revision.
    pub old_revision_id: String,
    /// Newly active immutable revision.
    pub new_revision_id: String,
    /// Proposal authorizing the transition.
    pub proposal_id: String,
}

/// Successful pure write reduction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryWriteOutcome {
    /// Newly active record.
    pub record: MemoryRecord,
    /// Exact supersession binding, if an active revision was replaced.
    pub supersession: Option<MemorySupersession>,
}

/// Exact tombstone command target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryTombstone {
    /// Stable logical record identity.
    pub record_id: String,
    /// Exact active revision to tombstone.
    pub revision_id: String,
}

impl MemoryTombstone {
    /// Returns the stable logical record target.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }

    /// Returns the exact active revision target.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
}

/// Deterministic append-only record state used by Runtime projections.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MemoryState {
    revisions: Vec<MemoryRecord>,
}

impl MemoryState {
    /// Validates unique revision identities and at most one active revision per record.
    pub fn new(revisions: Vec<MemoryRecord>) -> Result<Self, MemoryError> {
        let mut identities = BTreeSet::new();
        let mut active = BTreeSet::new();
        for record in &revisions {
            if !identities.insert((record.record_id.clone(), record.revision_id.clone()))
                || (record.status == MemoryStatus::Active
                    && !active.insert(record.record_id.clone()))
            {
                return Err(MemoryError::new(MemoryErrorCode::CorruptMemoryState));
            }
        }
        Ok(Self { revisions })
    }

    /// Applies an authorized commit atomically or leaves state unchanged.
    pub fn commit(
        &mut self,
        proposal: &MemoryProposal,
        commit: &MemoryCommit,
    ) -> Result<MemoryWriteOutcome, MemoryError> {
        let active_index = self.revisions.iter().position(|record| {
            record.record_id == commit.record_id && record.status == MemoryStatus::Active
        });
        let active_revision = active_index.map(|index| self.revisions[index].revision_id.as_str());
        if active_revision != proposal.expected_active_revision_id()
            || active_revision != commit.supersedes_revision_id.as_deref()
            || self.revisions.iter().any(|record| {
                record.record_id == commit.record_id && record.revision_id == commit.revision_id
            })
        {
            return Err(MemoryError::new(MemoryErrorCode::RevisionConflict));
        }
        let record = MemoryRecord::new(
            &commit.record_id,
            &commit.revision_id,
            &proposal.namespace_id,
            proposal.scope.clone(),
            proposal.kind,
            proposal.content.clone(),
            proposal.evidence.clone(),
            MemoryStatus::Active,
            proposal.sensitivity,
            proposal.confidence_basis_points,
            commit.valid_from_position,
            commit.supersedes_revision_id.clone(),
            commit.expires_at_utc.clone(),
        )?;
        let supersession = active_index.map(|index| MemorySupersession {
            record_id: commit.record_id.clone(),
            old_revision_id: self.revisions[index].revision_id.clone(),
            new_revision_id: commit.revision_id.clone(),
            proposal_id: proposal.proposal_id.clone(),
        });
        if let Some(index) = active_index {
            self.revisions[index].status = MemoryStatus::Superseded;
        }
        self.revisions.push(record.clone());
        Ok(MemoryWriteOutcome {
            record,
            supersession,
        })
    }

    /// Tombstones only the exact active revision.
    pub fn tombstone(&mut self, target: &MemoryTombstone) -> Result<(), MemoryError> {
        let Some(record) = self.revisions.iter_mut().find(|record| {
            record.record_id == target.record_id && record.revision_id == target.revision_id
        }) else {
            return Err(MemoryError::new(MemoryErrorCode::RevisionConflict));
        };
        if record.status != MemoryStatus::Active {
            return Err(MemoryError::new(MemoryErrorCode::RevisionConflict));
        }
        record.status = MemoryStatus::Tombstoned;
        Ok(())
    }

    /// Returns all immutable revisions in projection order.
    pub fn revisions(&self) -> &[MemoryRecord] {
        &self.revisions
    }
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) fn canonical_utc(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|time| {
        time.with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::AutoSi, true)
            == value
    })
}
