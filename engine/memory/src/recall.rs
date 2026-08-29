use std::cmp::Reverse;
use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::{
    values::{valid_digest, valid_id, valid_text, MAX_REFERENCE_BYTES},
    HypothesisState, MemoryAuthority, MemoryError, MemoryErrorCode, MemoryRole, MemoryType,
};

const MAX_MENU_LABEL_BYTES: usize = 256;
const MAX_RECALL_ITEMS: u32 = 256;
const MAX_RECALL_BYTES: u64 = 1_048_576;
const MAX_SCORE: u16 = 10_000;
const EXPLORATION_ALGORITHM: &str = "hash-explore-v1";
const EXPLORATION_DOMAIN: &str = "garive.memory-explore.v1";

/// Context product receiving a bounded memory selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecallProduct {
    /// Redacted descriptor menu; archived entries are forbidden.
    Menu,
    /// Explicit detail retrieval; archived entries may be requested.
    Detail,
}

/// Exact integer scoring inputs supplied by a frozen retriever revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecallScore {
    /// Query relevance in basis points.
    pub relevance: u16,
    /// Recency in basis points.
    pub recency: u16,
    /// Importance in basis points.
    pub importance: u16,
}

impl RecallScore {
    fn valid(self) -> bool {
        self.relevance <= MAX_SCORE && self.recency <= MAX_SCORE && self.importance <= MAX_SCORE
    }

    const fn total(self) -> u32 {
        self.relevance as u32 + self.recency as u32 + self.importance as u32
    }
}

/// Authorized, scored metadata eligible for menu or detail selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryRecallCandidate {
    record_id: String,
    revision_id: String,
    memory_type: MemoryType,
    role: MemoryRole,
    authority: MemoryAuthority,
    state: HypothesisState,
    safe_label: String,
    content_digest: String,
    content_bytes: u64,
    evidence_count: u32,
    score: RecallScore,
}

impl MemoryRecallCandidate {
    /// Validates one already-authorized candidate and its exact byte charge.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        memory_type: MemoryType,
        role: MemoryRole,
        authority: MemoryAuthority,
        state: HypothesisState,
        safe_label: impl Into<String>,
        content_digest: impl Into<String>,
        content_bytes: u64,
        evidence_count: u32,
        score: RecallScore,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            memory_type,
            role,
            authority,
            state,
            safe_label: safe_label.into(),
            content_digest: content_digest.into(),
            content_bytes,
            evidence_count,
            score,
        };
        if !valid_id(&value.record_id)
            || !valid_id(&value.revision_id)
            || !valid_text(&value.safe_label, MAX_MENU_LABEL_BYTES)
            || !valid_digest(&value.content_digest)
            || value.content_bytes == 0
            || value.evidence_count == 0
            || !value.score.valid()
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the stable record identity.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the cognitive type.
    pub const fn memory_type(&self) -> MemoryType {
        self.memory_type
    }
    /// Returns the content role.
    pub const fn role(&self) -> MemoryRole {
        self.role
    }
    /// Returns the provenance authority.
    pub const fn authority(&self) -> MemoryAuthority {
        self.authority
    }
    /// Returns the hypothesis state.
    pub const fn state(&self) -> HypothesisState {
        self.state
    }
    /// Returns the redacted bounded label.
    pub fn safe_label(&self) -> &str {
        &self.safe_label
    }
    /// Returns the exact content digest.
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }
    /// Returns the exact byte budget charge.
    pub const fn content_bytes(&self) -> u64 {
        self.content_bytes
    }
    /// Returns the durable evidence count.
    pub const fn evidence_count(&self) -> u32 {
        self.evidence_count
    }
}

/// Explicit deterministic exploration inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallExploration {
    algorithm_revision: String,
    seed: u64,
    slots: u32,
}

impl RecallExploration {
    /// Admits only the implemented hash exploration revision and non-zero slots.
    pub fn new(
        algorithm_revision: impl Into<String>,
        seed: u64,
        slots: u32,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            algorithm_revision: algorithm_revision.into(),
            seed,
            slots,
        };
        if value.algorithm_revision != EXPLORATION_ALGORITHM || slots == 0 {
            return Err(MemoryError::new(MemoryErrorCode::SelectionUnreplayable));
        }
        Ok(value)
    }
}

/// Frozen filters and budgets for one menu or detail selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallSelectionRequest {
    product: RecallProduct,
    allowed_types: Vec<MemoryType>,
    allowed_roles: Vec<MemoryRole>,
    allowed_states: Vec<HypothesisState>,
    selection_policy_revision: String,
    max_items: u32,
    max_total_bytes: u64,
    exploration: Option<RecallExploration>,
}

impl RecallSelectionRequest {
    /// Validates canonical filters, product rules, budgets and exploration shape.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        product: RecallProduct,
        allowed_types: Vec<MemoryType>,
        allowed_roles: Vec<MemoryRole>,
        allowed_states: Vec<HypothesisState>,
        selection_policy_revision: impl Into<String>,
        max_items: u32,
        max_total_bytes: u64,
        exploration: Option<RecallExploration>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            product,
            allowed_types,
            allowed_roles,
            allowed_states,
            selection_policy_revision: selection_policy_revision.into(),
            max_items,
            max_total_bytes,
            exploration,
        };
        if value.allowed_types.is_empty()
            || !ordered_unique(&value.allowed_types)
            || value.allowed_roles.is_empty()
            || !ordered_unique(&value.allowed_roles)
            || value.allowed_states.is_empty()
            || !ordered_unique(&value.allowed_states)
            || !valid_text(&value.selection_policy_revision, MAX_REFERENCE_BYTES)
            || !(1..=MAX_RECALL_ITEMS).contains(&value.max_items)
            || !(1..=MAX_RECALL_BYTES).contains(&value.max_total_bytes)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        if value.allowed_states.contains(&HypothesisState::Promoted)
            || value.product == RecallProduct::Menu
                && value.allowed_states.contains(&HypothesisState::Archived)
            || value.allowed_states.contains(&HypothesisState::Candidate)
                && value.exploration.is_none()
            || value
                .exploration
                .as_ref()
                .is_some_and(|item| item.slots > value.max_items)
        {
            return Err(MemoryError::new(MemoryErrorCode::SelectionUnreplayable));
        }
        Ok(value)
    }
}

