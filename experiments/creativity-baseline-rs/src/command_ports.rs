use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    command_process::CommandProcess, CandidateVerdict, CommandPortConfig, CreativityBaselineError,
    CreativityBaselineErrorCode, CreativityEvaluatorPort, CreativityGeneratorPort,
    EvaluatorRequest, ExperimentPortDescriptor, GeneratedArm, GeneratedCandidate, GeneratorRequest,
};

/// Strict command-backed one-attempt creativity generator.
pub struct CommandCreativityGenerator {
    process: CommandProcess,
}

impl CommandCreativityGenerator {
    /// Constructs a permanently non-publishable bounded command generator.
    pub fn new(config: CommandPortConfig) -> Result<Self, CreativityBaselineError> {
        Ok(Self {
            process: CommandProcess::new("generator", config)?,
        })
    }
}

impl CreativityGeneratorPort for CommandCreativityGenerator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        self.process.descriptor()
    }

    fn generate(
        &self,
        request: GeneratorRequest<'_>,
    ) -> Result<GeneratedArm, CreativityBaselineError> {
        let response: GeneratorResponse =
            serde_json::from_slice(&self.process.execute(&GeneratorWireRequest {
                schema_version: 1,
                task_id: request.task_id.as_str(),
                arm: request.arm.wire_name(),
                prompt: request.prompt,
                seed: request.seed,
                max_candidates: request.max_candidates,
                max_candidate_utf8_bytes: request.max_candidate_utf8_bytes,
                max_total_candidate_utf8_bytes: request.max_total_candidate_utf8_bytes,
            })?)
            .map_err(|_| error(CreativityBaselineErrorCode::GeneratorFailure))?;
        if response.schema_version != 1 {
            return Err(error(CreativityBaselineErrorCode::GeneratorFailure));
        }
        Ok(GeneratedArm {
            candidates: response
                .candidates
                .into_iter()
                .map(|value| GeneratedCandidate {
                    candidate_id: value.candidate_id,
                    content: value.content,
                })
                .collect(),
            selected_candidate_id: response.selected_candidate_id,
        })
    }
}

/// Strict command-backed one-attempt blind creativity evaluator.
pub struct CommandCreativityEvaluator {
    process: CommandProcess,
}

impl CommandCreativityEvaluator {
    /// Constructs a permanently non-publishable bounded command evaluator.
    pub fn new(config: CommandPortConfig) -> Result<Self, CreativityBaselineError> {
        Ok(Self {
            process: CommandProcess::new("evaluator", config)?,
        })
    }
}

impl CreativityEvaluatorPort for CommandCreativityEvaluator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        self.process.descriptor()
    }

    fn evaluate(
        &self,
        request: EvaluatorRequest<'_>,
    ) -> Result<Vec<CandidateVerdict>, CreativityBaselineError> {
        let rubric: Value = serde_json::from_str(request.rubric_json)
            .map_err(|_| error(CreativityBaselineErrorCode::EvaluatorFailure))?;
        let candidates = request
            .candidates
            .iter()
            .map(|value| CandidateWireRequest {
                candidate_id: &value.candidate_id,
                content: &value.content,
            })
            .collect::<Vec<_>>();
        let response: EvaluatorResponse =
            serde_json::from_slice(&self.process.execute(&EvaluatorWireRequest {
                schema_version: 1,
                task_id: request.task_id.as_str(),
                evaluator_rubric: &rubric,
                candidates,
            })?)
            .map_err(|_| error(CreativityBaselineErrorCode::EvaluatorFailure))?;
        if response.schema_version != 1 {
            return Err(error(CreativityBaselineErrorCode::EvaluatorFailure));
        }
        Ok(response
            .verdicts
            .into_iter()
            .map(|value| CandidateVerdict {
                candidate_id: value.candidate_id,
                correct: value.correct,
                correct_cluster_id: value.correct_cluster_id,
            })
            .collect())
    }
}

#[derive(Serialize)]
struct GeneratorWireRequest<'a> {
    schema_version: u32,
    task_id: &'a str,
    arm: &'static str,
    prompt: &'a str,
    seed: u64,
    max_candidates: u64,
    max_candidate_utf8_bytes: usize,
    max_total_candidate_utf8_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratorResponse {
    schema_version: u32,
    candidates: Vec<CandidateResponse>,
    selected_candidate_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateResponse {
    candidate_id: String,
    content: String,
}

#[derive(Serialize)]
struct EvaluatorWireRequest<'a> {
    schema_version: u32,
    task_id: &'a str,
    evaluator_rubric: &'a Value,
    candidates: Vec<CandidateWireRequest<'a>>,
}

#[derive(Serialize)]
struct CandidateWireRequest<'a> {
    candidate_id: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EvaluatorResponse {
    schema_version: u32,
    verdicts: Vec<VerdictResponse>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerdictResponse {
    candidate_id: String,
    correct: bool,
    correct_cluster_id: Option<String>,
}

const fn error(code: CreativityBaselineErrorCode) -> CreativityBaselineError {
    CreativityBaselineError::new(code)
}
