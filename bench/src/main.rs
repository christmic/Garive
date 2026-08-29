use std::{fs::OpenOptions, process::ExitCode, time::Instant};

use bench::{
    parse_cases, parse_explicit_config, run_benchmark, BenchmarkMode, BenchmarkRunConfig,
    CaseLoadLimits, CommandAgentDriver, CommandEnvironmentPool, CommandPortConfig, ExactSweIntake,
    JsonlResultSink, OfficialEvaluatorConfig, RunnerPorts, StdOfficialProcess,
    SweBenchOfficialEvaluator, SweDataset, TrackingCompletion, TrackingDescriptor,
    UnifiedDiffPatchAdapter,
};
use garive_eval::{EvaluationCaseOutcome, EvaluationLimits};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunFileConfig {
    cases_path: String,
    tracking_path: String,
    dataset: String,
    mode: String,
    jobs: usize,
    warm_capacity: usize,
    case_limits: RawCaseLimits,
    max_case_duration_ms: u64,
    max_patch_bytes: usize,
    environment_port: CommandPortConfig,
    agent_port: CommandPortConfig,
    official: RawOfficial,
    tracking: RawTracking,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCaseLimits {
    max_cases: usize,
    max_document_bytes: usize,
    max_line_bytes: usize,
    max_problem_bytes: usize,
    max_tests_per_group: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOfficial {
    python_executable: String,
    predictions_directory: String,
    run_directory: String,
    run_id: String,
    model_name: String,
    timeout_seconds: u64,
    environment: Vec<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTracking {
    suite_id: String,
    dataset_revision: String,
    harness_revision: String,
    agent_revision: String,
    dirty: bool,
    intake_adapter: String,
    patch_adapter: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    match execute().await {
        Ok(has_infrastructure_failure) if has_infrastructure_failure => ExitCode::from(2),
        Ok(_) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("{}", json!({"error": code}));
            ExitCode::from(2)
        }
    }
}

async fn execute() -> Result<bool, &'static str> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.len() != 3 || arguments[1] != "run" {
        return Err("usage: bench run <explicit-config.json>");
    }
    let config_bytes = bounded_read(&arguments[2], 1_048_576)?;
    let (config, config_digest): (RunFileConfig, String) =
        parse_explicit_config(&config_bytes).map_err(|error| error.code().wire_name())?;
    let dataset = match config.dataset.as_str() {
        "swe-bench-lite" => SweDataset::Lite,
        "swe-bench-verified" => SweDataset::Verified,
        _ => return Err("invalid_configuration"),
    };
    let mode = match config.mode.as_str() {
        "development" => BenchmarkMode::Development,
        "official-published" => BenchmarkMode::OfficialPublished,
        _ => return Err("invalid_configuration"),
    };
    if !config
        .tracking
        .dataset_revision
        .contains(dataset.official_name())
    {
        return Err("invalid_configuration");
    }
    let case_bytes = bounded_read(&config.cases_path, config.case_limits.max_document_bytes)?;
    let cases = parse_cases(
        &case_bytes,
        CaseLoadLimits {
            max_cases: config.case_limits.max_cases,
            max_document_bytes: config.case_limits.max_document_bytes,
            max_line_bytes: config.case_limits.max_line_bytes,
            max_problem_bytes: config.case_limits.max_problem_bytes,
            max_tests_per_group: config.case_limits.max_tests_per_group,
        },
    )
    .map_err(|error| error.code().wire_name())?;
    let environment = CommandEnvironmentPool::new(config.environment_port, config.warm_capacity)
        .map_err(|error| error.code().wire_name())?;
    let agent =
        CommandAgentDriver::new(config.agent_port).map_err(|error| error.code().wire_name())?;
    let patch = UnifiedDiffPatchAdapter::new(config.max_patch_bytes)
        .map_err(|error| error.code().wire_name())?;
    let official_process = StdOfficialProcess;
    let evaluator = SweBenchOfficialEvaluator::new(
        OfficialEvaluatorConfig {
            python_executable: config.official.python_executable,
            dataset,
            predictions_directory: config.official.predictions_directory,
            run_directory: config.official.run_directory,
            run_id: config.official.run_id.clone(),
            model_name: config.official.model_name,
            jobs: config.jobs,
            timeout_seconds: config.official.timeout_seconds,
            environment: config.official.environment,
        },
        &official_process,
    )
    .map_err(|error| error.code().wire_name())?;
    let tracking_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.tracking_path)
        .map_err(|_| "invalid_tracking")?;
    let sink = JsonlResultSink::new(
        tracking_file,
        TrackingDescriptor {
            run_id: config.official.run_id,
            suite_id: config.tracking.suite_id,
            dataset_revision: config.tracking.dataset_revision,
            harness_revision: config.tracking.harness_revision,
            agent_revision: config.tracking.agent_revision,
            dirty: config.tracking.dirty,
            config_digest,
            intake_adapter: config.tracking.intake_adapter,
            patch_adapter: config.tracking.patch_adapter,
            environment_kind: if mode == BenchmarkMode::OfficialPublished {
                "official".into()
            } else {
                "self-cow".into()
            },
            jobs: config.jobs,
            case_count: cases.len(),
            publishable: mode == BenchmarkMode::OfficialPublished,
        },
    )
    .map_err(|error| error.code().wire_name())?;
    let started = Instant::now();
    let results = run_benchmark(
        &cases,
        BenchmarkRunConfig {
            jobs: config.jobs,
            mode,
        },
        RunnerPorts {
            environments: &environment,
            intake: &ExactSweIntake,
            agent: &agent,
            patch: &patch,
            evaluator: &evaluator,
            results: &sink,
        },
    )
    .await
    .map_err(|error| error.code().wire_name())?;
    let completion = sink
        .finish(
            &results,
            started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            EvaluationLimits {
                max_cases: cases.len(),
                max_case_duration_ms: config.max_case_duration_ms,
            },
        )
        .map_err(|error| error.code().wire_name())?;
    let summary = match completion {
        TrackingCompletion::Development(summary) => summary,
        TrackingCompletion::Published(baseline) => baseline.summary,
    };
    println!(
        "{}",
        json!({
            "attempted": summary.attempted, "passed": summary.passed, "failed": summary.failed,
            "infrastructure_failed": summary.infrastructure_failed,
            "score_numerator": summary.score.map(|value| value.numerator),
            "score_denominator": summary.score.map(|value| value.denominator),
        })
    );
    eprintln!(
        "SWE-bench: {}/{} resolved; {} infrastructure failures",
        summary.passed, summary.attempted, summary.infrastructure_failed
    );
    Ok(results
        .iter()
        .any(|result| result.outcome == EvaluationCaseOutcome::InfrastructureFailure))
}

fn bounded_read(path: &str, maximum: usize) -> Result<Vec<u8>, &'static str> {
    if path.is_empty() || maximum == 0 {
        return Err("invalid_configuration");
    }
    let metadata = std::fs::metadata(path).map_err(|_| "invalid_configuration")?;
    if metadata.len() > maximum as u64 {
        return Err("document_too_large");
    }
    std::fs::read(path).map_err(|_| "invalid_configuration")
}
