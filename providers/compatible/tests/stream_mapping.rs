use garive_anthropic_messages::MessagesStreamDecoder;
use garive_llm::{InvokeOutcome, ModelStopReason, StreamValidator};
use garive_openai_responses::{ResponsesAdapter, ResponsesAdapterConfig};
use garive_provider_compatible::{
    normalize_messages, normalize_responses, MessagesStreamMapper, ResponsesStreamMapper,
};
use serde_json::{json, Value};

fn response(status: &str, output: Value) -> Value {
    json!({"id":"resp","created_at":1.0,"error":null,"incomplete_details":null,
        "instructions":null,"metadata":null,"model":"model","object":"response",
        "output":output,"parallel_tool_calls":true,"temperature":null,
        "tool_choice":"auto","tools":[],"top_p":null,"status":status,"usage":null})
}

fn frame(value: &Value) -> Vec<u8> {
    format!(
        "event: {}\ndata: {}\n\n",
        value["type"].as_str().expect("event type"),
        value
    )
    .into_bytes()
}

#[test]
fn responses_stream_events_validate_and_terminal_matches_buffered_normalization() {
    let added_item = json!({"id":"msg","type":"message","role":"assistant",
        "status":"in_progress","content":[]});
    let text = json!({"type":"output_text","text":"hi","annotations":[]});
    let completed_item = json!({"id":"msg","type":"message","role":"assistant",
        "status":"completed","content":[text.clone()]});
    let values = [
        json!({"type":"response.created","sequence_number":0,"response":response("in_progress",json!([]))}),
        json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":added_item}),
        json!({"type":"response.content_part.added","sequence_number":2,"output_index":0,"content_index":0,"item_id":"msg","part":{"type":"output_text","text":"","annotations":[]}}),
        json!({"type":"response.output_text.delta","sequence_number":3,"output_index":0,"content_index":0,"item_id":"msg","delta":"hi","logprobs":[]}),
        json!({"type":"response.output_text.done","sequence_number":4,"output_index":0,"content_index":0,"item_id":"msg","text":"hi","logprobs":[]}),
        json!({"type":"response.content_part.done","sequence_number":5,"output_index":0,"content_index":0,"item_id":"msg","part":text}),
        json!({"type":"response.output_item.done","sequence_number":6,"output_index":0,"item":completed_item.clone()}),
        json!({"type":"response.completed","sequence_number":7,"response":response("completed",json!([completed_item]))}),
    ];
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(frame)
        .chain(b"data: [DONE]\n\n".iter().copied())
        .collect();
    let adapter = ResponsesAdapter::new(
        ResponsesAdapterConfig::new("https://example.test/responses", vec![])
            .expect("explicit test config"),
    );
    let mut decoder = adapter.stream_decoder();
    let protocol_events = decoder.push(&bytes).expect("valid protocol stream");
    decoder.finish().expect("complete stream");

    let mut mapper = ResponsesStreamMapper::new(false);
    let mut validator = StreamValidator::default();
    let mut terminal = None;
    for event in &protocol_events {
        let mapping = mapper.accept(event).expect("semantic mapping");
        for event in &mapping.events {
            validator.accept(event).expect("neutral event invariants");
        }
        terminal = mapping.terminal.or(terminal);
    }
    let buffered: garive_openai_responses::Response =
        serde_json::from_value(response("completed", json!([completed_item])))
            .expect("buffered response");
    assert_eq!(
        terminal,
        Some(normalize_responses(&buffered, false).unwrap())
    );
}

#[test]
fn messages_official_stream_maps_to_valid_events_and_buffered_terminal() {
    let bytes = include_bytes!("../../../spec/fixtures/protocols/anthropic-messages/complete.sse");
    let mut decoder = MessagesStreamDecoder::new();
    let protocol_events = decoder.push(bytes).expect("valid protocol stream");
    decoder.finish().expect("complete stream");

    let mut mapper = MessagesStreamMapper::new(false);
    let mut validator = StreamValidator::default();
    let mut terminal = None;
    for event in &protocol_events {
        let mapping = mapper.accept(event).expect("semantic mapping");
        for event in &mapping.events {
            validator.accept(event).expect("neutral event invariants");
        }
        terminal = mapping.terminal.or(terminal);
    }

    let buffered: garive_anthropic_messages::MessageResponse = serde_json::from_value(json!({
        "id":"msg_stream","type":"message","role":"assistant","model":"claude-sonnet-4-5",
        "content":[
            {"type":"text","text":"hello back"},
            {"type":"tool_use","id":"toolu_1","name":"weather","input":{"city":"Paris"}}
        ],
        "stop_reason":"tool_use","stop_sequence":null,
        "usage":{"input_tokens":4,"cache_creation_input_tokens":1,"cache_read_input_tokens":1,"output_tokens":5}
    }))
    .expect("buffered message");
    let expected = normalize_messages(&buffered, false).expect("buffered normalization");
    assert_eq!(terminal, Some(expected));
    assert!(matches!(
        terminal,
        Some(InvokeOutcome::Completed {
            stop_reason: ModelStopReason::ToolUse,
            ..
        })
    ));
}
