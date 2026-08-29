use std::fs;

use garive_creativity_baseline::{
    load_creativity_corpus, reserve_publication_evidence, run_creativity_baseline,
    CandidateVerdict, CreativityBaselineError, CreativityEvaluatorPort, CreativityGeneratorPort,
    EvaluatorRequest, ExperimentPortDescriptor, GeneratedArm, GeneratedCandidate, GeneratorRequest,
    ModelProtocol, PublicationEvidenceProvenance, PublicationModelCoordinate,
};
use garive_eval::CreativityArm;
use garive_experiment_evidence::GitAttestationDescriptor;
use serde_json::Value;
use tempfile::tempdir;

const CORPUS: &[u8] = include_bytes!("../../../spec/fixtures/eval/creativity-corpus-v1.json");

#[test]
fn valid_publication_is_content_free_v2_and_never_overwrites() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("evidence.json");
    let generator = Generator(descriptor("generator"));
    let evaluator = Evaluator(descriptor("evaluator"));
    let run = run_creativity_baseline(
        &load_creativity_corpus(CORPUS).unwrap(),
        &generator,
        &evaluator,
        99,
    )
    .unwrap();
    let generator_coordinate = coordinate(ModelProtocol::ResponsesCompatible, &generator.0);
    let evaluator_coordinate = coordinate(ModelProtocol::MessagesCompatible, &evaluator.0);
    let mut reservation = reserve_publication_evidence(path.clone()).unwrap();
    reservation
        .commit(
            &run,
            &generator_coordinate,
            &evaluator_coordinate,
            provenance(),
        )
        .unwrap();
    let before = fs::read(&path).unwrap();
    let document: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(document["contract"], "garive.creativity-baseline-evidence");
    assert_eq!(document["version"], 2);
    assert_eq!(document["publishable"], true);
    assert_eq!(document["pairs"].as_array().unwrap().len(), 4);
    assert_eq!(document["generator"]["protocol"], "responses_compatible");
    assert_eq!(document["evaluator"]["protocol"], "messages_compatible");
    let text = String::from_utf8(before.clone()).unwrap();
    for forbidden in [
        "generator_prompt",
        "evaluator_rubric",
        "candidate-0",
        "candidate content",
        "credential",
        "authorization",
        "selected_candidate_id",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    assert!(reserve_publication_evidence(path.clone()).is_err());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn abandoned_or_invalid_reservations_leave_no_evidence_document() {
    let directory = tempdir().unwrap();
    let abandoned = directory.path().join("abandoned.json");
    drop(reserve_publication_evidence(abandoned.clone()).unwrap());
    assert!(!abandoned.exists());

    let invalid = directory.path().join("invalid.json");
    let generator = Generator(descriptor("generator"));
    let evaluator = Evaluator(descriptor("evaluator"));
    let run = run_creativity_baseline(
        &load_creativity_corpus(CORPUS).unwrap(),
        &generator,
        &evaluator,
        1,
    )
    .unwrap();
    let mut mismatched = descriptor("other");
    mismatched.publishable = false;
    let mut reservation = reserve_publication_evidence(invalid.clone()).unwrap();
    assert!(reservation
        .commit(
            &run,
            &coordinate(ModelProtocol::ResponsesCompatible, &mismatched),
            &coordinate(ModelProtocol::MessagesCompatible, &evaluator.0),
            provenance(),
        )
        .is_err());
    drop(reservation);
    assert!(!invalid.exists());
}

fn descriptor(kind: &str) -> ExperimentPortDescriptor {
    ExperimentPortDescriptor::new(kind, "template-v1", digest(kind), true).unwrap()
}

fn digest(seed: &str) -> String {
    let byte = if seed == "generator" { 'a' } else { 'b' };
    byte.to_string().repeat(64)
}

fn coordinate(
    protocol: ModelProtocol,
    port: &ExperimentPortDescriptor,
) -> PublicationModelCoordinate {
    PublicationModelCoordinate {
        protocol,
        target_id: format!("{protocol:?}-target"),
        model_id: "model".into(),
        model_revision: "model-revision".into(),
        port: port.clone(),
    }
}

fn provenance() -> PublicationEvidenceProvenance {
    PublicationEvidenceProvenance {
        garive_revision: "1".repeat(40),
        runner_revision: "cr-b-v1".into(),
        git_attestation: GitAttestationDescriptor {
            executable_digest: "c".repeat(64),
            configuration_digest: "d".repeat(64),
        },
    }
}

struct Generator(ExperimentPortDescriptor);
impl CreativityGeneratorPort for Generator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.0
    }

    fn generate(
        &self,
        request: GeneratorRequest<'_>,
    ) -> Result<GeneratedArm, CreativityBaselineError> {
        let count = if request.arm == CreativityArm::Control {
            1
        } else {
            2
        };
        Ok(GeneratedArm {
            candidates: (0..count)
                .map(|index| GeneratedCandidate {
                    candidate_id: format!("candidate-{index}"),
                    content: format!("candidate content {index}"),
                })
                .collect(),
            selected_candidate_id: "candidate-0".into(),
        })
    }
}

struct Evaluator(ExperimentPortDescriptor);
impl CreativityEvaluatorPort for Evaluator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.0
    }

    fn evaluate(
        &self,
        request: EvaluatorRequest<'_>,
    ) -> Result<Vec<CandidateVerdict>, CreativityBaselineError> {
        Ok(request
            .candidates
            .iter()
            .map(|candidate| CandidateVerdict {
                candidate_id: candidate.candidate_id.clone(),
                correct: true,
                correct_cluster_id: Some(format!("cluster-{}", candidate.candidate_id)),
            })
            .collect())
    }
}
