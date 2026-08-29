use crate::{
    values::{valid_digest, valid_id, valid_text, MAX_REFERENCE_BYTES},
    ContentBinding, DurableFactReference, MemoryAuthority, MemoryAuthorityBinding, MemoryError,
    MemoryErrorCode, MemoryRole, MemoryScopeBinding, MemoryType,
};

const MAX_CANDIDATE_EVIDENCE: usize = 64;

/// Explicit origin of one untrusted candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryCandidateSource {
    /// Direct authenticated user command.
    ExplicitUserCommand,
    /// Automatic Session-end episode extraction.
    SessionEnd,
    /// Hot capture from an exit summary.
    ExitSummary,
    /// Scheduled distillation over a fixed prefix.
    ScheduledDistillation,
}

/// Candidate operation before authority and durable M0 mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryCandidateIntent {
    /// Propose learned content with explicit classification and evidence.
    Learn {
        /// Cognitive lifecycle type.
        memory_type: MemoryType,
        /// Preserved content role.
        role: MemoryRole,
        /// Receipt-shaped provenance authority.
        authority: MemoryAuthorityBinding,
        /// Scope class and optional aggregation binding.
        scope: MemoryScopeBinding,
        /// Exact content binding.
        content: ContentBinding,
        /// Exact UTF-8 or externally verified byte charge.
        content_bytes: u64,
        /// Ordered durable source evidence.
        evidence: Vec<DurableFactReference>,
    },
    /// Explicit user request to forget one exact active revision.
    Forget {
        /// Logical record identity.
        record_id: String,
        /// Exact revision identity.
        revision_id: String,
        /// User-declared authority receipt.
        authority: MemoryAuthorityBinding,
    },
}

/// Bounded untrusted input to the four-decision reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryCandidate {
    candidate_id: String,
    namespace_id: String,
    extractor_revision: String,
    source: MemoryCandidateSource,
    intent: MemoryCandidateIntent,
}

impl MemoryCandidate {
    /// Validates candidate source authority, content bytes and evidence.
    pub fn new(
        candidate_id: impl Into<String>,
        namespace_id: impl Into<String>,
        extractor_revision: impl Into<String>,
        source: MemoryCandidateSource,
        intent: MemoryCandidateIntent,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            candidate_id: candidate_id.into(),
            namespace_id: namespace_id.into(),
            extractor_revision: extractor_revision.into(),
            source,
            intent,
        };
        if !valid_id(&value.candidate_id)
            || !valid_id(&value.namespace_id)
            || !valid_text(&value.extractor_revision, MAX_REFERENCE_BYTES)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        match &value.intent {
            MemoryCandidateIntent::Learn {
                authority,
                content,
                content_bytes,
                evidence,
                ..
            } => {
                let expected = if source == MemoryCandidateSource::ExplicitUserCommand {
                    MemoryAuthority::UserDeclared
                } else {
                    MemoryAuthority::AgentLearned
                };
                if authority.authority() != expected
                    || *content_bytes == 0
                    || evidence.is_empty()
                    || evidence.len() > MAX_CANDIDATE_EVIDENCE
                    || !ordered_unique(evidence)
                    || content
                        .inline_utf8()
                        .is_some_and(|text| text.len() as u64 != *content_bytes)
                {
                    return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
                }
            }
            MemoryCandidateIntent::Forget {
                record_id,
                revision_id,
                authority,
            } => {
                if source != MemoryCandidateSource::ExplicitUserCommand
                    || authority.authority() != MemoryAuthority::UserDeclared
                    || !valid_id(record_id)
                    || !valid_id(revision_id)
                {
                    return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
                }
            }
        }
        Ok(value)
    }
    /// Returns the candidate identity.
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }
    /// Returns the opaque namespace.
    pub fn namespace_id(&self) -> &str {
        &self.namespace_id
    }
    /// Returns the extraction policy revision.
    pub fn extractor_revision(&self) -> &str {
        &self.extractor_revision
    }
    /// Returns the explicit source.
    pub const fn source(&self) -> MemoryCandidateSource {
        self.source
    }
    /// Returns the proposed operation.
    pub const fn intent(&self) -> &MemoryCandidateIntent {
        &self.intent
    }
}

/// Stability conclusion supplied by a versioned admission policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateStability {
    /// Evidence is stable enough for an M0 proposal.
    Confirmed,
    /// More observation is required; persist only a safe Noop decision.
    Uncertain,
}

/// Exact inputs to deterministic candidate admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmissionAssessment {
    generalizable: bool,
    stability: CandidateStability,
    exact_duplicate_revision_id: Option<String>,
    conflicting_active_revision_id: Option<String>,
}

impl AdmissionAssessment {
    /// Rejects ambiguous duplicate/conflict or malformed revision bindings.
    pub fn new(
        generalizable: bool,
        stability: CandidateStability,
        exact_duplicate_revision_id: Option<String>,
        conflicting_active_revision_id: Option<String>,
    ) -> Result<Self, MemoryError> {
        if exact_duplicate_revision_id.is_some() && conflicting_active_revision_id.is_some()
            || exact_duplicate_revision_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
            || conflicting_active_revision_id
                .as_deref()
                .is_some_and(|value| !valid_id(value))
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(Self {
            generalizable,
            stability,
            exact_duplicate_revision_id,
            conflicting_active_revision_id,
        })
    }
}

