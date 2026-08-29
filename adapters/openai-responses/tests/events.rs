use garive_adapter_openai_responses::{PortableEventKind, ResponseStreamEvent};
use serde_json::json;

#[test]
fn every_portable_delta_family_validates_required_fields() {
    let cases = [
        json!({"type":"response.output_text.delta","sequence_number":1,"output_index":0,
            "content_index":0,"item_id":"msg","delta":"你","logprobs":[]}),
        json!({"type":"response.refusal.done","sequence_number":2,"output_index":0,
            "content_index":0,"item_id":"msg","refusal":"no"}),
        json!({"type":"response.function_call_arguments.done","sequence_number":3,
            "output_index":1,"item_id":"call","arguments":"{}"}),
        json!({"type":"response.reasoning_summary_text.delta","sequence_number":4,
            "output_index":2,"summary_index":0,"item_id":"reason","delta":"summary"}),
        json!({"type":"response.reasoning_text.done","sequence_number":5,
            "output_index":2,"content_index":0,"item_id":"reason","text":"detail"}),
    ];
    for value in cases {
        let event: ResponseStreamEvent = serde_json::from_value(value).unwrap();
        assert!(matches!(event, ResponseStreamEvent::Portable { .. }));
        assert!(event.sequence_number().unwrap() > 0);
    }
}

#[test]
fn hosted_event_is_lossless_extension() {
    let value = json!({"type":"response.web_search_call.searching","sequence_number":7,
        "output_index":0,"item_id":"search"});
    let event: ResponseStreamEvent = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(event.discriminator(), "response.web_search_call.searching");
    assert_eq!(event.sequence_number(), Some(7));
    assert_eq!(serde_json::to_value(event).unwrap(), value);
}

#[test]
fn portable_event_rejects_missing_or_invalid_fields() {
    for value in [
        json!({"type":"response.output_text.delta","sequence_number":1}),
        json!({"type":"response.output_text.delta","sequence_number":-1,
            "output_index":0,"content_index":0,"item_id":"msg","delta":"x"}),
        json!({"sequence_number":1}),
    ] {
        assert!(serde_json::from_value::<ResponseStreamEvent>(value).is_err());
    }
}

#[test]
fn discriminator_catalog_uses_exact_wire_names() {
    assert_eq!(PortableEventKind::Created.as_str(), "response.created");
    assert_eq!(
        PortableEventKind::FunctionArgumentsDelta.as_str(),
        "response.function_call_arguments.delta"
    );
    assert_eq!(PortableEventKind::Error.as_str(), "error");
}
