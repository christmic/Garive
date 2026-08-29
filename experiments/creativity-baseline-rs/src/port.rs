use garive_eval::{CreativityArm, EvaluationCaseId};

use crate::CreativityBaselineError;

/// Immutable non-secret implementation/configuration binding for one port.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentPortDescriptor {
    /// Stable implementation identity.
    pub implementation_id: String,
    /// Exact implementation/configuration vocabulary revision.
    pub implementation_revision: String,
    /// SHA-256 of canonical non-secret port configuration.
    pub config_digest: String,
    /// Whether this exact implementation may enter publication evidence.
    pub publishable: bool,
}

impl ExperimentPortDescriptor {
    /// Validates bounded identities and a lowercase SHA-256 digest.
    pub fn new(
        implementation_id: impl Into<String>,
        implementation_revision: impl Into<String>,
        config_digest: impl Into<String>,
        publishable: bool,
    ) -> Option<Self> {
        let value = Self {
            implementation_id: implementation_id.into(),
            implementation_revision: implementation_revision.into(),
            config_digest: config_digest.into(),
            publishable,
        };
        let identity = |text: &str| !text.is_empty() && text.len() <= 256;
        if !identity(&value.implementation_id)
            || !identity(&value.implementation_revision)
            || value.config_digest.len() != 64
            || !value
                .config_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            None
        } else {
            Some(value)
        }
    }
}

/// Exact rubric-free request visible to one generator invocation.
pub struct GeneratorRequest<'a> {
    /// Stable task identity.
    pub task_id: &'a EvaluationCaseId,
    /// Explicit paired arm.
    pub arm: CreativityArm,
    /// Generator-only task text.
    pub prompt: &'a str,
    /// Deterministic non-secret run seed.
    pub seed: u64,
    /// Exact maximum candidate count for this arm.
    pub max_candidates: u64,
    /// Maximum UTF-8 bytes per candidate.
    pub max_candidate_utf8_bytes: usize,
    /// Maximum aggregate UTF-8 bytes across candidates.
    pub max_total_candidate_utf8_bytes: usize,
}

/// One inert generated candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedCandidate {
    /// Stable candidate identity scoped to this arm.
    pub candidate_id: String,
    /// Candidate text; it grants no authority and performs no effect.
    pub content: String,
}

/// Complete raw output from one generator invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedArm {
    /// Ordered candidates.
    pub candidates: Vec<GeneratedCandidate>,
    /// Candidate chosen by the generator under this arm.
    pub selected_candidate_id: String,
}

/// Exact arm/selection-blind evaluator request.
pub struct EvaluatorRequest<'a> {
    /// Stable task identity.
    pub task_id: &'a EvaluationCaseId,
    /// Evaluator-only strict JSON rubric.
    pub rubric_json: &'a str,
    /// Ordered inert candidates; selection is deliberately absent.
    pub candidates: &'a [GeneratedCandidate],
}

/// One evaluator verdict with correct-only semantic clustering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateVerdict {
    /// Exact candidate identity.
    pub candidate_id: String,
    /// Whether the candidate satisfies the evaluator rubric.
    pub correct: bool,
    /// Semantic cluster identity, present exactly when correct.
    pub correct_cluster_id: Option<String>,
}

/// Injected one-attempt generator boundary.
pub trait CreativityGeneratorPort {
    /// Returns the immutable implementation binding.
    fn descriptor(&self) -> &ExperimentPortDescriptor;

    /// Performs exactly one bounded generation attempt.
    fn generate(
        &self,
        request: GeneratorRequest<'_>,
    ) -> Result<GeneratedArm, CreativityBaselineError>;
}

/// Injected one-attempt blind evaluator boundary.
pub trait CreativityEvaluatorPort {
    /// Returns the immutable implementation binding.
    fn descriptor(&self) -> &ExperimentPortDescriptor;

    /// Performs exactly one bounded rubric evaluation attempt.
    fn evaluate(
        &self,
        request: EvaluatorRequest<'_>,
    ) -> Result<Vec<CandidateVerdict>, CreativityBaselineError>;
}
