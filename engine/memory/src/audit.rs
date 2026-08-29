use std::collections::{BTreeMap, BTreeSet};

use crate::{
    values::{valid_digest, valid_id},
    HypothesisState, MemoryError, MemoryErrorCode, MemoryType,
};

const MAX_AUDIT_ENTRIES: usize = 4_096;
const MAX_AUDIT_CONTRADICTIONS: usize = 4_096;
const MAX_RETENTION_SCORE: u16 = 10_000;

/// Frozen policy controlling one read-only health audit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryAuditPolicy {
    /// Maximum Active entries.
    pub max_active_records: u32,
    /// Maximum bytes charged by Active entries.
    pub max_active_bytes: u64,
    /// Positions since verification before stale reporting.
    pub stale_after_positions: u64,
    /// Uses below this value are reported low-use.
    pub low_use_threshold: u64,
    /// Maximum report rows, excluding required quota actions.
    pub max_report_items: u32,
}

impl MemoryAuditPolicy {
    /// Rejects zero safety and report bounds.
    pub fn validate(self) -> Result<Self, MemoryError> {
        if self.max_active_records == 0
            || self.max_active_bytes == 0
            || self.stale_after_positions == 0
            || self.max_report_items == 0
        {
            Err(MemoryError::new(MemoryErrorCode::InvalidMemory))
        } else {
            Ok(self)
        }
    }
}

/// One immutable inventory row supplied to audit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryAuditEntry {
    /// Record identity.
    pub record_id: String,
    /// Revision identity.
    pub revision_id: String,
    /// Cognitive type.
    pub memory_type: MemoryType,
    /// Current hypothesis state.
    pub state: HypothesisState,
    /// Exact content digest.
    pub content_digest: String,
    /// Exact byte charge.
    pub content_bytes: u64,
    /// Committed helpful-use count.
    pub use_count: u64,
    /// Last verified durable position.
    pub last_verified_position: u64,
    /// Versioned retention score in basis points.
    pub retention_score_basis_points: u16,
}

impl MemoryAuditEntry {
    /// Validates identity, digest, sizes and score.
    pub fn validate(self) -> Result<Self, MemoryError> {
        if !valid_id(&self.record_id)
            || !valid_id(&self.revision_id)
            || !valid_digest(&self.content_digest)
            || self.content_bytes == 0
            || self.last_verified_position == 0
            || self.retention_score_basis_points > MAX_RETENTION_SCORE
        {
            Err(MemoryError::new(MemoryErrorCode::InvalidMemory))
        } else {
            Ok(self)
        }
    }
    fn identity(&self) -> (String, String) {
        (self.record_id.clone(), self.revision_id.clone())
    }
}

/// Explicit contradiction candidate from a versioned detector.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryContradiction {
    /// Canonically lower identity.
    pub left: (String, String),
    /// Canonically higher identity.
    pub right: (String, String),
}

/// Read-only maintenance proposal; applying it requires a later durable transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryAuditAction {
    /// Active entry should leave the hot set.
    Cool {
        /// Target identity.
        identity: (String, String),
    },
    /// Stale Cold entry should become explicit-query only.
    Archive {
        /// Target identity.
        identity: (String, String),
    },
}

/// Bounded deterministic audit report with no mutation authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryAuditReport {
    /// Duplicate content identity groups.
    pub duplicate_groups: Vec<Vec<(String, String)>>,
    /// Validated explicit contradiction pairs.
    pub contradictions: Vec<MemoryContradiction>,
    /// Stale identities.
    pub stale: Vec<(String, String)>,
    /// Low-use identities.
    pub low_use: Vec<(String, String)>,
    /// Required hot-quota and stale-state proposals.
    pub actions: Vec<MemoryAuditAction>,
    /// Diagnostic rows were omitted by the report bound.
    pub truncated: bool,
}

