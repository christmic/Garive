use std::time::Duration;

use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelItem, ModelStopReason, RejectionKind, TokenCount,
    UnavailableKind,
};
use garive_provider_compatible::{
    classify_protocol_error, normalize_messages, normalize_responses, CompatibleProviderError,
    ErrorDisposition, ErrorSignature, ProtocolErrorPolicy,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/providers/compatible-mapping-v1.json"
    ))
    .expect("valid shared fixture")
}

fn responses_wire(case: &Value) -> Value {
    let source = &case["response"];
    let output = source["items"]
        .as_array()
        .expect("items")
        .iter()
        .enumerate()
        .map(|(index, item)| match item["kind"].as_str().expect("item kind") {
            "text" => json!({
                "type": "message", "id": format!("msg-{index}"), "role": "assistant",
                "status": "completed", "content": [{"type": "output_text", "text": item["text"], "annotations": []}]
            }),
            "tool" => json!({
                "type": "function_call", "call_id": item["model_call_id"],
                "name": item["tool_name"], "arguments": item["arguments"].to_string(), "status": "completed"
            }),
            other => panic!("unsupported fixture item {other}"),
        })
        .collect::<Vec<_>>();
    let usage = source["usage"].as_object().map(|usage| {
        let input = usage["input"].as_u64().expect("input usage");
        let output = usage["output"].as_u64().expect("output usage");
        json!({
            "input_tokens": input,
            "input_tokens_details": {"cached_tokens": usage["cache_read"], "cache_write_tokens": usage["cache_write"]},
            "output_tokens": output,
            "output_tokens_details": {"reasoning_tokens": 0},
            "total_tokens": input + output
        })
    });
    json!({
        "id": "response-1", "created_at": 1.0, "error": null,
        "incomplete_details": source.get("reason").filter(|value| !value.is_null()).map(|reason| json!({"reason": reason})),
        "instructions": null, "metadata": null, "model": "fixture", "object": "response",
        "output": output, "parallel_tool_calls": false, "temperature": null, "text": null,
        "tool_choice": "auto", "tools": [], "top_p": null, "status": source["status"], "usage": usage
    })
}

fn messages_wire(case: &Value) -> Value {
    let source = &case["response"];
    let content = source["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|item| match item["kind"].as_str().expect("item kind") {
            "refusal" | "text" => json!({"type": "text", "text": item["text"]}),
            other => panic!("unsupported fixture item {other}"),
        })
        .collect::<Vec<_>>();
    let usage = source["usage"].as_object().expect("messages usage");
    json!({
        "id": "message-1", "type": "message", "role": "assistant", "model": "fixture",
        "content": content, "stop_reason": source["stop_reason"], "stop_sequence": null,
        "usage": {"input_tokens": usage["input"], "output_tokens": usage["output"],
            "cache_read_input_tokens": usage["cache_read"], "cache_creation_input_tokens": usage["cache_write"]}
    })
}

#[test]
fn shared_responses_terminals_preserve_items_usage_and_interruption() {
    let fixture = fixture();
    let completed: garive_openai_responses::Response =
        serde_json::from_value(responses_wire(&fixture["outcome_cases"][0])).expect("response");
    let outcome = normalize_responses(&completed, false).expect("normalize completed");
    let InvokeOutcome::Completed {
        items,
        usage,
        stop_reason,
    } = outcome
    else {
        panic!("completed outcome")
    };
    assert_eq!(stop_reason, ModelStopReason::ToolUse);
    assert!(matches!(items[0], ModelItem::Text { .. }));
    assert!(matches!(items[1], ModelItem::ToolIntent { .. }));
    assert_eq!(usage.input_tokens, TokenCount::Known(10));
    assert_eq!(usage.output_tokens, TokenCount::Known(4));

    let incomplete: garive_openai_responses::Response =
        serde_json::from_value(responses_wire(&fixture["outcome_cases"][1])).expect("response");
    assert!(matches!(
        normalize_responses(&incomplete, false).expect("normalize incomplete"),
        InvokeOutcome::Interrupted {
            kind: InterruptionKind::OutputLimit,
            ..
        }
    ));
}

#[test]
fn shared_messages_terminals_distinguish_refusal_and_context_rejection() {
    let fixture = fixture();
    let refusal: garive_anthropic_messages::MessageResponse =
        serde_json::from_value(messages_wire(&fixture["outcome_cases"][2])).expect("message");
    let outcome = normalize_messages(&refusal, false).expect("normalize refusal");
    assert!(matches!(
        outcome,
        InvokeOutcome::Completed {
            stop_reason: ModelStopReason::Refusal,
            ref items,
            ..
        } if matches!(items[0], ModelItem::Refusal { .. })
    ));

    let context: garive_anthropic_messages::MessageResponse =
        serde_json::from_value(messages_wire(&fixture["outcome_cases"][3])).expect("message");
    assert!(matches!(
        normalize_messages(&context, false).expect("normalize context"),
        InvokeOutcome::Rejected {
            kind: RejectionKind::ContextOverflow,
            ..
        }
    ));
}

#[test]
fn shared_error_cases_use_only_exact_signatures() {
    let fixture = fixture();
    let policy = ProtocolErrorPolicy::new([
        (
            ErrorSignature {
                status: 401,
                protocol_type: "authentication_error".into(),
                code: None,
            },
            ErrorDisposition::Rejected(RejectionKind::Authentication),
        ),
        (
            ErrorSignature {
                status: 429,
                protocol_type: "rate_limit_error".into(),
                code: Some("rate_limit".into()),
            },
            ErrorDisposition::Unavailable(UnavailableKind::RateLimited),
        ),
    ])
    .expect("unique policy");

    for case in fixture["error_cases"].as_array().expect("error cases") {
        let signature = ErrorSignature {
            status: case["status"].as_u64().expect("status") as u16,
            protocol_type: case["type"].as_str().expect("type").to_owned(),
            code: case["code"].as_str().map(str::to_owned),
        };
        let result = classify_protocol_error(&policy, signature, Some(Duration::from_secs(2)));
        match case["expected"].as_str().expect("expected") {
            "authentication" => assert!(matches!(
                result,
                Ok(InvokeOutcome::Rejected {
                    kind: RejectionKind::Authentication,
                    ..
                })
            )),
            "rate_limited" => assert!(matches!(
                result,
                Ok(InvokeOutcome::Unavailable {
                    kind: UnavailableKind::RateLimited,
                    retry_after: Some(_)
                })
            )),
            "unclassified_protocol_error" => assert_eq!(
                result,
                Err(CompatibleProviderError::UnclassifiedProtocolError)
            ),
            other => panic!("unsupported fixture expectation {other}"),
        }
    }
}
