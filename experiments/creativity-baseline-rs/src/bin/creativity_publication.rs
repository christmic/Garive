use std::{fs, path::PathBuf, process::ExitCode};

use garive_creativity_baseline::{
    build_publication_evaluator, build_publication_generator, load_creativity_corpus,
    model_endpoint_publication_eligible, reserve_publication_evidence, run_creativity_baseline,
    ModelEndpointConfig, PublicationEvidenceProvenance, SystemCredentialReferenceResolver,
};
use garive_experiment_evidence::{attest_clean_revision, GitAttestationConfig};
use serde::Deserialize;
use serde_json::json;

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
    let mut reservation =
        reserve_publication_evidence(config.evidence_path.clone()).map_err(|error| error.code())?;
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
    reservation
        .commit(
            &run,
            &generator_coordinate,
            &evaluator_coordinate,
            PublicationEvidenceProvenance {
                garive_revision: config.garive_revision,
                runner_revision: config.runner_revision,
                git_attestation: attestation,
            },
        )
        .map_err(|error| error.code())?;
    println!(
        "{}",
        json!({"task_count":run.summary.ordered_pairs.len(),
        "publishable":true,"evidence_path":config.evidence_path})
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
