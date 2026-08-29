use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use garive_creativity_baseline::{
    CreativityEvaluatorPort, CreativityGeneratorPort, EvaluatorRequest, ExperimentPortDescriptor,
    GeneratorRequest, ModelCreativityConfig, ModelCreativityEvaluator, ModelCreativityGenerator,
};
use garive_eval::{CreativityArm, EvaluationCaseId};
use garive_llm::{
    InvokeOutcome, ModelCancellation, ModelFuture, ModelInputContent, ModelInputItem, ModelItem,
    ModelObserver, ModelPort, ModelPortFailure, ModelRequest, ModelStopReason, ModelUsage,
    TokenCount, UsageSource,
};

#[test]
fn generator_is_rubric_free_and_evaluator_is_arm_and_selection_blind() {
    let generator_shared = shared(vec![completed(
        r#"{"schema_version":1,"candidates":[{"candidate_id":"a","content":"first"},{"candidate_id":"b","content":"second"}],"selected_candidate_id":"b"}"#,
    )]);
    let generator = ModelCreativityGenerator::new(
        config("generator"),
        Box::new(FakeModel(generator_shared.clone())),
    )
    .unwrap();
    let task_id = EvaluationCaseId::new("task-a").unwrap();
    let generated = generator
        .generate(GeneratorRequest {
            task_id: &task_id,
            arm: CreativityArm::BoundedAlternatives,
            prompt: "generator-only prompt",
            seed: 7,
            max_candidates: 3,
            max_candidate_utf8_bytes: 100,
            max_total_candidate_utf8_bytes: 300,
        })
        .unwrap();
    assert_eq!(generated.selected_candidate_id, "b");
    let generator_request = generator_shared.requests.lock().unwrap().pop().unwrap();
    let generator_text = request_text(&generator_request);
    assert!(generator_text.contains("generator-only prompt"));
    assert!(generator_text.contains("bounded_alternatives"));
    assert!(!generator_text.contains("evaluator_rubric"));
    assert!(generator_request.tools.is_empty());

    let evaluator_shared = shared(vec![completed(
        r#"{"schema_version":1,"verdicts":[{"candidate_id":"a","correct":true,"correct_cluster_id":"one"},{"candidate_id":"b","correct":false,"correct_cluster_id":null}]}"#,
    )]);
    let evaluator = ModelCreativityEvaluator::new(
        config("evaluator"),
        Box::new(FakeModel(evaluator_shared.clone())),
    )
    .unwrap();
    let verdicts = evaluator
        .evaluate(EvaluatorRequest {
            task_id: &task_id,
            rubric_json: r#"{"required":"value"}"#,
            candidates: &generated.candidates,
        })
        .unwrap();
    assert_eq!(verdicts.len(), 2);
    let evaluator_request = evaluator_shared.requests.lock().unwrap().pop().unwrap();
    let evaluator_text = request_text(&evaluator_request);
    assert!(evaluator_text.contains("evaluator_rubric"));
    assert!(evaluator_text.contains("required"));
    assert!(!evaluator_text.contains("bounded_alternatives"));
    assert!(!evaluator_text.contains("selected_candidate"));
    assert!(!evaluator_text.contains("generator-only prompt"));
    assert!(evaluator_request.tools.is_empty());
}

#[test]
fn non_completed_non_text_malformed_and_port_failure_stop_without_retry() {
    let cases = vec![
        Ok(InvokeOutcome::Unavailable {
            kind: garive_llm::UnavailableKind::ModelUnavailable,
            retry_after: None,
        }),
        Ok(InvokeOutcome::Completed {
            items: vec![ModelItem::Refusal { text: "no".into() }],
            usage: usage(),
            stop_reason: ModelStopReason::Refusal,
        }),
        completed("not-json"),
        Err(ModelPortFailure::RequiredPortFailure),
    ];
    for response in cases {
        let shared = shared(vec![response]);
        let generator =
            ModelCreativityGenerator::new(config("generator"), Box::new(FakeModel(shared.clone())))
                .unwrap();
        assert!(generator
            .generate(GeneratorRequest {
                task_id: &EvaluationCaseId::new("task").unwrap(),
                arm: CreativityArm::Control,
                prompt: "prompt",
                seed: 1,
                max_candidates: 1,
                max_candidate_utf8_bytes: 10,
                max_total_candidate_utf8_bytes: 10,
            })
            .is_err());
        assert_eq!(shared.calls(), 1);
    }

    assert!(ModelCreativityEvaluator::new(
        ModelCreativityConfig {
            target_id: String::new(),
            max_output_tokens: 1,
            descriptor: descriptor("bad"),
        },
        Box::new(FakeModel(shared(Vec::new()))),
    )
    .is_err());
}

fn config(kind: &str) -> ModelCreativityConfig {
    ModelCreativityConfig {
        target_id: "target".into(),
        max_output_tokens: 512,
        descriptor: descriptor(kind),
    }
}

fn descriptor(kind: &str) -> ExperimentPortDescriptor {
    ExperimentPortDescriptor::new(kind, "v1", "a".repeat(64), true).unwrap()
}

fn completed(text: &str) -> Result<InvokeOutcome, ModelPortFailure> {
    Ok(InvokeOutcome::Completed {
        items: vec![ModelItem::Text { text: text.into() }],
        usage: usage(),
        stop_reason: ModelStopReason::EndTurn,
    })
}

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(1),
        output_tokens: TokenCount::Known(1),
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn request_text(request: &ModelRequest) -> String {
    request
        .input_items
        .iter()
        .flat_map(|item| match item {
            ModelInputItem::Message { content, .. } => content,
            _ => panic!("unexpected model input"),
        })
        .map(|content| match content {
            ModelInputContent::Text(text) => text.as_str(),
            _ => panic!("unexpected model content"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct Shared {
    responses: Mutex<VecDeque<Result<InvokeOutcome, ModelPortFailure>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Shared {
    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

fn shared(responses: Vec<Result<InvokeOutcome, ModelPortFailure>>) -> Arc<Shared> {
    Arc::new(Shared {
        responses: Mutex::new(responses.into()),
        requests: Mutex::new(Vec::new()),
    })
}

struct FakeModel(Arc<Shared>);

impl ModelPort for FakeModel {
    fn invoke<'a>(
        &'a self,
        request: &'a ModelRequest,
        _: &'a mut dyn ModelObserver,
        _: &'a dyn ModelCancellation,
    ) -> ModelFuture<'a> {
        self.0.requests.lock().unwrap().push(request.clone());
        let response = self.0.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move { response })
    }
}
