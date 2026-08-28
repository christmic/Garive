use std::collections::BTreeSet;

use garive_llm::{MediaKind, ModelInputContent, ModelInputItem};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContextPurpose {
    Inference,
    Governance,
    ToolPreparation,
    Summarization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CandidateKind {
    Instruction,
    UserInput,
    ModelOutput,
    ToolObservation,
    Approval,
    Summary,
    SystemNotice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retention {
    Required,
    Optional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Visibility {
    Visible,
    Redacted,
    Purposes(BTreeSet<ContextPurpose>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FactRef {
    pub session_id: String,
    pub position: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextCandidate {
    pub fact_ref: FactRef,
    pub kind: CandidateKind,
    pub retention: Retention,
    pub visibility: Visibility,
    pub items: Vec<ModelInputItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextRequest {
    pub session_id: String,
    pub turn_id: String,
    pub purpose: ContextPurpose,
    pub after_position: Option<u64>,
    pub through_position: u64,
    pub max_items: usize,
    pub max_utf8_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextItem {
    Input {
        fact_ref: FactRef,
        item: ModelInputItem,
    },
    RedactedItem {
        fact_ref: FactRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSurface {
    pub purpose: ContextPurpose,
    pub from_position: u64,
    pub through_position: u64,
    pub items: Vec<ContextItem>,
    pub retained_refs: Vec<FactRef>,
    pub dropped_refs: Vec<FactRef>,
    pub filtered_refs: Vec<FactRef>,
    pub item_count: usize,
    pub utf8_bytes: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextDerivationError {
    InvalidRequest,
    SessionMismatch,
    PositionBeyondSurface,
    NonIncreasingPosition,
    DuplicateReference,
    EmptyRequiredContent,
    InvalidVisibility,
    BudgetOverflow,
    RequiredFactsExceedBudget {
        item_count: usize,
        utf8_bytes: usize,
    },
}

impl ContextDerivationError {
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
