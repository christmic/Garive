use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
};

use garive_eval::{CreativityAggregate, CreativityArmEvidence};
use garive_experiment_evidence::GitAttestationDescriptor;
use serde_json::{json, Value};

use crate::{CreativityBaselineRun, PublicationModelCoordinate};

const EVIDENCE_CONTRACT: &str = "garive.creativity-baseline-evidence";

/// Exact clean-revision values added to one CR-B evidence document.
pub struct PublicationEvidenceProvenance {
    /// Exact clean Garive revision.
    pub garive_revision: String,
    /// Exact runner/template revision.
    pub runner_revision: String,
    /// Bounded Git executable and configuration binding.
    pub git_attestation: GitAttestationDescriptor,
}

/// Stable content-free publication evidence sink failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationEvidenceError {
    /// Provenance, coordinates or run descriptors contradicted publication.
    InvalidEvidence,
    /// The destination could not be exclusively reserved.
    CreateFailed,
    /// JSON encoding failed.
    EncodeFailed,
    /// The reserved document could not be written and synchronized.
    WriteFailed,
}

impl PublicationEvidenceError {
    /// Returns the stable machine-readable failure code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidEvidence => "invalid_publication_evidence",
            Self::CreateFailed => "evidence_create_failed",
            Self::EncodeFailed => "evidence_encode_failed",
            Self::WriteFailed => "evidence_write_failed",
        }
    }
}

/// Exclusively created CR-B destination removed on failure before commit.
pub struct PublicationEvidenceReservation {
    path: PathBuf,
    file: Option<File>,
}

/// Reserves a non-overwriting destination before any model or credential call.
pub fn reserve_publication_evidence(
    path: PathBuf,
) -> Result<PublicationEvidenceReservation, PublicationEvidenceError> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| PublicationEvidenceError::CreateFailed)?;
    Ok(PublicationEvidenceReservation {
        path,
        file: Some(file),
    })
}

impl PublicationEvidenceReservation {
    /// Validates, writes and synchronizes one content-free evidence-v2 document.
    pub fn commit(
        &mut self,
        run: &CreativityBaselineRun,
        generator: &PublicationModelCoordinate,
        evaluator: &PublicationModelCoordinate,
        provenance: PublicationEvidenceProvenance,
    ) -> Result<(), PublicationEvidenceError> {
        if !identity(&provenance.garive_revision)
            || !identity(&provenance.runner_revision)
            || !digest(&provenance.git_attestation.executable_digest)
            || !digest(&provenance.git_attestation.configuration_digest)
            || !run.generator.publishable
            || !run.evaluator.publishable
            || run.generator != generator.port
            || run.evaluator != evaluator.port
        {
            return Err(PublicationEvidenceError::InvalidEvidence);
        }
        let pairs = run
            .summary
            .ordered_pairs
            .iter()
            .map(|pair| {
                json!({"task_id":pair.task_id.as_str(),"class":pair.task_class.wire_name(),
                    "control":arm(&pair.control),
                    "bounded_alternatives":arm(&pair.bounded_alternatives)})
            })
            .collect::<Vec<_>>();
        let classes = run
            .summary
            .classes
            .iter()
            .map(|class| {
                json!({"class":class.task_class.wire_name(),
                    "control":aggregate(&class.control),
                    "bounded_alternatives":aggregate(&class.bounded_alternatives)})
            })
            .collect::<Vec<_>>();
        let evidence = json!({
            "contract":EVIDENCE_CONTRACT,"version":2,"publishable":true,
            "garive_revision":provenance.garive_revision,
            "runner_revision":provenance.runner_revision,"dirty":false,"seed":run.seed,
            "corpus_id":run.corpus_id,"corpus_revision":run.corpus_revision,
            "corpus_digest":run.corpus_digest,"generator":coordinate(generator),
            "evaluator":coordinate(evaluator),
            "git_attestation":{
                "executable_digest":provenance.git_attestation.executable_digest,
                "configuration_digest":provenance.git_attestation.configuration_digest},
            "pairs":pairs,"summary":{"control":aggregate(&run.summary.control),
                "bounded_alternatives":aggregate(&run.summary.bounded_alternatives),
                "classes":classes},
        });
        let bytes = serde_json::to_vec_pretty(&evidence)
            .map_err(|_| PublicationEvidenceError::EncodeFailed)?;
        let file = self
            .file
            .as_mut()
            .ok_or(PublicationEvidenceError::WriteFailed)?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all())
            .map_err(|_| PublicationEvidenceError::WriteFailed)?;
        self.file.take();
        Ok(())
    }
}

impl Drop for PublicationEvidenceReservation {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn coordinate(value: &PublicationModelCoordinate) -> Value {
    json!({"protocol":value.protocol.wire_name(),"target_id":value.target_id,
        "model_id":value.model_id,"model_revision":value.model_revision,
        "implementation_id":value.port.implementation_id,
        "implementation_revision":value.port.implementation_revision,
        "config_digest":value.port.config_digest})
}

fn arm(value: &CreativityArmEvidence) -> Value {
    json!({"candidate_count":value.candidate_count,
        "correct_candidate_count":value.correct_candidate_count,
        "distinct_correct_cluster_count":value.distinct_correct_cluster_count,
        "selected_correct":value.selected_correct})
}

fn aggregate(value: &CreativityAggregate) -> Value {
    json!({"task_count":value.task_count,"candidate_count":value.candidate_count,
        "correct_candidate_count":value.correct_candidate_count,
        "correct_cluster_mean_numerator":value.correct_cluster_mean_numerator,
        "correct_cluster_mean_denominator":value.correct_cluster_mean_denominator,
        "selected_correct_numerator":value.selected_correct_numerator,
        "selected_correct_denominator":value.selected_correct_denominator})
}

fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
