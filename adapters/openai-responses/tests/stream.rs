use garive_adapter_openai_responses::{
    ResponsesAdapter, ResponsesAdapterConfig, ResponsesAdapterError,
};
use serde_json::{json, Value};

fn adapter() -> ResponsesAdapter {
    ResponsesAdapter::new(
        ResponsesAdapterConfig::new("https://example.test/responses", vec![]).unwrap(),
    )
}

fn response(status: &str, output: Value) -> Value {
    json!({"id":"resp","created_at":1.0,"error":null,"incomplete_details":null,
        "instructions":null,"metadata":null,"model":"model","object":"response",
        "output":output,"parallel_tool_calls":true,"temperature":null,
        "tool_choice":"auto","tools":[],"top_p":null,"status":status,"usage":null})
}

fn frame(value: Value) -> Vec<u8> {
    format!(
        "event: {}\ndata: {}\n\n",
        value["type"].as_str().unwrap(),
        value
    )
    .into_bytes()
}

#[test]
fn complete_message_lifecycle_decodes_incrementally() {
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
        .into_iter()
        .flat_map(frame)
        .chain(b"data: [DONE]\n\n".iter().copied())
        .collect();
    let mut decoder = adapter().stream_decoder();
    let mut count = 0;
    for chunk in bytes.chunks(3) {
        count += decoder.push(chunk).unwrap().len();
    }
    decoder.finish().unwrap();
    assert_eq!(count, 8);
}

#[test]
fn lifecycle_rejects_bad_order_identity_sequence_and_late_events() {
    let delta = frame(
        json!({"type":"response.output_text.delta","sequence_number":1,
        "output_index":0,"content_index":0,"item_id":"msg","delta":"x","logprobs":[]}),
    );
    assert_eq!(
        adapter().stream_decoder().push(&delta),
        Err(ResponsesAdapterError::InvalidLifecycle(
            "Responses event preceded response.created"
        ))
    );

    let created = frame(json!({"type":"response.created","sequence_number":2,
        "response":response("in_progress",json!([]))}));
    let mut decoder = adapter().stream_decoder();
    decoder.push(&created).unwrap();
    assert_eq!(
        decoder.push(&created),
        Err(ResponsesAdapterError::InvalidLifecycle(
            "Responses sequence_number must increase"
        ))
    );
}
