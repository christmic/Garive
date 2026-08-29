use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelCapability, ModelInputContent, ModelInputItem,
    ModelItem, ModelObserver, ModelOutputSettings, ModelPort, ModelRequest, ModelRequestId,
    ModelRole, ModelStopReason, ModelStreamEvent, ModelTargetId, ObserverDecision, TextMode,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    CandidateVerdict, CreativityBaselineError, CreativityBaselineErrorCode,
    CreativityEvaluatorPort, CreativityGeneratorPort, EvaluatorRequest, ExperimentPortDescriptor,
    GeneratedArm, GeneratedCandidate, GeneratorRequest,
};

/// Frozen generator request-template revision.
pub const GENERATOR_TEMPLATE_REVISION: &str = "creativity-generator-json-v1";
/// Frozen blind-evaluator request-template revision.
pub const EVALUATOR_TEMPLATE_REVISION: &str = "creativity-evaluator-json-v1";

const GENERATED_ARM_SCHEMA: &str = r#"{"type":"object","additionalProperties":false,"required":["schema_version","candidates","selected_candidate_id"],"properties":{"schema_version":{"const":1},"candidates":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["candidate_id","content"],"properties":{"candidate_id":{"type":"string"},"content":{"type":"string"}}}},"selected_candidate_id":{"type":"string"}}}"#;
const VERDICTS_SCHEMA: &str = r#"{"type":"object","additionalProperties":false,"required":["schema_version","verdicts"],"properties":{"schema_version":{"const":1},"verdicts":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["candidate_id","correct","correct_cluster_id"],"properties":{"candidate_id":{"type":"string"},"correct":{"type":"boolean"},"correct_cluster_id":{"type":["string","null"]}}}}}}"#;

/// Explicit model values shared by one CR-B generator or evaluator port.
pub struct ModelCreativityConfig {
    /// Exact neutral deployment target.
    pub target_id: String,
    /// Non-zero output-token limit for each call.
    pub max_output_tokens: u64,
    /// Canonical non-secret deployment/template binding.
    pub descriptor: ExperimentPortDescriptor,
}

/// One-call compatible-model creativity generator.
pub struct ModelCreativityGenerator {
    common: ModelCreativityPort,
}

impl ModelCreativityGenerator {
    /// Constructs a generator around an already configured normal model port.
    pub fn new(
        config: ModelCreativityConfig,
        model: Box<dyn ModelPort>,
    ) -> Result<Self, CreativityBaselineError> {
        Ok(Self {
            common: ModelCreativityPort::new(config, model)?,
        })
    }
}

