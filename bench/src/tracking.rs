use std::{io::Write, sync::Mutex};

use garive_eval::{
    summarize, EvaluationBaseline, EvaluationBaselineProvenance, EvaluationCaseOutcome,
    EvaluationCaseResult, EvaluationLimits, EvaluationRunId, EvaluationSuiteId, EvaluationSummary,
};
use serde::Serialize;

use crate::{BenchError, BenchErrorCode, BenchFuture, ResultSink};

/// Complete secret-free run provenance frozen before the first result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackingDescriptor {
    /// Stable run identity.
    pub run_id: String,
    /// Exact suite identity.
    pub suite_id: String,
    /// Official dataset revision/split.
    pub dataset_revision: String,
    /// Pinned official harness revision.
    pub harness_revision: String,
    /// Exact Garive revision.
    pub agent_revision: String,
    /// Whether the evaluated checkout was dirty.
    pub dirty: bool,
    /// Canonical run configuration digest.
    pub config_digest: String,
    /// Intake adapter identity/revision.
    pub intake_adapter: String,
    /// Patch adapter identity/revision.
    pub patch_adapter: String,
    /// `official` or `self-cow`.
    pub environment_kind: String,
    /// Declared bounded concurrency.
    pub jobs: usize,
    /// Exact source case count.
    pub case_count: usize,
    /// Whether this run may become a published baseline.
    pub publishable: bool,
}

/// Completed development summary or publication-grade baseline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackingCompletion {
    /// Non-publishable smoke/development evidence.
    Development(EvaluationSummary),
    /// Clean fully attributed official evidence.
    Published(EvaluationBaseline),
}

struct TrackingState<W> {
    writer: W,
    next_index: usize,
    finished: bool,
}

/// Append-only source-ordered JSONL result sink.
pub struct JsonlResultSink<W> {
    descriptor: TrackingDescriptor,
    state: Mutex<TrackingState<W>>,
}

impl<W: Write + Send> JsonlResultSink<W> {
    /// Validates provenance and appends the run-start record.
    pub fn new(writer: W, descriptor: TrackingDescriptor) -> Result<Self, BenchError> {
        validate_descriptor(&descriptor)?;
        let sink = Self {
            descriptor,
            state: Mutex::new(TrackingState {
                writer,
                next_index: 0,
                finished: false,
            }),
        };
        sink.write_start()?;
        Ok(sink)
    }

    /// Appends the terminal summary and constructs publication evidence when admitted.
    pub fn finish(
        &self,
        results: &[EvaluationCaseResult],
        duration_ms: u64,
        limits: EvaluationLimits,
    ) -> Result<TrackingCompletion, BenchError> {
        let summary = summarize(results, limits)
            .map_err(|_| BenchError::new(BenchErrorCode::InvalidTracking))?;
        let mut state = self.state.lock().map_err(|_| invalid_tracking())?;
        if state.finished
            || state.next_index != self.descriptor.case_count
            || results.len() != self.descriptor.case_count
        {
            return Err(invalid_tracking());
        }
        write_json(
            &mut state.writer,
            &RunEnd {
                schema_version: 1,
                kind: "run-end",
                run_id: &self.descriptor.run_id,
                duration_ms,
                attempted: summary.attempted,
                passed: summary.passed,
                failed: summary.failed,
                infrastructure_failed: summary.infrastructure_failed,
                not_attempted: summary.not_attempted,
                score_numerator: summary.score.map(|value| value.numerator),
                score_denominator: summary.score.map(|value| value.denominator),
            },
        )?;
        state.finished = true;
        drop(state);
        if !self.descriptor.publishable {
            return Ok(TrackingCompletion::Development(summary));
        }
        let provenance = EvaluationBaselineProvenance {
            run_id: EvaluationRunId::new(self.descriptor.run_id.clone())
                .map_err(|_| invalid_tracking())?,
            suite_id: EvaluationSuiteId::new(self.descriptor.suite_id.clone())
                .map_err(|_| invalid_tracking())?,
            dataset_revision: self.descriptor.dataset_revision.clone(),
            harness_revision: self.descriptor.harness_revision.clone(),
            agent_revision: self.descriptor.agent_revision.clone(),
            dirty: self.descriptor.dirty,
            config_digest: self.descriptor.config_digest.clone(),
        };
        EvaluationBaseline::new(provenance, summary)
            .map(TrackingCompletion::Published)
            .map_err(|_| invalid_tracking())
    }

