use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

const INTENT_CONTRACT: &str = "garive.schedule-intent";
const CONTRACT_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 128;

/// Stable Q0 validation, recurrence, authority, lease, or durability failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleErrorCode {
    /// Schedule values or relations are invalid.
    InvalidSchedule,
    /// No schedule exists for the supplied identity.
    ScheduleNotFound,
    /// The expected and active revisions differ.
    RevisionConflict,
    /// A continuation subject no longer names a resumable Turn.
    SubjectNotResumable,
    /// Runtime authority denied the operation.
    AuthorityDenied,
    /// A supplied clock value is invalid.
    ClockInvalid,
    /// Ordinal, delay, or timestamp arithmetic overflowed.
    OccurrenceOverflow,
    /// Misfire policy forbids the overdue occurrence.
    MisfireLimitExceeded,
    /// The worker no longer owns its operational lease.
    LeaseLost,
    /// The deterministic Runtime command conflicts.
    DispatchConflict,
    /// A required durable operation failed.
    DurabilityFailure,
    /// Persisted schedule facts form an impossible state.
    CorruptScheduleState,
}
impl ScheduleErrorCode {
    /// Returns the exact portable wire name.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidSchedule => "invalid_schedule",
            Self::ScheduleNotFound => "schedule_not_found",
            Self::RevisionConflict => "revision_conflict",
            Self::SubjectNotResumable => "subject_not_resumable",
            Self::AuthorityDenied => "authority_denied",
            Self::ClockInvalid => "clock_invalid",
            Self::OccurrenceOverflow => "occurrence_overflow",
            Self::MisfireLimitExceeded => "misfire_limit_exceeded",
            Self::LeaseLost => "lease_lost",
            Self::DispatchConflict => "dispatch_conflict",
            Self::DurabilityFailure => "durability_failure",
            Self::CorruptScheduleState => "corrupt_schedule_state",
        }
    }
}

/// Typed Q0 construction or reduction failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleError {
    code: ScheduleErrorCode,
}
impl ScheduleError {
    pub(crate) const fn new(code: ScheduleErrorCode) -> Self {
        Self { code }
    }
    /// Returns the stable failure classification.
    pub const fn code(&self) -> ScheduleErrorCode {
        self.code
    }
}

/// Portable command subject scheduled by Q0.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleSubject {
    /// Start a new Turn from an exact installed definition/input binding.
    StartTurn,
    /// Continue one suspended Turn with `ResourceReady`.
    ContinueTurnResourceReady,
}

/// Portable schedule timing without timezone or calendar semantics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScheduleTiming {
    /// Fire once at one exact UTC instant.
    At {
        /// Declared due instant.
        due_at_utc: String,
    },
    /// Repeat from one anchored instant by an exact millisecond delay.
    FixedDelay {
        /// Declared first due instant.
        first_due_at_utc: String,
        /// Non-zero delay in milliseconds.
        delay_ms: u64,
        /// Optional non-zero occurrence bound.
        #[serde(skip_serializing_if = "Option::is_none")]
        max_occurrences: Option<u64>,
    },
}

/// Portable overdue-occurrence policy.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    /// Fire only the earliest unhandled overdue occurrence.
    FireOnce,
    /// Persist one contiguous skipped range and advance.
    Skip,
    /// Disable the revision with an explicit failure.
    Fail,
}

/// Canonical inline schedule intent committed by Runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleIntentBinding {
    /// SHA-256 of the canonical UTF-8 intent.
    pub digest: String,
    /// RFC 8785 canonical intent JSON.
    pub inline_utf8: String,
}

/// Exact immutable portable schedule semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleIntent {
    schedule_id: String,
    revision_id: String,
    subject: ScheduleSubject,
    subject_binding_digest: String,
    timing: ScheduleTiming,
    misfire_policy: MisfirePolicy,
    max_lateness_ms: u64,
    effective_limits_digest: String,
}