impl CreativityGeneratorPort for ModelCreativityGenerator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.common.descriptor
    }

    fn generate(
        &self,
        request: GeneratorRequest<'_>,
    ) -> Result<GeneratedArm, CreativityBaselineError> {
        let payload = json!({
            "schema_version":1,"task_id":request.task_id.as_str(),
            "arm":request.arm.wire_name(),"seed":request.seed,
            "prompt":request.prompt,"max_candidates":request.max_candidates,
            "max_candidate_utf8_bytes":request.max_candidate_utf8_bytes,
            "max_total_candidate_utf8_bytes":request.max_total_candidate_utf8_bytes,
        });
        let response: GeneratorResponse = self.common.invoke_json(
            format!("cr-b-generator-{}-{}", request.task_id.as_str(), request.arm.wire_name()),
            GENERATOR_TEMPLATE_REVISION,
            "Generate and select candidate solutions from the supplied JSON. Return only the requested strict JSON object. Do not execute or claim authority.",
            payload,
            GENERATED_ARM_SCHEMA,
            CreativityBaselineErrorCode::GeneratorFailure,
        )?;
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

/// One-call arm-blind compatible-model creativity evaluator.
pub struct ModelCreativityEvaluator {
    common: ModelCreativityPort,
}

impl ModelCreativityEvaluator {
    /// Constructs an evaluator around an already configured normal model port.
    pub fn new(
        config: ModelCreativityConfig,
        model: Box<dyn ModelPort>,
    ) -> Result<Self, CreativityBaselineError> {
        Ok(Self {
            common: ModelCreativityPort::new(config, model)?,
        })
    }
}

impl CreativityEvaluatorPort for ModelCreativityEvaluator {
    fn descriptor(&self) -> &ExperimentPortDescriptor {
        &self.common.descriptor
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
            .map(|value| json!({"candidate_id":value.candidate_id,"content":value.content}))
            .collect::<Vec<_>>();
        let payload = json!({
            "schema_version":1,"task_id":request.task_id.as_str(),
            "evaluator_rubric":rubric,"candidates":candidates,
        });
        let digest = format!(
            "{:x}",
            Sha256::digest(
                serde_jcs::to_vec(&payload)
                    .map_err(|_| error(CreativityBaselineErrorCode::EvaluatorFailure))?
            )
        );
        let response: EvaluatorResponse = self.common.invoke_json(
            format!("cr-b-evaluator-{}-{digest}", request.task_id.as_str()),
            EVALUATOR_TEMPLATE_REVISION,
            "Evaluate every supplied candidate against the rubric. Return only the requested strict JSON object. A correct candidate must have a semantic cluster ID; an incorrect candidate must use null.",
            payload,
            VERDICTS_SCHEMA,
            CreativityBaselineErrorCode::EvaluatorFailure,
        )?;
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

struct ModelCreativityPort {
    target_id: String,
    max_output_tokens: u64,
    descriptor: ExperimentPortDescriptor,
    model: Box<dyn ModelPort>,
    runtime: tokio::runtime::Runtime,
}

impl ModelCreativityPort {
    fn new(
        config: ModelCreativityConfig,
        model: Box<dyn ModelPort>,
    ) -> Result<Self, CreativityBaselineError> {
        if config.target_id.is_empty()
            || config.target_id.len() > 256
            || config.max_output_tokens == 0
        {
            return Err(error(CreativityBaselineErrorCode::InvalidPort));
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| error(CreativityBaselineErrorCode::InvalidPort))?;
        Ok(Self {
            target_id: config.target_id,
            max_output_tokens: config.max_output_tokens,
            descriptor: config.descriptor,
            model,
            runtime,
        })
    }

    fn invoke_json<T: for<'de> Deserialize<'de>>(
        &self,
        request_id: String,
        template_revision: &str,
        instruction: &str,
        payload: Value,
        schema: &str,
        failure: CreativityBaselineErrorCode,
    ) -> Result<T, CreativityBaselineError> {
        let request = ModelRequest {
            request_id: ModelRequestId::new(request_id),
            target_id: ModelTargetId::new(self.target_id.clone()),
            required_capabilities: vec![ModelCapability::Text, ModelCapability::JsonOutput],
            input_items: vec![
                message(
                    ModelRole::System,
                    format!("template={template_revision}\n{instruction}"),
                ),
                message(ModelRole::User, payload.to_string()),
            ],
            tools: Vec::new(),
            output: ModelOutputSettings {
                max_output_tokens: Some(self.max_output_tokens),
                text_mode: TextMode::JsonSchema {
                    schema_json: schema.to_owned(),
                },
                reasoning_visibility: false,
            },
            trace_metadata: Vec::new(),
        };
        let mut observer = IgnoreObserver;
        let outcome = self
            .runtime
            .block_on(self.model.invoke(&request, &mut observer, &NeverCancel))
            .map_err(|_| error(failure))?;
        let InvokeOutcome::Completed {
            items,
            stop_reason: ModelStopReason::EndTurn | ModelStopReason::StopSequence,
            ..
        } = outcome
        else {
            return Err(error(failure));
        };
        let [ModelItem::Text { text }] = items.as_slice() else {
            return Err(error(failure));
        };
        serde_json::from_str(text).map_err(|_| error(failure))
    }
}

fn message(role: ModelRole, text: String) -> ModelInputItem {
    ModelInputItem::Message {
        role,
        content: vec![ModelInputContent::Text(text)],
    }
}

struct IgnoreObserver;
impl ModelObserver for IgnoreObserver {
    fn observe(&mut self, _: &ModelStreamEvent) -> ObserverDecision {
        ObserverDecision::Continue
    }
}

struct NeverCancel;
impl ModelCancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
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
