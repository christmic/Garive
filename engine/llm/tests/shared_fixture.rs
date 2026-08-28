use std::{fs, path::PathBuf, time::Duration};

use garive_llm::{
    InterruptionKind, InvokeOutcome, MediaKind, ModelItem, ModelStopReason, ModelUsage,
    ReasoningContent, RejectionKind, TokenCount, UnavailableKind, UsageSource, UsageTotal,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/model-outcome.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn count(value: &Value) -> TokenCount {
    match value.as_str().unwrap() {
        "unknown" => TokenCount::Unknown,
        known => TokenCount::Known(known.parse().unwrap()),
    }
}

fn usage(value: &Value) -> ModelUsage {
    ModelUsage {
        input_tokens: count(&value["input"]),
        output_tokens: count(&value["output"]),
        cache_read_tokens: value.get("cache_read").map(count),
        cache_write_tokens: value.get("cache_write").map(count),
        source: match value["source"].as_str().unwrap() {
            "provider-reported" => UsageSource::ProviderReported,
            "estimated" => UsageSource::Estimated,
            other => panic!("unknown usage source: {other}"),
        },
    }
}

fn items(kinds: &Value) -> Vec<ModelItem> {
    kinds
        .as_array()
        .unwrap()
        .iter()
        .map(|kind| match kind.as_str().unwrap() {
            "text" => ModelItem::Text {
                text: "text".into(),
            },
            "reasoning" => ModelItem::Reasoning {
                content: ReasoningContent::OpaqueReference("reasoning".into()),
            },
            "tool-intent" => ModelItem::ToolIntent {
                model_call_id: "call".into(),
                tool_name: "tool".into(),
                arguments_json: "{}".into(),
            },
            "tool-observation" => ModelItem::ToolObservation {
                model_call_id: "call".into(),
                result_json: "{}".into(),
            },
            "media-reference" => ModelItem::MediaReference {
                media_kind: MediaKind::Image,
                reference: "media".into(),
            },
            other => panic!("unknown item kind: {other}"),
        })
        .collect()
}

fn rendered_total(total: UsageTotal) -> String {
    match total {
        UsageTotal::Known(value) => format!("known:{value}"),
        UsageTotal::Unknown => "unknown".into(),
        UsageTotal::Overflow => "overflow".into(),
    }
}

#[test]
fn rust_consumes_every_model_outcome_case() {
    let document = fixture();
    assert_eq!(document["schema_version"], 1);
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        7,
        "fixture coverage changed; review both runners"
    );

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let usage = usage(&case["usage"]);
        let input = &case["outcome"];
        let model_items = items(&input["item_kinds"]);
        let outcome = match input["envelope"].as_str().unwrap() {
            "completed" => InvokeOutcome::Completed {
                items: model_items.clone(),
                usage,
                stop_reason: ModelStopReason::EndTurn,
            },
            "rejected" => InvokeOutcome::Rejected {
                kind: match input["reason"].as_str().unwrap() {
                    "context-overflow" => RejectionKind::ContextOverflow,
                    "authentication" => RejectionKind::Authentication,
                    "content-policy" => RejectionKind::ContentPolicy,
                    other => panic!("{name}: unknown rejection {other}"),
                },
                sanitized_evidence: "fixture".into(),
            },
            "interrupted" => InvokeOutcome::Interrupted {
                kind: match input["reason"].as_str().unwrap() {
                    "cancelled" => InterruptionKind::Cancelled,
                    "output-limit" => InterruptionKind::OutputLimit,
                    "transport" => InterruptionKind::Transport,
                    other => panic!("{name}: unknown interruption {other}"),
                },
                partial_items: model_items.clone(),
                usage,
            },
            "unavailable" => InvokeOutcome::Unavailable {
                kind: match input["reason"].as_str().unwrap() {
                    "rate-limited" => UnavailableKind::RateLimited,
                    "model-unavailable" => UnavailableKind::ModelUnavailable,
                    "circuit-open" => UnavailableKind::CircuitOpen,
                    other => panic!("{name}: unknown unavailable {other}"),
                },
                retry_after: Some(Duration::from_secs(1)),
            },
            other => panic!("{name}: unknown envelope {other}"),
        };

        let expected = &case["expected"];
        assert_eq!(
            rendered_total(usage.total_tokens()),
            expected["total"],
            "{name}"
        );
        assert_eq!(
            format!("{:?}", outcome.kind()).to_lowercase(),
            expected["kind"],
            "{name}"
        );
        assert_eq!(
            outcome.is_success(),
            expected["success"].as_bool().unwrap(),
            "{name}"
        );
        assert_eq!(
            outcome.is_partial(),
            expected["partial"].as_bool().unwrap(),
            "{name}"
        );
        let actual_items: &[ModelItem] = match &outcome {
            InvokeOutcome::Completed { items, .. } => items.as_slice(),
            InvokeOutcome::Interrupted { partial_items, .. } => partial_items.as_slice(),
            _ => &[],
        };
        assert_eq!(actual_items, model_items, "{name}");
    }
}
