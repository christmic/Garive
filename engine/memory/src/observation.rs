use crate::{
    values::{valid_digest, valid_id, valid_text},
    DurableFactReference, LifecycleEvent, MemoryError, MemoryErrorCode, MemoryLifecycle,
};

const MAX_OBSERVATION_EVIDENCE: usize = 64;
const MAX_REASON_BYTES: usize = 256;

/// Admitted class of committed reality evidence.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ObservationEvidenceKind {
    /// Committed tool result.
    ToolResult,
    /// Committed test result.
    TestResult,
    /// Governed external-effect receipt.
    EffectReceipt,
    /// Explicit user correction.
    UserCorrection,
    /// Output of an admitted deterministic verifier.
    DeterministicVerifier,
}

/// Typed durable evidence used by one observation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ObservationEvidence {
    kind: ObservationEvidenceKind,
    fact: DurableFactReference,
}

impl ObservationEvidence {
    /// Binds an admitted evidence class to one exact durable fact.
    pub const fn new(kind: ObservationEvidenceKind, fact: DurableFactReference) -> Self {
        Self { kind, fact }
    }
    /// Returns the evidence class Runtime must verify against the fact kind.
    pub const fn kind(&self) -> ObservationEvidenceKind {
        self.kind
    }
    /// Returns the exact durable fact binding.
    pub const fn fact(&self) -> &DurableFactReference {
        &self.fact
    }
}

/// Bounded application claim awaiting real-world reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryObligation {
    obligation_id: String,
    record_id: String,
    revision_id: String,
    application_fact: DurableFactReference,
    expected_outcome_digest: String,
    application_scope_digest: String,
    attribution_policy_revision: String,
    expires_at_position: u64,
}

impl MemoryObligation {
    /// Constructs an obligation from an application fact, never from citation text alone.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        obligation_id: impl Into<String>,
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        application_fact: DurableFactReference,
        expected_outcome_digest: impl Into<String>,
        application_scope_digest: impl Into<String>,
        attribution_policy_revision: impl Into<String>,
        expires_at_position: u64,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            obligation_id: obligation_id.into(),
            record_id: record_id.into(),
            revision_id: revision_id.into(),
            application_fact,
            expected_outcome_digest: expected_outcome_digest.into(),
            application_scope_digest: application_scope_digest.into(),
            attribution_policy_revision: attribution_policy_revision.into(),
            expires_at_position,
        };
        if !valid_id(&value.obligation_id)
            || !valid_id(&value.record_id)
            || !valid_id(&value.revision_id)
            || !valid_digest(&value.expected_outcome_digest)
            || !valid_digest(&value.application_scope_digest)
            || !valid_text(&value.attribution_policy_revision, 512)
            || value.expires_at_position <= value.application_fact.position()
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        Ok(value)
    }
    /// Returns the obligation identity.
    pub fn obligation_id(&self) -> &str {
        &self.obligation_id
    }
    /// Returns the applied memory record.
    pub fn record_id(&self) -> &str {
        &self.record_id
    }
    /// Returns the applied immutable revision.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the application fact.
    pub const fn application_fact(&self) -> &DurableFactReference {
        &self.application_fact
    }
    /// Returns the expected outcome descriptor digest.
    pub fn expected_outcome_digest(&self) -> &str {
        &self.expected_outcome_digest
    }
    /// Returns the application scope digest.
    pub fn application_scope_digest(&self) -> &str {
        &self.application_scope_digest
    }
    /// Returns the frozen attribution policy revision.
    pub fn attribution_policy_revision(&self) -> &str {
        &self.attribution_policy_revision
    }
    /// Returns the inclusive observation expiry position.
    pub const fn expires_at_position(&self) -> u64 {
        self.expires_at_position
    }
}

/// Reality verdict bound to an observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ObservationVerdict {
    /// Outcome verified the applied hypothesis.
    Verified,
    /// Outcome falsified it, with explicit scope attribution.
    Falsified {
        /// Whether failure occurred within the declared scope.
        in_scope: bool,
        /// Required only for an out-of-scope narrowing candidate.
        observed_scope_digest: Option<String>,
    },
    /// Evidence was inconclusive.
    Neutral {
        /// Bounded safe reason, not raw evidence content.
        safe_reason: String,
    },
}

/// One typed observation of an open obligation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryObservation {
    observation_id: String,
    obligation_id: String,
    position: u64,
    verifier_revision: String,
    evidence: Vec<ObservationEvidence>,
    verdict: ObservationVerdict,
}

