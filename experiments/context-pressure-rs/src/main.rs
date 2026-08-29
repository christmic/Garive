use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
};

use garive_context_pressure::{
    attest_clean_revision, build_publication_provider_counter, load_corpus,
    measure_context_pressure, CommandTokenCounter, CommandTokenCounterConfig, GitAttestationConfig,
    ProviderCounterRunConfig, SystemCredentialReferenceResolver, TokenCounter,
};
use serde::Deserialize;
use serde_json::json;

const MAX_CONFIG_BYTES: u64 = 1_048_576;
const MAX_CORPUS_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    corpus_path: PathBuf,
    evidence_path: PathBuf,
    garive_revision: String,
    runner_revision: String,
    dirty: bool,
    #[serde(default)]
    git: Option<GitAttestationConfig>,
    counter: CounterDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandCounterDocument {
    counter_id: String,
    counter_revision: String,
    publishable: bool,
    executable: PathBuf,
    argv: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CounterDocument {
    Command(CommandCounterDocument),
    AnthropicMessagesExact(ProviderCounterRunConfig),
}

impl CounterDocument {
    fn publication_requested(&self) -> Result<bool, &'static str> {
        match self {
            Self::Command(value) if value.publishable => Err("invalid_counter"),
            Self::Command(_) => Ok(false),
            Self::AnthropicMessagesExact(value) => Ok(value.publication_requested()),
        }
    }
}

fn main() -> ExitCode {
    match execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("{}", json!({"error":code}));
            ExitCode::from(2)
        }
    }
}

fn execute() -> Result<(), &'static str> {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.len() != 3 || arguments[1] != "run" {
        return Err("usage: garive-context-pressure run <explicit-config.json>");
    }
    let config_bytes = bounded_read(&PathBuf::from(&arguments[2]), MAX_CONFIG_BYTES)?;
    let config: RunConfig =
        serde_json::from_slice(&config_bytes).map_err(|_| "invalid_configuration")?;
    if !identity(&config.garive_revision) || !identity(&config.runner_revision) {
        return Err("invalid_provenance");
    }
    let publication_requested = config.counter.publication_requested()?;
    if publication_requested {
        if config.dirty {
            return Err("invalid_provenance");
        }
        attest_clean_revision(
            config.git.as_ref().ok_or("invalid_provenance")?,
            &config.garive_revision,
        )
        .map_err(|_| "invalid_provenance")?;
    }
    let corpus_bytes = bounded_read(&config.corpus_path, MAX_CORPUS_BYTES)?;
    let corpus = load_corpus(&corpus_bytes).map_err(|_| "invalid_corpus")?;
    let counter: Box<dyn TokenCounter> = match config.counter {
        CounterDocument::Command(value) => Box::new(
            CommandTokenCounter::new(CommandTokenCounterConfig {
                counter_id: value.counter_id,
                counter_revision: value.counter_revision,
                publishable: value.publishable,
                executable: value.executable,
                argv: value.argv,
                cwd: value.cwd,
                environment: value.environment,
                timeout_ms: value.timeout_ms,
                max_stdout_bytes: value.max_stdout_bytes,
                max_stderr_bytes: value.max_stderr_bytes,
            })
            .map_err(|_| "invalid_counter")?,
        ),
        CounterDocument::AnthropicMessagesExact(value) => Box::new(
            build_publication_provider_counter(value, &SystemCredentialReferenceResolver)
                .map_err(|error| error.code())?,
        ),
    };
    let run =
        measure_context_pressure(&corpus, counter.as_ref()).map_err(|_| "measurement_failed")?;
    let cases = run
        .summary
        .ordered_cases
        .iter()
        .map(|value| {
            json!({
                "case_id":value.case_id.as_str(),
                "workload_class":value.workload_class.wire_name(),
                "item_count":value.item_count,"utf8_bytes":value.utf8_bytes,
                "input_tokens":value.input_tokens,
                "model_input_limit_tokens":value.model_input_limit_tokens,
                "pressure_basis_points":value.pressure_basis_points,
            })
        })
        .collect::<Vec<_>>();
    let classes = run
        .summary
        .classes
        .iter()
        .map(|value| {
            json!({
                "workload_class":value.workload_class.wire_name(),
                "case_count":value.case_count,
                "max_pressure_basis_points":value.max_pressure_basis_points,
                "mean_pressure_numerator":value.mean_pressure_numerator,
                "mean_pressure_denominator":value.mean_pressure_denominator,
            })
        })
        .collect::<Vec<_>>();
    let evidence = json!({
        "contract":"garive.context-pressure-evidence","version":1,
        "garive_revision":config.garive_revision,"runner_revision":config.runner_revision,
        "dirty":config.dirty,"publishable":run.counter.publishable && !config.dirty,
        "corpus_id":run.corpus_id,"corpus_revision":run.corpus_revision,
        "corpus_digest":run.corpus_digest,
        "counter_id":run.counter.counter_id,"counter_revision":run.counter.counter_revision,
        "counter_config_digest":run.counter.config_digest,
        "cases":cases,"classes":classes,
    });
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config.evidence_path)
        .map_err(|_| "evidence_create_failed")?;
    let bytes = serde_json::to_vec_pretty(&evidence).map_err(|_| "evidence_encode_failed")?;
    output
        .write_all(&bytes)
        .and_then(|_| output.write_all(b"\n"))
        .map_err(|_| "evidence_write_failed")?;
    println!(
        "{}",
        json!({"case_count":run.summary.ordered_cases.len(),
            "publishable":run.counter.publishable && !config.dirty,
            "evidence_path":config.evidence_path})
    );
    Ok(())
}

fn bounded_read(path: &PathBuf, maximum: u64) -> Result<Vec<u8>, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "input_read_failed")?;
    if metadata.len() == 0 || metadata.len() > maximum {
        return Err("input_size_invalid");
    }
    fs::read(path).map_err(|_| "input_read_failed")
}

fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256
}
