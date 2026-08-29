use std::collections::BTreeSet;

use garive_eval::{CreativityTaskClass, EvaluationCaseId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CreativityBaselineError, CreativityBaselineErrorCode};

const CONTRACT: &str = "garive.creativity-corpus";
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;
const MAX_TASKS: usize = 1024;
const MAX_PROMPT_BYTES: usize = 16 * 1024;
const MAX_RUBRIC_BYTES: usize = 64 * 1024;
const MAX_CANDIDATES: u64 = 16;

/// Validated gold-separated task used by both paired arms.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityTask {
    /// Stable task identity.
    pub task_id: EvaluationCaseId,
    /// Neutral evaluation class.
    pub task_class: CreativityTaskClass,
    /// Exact generator-only task text.
    pub generator_prompt: String,
    /// Exact evaluator-only strict JSON rubric.
    pub evaluator_rubric_json: String,
    /// Maximum candidates admitted from the alternatives arm.
    pub max_candidates: u64,
    /// Maximum UTF-8 bytes in one candidate.
    pub max_candidate_utf8_bytes: usize,
    /// Maximum UTF-8 bytes across candidates in one arm.
    pub max_total_candidate_utf8_bytes: usize,
}

/// Strict versioned CR-A corpus and canonical digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityCorpus {
    /// Stable corpus identity.
    pub corpus_id: String,
    /// Exact corpus revision.
    pub corpus_revision: String,
    /// SHA-256 of RFC 8785 corpus JSON.
    pub canonical_digest: String,
    /// Source-ordered complete tasks.
    pub tasks: Vec<CreativityTask>,
}

/// Loads and validates one bounded strict schema-v1 creativity corpus.
pub fn load_creativity_corpus(input: &[u8]) -> Result<CreativityCorpus, CreativityBaselineError> {
    if input.is_empty() || input.len() > MAX_DOCUMENT_BYTES {
        return Err(error(CreativityBaselineErrorCode::InvalidDocument));
    }
    let raw: RawCorpus = serde_json::from_slice(input)
        .map_err(|_| error(CreativityBaselineErrorCode::InvalidDocument))?;
    if raw.contract != CONTRACT
        || raw.version != 1
        || !identity(&raw.corpus_id)
        || !identity(&raw.corpus_revision)
        || raw.tasks.is_empty()
        || raw.tasks.len() > MAX_TASKS
    {
        return Err(error(CreativityBaselineErrorCode::InvalidCorpus));
    }
    let canonical =
        serde_jcs::to_vec(&raw).map_err(|_| error(CreativityBaselineErrorCode::InvalidDocument))?;
    let mut identities = BTreeSet::new();
    let mut classes = BTreeSet::new();
    let mut tasks = Vec::with_capacity(raw.tasks.len());
    for value in raw.tasks {
        if !identities.insert(value.task_id.clone()) {
            return Err(error(CreativityBaselineErrorCode::InvalidCorpus));
        }
        let task = convert(value)?;
        classes.insert(task.task_class);
        tasks.push(task);
    }
    if classes != BTreeSet::from(CreativityTaskClass::ALL) {
        return Err(error(CreativityBaselineErrorCode::InvalidCorpus));
    }
    Ok(CreativityCorpus {
        corpus_id: raw.corpus_id,
        corpus_revision: raw.corpus_revision,
        canonical_digest: format!("{:x}", Sha256::digest(canonical)),
        tasks,
    })
}

fn convert(value: RawTask) -> Result<CreativityTask, CreativityBaselineError> {
    let task_class = match value.task_class.as_str() {
        "design_alternatives" => CreativityTaskClass::DesignAlternatives,
        "diagnostic_hypotheses" => CreativityTaskClass::DiagnosticHypotheses,
        "constraint_reconciliation" => CreativityTaskClass::ConstraintReconciliation,
        "transformation_reframing" => CreativityTaskClass::TransformationReframing,
        _ => return Err(error(CreativityBaselineErrorCode::InvalidCorpus)),
    };
    let maximum_total = value
        .max_candidates
        .checked_mul(value.max_candidate_utf8_bytes as u64)
        .ok_or_else(|| error(CreativityBaselineErrorCode::InvalidCorpus))?;
    let minimum_total = value
        .max_candidate_utf8_bytes
        .checked_mul(2)
        .ok_or_else(|| error(CreativityBaselineErrorCode::InvalidCorpus))?;
    if !identity(&value.task_id)
        || value.generator_prompt.is_empty()
        || value.generator_prompt.len() > MAX_PROMPT_BYTES
        || value.evaluator_rubric_json.is_empty()
        || value.evaluator_rubric_json.len() > MAX_RUBRIC_BYTES
        || serde_json::from_str::<serde_json::Value>(&value.evaluator_rubric_json).is_err()
        || !(2..=MAX_CANDIDATES).contains(&value.max_candidates)
        || value.max_candidate_utf8_bytes == 0
        || value.max_total_candidate_utf8_bytes < minimum_total
        || value.max_total_candidate_utf8_bytes as u64 > maximum_total
    {
        return Err(error(CreativityBaselineErrorCode::InvalidCorpus));
    }
    Ok(CreativityTask {
        task_id: EvaluationCaseId::new(value.task_id)
            .map_err(|_| error(CreativityBaselineErrorCode::InvalidCorpus))?,
        task_class,
        generator_prompt: value.generator_prompt,
        evaluator_rubric_json: value.evaluator_rubric_json,
        max_candidates: value.max_candidates,
        max_candidate_utf8_bytes: value.max_candidate_utf8_bytes,
        max_total_candidate_utf8_bytes: value.max_total_candidate_utf8_bytes,
    })
}

fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}

const fn error(code: CreativityBaselineErrorCode) -> CreativityBaselineError {
    CreativityBaselineError::new(code)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawCorpus {
    contract: String,
    version: u32,
    corpus_id: String,
    corpus_revision: String,
    tasks: Vec<RawTask>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawTask {
    task_id: String,
    #[serde(rename = "class")]
    task_class: String,
    generator_prompt: String,
    evaluator_rubric_json: String,
    max_candidates: u64,
    max_candidate_utf8_bytes: usize,
    max_total_candidate_utf8_bytes: usize,
}