impl MemoryObservation {
    /// Validates identity, ordering, evidence bounds, attribution and safe reason.
    pub fn new(
        observation_id: impl Into<String>,
        obligation_id: impl Into<String>,
        position: u64,
        verifier_revision: impl Into<String>,
        evidence: Vec<ObservationEvidence>,
        verdict: ObservationVerdict,
    ) -> Result<Self, MemoryError> {
        let value = Self {
            observation_id: observation_id.into(),
            obligation_id: obligation_id.into(),
            position,
            verifier_revision: verifier_revision.into(),
            evidence,
            verdict,
        };
        if !valid_id(&value.observation_id)
            || !valid_id(&value.obligation_id)
            || value.position == 0
            || !valid_text(&value.verifier_revision, 512)
            || value.evidence.is_empty()
            || value.evidence.len() > MAX_OBSERVATION_EVIDENCE
            || !ordered_unique(&value.evidence)
        {
            return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
        }
        match &value.verdict {
            ObservationVerdict::Falsified {
                in_scope,
                observed_scope_digest,
            } => {
                if *in_scope == observed_scope_digest.is_some()
                    || observed_scope_digest
                        .as_deref()
                        .is_some_and(|digest| !valid_digest(digest))
                {
                    return Err(MemoryError::new(MemoryErrorCode::AttributionUnsupported));
                }
            }
            ObservationVerdict::Neutral { safe_reason }
                if !valid_text(safe_reason, MAX_REASON_BYTES) =>
            {
                return Err(MemoryError::new(MemoryErrorCode::InvalidMemory));
            }
            ObservationVerdict::Verified | ObservationVerdict::Neutral { .. } => {}
        }
        Ok(value)
    }
    /// Returns the observation identity.
    pub fn observation_id(&self) -> &str {
        &self.observation_id
    }
    /// Returns the obligation identity.
    pub fn obligation_id(&self) -> &str {
        &self.obligation_id
    }
    /// Returns the durable observation position.
    pub const fn position(&self) -> u64 {
        self.position
    }
    /// Returns the verifier revision.
    pub fn verifier_revision(&self) -> &str {
        &self.verifier_revision
    }
    /// Returns ordered typed reality evidence.
    pub fn evidence(&self) -> &[ObservationEvidence] {
        &self.evidence
    }
    /// Returns the reconciled verdict.
    pub const fn verdict(&self) -> &ObservationVerdict {
        &self.verdict
    }
}

/// Candidate for explicit supersession with a narrower observed scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScopeNarrowingCandidate {
    /// Source logical record.
    pub record_id: String,
    /// Source immutable revision.
    pub revision_id: String,
    /// Original application scope binding.
    pub application_scope_digest: String,
    /// Observed out-of-scope binding to exclude or specialize.
    pub observed_scope_digest: String,
    /// Reality evidence supporting the narrowing proposal.
    pub evidence: Vec<ObservationEvidence>,
}

/// Pure observation reduction output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationReduction {
    /// Updated exact evidence lifecycle.
    pub lifecycle: MemoryLifecycle,
    /// Out-of-scope narrowing proposal, never an implicit update.
    pub narrowing: Option<ScopeNarrowingCandidate>,
}

/// Reconciles one observation without treating an application citation as proof.
pub fn reduce_observation(
    obligation: &MemoryObligation,
    observation: &MemoryObservation,
    lifecycle: &MemoryLifecycle,
) -> Result<ObservationReduction, MemoryError> {
    if observation.obligation_id != obligation.obligation_id {
        return Err(MemoryError::new(MemoryErrorCode::AttributionUnsupported));
    }
    if observation.position > obligation.expires_at_position {
        return Err(MemoryError::new(MemoryErrorCode::InvalidTransition));
    }
    let (event, narrowing) = match &observation.verdict {
        ObservationVerdict::Verified => (
            LifecycleEvent::Verified {
                position: observation.position,
            },
            None,
        ),
        ObservationVerdict::Falsified { in_scope: true, .. } => (
            LifecycleEvent::Falsified {
                position: observation.position,
                in_scope: true,
            },
            None,
        ),
        ObservationVerdict::Falsified {
            in_scope: false,
            observed_scope_digest: Some(observed),
        } => (
            LifecycleEvent::Falsified {
                position: observation.position,
                in_scope: false,
            },
            Some(ScopeNarrowingCandidate {
                record_id: obligation.record_id.clone(),
                revision_id: obligation.revision_id.clone(),
                application_scope_digest: obligation.application_scope_digest.clone(),
                observed_scope_digest: observed.clone(),
                evidence: observation.evidence.clone(),
            }),
        ),
        ObservationVerdict::Neutral { .. } => (
            LifecycleEvent::Neutral {
                position: observation.position,
            },
            None,
        ),
        ObservationVerdict::Falsified {
            in_scope: false,
            observed_scope_digest: None,
        } => {
            return Err(MemoryError::new(MemoryErrorCode::AttributionUnsupported));
        }
    };
    Ok(ObservationReduction {
        lifecycle: lifecycle.apply(event)?,
        narrowing,
    })
}

fn ordered_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}