    /// Returns the writer after a completed run.
    pub fn into_writer(self) -> Result<W, BenchError> {
        let state = self.state.into_inner().map_err(|_| invalid_tracking())?;
        if !state.finished {
            return Err(invalid_tracking());
        }
        Ok(state.writer)
    }

    fn write_start(&self) -> Result<(), BenchError> {
        let mut state = self.state.lock().map_err(|_| invalid_tracking())?;
        write_json(
            &mut state.writer,
            &RunStart {
                schema_version: 1,
                kind: "run-start",
                run_id: &self.descriptor.run_id,
                suite_id: &self.descriptor.suite_id,
                dataset_revision: &self.descriptor.dataset_revision,
                harness_revision: &self.descriptor.harness_revision,
                agent_revision: &self.descriptor.agent_revision,
                dirty: self.descriptor.dirty,
                config_digest: &self.descriptor.config_digest,
                intake_adapter: &self.descriptor.intake_adapter,
                patch_adapter: &self.descriptor.patch_adapter,
                environment_kind: &self.descriptor.environment_kind,
                jobs: self.descriptor.jobs,
                case_count: self.descriptor.case_count,
                publishable: self.descriptor.publishable,
            },
        )
    }
}

impl<W: Write + Send> ResultSink for JsonlResultSink<W> {
    fn append<'a>(
        &'a self,
        source_index: usize,
        result: &'a EvaluationCaseResult,
    ) -> BenchFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self.state.lock().map_err(|_| invalid_tracking())?;
            if state.finished || source_index != state.next_index {
                return Err(invalid_tracking());
            }
            write_json(
                &mut state.writer,
                &CaseResult {
                    schema_version: 1,
                    kind: "case-result",
                    run_id: &self.descriptor.run_id,
                    source_index,
                    case_id: result.case_id.as_str(),
                    outcome: outcome(result.outcome),
                    duration_ms: result.duration_ms,
                    input_tokens: result.input_tokens,
                    output_tokens: result.output_tokens,
                },
            )?;
            state.next_index += 1;
            Ok(())
        })
    }
}

#[derive(Serialize)]
struct RunStart<'a> {
    schema_version: u8,
    kind: &'static str,
    run_id: &'a str,
    suite_id: &'a str,
    dataset_revision: &'a str,
    harness_revision: &'a str,
    agent_revision: &'a str,
    dirty: bool,
    config_digest: &'a str,
    intake_adapter: &'a str,
    patch_adapter: &'a str,
    environment_kind: &'a str,
    jobs: usize,
    case_count: usize,
    publishable: bool,
}

#[derive(Serialize)]
struct CaseResult<'a> {
    schema_version: u8,
    kind: &'static str,
    run_id: &'a str,
    source_index: usize,
    case_id: &'a str,
    outcome: &'static str,
    duration_ms: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Serialize)]
struct RunEnd<'a> {
    schema_version: u8,
    kind: &'static str,
    run_id: &'a str,
    duration_ms: u64,
    attempted: u64,
    passed: u64,
    failed: u64,
    infrastructure_failed: u64,
    not_attempted: u64,
    score_numerator: Option<u64>,
    score_denominator: Option<u64>,
}

fn validate_descriptor(value: &TrackingDescriptor) -> Result<(), BenchError> {
    let text = [
        &value.run_id,
        &value.suite_id,
        &value.dataset_revision,
        &value.harness_revision,
        &value.agent_revision,
        &value.intake_adapter,
        &value.patch_adapter,
    ];
    if text.iter().any(|item| item.is_empty() || item.len() > 256)
        || value.config_digest.len() != 64
        || !value
            .config_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !matches!(value.environment_kind.as_str(), "official" | "self-cow")
        || value.jobs == 0
        || value.jobs > 64
        || value.case_count == 0
        || value.publishable
            && (value.environment_kind != "official" || value.jobs == 1 || value.dirty)
    {
        return Err(invalid_tracking());
    }
    Ok(())
}

fn write_json(writer: &mut impl Write, value: &impl Serialize) -> Result<(), BenchError> {
    serde_json::to_writer(&mut *writer, value).map_err(|_| invalid_tracking())?;
    writer.write_all(b"\n").map_err(|_| invalid_tracking())
}

const fn outcome(value: EvaluationCaseOutcome) -> &'static str {
    match value {
        EvaluationCaseOutcome::Passed => "passed",
        EvaluationCaseOutcome::Failed => "failed",
        EvaluationCaseOutcome::InfrastructureFailure => "infrastructure_failure",
        EvaluationCaseOutcome::NotAttempted => "not_attempted",
    }
}

fn invalid_tracking() -> BenchError {
    BenchError::from_port(BenchErrorCode::InvalidTracking)
}
