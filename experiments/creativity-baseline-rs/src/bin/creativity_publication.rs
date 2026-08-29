use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    process::ExitCode,
};

use garive_creativity_baseline::{
    build_publication_evaluator, build_publication_generator, load_creativity_corpus,
    model_endpoint_publication_eligible, run_creativity_baseline, ModelEndpointConfig,
    PublicationModelCoordinate, SystemCredentialReferenceResolver,
};
use garive_eval::{CreativityAggregate, CreativityArmEvidence};
use garive_experiment_evidence::{attest_clean_revision, GitAttestationConfig};
use serde::Deserialize;
use serde_json::{json, Value};

const MAX_CONFIG_BYTES: u64 = 1_048_576;
const MAX_CORPUS_BYTES: u64 = 1_048_576;
const EVIDENCE_CONTRACT: &str = "garive.creativity-baseline-evidence";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunConfig {
    corpus_path: PathBuf,
    evidence_path: PathBuf,
    garive_revision: String,
    runner_revision: String,
    dirty: bool,
    seed: u64,
    git: GitAttestationConfig,
    generator: ModelEndpointConfig,
    evaluator: ModelEndpointConfig,
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
        return Err("usage: garive-creativity-publication run <explicit-config.json>");
    }
    let config: RunConfig = serde_json::from_slice(&bounded_read(
        &PathBuf::from(&arguments[2]),
        MAX_CONFIG_BYTES,
    )?)
    .map_err(|_| "invalid_configuration")?;
    if config.dirty
        || !identity(&config.garive_revision)
        || !identity(&config.runner_revision)
        || !model_endpoint_publication_eligible(&config.generator)
            .map_err(|_| "invalid_generator")?
        || !model_endpoint_publication_eligible(&config.evaluator)
            .map_err(|_| "invalid_evaluator")?
    {
        return Err("invalid_provenance");
    }
    let attestation = attest_clean_revision(&config.git, &config.garive_revision)
        .map_err(|_| "invalid_provenance")?;
    let corpus = load_creativity_corpus(&bounded_read(&config.corpus_path, MAX_CORPUS_BYTES)?)
        .map_err(|_| "invalid_corpus")?;
    let mut reservation = EvidenceReservation::new(config.evidence_path.clone())?;
    let resolver = SystemCredentialReferenceResolver;
    let (generator, generator_coordinate) =
        build_publication_generator(config.generator, &resolver)
            .map_err(|_| "generator_build_failed")?;
    let (evaluator, evaluator_coordinate) =
        build_publication_evaluator(config.evaluator, &resolver)
            .map_err(|_| "evaluator_build_failed")?;
    let run = run_creativity_baseline(&corpus, &generator, &evaluator, config.seed)
        .map_err(|error| error.code().wire_name())?;
    if !run.generator.publishable || !run.evaluator.publishable {
        return Err("invalid_provenance");
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
            json!({"class":class.task_class.wire_name(),"control":aggregate(&class.control),
                "bounded_alternatives":aggregate(&class.bounded_alternatives)})
        })
        .collect::<Vec<_>>();
    let evidence = json!({
        "contract":EVIDENCE_CONTRACT,"version":2,"publishable":true,
        "garive_revision":config.garive_revision,"runner_revision":config.runner_revision,
        "dirty":false,"seed":run.seed,
        "corpus_id":run.corpus_id,"corpus_revision":run.corpus_revision,
        "corpus_digest":run.corpus_digest,
        "generator":coordinate(&generator_coordinate),
        "evaluator":coordinate(&evaluator_coordinate),
        "git_attestation":{"executable_digest":attestation.executable_digest,
            "configuration_digest":attestation.configuration_digest},
        "pairs":pairs,"summary":{"control":aggregate(&run.summary.control),
            "bounded_alternatives":aggregate(&run.summary.bounded_alternatives),
            "classes":classes},
    });
    reservation.commit(&evidence)?;
    println!(
        "{}",
        json!({"task_count":run.summary.ordered_pairs.len(),
        "publishable":true,"evidence_path":config.evidence_path})
    );
    Ok(())
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

struct EvidenceReservation {
    path: PathBuf,
    file: Option<File>,
}

impl EvidenceReservation {
    fn new(path: PathBuf) -> Result<Self, &'static str> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| "evidence_create_failed")?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    fn commit(&mut self, evidence: &Value) -> Result<(), &'static str> {
        let bytes = serde_json::to_vec_pretty(evidence).map_err(|_| "evidence_encode_failed")?;
        let file = self.file.as_mut().ok_or("evidence_write_failed")?;
        file.write_all(&bytes)
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|_| "evidence_write_failed")?;
        self.file.take();
        Ok(())
    }
}

impl Drop for EvidenceReservation {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = fs::remove_file(&self.path);
        }
    }
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
