use crate::{values::valid_digest, MemoryError, MemoryErrorCode};

/// Recall and evidence lifecycle independent of M0 revision status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HypothesisState {
    /// Awaiting real-world verification.
    Candidate,
    /// Eligible for ordinary bounded recall.
    Active,
    /// Searchable but down-ranked by maintenance policy.
    Cold,
    /// Available only to explicit detail queries.
    Archived,
    /// Published to Knowledge and excluded from ordinary recall.
    Promoted,
}

/// Exact portable evidence counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EvidenceTally {
    /// Accepted positive observations.
    pub verified: u64,
    /// Accepted in-scope negative observations.
    pub falsified: u64,
    /// Neutral or out-of-scope observations.
    pub neutral: u64,
}

/// One immutable lifecycle transition input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LifecycleEvent {
    /// Reality-backed positive observation.
    Verified {
        /// Durable observation position.
        position: u64,
    },
    /// Reality-backed negative observation with explicit attribution.
    Falsified {
        /// Durable observation position.
        position: u64,
        /// Whether the failed outcome falls within the hypothesis scope.
        in_scope: bool,
    },
    /// Inconclusive admitted observation.
    Neutral {
        /// Durable observation position.
        position: u64,
    },
    /// Explicit retention/use-policy down-rank.
    Cool {
        /// Durable maintenance position.
        position: u64,
    },
    /// Explicit maintenance archive decision.
    Archive {
        /// Durable maintenance position.
        position: u64,
    },
    /// Committed Knowledge publication receipt.
    Promote {
        /// Durable transition position.
        position: u64,
        /// Exact publication receipt digest.
        receipt_digest: Option<String>,
    },
}

impl LifecycleEvent {
    const fn position(&self) -> u64 {
        match self {
            Self::Verified { position }
            | Self::Falsified { position, .. }
            | Self::Neutral { position }
            | Self::Cool { position }
            | Self::Archive { position }
            | Self::Promote { position, .. } => *position,
        }
    }
}

/// Pure M1 hypothesis lifecycle projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryLifecycle {
    state: HypothesisState,
    tally: EvidenceTally,
    last_observed_position: u64,
    promoted_knowledge_receipt_digest: Option<String>,
}

impl MemoryLifecycle {
    /// Constructs a projection recovered from a verified durable prefix.
    pub fn new(
        state: HypothesisState,
        tally: EvidenceTally,
        last_observed_position: u64,
        promoted_knowledge_receipt_digest: Option<String>,
    ) -> Result<Self, MemoryError> {
        if last_observed_position == 0
            || (state == HypothesisState::Promoted) != promoted_knowledge_receipt_digest.is_some()
            || promoted_knowledge_receipt_digest
                .as_deref()
                .is_some_and(|value| !valid_digest(value))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
        }
        Ok(Self {
            state,
            tally,
            last_observed_position,
            promoted_knowledge_receipt_digest,
        })
    }

    /// Applies one strictly later transition without partial mutation.
    pub fn apply(&self, event: LifecycleEvent) -> Result<Self, MemoryError> {
        if event.position() <= self.last_observed_position {
            return Err(MemoryError::new(MemoryErrorCode::DuplicateObservation));
        }
        if self.state == HypothesisState::Promoted {
            return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
        }
        let mut next = self.clone();
        next.last_observed_position = event.position();
        match event {
            LifecycleEvent::Verified { .. } => {
                next.tally.verified = increment(next.tally.verified)?;
                next.state = match self.state {
                    HypothesisState::Candidate | HypothesisState::Cold => HypothesisState::Active,
                    HypothesisState::Active => HypothesisState::Active,
                    HypothesisState::Archived | HypothesisState::Promoted => {
                        return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
                    }
                };
            }
            LifecycleEvent::Falsified { in_scope, .. } => {
                if in_scope {
                    next.tally.falsified = increment(next.tally.falsified)?;
                } else {
                    next.tally.neutral = increment(next.tally.neutral)?;
                }
            }
            LifecycleEvent::Neutral { .. } => next.tally.neutral = increment(next.tally.neutral)?,
            LifecycleEvent::Cool { .. } if self.state == HypothesisState::Active => {
                next.state = HypothesisState::Cold;
            }
            LifecycleEvent::Archive { .. } if self.state == HypothesisState::Cold => {
                next.state = HypothesisState::Archived;
            }
            LifecycleEvent::Promote { receipt_digest, .. } => {
                let receipt = receipt_digest
                    .ok_or_else(|| MemoryError::new(MemoryErrorCode::PromotionReceiptRequired))?;
                if !valid_digest(&receipt) {
                    return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
                }
                next.state = HypothesisState::Promoted;
                next.promoted_knowledge_receipt_digest = Some(receipt);
            }
            LifecycleEvent::Cool { .. } | LifecycleEvent::Archive { .. } => {
                return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
            }
        }
        Ok(next)
    }

    /// Returns the current hypothesis state.
    pub const fn state(&self) -> HypothesisState {
        self.state
    }
    /// Returns exact evidence counts.
    pub const fn tally(&self) -> EvidenceTally {
        self.tally
    }
    /// Returns the last applied durable position.
    pub const fn last_observed_position(&self) -> u64 {
        self.last_observed_position
    }
    /// Returns the Knowledge receipt after promotion.
    pub fn promoted_knowledge_receipt_digest(&self) -> Option<&str> {
        self.promoted_knowledge_receipt_digest.as_deref()
    }
}

fn increment(value: u64) -> Result<u64, MemoryError> {
    value
        .checked_add(1)
        .ok_or_else(|| MemoryError::new(MemoryErrorCode::InvalidTransition))
}