/// Safe reason why a candidate produced no write proposal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceNoopCode {
    /// One-off detail failed admission.
    NotGeneralizable,
    /// Stability policy deferred the candidate.
    UnstableDeferred,
    /// Exact content already exists.
    Duplicate,
}

/// Explicit ADD/UPDATE/DELETE/NOOP output with no write authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryMaintenanceDecision {
    /// New M0 proposal.
    Add {
        /// Stable proposal identity.
        proposal_id: String,
    },
    /// Optimistic update of an exact active revision.
    Update {
        /// Stable proposal identity.
        proposal_id: String,
        /// Expected prior revision.
        expected_active_revision_id: String,
    },
    /// Authorized forget must next become an M0 tombstone request.
    Delete {
        /// Stable command identity.
        command_id: String,
        /// Target logical record.
        record_id: String,
        /// Target immutable revision.
        revision_id: String,
    },
    /// No write is proposed.
    Noop {
        /// Stable safe classification.
        code: MaintenanceNoopCode,
    },
}

/// Reduces one candidate under the normative admission order.
pub fn decide_candidate(
    candidate: &MemoryCandidate,
    assessment: Option<&AdmissionAssessment>,
    decision_id: impl Into<String>,
) -> Result<MemoryMaintenanceDecision, MemoryError> {
    let decision_id = decision_id.into();
    if !valid_id(&decision_id) {
        return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
    }
    match candidate.intent() {
        MemoryCandidateIntent::Forget {
            record_id,
            revision_id,
            ..
        } => {
            if assessment.is_some() {
                return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
            }
            Ok(MemoryMaintenanceDecision::Delete {
                command_id: decision_id,
                record_id: record_id.clone(),
                revision_id: revision_id.clone(),
            })
        }
        MemoryCandidateIntent::Learn { .. } => {
            let assessment =
                assessment.ok_or_else(|| MemoryError::new(MemoryErrorCode::InvalidMemory))?;
            if !assessment.generalizable {
                return Ok(MemoryMaintenanceDecision::Noop {
                    code: MaintenanceNoopCode::NotGeneralizable,
                });
            }
            if assessment.stability == CandidateStability::Uncertain {
                return Ok(MemoryMaintenanceDecision::Noop {
                    code: MaintenanceNoopCode::UnstableDeferred,
                });
            }
            if assessment.exact_duplicate_revision_id.is_some() {
                return Ok(MemoryMaintenanceDecision::Noop {
                    code: MaintenanceNoopCode::Duplicate,
                });
            }
            if let Some(revision) = &assessment.conflicting_active_revision_id {
                return Ok(MemoryMaintenanceDecision::Update {
                    proposal_id: decision_id,
                    expected_active_revision_id: revision.clone(),
                });
            }
            Ok(MemoryMaintenanceDecision::Add {
                proposal_id: decision_id,
            })
        }
    }
}

/// Exact scheduled distillation progress for one extractor and Session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistillationWatermark {
    /// Frozen extractor revision.
    pub extractor_revision: String,
    /// Source Session identity.
    pub session_id: String,
    /// Inclusive consumed prefix.
    pub through_position: u64,
    /// Digest over the exact candidate batch.
    pub batch_digest: String,
}

impl DistillationWatermark {
    /// Validates one non-zero checkpoint.
    pub fn new(
        extractor_revision: impl Into<String>,
        session_id: impl Into<String>,
        through_position: u64,
        batch_digest: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            extractor_revision: extractor_revision.into(),
            session_id: session_id.into(),
            through_position,
            batch_digest: batch_digest.into(),
        };
        if !valid_text(&value.extractor_revision, MAX_REFERENCE_BYTES)
            || !valid_id(&value.session_id)
            || value.through_position == 0
            || !valid_digest(&value.batch_digest)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
}

/// Idempotent watermark reduction disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatermarkDisposition {
    /// New prefix accepted.
    Advanced,
    /// Exact prior checkpoint replayed.
    Replayed,
}

/// Validates monotonic progress under one frozen extractor/Session binding.
pub fn advance_distillation(
    prior: Option<&DistillationWatermark>,
    next: &DistillationWatermark,
) -> Result<WatermarkDisposition, MemoryError> {
    let Some(prior) = prior else {
        return Ok(WatermarkDisposition::Advanced);
    };
    if prior.extractor_revision != next.extractor_revision
        || prior.session_id != next.session_id
        || next.through_position < prior.through_position
        || next.through_position == prior.through_position
            && next.batch_digest != prior.batch_digest
    {
        return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
    }
    Ok(if next.through_position == prior.through_position {
        WatermarkDisposition::Replayed
    } else {
        WatermarkDisposition::Advanced
    })
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
