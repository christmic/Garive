use garive_adapter_openai_responses::{
    ResponsesAdapter, ResponsesAdapterConfig, ResponsesAdapterError,
};
use std::{fs, path::PathBuf};

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../spec/fixtures/protocols/openai-responses")
            .join(name),
    )
    .unwrap()
}

fn adapter() -> ResponsesAdapter {
    ResponsesAdapter::new(
        ResponsesAdapterConfig::new("https://example.test/responses", vec![]).unwrap(),
    )
}

#[test]
fn pinned_official_stream_survives_small_transport_chunks() {
    let mut decoder = adapter().stream_decoder();
    let mut events = Vec::new();
    for chunk in fixture("complete.sse").chunks(5) {
        events.extend(decoder.push(chunk).unwrap());
    }
    decoder.finish().unwrap();
    assert_eq!(events.len(), 9);
    assert_eq!(events.first().unwrap().discriminator(), "response.created");
    assert_eq!(events.last().unwrap().discriminator(), "response.completed");
}

#[test]
fn composite_and_incomplete_official_streams_reach_exact_terminals() {
    for (name, expected_count, terminal) in [
        ("composite.sse", 16, "response.completed"),
        ("incomplete.sse", 5, "response.incomplete"),
    ] {
        let mut decoder = adapter().stream_decoder();
        let mut events = Vec::new();
        for chunk in fixture(name).chunks(7) {
            events.extend(decoder.push(chunk).unwrap());
        }
        decoder.finish().unwrap();
        assert_eq!(events.len(), expected_count);
        assert_eq!(events.last().unwrap().discriminator(), terminal);
    }
}

#[test]
fn pinned_truncated_stream_preserves_events_but_fails_eof() {
    let mut decoder = adapter().stream_decoder();
    let events = decoder.push(&fixture("truncated.sse")).unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(
        decoder.finish(),
        Err(ResponsesAdapterError::TruncatedStream)
    );
}

#[test]
fn eof_without_protocol_terminal_is_truncated() {
    let mut decoder = adapter().stream_decoder();
    let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp\",\"created_at\":1.0,\"error\":null,\"incomplete_details\":null,\"instructions\":null,\"metadata\":null,\"model\":\"model\",\"object\":\"response\",\"output\":[],\"parallel_tool_calls\":true,\"temperature\":null,\"tool_choice\":\"auto\",\"tools\":[],\"top_p\":null,\"status\":\"in_progress\",\"usage\":null}}\n\n";
    decoder.push(created).unwrap();
    assert_eq!(
        decoder.finish(),
        Err(ResponsesAdapterError::TruncatedStream)
    );
}
