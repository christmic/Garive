use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    BenchError, BenchErrorCode, BenchFuture, EvaluationVerdict, OfficialEvaluator, SweCase,
    SweDataset,
};

/// Exact official prediction record.
#[derive(Serialize)]
struct OfficialPrediction<'a> {
    instance_id: &'a str,
    model_name_or_path: &'a str,
    model_patch: &'a str,
}

/// Explicit, environment-independent official harness configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialEvaluatorConfig {
    /// Python executable selected by the runner.
    pub python_executable: String,
    /// Exact public dataset.
    pub dataset: SweDataset,
    /// Explicit directory receiving collision-free per-case predictions.
    pub predictions_directory: String,
    /// Explicit harness working/run directory.
    pub run_directory: String,
    /// Non-empty run identity.
    pub run_id: String,
    /// Agent/model label written to the official prediction.
    pub model_name: String,
    /// Official harness worker bound.
    pub jobs: usize,
    /// Per-instance official timeout in seconds.
    pub timeout_seconds: u64,
    /// Explicit environment admitted after clearing inherited values.
    pub environment: Vec<(String, String)>,
}

/// Complete subprocess request; the implementation must clear inherited environment first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialInvocation {
    /// Executable path/name.
    pub executable: String,
    /// Exact ordered argv excluding argv[0].
    pub arguments: Vec<String>,
    /// Explicit working directory.
    pub working_directory: String,
    /// Exact per-case predictions file path.
    pub prediction_path: String,
    /// Exact final report file path emitted by the pinned harness.
    pub report_path: String,
    /// Exact prediction file bytes to materialize at the configured path.
    pub prediction_jsonl: Vec<u8>,
    /// Explicit environment after inherited values are cleared.
    pub environment: Vec<(String, String)>,
}

/// Sanitized official subprocess result and final report bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficialProcessOutput {
    /// Process exit status.
    pub exit_code: i32,
    /// Exact final run-report JSON read by the process boundary.
    pub report_json: Vec<u8>,
}

/// Injected process/filesystem boundary for the official Python harness.
pub trait OfficialProcess: Sync {
    /// Materializes predictions, invokes the harness, and reads its final report.
    fn invoke<'a>(
        &'a self,
        invocation: OfficialInvocation,
    ) -> BenchFuture<'a, OfficialProcessOutput>;
}

/// Tokio-backed official process boundary with cleared inherited environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdOfficialProcess;

impl OfficialProcess for StdOfficialProcess {
    fn invoke<'a>(
        &'a self,
        invocation: OfficialInvocation,
    ) -> BenchFuture<'a, OfficialProcessOutput> {
        Box::pin(async move {
            tokio::fs::write(&invocation.prediction_path, &invocation.prediction_jsonl)
                .await
                .map_err(|_| invalid_evaluation())?;
            let status = tokio::process::Command::new(&invocation.executable)
                .args(&invocation.arguments)
                .current_dir(&invocation.working_directory)
                .env_clear()
                .envs(invocation.environment)
                .status()
                .await
                .map_err(|_| invalid_evaluation())?;
            let report_json = tokio::fs::read(&invocation.report_path)
                .await
                .map_err(|_| invalid_evaluation())?;
            Ok(OfficialProcessOutput {
                exit_code: status.code().unwrap_or(-1),
                report_json,
            })
        })
    }
}

/// Official evaluator adapter that never interprets test output itself.
pub struct SweBenchOfficialEvaluator<'a> {
    config: OfficialEvaluatorConfig,
    process: &'a dyn OfficialProcess,
}

impl<'a> SweBenchOfficialEvaluator<'a> {
    /// Validates exact executable, path, run, model, jobs, timeout and environment values.
    pub fn new(
        config: OfficialEvaluatorConfig,
        process: &'a dyn OfficialProcess,
    ) -> Result<Self, BenchError> {
        validate_config(&config)?;
        Ok(Self { config, process })
    }
}

impl OfficialEvaluator for SweBenchOfficialEvaluator<'_> {
    fn evaluate<'a>(
        &'a self,
        case: &'a SweCase,
        patch: &'a str,
    ) -> BenchFuture<'a, EvaluationVerdict> {
        Box::pin(async move {
            if !safe_component(case.instance_id.as_str()) {
                return Err(invalid_evaluation());
            }
            let prediction = prediction(case, &self.config.model_name, patch)?;
            let invocation = invocation(&self.config, case, prediction);
            let output = self.process.invoke(invocation).await?;
            if output.exit_code != 0 {
                return Err(invalid_evaluation());
            }
            parse_single_report(&output.report_json, case.instance_id.as_str())
        })
    }
}

