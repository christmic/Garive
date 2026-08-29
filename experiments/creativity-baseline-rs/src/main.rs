use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
};

use garive_creativity_baseline::{
    load_creativity_corpus, run_creativity_baseline, CommandCreativityEvaluator,
    CommandCreativityGenerator, CommandPortConfig,
};
use garive_eval::{CreativityAggregate, CreativityArmEvidence};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_CONFIG_BYTES: u64 = 1_048_576;
const MAX_CORPUS_BYTES: u64 = 1_048_576;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    corpus_path: PathBuf,
    evidence_path: PathBuf,
    garive_revision: String,
    runner_revision: String,
    dirty: bool,
    seed: u64,
    generator: CommandDocument,
    evaluator: CommandDocument,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandDocument {
    implementation_id: String,
    implementation_revision: String,
    publishable: bool,
    executable: PathBuf,
    argv: Vec<String>,
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
}

impl From<CommandDocument> for CommandPortConfig {
    fn from(value: CommandDocument) -> Self {
        Self {
            implementation_id: value.implementation_id,
            implementation_revision: value.implementation_revision,
            publishable: value.publishable,
            executable: value.executable,
            argv: value.argv,
            cwd: value.cwd,
            environment: value.environment,
            timeout_ms: value.timeout_ms,
            max_stdout_bytes: value.max_stdout_bytes,
            max_stderr_bytes: value.max_stderr_bytes,
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
        return Err("usage: garive-creativity-baseline run <explicit-config.json>");
    }
    let config: RunConfig = serde_json::from_slice(&bounded_read(
        &PathBuf::from(&arguments[2]),
        MAX_CONFIG_BYTES,
    )?)
    .map_err(|_| "invalid_configuration")?;
    if !identity(&config.garive_revision) || !identity(&config.runner_revision) {
        return Err("invalid_provenance");
    }
    let corpus = load_creativity_corpus(&bounded_read(&config.corpus_path, MAX_CORPUS_BYTES)?)
        .map_err(|_| "invalid_corpus")?;
    let generator = CommandCreativityGenerator::new(config.generator.into())
        .map_err(|_| "invalid_generator")?;
    let evaluator = CommandCreativityEvaluator::new(config.evaluator.into())
        .map_err(|_| "invalid_evaluator")?;
    let run = run_creativity_baseline(&corpus, &generator, &evaluator, config.seed)
        .map_err(|error| error.code().wire_name())?;
    let pairs = run
        .summary
        .ordered_pairs
        .iter()
        .map(|pair| {
            json!({
                "task_id":pair.task_id.as_str(),
                "class":pair.task_class.wire_name(),
                "control":arm(&pair.control),
                "bounded_alternatives":arm(&pair.bounded_alternatives),
            })
        })
        .collect::<Vec<_>>();
    let classes = run
        .summary
        .classes
        .iter()
        .map(|class| {
            json!({
                "class":class.task_class.wire_name(),
                "control":aggregate(&class.control),
                "bounded_alternatives":aggregate(&class.bounded_alternatives),
            })
        })
        .collect::<Vec<_>>();
    let evidence = json!({
        "contract":"garive.creativity-baseline-evidence","version":1,
        "maturity":"non_publication_prerequisite","publishable":false,
        "garive_revision":config.garive_revision,"runner_revision":config.runner_revision,
        "dirty":config.dirty,"seed":run.seed,
        "corpus_id":run.corpus_id,"corpus_revision":run.corpus_revision,
        "corpus_digest":run.corpus_digest,
        "generator":descriptor(&run.generator),"evaluator":descriptor(&run.evaluator),
        "pairs":pairs,
        "summary":{
            "control":aggregate(&run.summary.control),
            "bounded_alternatives":aggregate(&run.summary.bounded_alternatives),
            "classes":classes,
        },
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
        json!({"task_count":run.summary.ordered_pairs.len(),
            "publishable":false,"evidence_path":config.evidence_path})
    );
    Ok(())
}

fn descriptor(value: &garive_creativity_baseline::ExperimentPortDescriptor) -> Value {
    json!({
        "implementation_id":value.implementation_id,
        "implementation_revision":value.implementation_revision,
        "config_digest":value.config_digest,
        "publishable":value.publishable,
    })
}

fn arm(value: &CreativityArmEvidence) -> Value {
    json!({
        "candidate_count":value.candidate_count,
        "correct_candidate_count":value.correct_candidate_count,
        "distinct_correct_cluster_count":value.distinct_correct_cluster_count,
        "selected_correct":value.selected_correct,
    })
}

fn aggregate(value: &CreativityAggregate) -> Value {
    json!({
        "task_count":value.task_count,"candidate_count":value.candidate_count,
        "correct_candidate_count":value.correct_candidate_count,
        "correct_cluster_mean_numerator":value.correct_cluster_mean_numerator,
        "correct_cluster_mean_denominator":value.correct_cluster_mean_denominator,
        "selected_correct_numerator":value.selected_correct_numerator,
        "selected_correct_denominator":value.selected_correct_denominator,
    })
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