impl ScheduleIntent {
    /// Validates identities, digests, canonical UTC timestamps and non-zero bounds.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        schedule_id: impl Into<String>,
        revision_id: impl Into<String>,
        subject: ScheduleSubject,
        subject_binding_digest: impl Into<String>,
        timing: ScheduleTiming,
        misfire_policy: MisfirePolicy,
        max_lateness_ms: u64,
        effective_limits_digest: impl Into<String>,
    ) -> Result<Self, ScheduleError> {
        let value = Self {
            schedule_id: schedule_id.into(),
            revision_id: revision_id.into(),
            subject,
            subject_binding_digest: subject_binding_digest.into(),
            timing,
            misfire_policy,
            max_lateness_ms,
            effective_limits_digest: effective_limits_digest.into(),
        };
        if !valid_id(&value.schedule_id)
            || !valid_id(&value.revision_id)
            || !valid_digest(&value.subject_binding_digest)
            || !valid_timing(&value.timing)
            || value.max_lateness_ms == 0
            || !valid_digest(&value.effective_limits_digest)
        {
            Err(ScheduleError::new(ScheduleErrorCode::InvalidSchedule))
        } else {
            Ok(value)
        }
    }

    /// Reconstructs and verifies one persisted canonical inline intent binding.
    pub fn from_binding(
        schedule_id: impl Into<String>,
        revision_id: impl Into<String>,
        binding: &ScheduleIntentBinding,
    ) -> Result<Self, ScheduleError> {
        let value: IntentSemantics = serde_json::from_str(&binding.inline_utf8)
            .map_err(|_| ScheduleError::new(ScheduleErrorCode::CorruptScheduleState))?;
        let canonical = serde_jcs::to_string(&value)
            .map_err(|_| ScheduleError::new(ScheduleErrorCode::CorruptScheduleState))?;
        let actual = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        if canonical != binding.inline_utf8
            || actual != binding.digest
            || value.contract != INTENT_CONTRACT
            || value.version != CONTRACT_VERSION
        {
            return Err(ScheduleError::new(ScheduleErrorCode::CorruptScheduleState));
        }
        Self::new(
            schedule_id,
            revision_id,
            value.subject,
            value.subject_binding_digest,
            value.timing,
            value.misfire_policy,
            value.max_lateness_ms,
            value.effective_limits_digest,
        )
        .map_err(|_| ScheduleError::new(ScheduleErrorCode::CorruptScheduleState))
    }

    /// Computes RFC 8785 SHA-256 over portable intent semantics.
    pub fn intent_digest(&self) -> Result<String, ScheduleError> {
        self.intent_binding().map(|binding| binding.digest)
    }

    /// Returns the canonical inline content binding for `schedule.created`.
    pub fn intent_binding(&self) -> Result<ScheduleIntentBinding, ScheduleError> {
        let value = self.intent_value();
        let bytes = serde_jcs::to_vec(&value)
            .map_err(|_| ScheduleError::new(ScheduleErrorCode::InvalidSchedule))?;
        let inline_utf8 = String::from_utf8(bytes)
            .map_err(|_| ScheduleError::new(ScheduleErrorCode::InvalidSchedule))?;
        let digest = format!("{:x}", Sha256::digest(inline_utf8.as_bytes()));
        Ok(ScheduleIntentBinding {
            digest,
            inline_utf8,
        })
    }

    /// Returns schedule identity.
    pub fn schedule_id(&self) -> &str {
        &self.schedule_id
    }
    /// Returns immutable revision identity.
    pub fn revision_id(&self) -> &str {
        &self.revision_id
    }
    /// Returns the portable subject.
    pub const fn subject(&self) -> ScheduleSubject {
        self.subject
    }
    /// Returns exact subject binding digest.
    pub fn subject_binding_digest(&self) -> &str {
        &self.subject_binding_digest
    }
    /// Returns timing semantics.
    pub const fn timing(&self) -> &ScheduleTiming {
        &self.timing
    }
    /// Returns misfire policy.
    pub const fn misfire_policy(&self) -> MisfirePolicy {
        self.misfire_policy
    }
    /// Returns non-zero maximum lateness.
    pub const fn max_lateness_ms(&self) -> u64 {
        self.max_lateness_ms
    }
    /// Returns effective-limits digest.
    pub fn effective_limits_digest(&self) -> &str {
        &self.effective_limits_digest
    }
    pub(crate) fn intent_value(&self) -> serde_json::Value {
        json!({
            "contract": INTENT_CONTRACT, "version": CONTRACT_VERSION,
            "subject": self.subject, "subject_binding_digest": self.subject_binding_digest,
            "timing": self.timing, "misfire_policy": self.misfire_policy,
            "max_lateness_ms": self.max_lateness_ms,
            "effective_limits_digest": self.effective_limits_digest,
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct IntentSemantics {
    contract: String,
    version: u32,
    subject: ScheduleSubject,
    subject_binding_digest: String,
    timing: ScheduleTiming,
    misfire_policy: MisfirePolicy,
    max_lateness_ms: u64,
    effective_limits_digest: String,
}

pub(crate) fn canonical_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value).ok().and_then(|time| {
        let utc = time.with_timezone(&Utc);
        (utc.to_rfc3339_opts(SecondsFormat::AutoSi, true) == value).then_some(utc)
    })
}
pub(crate) fn canonical_digest(value: &serde_json::Value) -> Result<String, ScheduleError> {
    let bytes = serde_jcs::to_vec(value)
        .map_err(|_| ScheduleError::new(ScheduleErrorCode::InvalidSchedule))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn valid_timing(value: &ScheduleTiming) -> bool {
    match value {
        ScheduleTiming::At { due_at_utc } => canonical_utc(due_at_utc).is_some(),
        ScheduleTiming::FixedDelay {
            first_due_at_utc,
            delay_ms,
            max_occurrences,
        } => {
            canonical_utc(first_due_at_utc).is_some()
                && *delay_ms != 0
                && max_occurrences.map_or(true, |count| count != 0)
        }
    }
}
fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && value.trim() == value
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
