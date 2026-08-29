use crate::{
    EvaluationError, EvaluationErrorCode, EvaluationRunId, EvaluationSuiteId, EvaluationSummary,
};

/// Immutable provenance binding one reproducible evaluation baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationBaseline {
    /// Stable run identity.
    pub run_id: EvaluationRunId,
    /// Exact evaluation suite.
    pub suite_id: EvaluationSuiteId,
    /// Public dataset identity/revision.
    pub dataset_revision: String,
    /// Official harness revision.
    pub harness_revision: String,
    /// Exact clean Garive revision under evaluation.
    pub agent_revision: String,
    /// SHA-256 digest of canonical run configuration.
    pub config_digest: String,
    /// Reduced evaluation evidence.
    pub summary: EvaluationSummary,
}

/// Complete non-summary provenance supplied to one baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EvaluationBaselineProvenance {
    /// Stable run identity.
    pub run_id: EvaluationRunId,
    /// Exact evaluation suite.
    pub suite_id: EvaluationSuiteId,
    /// Public dataset identity/revision.
    pub dataset_revision: String,
    /// Official harness revision.
    pub harness_revision: String,
    /// Exact Garive revision under evaluation.
    pub agent_revision: String,
    /// Whether the evaluated checkout had uncommitted changes.
    pub dirty: bool,
    /// SHA-256 digest of canonical run configuration.
    pub config_digest: String,
}

impl EvaluationBaseline {
    /// Validates complete, clean and digest-bound provenance.
    pub fn new(
        provenance: EvaluationBaselineProvenance,
        summary: EvaluationSummary,
    ) -> Result<Self, EvaluationError> {
        if provenance.dirty
            || [
                provenance.dataset_revision.as_str(),
                provenance.harness_revision.as_str(),
                provenance.agent_revision.as_str(),
            ]
            .iter()
            .any(|value| value.is_empty() || value.len() > 256)
            || provenance.config_digest.len() != 64
            || !provenance
                .config_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(EvaluationError::new(EvaluationErrorCode::InvalidBaseline));
        }
        Ok(Self {
            run_id: provenance.run_id,
            suite_id: provenance.suite_id,
            dataset_revision: provenance.dataset_revision,
            harness_revision: provenance.harness_revision,
            agent_revision: provenance.agent_revision,
            config_digest: provenance.config_digest.to_ascii_lowercase(),
            summary,
        })
    }
}
