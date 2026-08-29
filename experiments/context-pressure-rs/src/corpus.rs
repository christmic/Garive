use std::collections::{BTreeSet, HashSet};

use garive_core::{
    derive_context, CandidateKind, ContextCandidate, ContextPurpose, ContextRequest, FactRef,
    Retention, Visibility,
};
use garive_eval::{ContextWorkloadClass, EvaluationCaseId};
use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{ContextPressureError, ContextPressureErrorCode};

const CONTRACT: &str = "garive.context-pressure-corpus";
const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const MAX_CASES: usize = 1_024;

/// Validated reference case ready for pure C2 derivation and token counting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureCase {
    /// Stable case identity.
    pub case_id: EvaluationCaseId,
    /// Required reference workload category.
    pub workload_class: ContextWorkloadClass,
    /// Exact uncompressed C2 request.
    pub request: ContextRequest,
    /// Ordered provider-neutral candidate stream.
    pub candidates: Vec<ContextCandidate>,
    /// Exact model input limit for pressure measurement.
    pub model_input_limit_tokens: u64,
}

/// Strict versioned corpus plus its canonical content digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextPressureCorpus {
    /// Stable corpus identity.
    pub corpus_id: String,
    /// Exact corpus revision.
    pub corpus_revision: String,
    /// Lowercase SHA-256 of RFC 8785 corpus JSON.
    pub canonical_digest: String,
    /// Source-ordered validated cases.
    pub cases: Vec<ContextPressureCase>,
}

/// Loads one bounded strict schema-v1 C7-A corpus.
pub fn load_corpus(input: &[u8]) -> Result<ContextPressureCorpus, ContextPressureError> {
    if input.is_empty() || input.len() > MAX_DOCUMENT_BYTES {
        return Err(error(ContextPressureErrorCode::InvalidDocument));
    }
    let raw: RawCorpus = serde_json::from_slice(input)
        .map_err(|_| error(ContextPressureErrorCode::InvalidDocument))?;
    if raw.contract != CONTRACT
        || raw.version != 1
        || !valid_identity(&raw.corpus_id)
        || !valid_identity(&raw.corpus_revision)
        || raw.cases.is_empty()
        || raw.cases.len() > MAX_CASES
    {
        return Err(error(ContextPressureErrorCode::InvalidCorpus));
    }
    let canonical =
        serde_jcs::to_vec(&raw).map_err(|_| error(ContextPressureErrorCode::InvalidDocument))?;
    let mut ids = HashSet::new();
    let mut classes = BTreeSet::new();
    let mut cases = Vec::with_capacity(raw.cases.len());
    for value in raw.cases {
        if !ids.insert(value.case_id.clone()) || value.model_input_limit_tokens == 0 {
            return Err(error(ContextPressureErrorCode::InvalidCorpus));
        }
        let case = convert_case(value)?;
        classes.insert(case.workload_class);
        let surface = derive_context(&case.request, &case.candidates)
            .map_err(|_| error(ContextPressureErrorCode::InvalidContext))?;
        if !surface.dropped_refs.is_empty() {
            return Err(error(ContextPressureErrorCode::CompressedInput));
        }
        cases.push(case);
    }
    if classes != BTreeSet::from(ContextWorkloadClass::ALL) {
        return Err(error(ContextPressureErrorCode::InvalidCorpus));
    }
    Ok(ContextPressureCorpus {
        corpus_id: raw.corpus_id,
        corpus_revision: raw.corpus_revision,
        canonical_digest: format!("{:x}", Sha256::digest(canonical)),
        cases,
    })
}

fn convert_case(value: RawCase) -> Result<ContextPressureCase, ContextPressureError> {
    let workload_class = class(&value.workload_class)?;
    let request = ContextRequest {
        session_id: value.request.session_id,
        turn_id: value.request.turn_id,
        purpose: ContextPurpose::Inference,
        after_position: value.request.after_position,
        through_position: value.request.through_position,
        max_items: value.request.max_items,
        max_utf8_bytes: value.request.max_utf8_bytes,
    };
    let candidates = value
        .candidates
        .into_iter()
        .map(|candidate| convert_candidate(&request.session_id, candidate))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ContextPressureCase {
        case_id: EvaluationCaseId::new(value.case_id)
            .map_err(|_| error(ContextPressureErrorCode::InvalidCorpus))?,
        workload_class,
        request,
        candidates,
        model_input_limit_tokens: value.model_input_limit_tokens,
    })
}

