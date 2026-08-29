use garive_adapter_openai_responses::{
    DecodedResponse, Header, OutputContent, ResponseOutputItem, ResponseStatus, ResponsesAdapter,
    ResponsesAdapterConfig, ResponsesAdapterError,
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
        ResponsesAdapterConfig::new("https://compatible.example/responses", vec![]).unwrap(),
    )
}

#[test]
fn complete_official_response_retains_items_and_usage() {
    let DecodedResponse::Response { response, .. } = adapter()
        .decode_response(200, &[], &fixture("ordinary.json"))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(response.status, Some(ResponseStatus::Completed));
    assert_eq!(response.output.len(), 2);
    assert_eq!(response.usage.unwrap().total_tokens, 17);
}

#[test]
fn refusal_and_incomplete_fixtures_remain_distinct_protocol_values() {
    let DecodedResponse::Response { response, .. } = adapter()
        .decode_response(200, &[], &fixture("refusal.json"))
        .unwrap()
    else {
        panic!()
    };
    let ResponseOutputItem::Message(message) = &response.output[0] else {
        panic!()
    };
    assert!(matches!(message.content[0], OutputContent::Refusal(_)));

    let DecodedResponse::Response { response, .. } = adapter()
        .decode_response(200, &[], &fixture("content-filter.json"))
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(response.status, Some(ResponseStatus::Incomplete));
    assert_eq!(
        response.incomplete_details.as_ref().unwrap().reason,
        "content_filter"
    );
}

#[test]
fn non_success_error_remains_protocol_data() {
    let body = br#"{"error":{"type":"rate_limit_error","code":"quota","message":"busy","param":null,"future":7}}"#;
    let DecodedResponse::Error { status, error, .. } =
        adapter().decode_response(429, &[], body).unwrap()
    else {
        panic!()
    };
    assert_eq!(status, 429);
    assert_eq!(error.error.r#type, "rate_limit_error");
    assert_eq!(error.error.extensions["future"], 7);
}

#[test]
fn media_and_usage_invariants_fail_closed() {
    let invalid_media = Header::new("content-type", "text/plain", false).unwrap();
    let mut invalid_total = fixture("ordinary.json");
    let mut value: serde_json::Value = serde_json::from_slice(&invalid_total).unwrap();
    value["usage"]["total_tokens"] = 18.into();
    invalid_total = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        adapter().decode_response(200, &[], &invalid_total),
        Err(ResponsesAdapterError::InvalidJson)
    );
    assert_eq!(
        adapter().decode_response(200, &[invalid_media], &fixture("ordinary.json")),
        Err(ResponsesAdapterError::InvalidMediaType)
    );
}

#[test]
fn shared_error_matrix_remains_unclassified_protocol_data() {
    let matrix: serde_json::Value = serde_json::from_slice(&fixture("errors.json")).unwrap();
    for case in matrix["cases"].as_array().unwrap() {
        let status = u16::try_from(case["status"].as_u64().unwrap()).unwrap();
        let body = serde_json::to_vec(&case["body"]).unwrap();
        let DecodedResponse::Error {
            status: decoded,
            error,
            ..
        } = adapter().decode_response(status, &[], &body).unwrap()
        else {
            panic!("expected error")
        };
        assert_eq!(decoded, status);
        assert_eq!(error.error.r#type, case["expected_error_type"]);
    }
}
