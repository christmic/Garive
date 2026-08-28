use std::collections::BTreeSet;

use garive_llm::{MediaKind, ModelInputContent, ModelInputItem};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
/// Consumer-specific reason for deriving a bounded context surface.
pub enum ContextPurpose {
    /// Input for a model inference request.
    Inference,
    /// Evidence for policy, authorization, or safety decisions.
    Governance,
    /// Input needed to prepare a tool invocation.
    ToolPreparation,
    /// Input used to build a durable summary.
    Summarization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Semantic class of a ledger fact considered for context.
pub enum CandidateKind {
    /// Trusted instruction that constrains the agent.
    Instruction,
    /// User-provided input.
    UserInput,
    /// Prior model output.
    ModelOutput,
    /// Observation returned by a tool.
    ToolObservation,
    /// Approval or denial decision.
    Approval,
    /// Durable summary derived from earlier facts.
    Summary,
    /// System-generated operational notice.
    SystemNotice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Whether a candidate may be dropped to satisfy a budget.
pub enum Retention {
    /// The candidate must fit or derivation fails.
    Required,
    /// The candidate may be dropped, oldest first, when budgets are tight.
    Optional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Purpose-based disclosure rule applied before budgeting.
pub enum Visibility {
    /// Include the candidate's model input items.
    Visible,
    /// Preserve the fact reference but replace its content with a redaction.
    Redacted,
    /// Include content only for the listed purposes.
    Purposes(BTreeSet<ContextPurpose>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
/// Stable reference to one ordered fact in a session ledger.
pub struct FactRef {
    /// Session that owns the fact.
    pub session_id: String,
    /// One-based durable position within the session.
    pub position: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Validated input candidate presented to the pure derivation algorithm.
pub struct ContextCandidate {
    /// Durable identity and ordering key of the source fact.
    pub fact_ref: FactRef,
    /// Semantic fact class used by callers and audit evidence.
    pub kind: CandidateKind,
    /// Budget retention rule.
    pub retention: Retention,
    /// Purpose-specific disclosure rule.
    pub visibility: Visibility,
    /// Model input items contributed when visible.
    pub items: Vec<ModelInputItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Bounds and ledger window for one deterministic context derivation.
pub struct ContextRequest {
    /// Session whose facts may enter the surface.
    pub session_id: String,
    /// Turn requesting context; required for traceability.
    pub turn_id: String,
    /// Consumer purpose used for visibility filtering.
    pub purpose: ContextPurpose,
    /// Exclusive lower fact position, if deriving an incremental surface.
    pub after_position: Option<u64>,
    /// Inclusive upper fact position captured for this derivation.
    pub through_position: u64,
    /// Maximum number of output items, including redaction placeholders.
    pub max_items: usize,
    /// Maximum UTF-8 bytes across visible model input fields.
    pub max_utf8_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One auditable item emitted in a derived context surface.
pub enum ContextItem {
    /// Visible model input tied to its originating fact.
    Input {
        /// Source ledger fact.
        fact_ref: FactRef,
        /// Model input content copied from the candidate.
        item: ModelInputItem,
    },
    /// Placeholder proving that a fact existed but was redacted.
    RedactedItem {
        /// Source ledger fact whose content was withheld.
        fact_ref: FactRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Deterministic, bounded projection supplied to a context consumer.
pub struct ContextSurface {
    /// Purpose under which visibility was evaluated.
    pub purpose: ContextPurpose,
    /// Inclusive lower position represented by the request window.
    pub from_position: u64,
    /// Inclusive upper position captured by the request.
    pub through_position: u64,
    /// Visible inputs and redaction placeholders in ledger order.
    pub items: Vec<ContextItem>,
    /// Candidate references retained in the output.
    pub retained_refs: Vec<FactRef>,
    /// Eligible optional references dropped for budget pressure.
    pub dropped_refs: Vec<FactRef>,
    /// References excluded by window or visibility rules.
    pub filtered_refs: Vec<FactRef>,
    /// Number of emitted items.
    pub item_count: usize,
    /// UTF-8 bytes charged against the content budget.
    pub utf8_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Contract violation or bounded-derivation failure.
pub enum ContextDerivationError {
    /// Request identifiers, bounds, or budgets are invalid.
    InvalidRequest,
    /// A candidate belongs to a different session.
    SessionMismatch,
    /// A candidate position is zero or beyond the captured surface.
    PositionBeyondSurface,
    /// Candidate positions are not strictly increasing.
    NonIncreasingPosition,
    /// Two adjacent ordered candidates reference the same fact position.
    DuplicateReference,
    /// A required visible candidate contributes no usable content.
    EmptyRequiredContent,
    /// A purpose-restricted visibility rule names no purpose.
    InvalidVisibility,
    /// Internal checked arithmetic detected a size overflow.
    BudgetOverflow,
    /// Required candidates alone exceed a declared request budget.
    RequiredFactsExceedBudget {
        /// Required output item count.
        item_count: usize,
        /// Required visible UTF-8 byte count.
        utf8_bytes: usize,
    },
}

impl ContextDerivationError {
    /// Returns the stable machine-readable error code used by fixtures and adapters.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid-request",
            Self::SessionMismatch => "session-mismatch",
            Self::PositionBeyondSurface => "position-beyond-surface",
            Self::NonIncreasingPosition => "non-increasing-position",
            Self::DuplicateReference => "duplicate-reference",
            Self::EmptyRequiredContent => "empty-required-content",
            Self::InvalidVisibility => "invalid-visibility",
            Self::BudgetOverflow => "budget-overflow",
            Self::RequiredFactsExceedBudget { .. } => "required-facts-exceed-budget",
        }
    }
}

struct Eligible<'a> {
    candidate: &'a ContextCandidate,
    item_count: usize,
    utf8_bytes: usize,
    redacted: bool,
}

/// Derives a deterministic surface without reading or mutating external state.
///
/// Candidates must be in strictly increasing ledger order. Visibility filtering
/// occurs before budgeting; required candidates are always retained, while the
/// newest optional candidates are retained until either budget is exhausted.
pub fn derive_context(
    request: &ContextRequest,
    candidates: &[ContextCandidate],
) -> Result<ContextSurface, ContextDerivationError> {
    let from_position = request
        .after_position
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(ContextDerivationError::InvalidRequest)?;
    if request.session_id.is_empty()
        || request.turn_id.is_empty()
        || request.through_position == 0
        || request.max_items == 0
        || request.max_utf8_bytes == 0
        || request
            .after_position
            .is_some_and(|after| after >= request.through_position)
    {
        return Err(ContextDerivationError::InvalidRequest);
    }

    let mut last_position = None;
    let mut filtered_refs = Vec::new();
    let mut eligible = Vec::new();
    for candidate in candidates {
        validate_candidate(request, candidate, &mut last_position)?;
        if request
            .after_position
            .is_some_and(|after| candidate.fact_ref.position <= after)
            || !is_visible_for(&candidate.visibility, request.purpose)?
        {
            filtered_refs.push(candidate.fact_ref.clone());
            continue;
        }
        let redacted = candidate.visibility == Visibility::Redacted;
        let (item_count, utf8_bytes) = if redacted {
            (1, 0)
        } else {
            candidate_cost(&candidate.items)?
        };
        if candidate.retention == Retention::Required
            && !redacted
            && (item_count == 0 || utf8_bytes == 0)
        {
            return Err(ContextDerivationError::EmptyRequiredContent);
        }
        eligible.push(Eligible {
            candidate,
            item_count,
            utf8_bytes,
            redacted,
        });
    }

    let mut required_items = 0usize;
    let mut required_bytes = 0usize;
    let mut retained_positions = BTreeSet::new();
    for value in &eligible {
        if value.candidate.retention == Retention::Required {
            required_items = required_items
                .checked_add(value.item_count)
                .ok_or(ContextDerivationError::BudgetOverflow)?;
            required_bytes = required_bytes
                .checked_add(value.utf8_bytes)
                .ok_or(ContextDerivationError::BudgetOverflow)?;
            retained_positions.insert(value.candidate.fact_ref.position);
        }
    }
    if required_items > request.max_items || required_bytes > request.max_utf8_bytes {
        return Err(ContextDerivationError::RequiredFactsExceedBudget {
            item_count: required_items,
            utf8_bytes: required_bytes,
        });
    }

    let mut item_count = required_items;
    let mut utf8_bytes = required_bytes;
    for value in eligible.iter().rev() {
        if value.candidate.retention == Retention::Required {
            continue;
        }
        let next_items = item_count
            .checked_add(value.item_count)
            .ok_or(ContextDerivationError::BudgetOverflow)?;
        let next_bytes = utf8_bytes
            .checked_add(value.utf8_bytes)
            .ok_or(ContextDerivationError::BudgetOverflow)?;
        if next_items <= request.max_items && next_bytes <= request.max_utf8_bytes {
            item_count = next_items;
            utf8_bytes = next_bytes;
            retained_positions.insert(value.candidate.fact_ref.position);
        }
    }

    let mut items = Vec::with_capacity(item_count);
    let mut retained_refs = Vec::new();
    let mut dropped_refs = Vec::new();
    for value in eligible {
        if retained_positions.contains(&value.candidate.fact_ref.position) {
            retained_refs.push(value.candidate.fact_ref.clone());
            if value.redacted {
                items.push(ContextItem::RedactedItem {
                    fact_ref: value.candidate.fact_ref.clone(),
                });
            } else {
                items.extend(value.candidate.items.iter().cloned().map(|item| {
                    ContextItem::Input {
                        fact_ref: value.candidate.fact_ref.clone(),
                        item,
                    }
                }));
            }
        } else {
            dropped_refs.push(value.candidate.fact_ref.clone());
        }
    }
    Ok(ContextSurface {
        purpose: request.purpose,
        from_position,
        through_position: request.through_position,
        items,
        retained_refs,
        dropped_refs,
        filtered_refs,
        item_count,
        utf8_bytes,
    })
}

fn validate_candidate(
    request: &ContextRequest,
    candidate: &ContextCandidate,
    last_position: &mut Option<u64>,
) -> Result<(), ContextDerivationError> {
    if candidate.fact_ref.session_id != request.session_id {
        return Err(ContextDerivationError::SessionMismatch);
    }
    if candidate.fact_ref.position == 0 || candidate.fact_ref.position > request.through_position {
        return Err(ContextDerivationError::PositionBeyondSurface);
    }
    if let Some(last) = *last_position {
        if candidate.fact_ref.position == last {
            return Err(ContextDerivationError::DuplicateReference);
        }
        if candidate.fact_ref.position < last {
            return Err(ContextDerivationError::NonIncreasingPosition);
        }
    }
    if candidate.retention == Retention::Required && candidate.items.is_empty() {
        return Err(ContextDerivationError::EmptyRequiredContent);
    }
    *last_position = Some(candidate.fact_ref.position);
    Ok(())
}

fn is_visible_for(
    visibility: &Visibility,
    purpose: ContextPurpose,
) -> Result<bool, ContextDerivationError> {
    match visibility {
        Visibility::Visible | Visibility::Redacted => Ok(true),
        Visibility::Purposes(purposes) if purposes.is_empty() => {
            Err(ContextDerivationError::InvalidVisibility)
        }
        Visibility::Purposes(purposes) => Ok(purposes.contains(&purpose)),
    }
}

fn candidate_cost(items: &[ModelInputItem]) -> Result<(usize, usize), ContextDerivationError> {
    let mut bytes = 0usize;
    for item in items {
        bytes = bytes
            .checked_add(item_utf8_bytes(item)?)
            .ok_or(ContextDerivationError::BudgetOverflow)?;
    }
    Ok((items.len(), bytes))
}

fn item_utf8_bytes(item: &ModelInputItem) -> Result<usize, ContextDerivationError> {
    let strings: Vec<&str> = match item {
        ModelInputItem::Message { content, .. } => content
            .iter()
            .flat_map(|value| match value {
                ModelInputContent::Text(text) => vec![text.as_str()],
                ModelInputContent::MediaReference {
                    media_kind,
                    reference,
                    media_type,
                } => {
                    let mut values = vec![reference.as_str(), media_type.as_str()];
                    if let MediaKind::Other(name) = media_kind {
                        values.push(name.as_str());
                    }
                    values
                }
            })
            .collect(),
        ModelInputItem::ToolObservation {
            model_call_id,
            result_json,
        } => vec![model_call_id, result_json],
        ModelInputItem::ReasoningReference { reference } => vec![reference],
    };
    strings.into_iter().try_fold(0usize, |total, value| {
        total
            .checked_add(value.len())
            .ok_or(ContextDerivationError::BudgetOverflow)
    })
}