fn convert_candidate(
    session_id: &str,
    value: RawCandidate,
) -> Result<ContextCandidate, ContextPressureError> {
    Ok(ContextCandidate {
        fact_ref: FactRef {
            session_id: session_id.into(),
            position: value.position,
        },
        kind: kind(&value.kind)?,
        retention: match value.retention.as_str() {
            "required" => Retention::Required,
            "optional" => Retention::Optional,
            _ => return Err(error(ContextPressureErrorCode::InvalidCorpus)),
        },
        visibility: Visibility::Purposes(BTreeSet::from([ContextPurpose::Inference])),
        items: value
            .items
            .into_iter()
            .map(convert_item)
            .collect::<Result<_, _>>()?,
    })
}

fn convert_item(value: RawItem) -> Result<ModelInputItem, ContextPressureError> {
    Ok(match value {
        RawItem::Message { role, text } => ModelInputItem::Message {
            role: model_role(&role)?,
            content: vec![ModelInputContent::Text(text)],
        },
        RawItem::ToolObservation {
            model_call_id,
            result_json,
        } => ModelInputItem::ToolObservation {
            model_call_id,
            result_json,
        },
        RawItem::ReasoningReference { reference } => {
            ModelInputItem::ReasoningReference { reference }
        }
    })
}

fn class(value: &str) -> Result<ContextWorkloadClass, ContextPressureError> {
    match value {
        "conversation" => Ok(ContextWorkloadClass::Conversation),
        "tool_heavy" => Ok(ContextWorkloadClass::ToolHeavy),
        "capability_heavy" => Ok(ContextWorkloadClass::CapabilityHeavy),
        "long_running" => Ok(ContextWorkloadClass::LongRunning),
        _ => Err(error(ContextPressureErrorCode::InvalidCorpus)),
    }
}

fn kind(value: &str) -> Result<CandidateKind, ContextPressureError> {
    match value {
        "instruction" => Ok(CandidateKind::Instruction),
        "skill" => Ok(CandidateKind::Skill),
        "user_input" => Ok(CandidateKind::UserInput),
        "model_output" => Ok(CandidateKind::ModelOutput),
        "tool_observation" => Ok(CandidateKind::ToolObservation),
        "approval" => Ok(CandidateKind::Approval),
        "summary" => Ok(CandidateKind::Summary),
        "system_notice" => Ok(CandidateKind::SystemNotice),
        "memory" => Ok(CandidateKind::Memory),
        "knowledge" => Ok(CandidateKind::Knowledge),
        _ => Err(error(ContextPressureErrorCode::InvalidCorpus)),
    }
}

fn model_role(value: &str) -> Result<ModelRole, ContextPressureError> {
    match value {
        "system" => Ok(ModelRole::System),
        "developer" => Ok(ModelRole::Developer),
        "user" => Ok(ModelRole::User),
        "assistant" => Ok(ModelRole::Assistant),
        _ => Err(error(ContextPressureErrorCode::InvalidCorpus)),
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}

const fn error(code: ContextPressureErrorCode) -> ContextPressureError {
    ContextPressureError::new(code)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawCorpus {
    contract: String,
    version: u32,
    corpus_id: String,
    corpus_revision: String,
    cases: Vec<RawCase>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    case_id: String,
    workload_class: String,
    request: RawRequest,
    candidates: Vec<RawCandidate>,
    model_input_limit_tokens: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawRequest {
    session_id: String,
    turn_id: String,
    after_position: Option<u64>,
    through_position: u64,
    max_items: usize,
    max_utf8_bytes: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawCandidate {
    position: u64,
    kind: String,
    retention: String,
    items: Vec<RawItem>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum RawItem {
    Message {
        role: String,
        text: String,
    },
    ToolObservation {
        model_call_id: String,
        result_json: String,
    },
    ReasoningReference {
        reference: String,
    },
}