/// Why an item entered one exact result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecallSelectionKind {
    /// Selected by deterministic score ordering.
    Ranked,
    /// Selected into an explicitly seeded exploration slot.
    Explored,
}

/// One selected item and optional committed exploration draw.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallSelectionItem {
    candidate: MemoryRecallCandidate,
    kind: RecallSelectionKind,
    draw_hex: Option<String>,
}

impl RecallSelectionItem {
    /// Returns selected metadata.
    pub const fn candidate(&self) -> &MemoryRecallCandidate {
        &self.candidate
    }
    /// Returns the selection path.
    pub const fn kind(&self) -> RecallSelectionKind {
        self.kind
    }
    /// Returns the exact seeded draw prefix for replay evidence.
    pub fn draw_hex(&self) -> Option<&str> {
        self.draw_hex.as_deref()
    }
}

/// Ordered bounded selection suitable for commit-before-context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecallSelection {
    /// Ranked items followed by explicitly explored items.
    pub items: Vec<RecallSelectionItem>,
    /// Whether an eligible candidate was omitted by count or byte bounds.
    pub truncated: bool,
}

/// Selects authorized candidates under exact deterministic and exploration rules.
pub fn select_recall(
    candidates: &[MemoryRecallCandidate],
    request: &RecallSelectionRequest,
) -> Result<RecallSelection, MemoryError> {
    let mut identities = BTreeSet::new();
    if candidates
        .iter()
        .any(|value| !identities.insert((value.record_id(), value.revision_id())))
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    let eligible: Vec<_> = candidates
        .iter()
        .filter(|value| {
            request.allowed_types.contains(&value.memory_type)
                && request.allowed_roles.contains(&value.role)
                && request.allowed_states.contains(&value.state)
        })
        .collect();

    let mut explored = Vec::new();
    let mut explored_bytes = 0_u64;
    if let Some(config) = &request.exploration {
        let mut draws: Vec<_> = eligible
            .iter()
            .filter(|value| value.state == HypothesisState::Candidate)
            .map(|value| (*value, exploration_draw(config, value)))
            .collect();
        draws.sort_by(|(left, left_draw), (right, right_draw)| {
            left_draw
                .cmp(right_draw)
                .then_with(|| left.record_id.cmp(&right.record_id))
                .then_with(|| left.revision_id.cmp(&right.revision_id))
        });
        for (candidate, draw) in draws.into_iter().take(config.slots as usize) {
            let next = explored_bytes.saturating_add(candidate.content_bytes);
            if next > request.max_total_bytes {
                break;
            }
            explored_bytes = next;
            explored.push(RecallSelectionItem {
                candidate: candidate.clone(),
                kind: RecallSelectionKind::Explored,
                draw_hex: Some(draw),
            });
        }
    }

    let explored_ids: BTreeSet<_> = explored
        .iter()
        .map(|item| (item.candidate.record_id(), item.candidate.revision_id()))
        .collect();
    let mut ranked: Vec<_> = eligible
        .iter()
        .filter(|value| {
            value.state != HypothesisState::Candidate
                && !explored_ids.contains(&(value.record_id(), value.revision_id()))
        })
        .copied()
        .collect();
    ranked.sort_by_key(|value| {
        (
            Reverse(value.score.total()),
            Reverse(value.score.relevance),
            Reverse(value.score.recency),
            Reverse(value.score.importance),
            value.record_id(),
            value.revision_id(),
        )
    });

    let ranked_capacity = request.max_items as usize - explored.len();
    let mut ranked_items = Vec::new();
    let mut bytes = explored_bytes;
    for candidate in ranked.into_iter().take(ranked_capacity) {
        let next = bytes.saturating_add(candidate.content_bytes);
        if next > request.max_total_bytes {
            break;
        }
        bytes = next;
        ranked_items.push(RecallSelectionItem {
            candidate: candidate.clone(),
            kind: RecallSelectionKind::Ranked,
            draw_hex: None,
        });
    }
    ranked_items.extend(explored);
    let truncated = ranked_items.len() < eligible.len();
    Ok(RecallSelection {
        items: ranked_items,
        truncated,
    })
}

fn exploration_draw(config: &RecallExploration, candidate: &MemoryRecallCandidate) -> String {
    let mut digest = Sha256::new();
    let seed = config.seed.to_string();
    let values = [
        EXPLORATION_DOMAIN,
        &config.algorithm_revision,
        &seed,
        candidate.record_id(),
        candidate.revision_id(),
    ];
    for (index, value) in values.into_iter().enumerate() {
        digest.update(value.as_bytes());
        if index + 1 != values.len() {
            digest.update([0]);
        }
    }
    format!("{:x}", digest.finalize())[..16].to_owned()
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