fn prediction(case: &SweCase, model: &str, patch: &str) -> Result<Vec<u8>, BenchError> {
    let mut bytes = serde_json::to_vec(&OfficialPrediction {
        instance_id: case.instance_id.as_str(),
        model_name_or_path: model,
        model_patch: patch,
    })
    .map_err(|_| invalid_evaluation())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn invocation(
    config: &OfficialEvaluatorConfig,
    case: &SweCase,
    prediction_jsonl: Vec<u8>,
) -> OfficialInvocation {
    let case_run_id = format!("{}-{}", config.run_id, case.instance_id.as_str());
    let prediction_path = format!(
        "{}/{}.jsonl",
        config.predictions_directory.trim_end_matches('/'),
        case.instance_id.as_str()
    );
    let report_path = format!(
        "{}/{}.{}.json",
        config.run_directory.trim_end_matches('/'),
        config.model_name,
        case_run_id
    );
    OfficialInvocation {
        executable: config.python_executable.clone(),
        arguments: vec![
            "-m".into(),
            "swebench.harness.run_evaluation".into(),
            "--dataset_name".into(),
            config.dataset.official_name().into(),
            "--split".into(),
            "test".into(),
            "--predictions_path".into(),
            prediction_path.clone(),
            "--instance_ids".into(),
            case.instance_id.as_str().into(),
            "--max_workers".into(),
            config.jobs.to_string(),
            "--run_id".into(),
            case_run_id,
            "--timeout".into(),
            config.timeout_seconds.to_string(),
            "--cache_level".into(),
            "env".into(),
            "--clean".into(),
            "true".into(),
        ],
        working_directory: config.run_directory.clone(),
        prediction_path,
        report_path,
        prediction_jsonl,
        environment: config.environment.clone(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OfficialReport {
    total_instances: u64,
    submitted_instances: u64,
    completed_instances: u64,
    resolved_instances: u64,
    unresolved_instances: u64,
    empty_patch_instances: u64,
    error_instances: u64,
    completed_ids: Vec<String>,
    incomplete_ids: Vec<String>,
    empty_patch_ids: Vec<String>,
    submitted_ids: Vec<String>,
    resolved_ids: Vec<String>,
    unresolved_ids: Vec<String>,
    error_ids: Vec<String>,
    schema_version: u64,
}

fn parse_single_report(bytes: &[u8], case_id: &str) -> Result<EvaluationVerdict, BenchError> {
    let report: OfficialReport = serde_json::from_slice(bytes).map_err(|_| invalid_evaluation())?;
    let resolved = report.resolved_ids == [case_id];
    let unresolved = report.unresolved_ids == [case_id];
    if report.schema_version != 2
        || report.total_instances != 1
        || report.submitted_instances != 1
        || report.completed_instances != 1
        || report.resolved_instances != u64::from(resolved)
        || report.unresolved_instances != u64::from(unresolved)
        || resolved == unresolved
        || report.completed_ids != [case_id]
        || report.submitted_ids != [case_id]
        || report.empty_patch_instances != 0
        || report.error_instances != 0
        || !report.incomplete_ids.is_empty()
        || !report.empty_patch_ids.is_empty()
        || !report.error_ids.is_empty()
    {
        return Err(invalid_evaluation());
    }
    Ok(if resolved {
        EvaluationVerdict::Passed
    } else {
        EvaluationVerdict::Failed
    })
}

fn validate_config(config: &OfficialEvaluatorConfig) -> Result<(), BenchError> {
    let text = [
        &config.python_executable,
        &config.predictions_directory,
        &config.run_directory,
        &config.run_id,
        &config.model_name,
    ];
    let environment_keys = config
        .environment
        .iter()
        .map(|(key, _)| key)
        .collect::<BTreeSet<_>>();
    if text
        .iter()
        .any(|value| value.is_empty() || value.len() > 1_024)
        || !(2..=64).contains(&config.jobs)
        || config.timeout_seconds == 0
        || !config
            .model_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || environment_keys.len() != config.environment.len()
        || config
            .environment
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains('=') || value.contains('\0'))
    {
        return Err(BenchError::new(BenchErrorCode::InvalidEvaluation));
    }
    Ok(())
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn invalid_evaluation() -> BenchError {
    BenchError::from_port(BenchErrorCode::InvalidEvaluation)
}
