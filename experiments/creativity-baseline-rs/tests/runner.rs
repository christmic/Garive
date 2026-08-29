use std::cell::RefCell;

use garive_creativity_baseline::{
    load_creativity_corpus, run_creativity_baseline, CandidateVerdict, CreativityBaselineError,
    CreativityBaselineErrorCode, CreativityEvaluatorPort, CreativityGeneratorPort,
    EvaluatorRequest, ExperimentPortDescriptor, GeneratedArm, GeneratedCandidate, GeneratorRequest,
};
use garive_eval::CreativityArm;

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/eval/creativity-corpus-v1.json"
));

fn descriptor(id: &str) -> ExperimentPortDescriptor {
    ExperimentPortDescriptor::new(id, "v1", "a".repeat(64), false).unwrap()
}

type GeneratorCall = (String, CreativityArm, String, u64, u64);

struct Generator {
    descriptor: ExperimentPortDescriptor,
    calls: RefCell<Vec<GeneratorCall>>,
    invalid: bool,
}

impl CreativityGeneratorPort for Generator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.descriptor
    }

    fn generate(
        &self,
        request: GeneratorRequest<'_>,
    ) -> Result<GeneratedArm, CreativityBaselineError> {
        self.calls.borrow_mut().push((
            request.task_id.as_str().into(),
            request.arm,
            request.prompt.into(),
            request.seed,
            request.max_candidates,
        ));
        let count = match request.arm {
            CreativityArm::Control => 1,
            CreativityArm::BoundedAlternatives => 3,
        };
        let mut candidates = (0..count)
            .map(|index| GeneratedCandidate {
                candidate_id: format!("candidate-{index}"),
                content: format!("bounded candidate {index}"),
            })
            .collect::<Vec<_>>();
        if self.invalid {
            candidates[0].candidate_id.clear();
        }
        Ok(GeneratedArm {
            selected_candidate_id: "candidate-0".into(),
            candidates,
        })
    }
}

struct Evaluator {
    descriptor: ExperimentPortDescriptor,
    calls: RefCell<Vec<(String, String, Vec<String>)>>,
    invalid: bool,
}

impl CreativityEvaluatorPort for Evaluator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.descriptor
    }

    fn evaluate(
        &self,
        request: EvaluatorRequest<'_>,
    ) -> Result<Vec<CandidateVerdict>, CreativityBaselineError> {
        self.calls.borrow_mut().push((
            request.task_id.as_str().into(),
            request.rubric_json.into(),
            request
                .candidates
                .iter()
                .map(|value| value.candidate_id.clone())
                .collect(),
        ));
        let mut verdicts = request
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| CandidateVerdict {
                candidate_id: candidate.candidate_id.clone(),
                correct: index < 2,
                correct_cluster_id: (index < 2).then(|| format!("cluster-{index}")),
            })
            .collect::<Vec<_>>();
        if self.invalid {
            verdicts.pop();
        }
        Ok(verdicts)
    }
}

fn ports(invalid_generator: bool, invalid_evaluator: bool) -> (Generator, Evaluator) {
    (
        Generator {
            descriptor: descriptor("fixture-generator"),
            calls: RefCell::new(Vec::new()),
            invalid: invalid_generator,
        },
        Evaluator {
            descriptor: descriptor("fixture-evaluator"),
            calls: RefCell::new(Vec::new()),
            invalid: invalid_evaluator,
        },
    )
}

#[test]
fn sole_paired_route_is_bounded_blind_and_source_ordered() {
    let corpus = load_creativity_corpus(CORPUS.as_bytes()).unwrap();
    let (generator, evaluator) = ports(false, false);
    let run = run_creativity_baseline(&corpus, &generator, &evaluator, 42).unwrap();
    assert_eq!(run.summary.ordered_pairs.len(), 4);
    assert_eq!(run.summary.control.candidate_count, 4);
    assert_eq!(run.summary.bounded_alternatives.candidate_count, 12);
    assert_eq!(
        run.summary
            .bounded_alternatives
            .correct_cluster_mean_numerator,
        8
    );
    assert_eq!(generator.calls.borrow().len(), 8);
    assert_eq!(evaluator.calls.borrow().len(), 8);
    assert!(generator
        .calls
        .borrow()
        .iter()
        .all(|(_, arm, prompt, seed, maximum)| {
            *seed == 42
                && !prompt.contains("required_constraints")
                && *maximum == if *arm == CreativityArm::Control { 1 } else { 4 }
        }));
    assert!(evaluator
        .calls
        .borrow()
        .iter()
        .all(
            |(_, rubric, candidates)| rubric.contains("required_constraints")
                && !candidates.is_empty()
        ));
}

#[test]
fn invalid_generator_or_evaluator_stops_without_retry() {
    let corpus = load_creativity_corpus(CORPUS.as_bytes()).unwrap();
    let (generator, evaluator) = ports(true, false);
    assert_eq!(
        run_creativity_baseline(&corpus, &generator, &evaluator, 1)
            .unwrap_err()
            .code(),
        CreativityBaselineErrorCode::GeneratorFailure
    );
    assert_eq!(generator.calls.borrow().len(), 1);
    assert!(evaluator.calls.borrow().is_empty());

    let (generator, evaluator) = ports(false, true);
    assert_eq!(
        run_creativity_baseline(&corpus, &generator, &evaluator, 1)
            .unwrap_err()
            .code(),
        CreativityBaselineErrorCode::EvaluatorFailure
    );
    assert_eq!(generator.calls.borrow().len(), 1);
    assert_eq!(evaluator.calls.borrow().len(), 1);
}
