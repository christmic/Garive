use garive_context_pressure::{
    load_corpus, measure_context_pressure, TokenCounter, TokenCounterDescriptor,
    TokenCounterFailure,
};
use garive_llm::{ModelInputContent, ModelInputItem};

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/context-pressure-corpus-v1.json"
));

struct ExactFixtureCounter(TokenCounterDescriptor);
impl TokenCounter for ExactFixtureCounter {
    fn descriptor(&self) -> &TokenCounterDescriptor {
        &self.0
    }

    fn count_input_tokens(&self, items: &[ModelInputItem]) -> Result<u64, TokenCounterFailure> {
        let bytes = items.iter().map(item_bytes).sum::<usize>();
        Ok((bytes as u64).div_ceil(4))
    }
}

#[test]
fn sole_counter_route_reduces_every_reference_class() {
    let corpus = load_corpus(FIXTURE.as_bytes()).unwrap();
    let counter = ExactFixtureCounter(
        TokenCounterDescriptor::new("fixture-utf8", "v1", "a".repeat(64), false).unwrap(),
    );
    let first = measure_context_pressure(&corpus, &counter).unwrap();
    let second = measure_context_pressure(&corpus, &counter).unwrap();
    assert_eq!(first, second);
    assert!(!first.counter.publishable);
    assert_eq!(first.summary.ordered_cases.len(), 4);
    assert_eq!(first.summary.classes.len(), 4);
    assert!(first
        .summary
        .ordered_cases
        .iter()
        .all(|value| value.pressure_basis_points > 0));
}

fn item_bytes(value: &ModelInputItem) -> usize {
    match value {
        ModelInputItem::Message { content, .. } => content
            .iter()
            .map(|part| match part {
                ModelInputContent::Text(text) => text.len(),
                ModelInputContent::MediaReference {
                    media_kind: _,
                    reference,
                    media_type,
                } => reference.len() + media_type.len(),
            })
            .sum(),
        ModelInputItem::ToolObservation {
            model_call_id,
            result_json,
        } => model_call_id.len() + result_json.len(),
        ModelInputItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        } => model_call_id.len() + tool_name.len() + arguments_json.len(),
        ModelInputItem::ReasoningReference { reference } => reference.len(),
    }
}
