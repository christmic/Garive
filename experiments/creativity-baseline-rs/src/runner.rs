use std::collections::{BTreeMap, BTreeSet};

use garive_eval::{summarize_creativity, CreativityArm, CreativityArmEvidence, CreativitySummary};

use crate::{
    CandidateVerdict, CreativityBaselineError, CreativityBaselineErrorCode, CreativityCorpus,
    CreativityEvaluatorPort, CreativityGeneratorPort, EvaluatorRequest, ExperimentPortDescriptor,
    GeneratedArm, GeneratorRequest,
};

/// Complete deterministic non-secret CR-A paired run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreativityBaselineRun {
    /// Exact corpus identity.
    pub corpus_id: String,
    /// Exact corpus revision.
    pub corpus_revision: String,
    /// Canonical corpus SHA-256.
    pub corpus_digest: String,
    /// Frozen generator implementation/configuration.
    pub generator: ExperimentPortDescriptor,
    /// Frozen evaluator implementation/configuration.
    pub evaluator: ExperimentPortDescriptor,
    /// Exact deterministic run seed.
    pub seed: u64,
    /// Source-ordered paired reduction.
    pub summary: CreativitySummary,
}

/// Executes every task through adjacent control and alternatives arms exactly once.
pub fn run_creativity_baseline(
    corpus: &CreativityCorpus,
    generator: &dyn CreativityGeneratorPort,
    evaluator: &dyn CreativityEvaluatorPort,
    seed: u64,
) -> Result<CreativityBaselineRun, CreativityBaselineError> {
    validate_descriptor(generator.descriptor())?;
    validate_descriptor(evaluator.descriptor())?;
    let mut evidence = Vec::with_capacity(corpus.tasks.len() * CreativityArm::ALL.len());
    for task in &corpus.tasks {
        for arm in CreativityArm::ALL {
            let generated = generator
                .generate(GeneratorRequest {
                    task_id: &task.task_id,
                    arm,
                    prompt: &task.generator_prompt,
                    seed,
                    max_candidates: match arm {
                        CreativityArm::Control => 1,
                        CreativityArm::BoundedAlternatives => task.max_candidates,
                    },
                    max_candidate_utf8_bytes: task.max_candidate_utf8_bytes,
                    max_total_candidate_utf8_bytes: task.max_total_candidate_utf8_bytes,
                })
                .map_err(|_| error(CreativityBaselineErrorCode::GeneratorFailure))?;
            validate_generated(task, arm, &generated)?;
            let verdicts = evaluator
                .evaluate(EvaluatorRequest {
                    task_id: &task.task_id,
                    rubric_json: &task.evaluator_rubric_json,
                    candidates: &generated.candidates,
                })
                .map_err(|_| error(CreativityBaselineErrorCode::EvaluatorFailure))?;
            evidence.push(reduce_arm(task, arm, &generated, &verdicts)?);
        }
    }
    let summary = summarize_creativity(&evidence, corpus.tasks.len())
        .map_err(|_| error(CreativityBaselineErrorCode::ReductionFailure))?;
    Ok(CreativityBaselineRun {
        corpus_id: corpus.corpus_id.clone(),
        corpus_revision: corpus.corpus_revision.clone(),
        corpus_digest: corpus.canonical_digest.clone(),
        generator: generator.descriptor().clone(),
        evaluator: evaluator.descriptor().clone(),
        seed,
        summary,
    })
}

fn validate_generated(
    task: &crate::CreativityTask,
    arm: CreativityArm,
    generated: &GeneratedArm,
) -> Result<(), CreativityBaselineError> {
    let count = generated.candidates.len() as u64;
    let count_valid = match arm {
        CreativityArm::Control => count == 1,
        CreativityArm::BoundedAlternatives => (2..=task.max_candidates).contains(&count),
    };
    let mut identities = BTreeSet::new();
    let mut bytes = 0_usize;
    for candidate in &generated.candidates {
        if candidate.candidate_id.is_empty()
            || candidate.candidate_id.len() > 256
            || candidate.content.is_empty()
            || candidate.content.len() > task.max_candidate_utf8_bytes
            || !identities.insert(candidate.candidate_id.as_str())
        {
            return Err(error(CreativityBaselineErrorCode::GeneratorFailure));
        }
        bytes = bytes
            .checked_add(candidate.content.len())
            .ok_or_else(|| error(CreativityBaselineErrorCode::GeneratorFailure))?;
    }
    if !count_valid
        || bytes > task.max_total_candidate_utf8_bytes
        || !identities.contains(generated.selected_candidate_id.as_str())
    {
        return Err(error(CreativityBaselineErrorCode::GeneratorFailure));
    }
    Ok(())
}

fn reduce_arm(
    task: &crate::CreativityTask,
    arm: CreativityArm,
    generated: &GeneratedArm,
    verdicts: &[CandidateVerdict],
) -> Result<CreativityArmEvidence, CreativityBaselineError> {
    let candidates = generated
        .candidates
        .iter()
        .map(|value| value.candidate_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut by_id = BTreeMap::new();
    let mut correct = 0_u64;
    let mut clusters = BTreeSet::new();
    for verdict in verdicts {
        let cluster_shape = verdict.correct == verdict.correct_cluster_id.is_some();
        if !candidates.contains(verdict.candidate_id.as_str())
            || !cluster_shape
            || verdict
                .correct_cluster_id
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > 256)
            || by_id
                .insert(verdict.candidate_id.as_str(), verdict)
                .is_some()
        {
            return Err(error(CreativityBaselineErrorCode::EvaluatorFailure));
        }
        if verdict.correct {
            correct = correct
                .checked_add(1)
                .ok_or_else(|| error(CreativityBaselineErrorCode::ReductionFailure))?;
            clusters.insert(
                verdict
                    .correct_cluster_id
                    .as_deref()
                    .expect("shape checked"),
            );
        }
    }
    if by_id.len() != candidates.len() {
        return Err(error(CreativityBaselineErrorCode::EvaluatorFailure));
    }
    let selected_correct = by_id
        .get(generated.selected_candidate_id.as_str())
        .expect("coverage checked")
        .correct;
    CreativityArmEvidence::new(
        task.task_id.clone(),
        task.task_class,
        arm,
        u64::try_from(generated.candidates.len())
            .map_err(|_| error(CreativityBaselineErrorCode::ReductionFailure))?,
        correct,
        u64::try_from(clusters.len())
            .map_err(|_| error(CreativityBaselineErrorCode::ReductionFailure))?,
        selected_correct,
    )
    .map_err(|_| error(CreativityBaselineErrorCode::ReductionFailure))
}

fn validate_descriptor(value: &ExperimentPortDescriptor) -> Result<(), CreativityBaselineError> {
    ExperimentPortDescriptor::new(
        value.implementation_id.clone(),
        value.implementation_revision.clone(),
        value.config_digest.clone(),
        value.publishable,
    )
    .ok_or_else(|| error(CreativityBaselineErrorCode::InvalidPort))?;
    Ok(())
}

const fn error(code: CreativityBaselineErrorCode) -> CreativityBaselineError {
    CreativityBaselineError::new(code)
}