/// Audits canonical inventory under frozen position and policy inputs.
pub fn audit_memory(
    entries: &[MemoryAuditEntry],
    contradictions: &[MemoryContradiction],
    current_position: u64,
    policy: MemoryAuditPolicy,
) -> Result<MemoryAuditReport, MemoryError> {
    let policy = policy.validate()?;
    if current_position == 0
        || entries.len() > MAX_AUDIT_ENTRIES
        || contradictions.len() > MAX_AUDIT_CONTRADICTIONS
        || !entries
            .windows(2)
            .all(|pair| pair[0].identity() < pair[1].identity())
        || entries.iter().any(|entry| {
            entry.clone().validate().is_err() || entry.last_verified_position > current_position
        })
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    let identities: BTreeSet<_> = entries.iter().map(MemoryAuditEntry::identity).collect();
    if !contradictions.windows(2).all(|pair| {
        pair[0].left < pair[1].left || pair[0].left == pair[1].left && pair[0].right < pair[1].right
    }) || contradictions.iter().any(|pair| {
        pair.left >= pair.right
            || !identities.contains(&pair.left)
            || !identities.contains(&pair.right)
    }) {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    let mut by_digest: BTreeMap<&str, Vec<(String, String)>> = BTreeMap::new();
    for entry in entries {
        by_digest
            .entry(&entry.content_digest)
            .or_default()
            .push(entry.identity());
    }
    let duplicate_groups = by_digest
        .into_values()
        .filter(|group| group.len() > 1)
        .collect();
    let stale = entries
        .iter()
        .filter(|entry| {
            entry.state != HypothesisState::Promoted
                && current_position - entry.last_verified_position >= policy.stale_after_positions
        })
        .map(MemoryAuditEntry::identity)
        .collect();
    let low_use = entries
        .iter()
        .filter(|entry| {
            entry.state != HypothesisState::Promoted && entry.use_count < policy.low_use_threshold
        })
        .map(MemoryAuditEntry::identity)
        .collect();
    let mut active = entries
        .iter()
        .filter(|entry| entry.state == HypothesisState::Active)
        .collect::<Vec<_>>();
    let mut active_count = active.len() as u32;
    let mut active_bytes = active
        .iter()
        .try_fold(0_u64, |sum, entry| sum.checked_add(entry.content_bytes))
        .ok_or_else(|| MemoryError::new(MemoryErrorCode::InvalidMemory))?;
    active.sort_by_key(|entry| {
        (
            entry.retention_score_basis_points,
            entry.use_count,
            entry.last_verified_position,
            entry.record_id.as_str(),
            entry.revision_id.as_str(),
        )
    });
    let mut actions = Vec::new();
    for entry in active {
        if active_count <= policy.max_active_records && active_bytes <= policy.max_active_bytes {
            break;
        }
        active_count -= 1;
        active_bytes -= entry.content_bytes;
        actions.push(MemoryAuditAction::Cool {
            identity: entry.identity(),
        });
    }
    for entry in entries.iter().filter(|entry| {
        entry.state == HypothesisState::Cold
            && current_position - entry.last_verified_position >= policy.stale_after_positions
    }) {
        actions.push(MemoryAuditAction::Archive {
            identity: entry.identity(),
        });
    }
    if actions.len() > policy.max_report_items as usize {
        return Err(MemoryError::new(MemoryErrorCode::LimitExceeded));
    }
    let mut remaining = policy.max_report_items as usize - actions.len();
    let mut truncated = false;
    let duplicate_groups = take_bounded(duplicate_groups, &mut remaining, &mut truncated);
    let contradictions = take_bounded(contradictions.to_vec(), &mut remaining, &mut truncated);
    let stale = take_bounded(stale, &mut remaining, &mut truncated);
    let low_use = take_bounded(low_use, &mut remaining, &mut truncated);
    Ok(MemoryAuditReport {
        duplicate_groups,
        contradictions,
        stale,
        low_use,
        actions,
        truncated,
    })
}

fn take_bounded<T>(mut values: Vec<T>, remaining: &mut usize, truncated: &mut bool) -> Vec<T> {
    if values.len() > *remaining {
        values.truncate(*remaining);
        *truncated = true;
    }
    *remaining -= values.len();
    values
}
