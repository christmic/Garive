use garive_observability::{
    attribute_enum_values, signal_schema, AgentSignal, AgentSignalErrorCode, Attribute,
    AttributeValue, Correlation, Measurement, MeasurementUnit, MeasurementValue, RedactionClass,
    Severity, SIGNAL_NAMES,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/observability-v1.json"
    ))
    .expect("fixture JSON")
}

fn string(value: &Value, name: &str) -> String {
    value[name].as_str().expect(name).to_owned()
}

fn unit(value: &str) -> MeasurementUnit {
    match value {
        "count" => MeasurementUnit::Count,
        "bytes" => MeasurementUnit::Bytes,
        "milliseconds" => MeasurementUnit::Milliseconds,
        "tokens" => MeasurementUnit::Tokens,
        "basis_points" => MeasurementUnit::BasisPoints,
        _ => panic!("unknown unit {value}"),
    }
}

fn signal(value: &Value) -> Result<AgentSignal, AgentSignalErrorCode> {
    let correlation = &value["correlation"];
    let optional = |name: &str| {
        correlation
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let attributes = value["attributes"]
        .as_array()
        .expect("attributes")
        .iter()
        .map(|attribute| {
            let raw = &attribute["value"];
            let value = match raw["kind"].as_str().expect("attribute kind") {
                "string" => AttributeValue::String {
                    value: string(raw, "value"),
                },
                "bool" => AttributeValue::Bool {
                    value: raw["value"].as_bool().expect("bool value"),
                },
                "integer" => AttributeValue::Integer {
                    value: raw["value"].as_i64().expect("integer value"),
                },
                kind => panic!("unknown attribute kind {kind}"),
            };
            Attribute {
                name: string(attribute, "name"),
                value,
            }
        })
        .collect();
    let measurements = value["measurements"]
        .as_array()
        .expect("measurements")
        .iter()
        .map(|measurement| Measurement {
            name: string(measurement, "name"),
            value: match measurement["value"]["kind"].as_str().expect("value kind") {
                "known" => MeasurementValue::Known {
                    value: measurement["value"]["value"].as_u64().expect("known value"),
                },
                "unknown" => MeasurementValue::Unknown,
                kind => panic!("unknown measurement kind {kind}"),
            },
            unit: unit(measurement["unit"].as_str().expect("measurement unit")),
        })
        .collect();
    AgentSignal::new(
        string(value, "signal_name"),
        value["schema_version"].as_u64().expect("schema version") as u32,
        string(value, "observed_at_utc"),
        match value["severity"].as_str().expect("severity") {
            "trace" => Severity::Trace,
            "debug" => Severity::Debug,
            "info" => Severity::Info,
            "warn" => Severity::Warn,
            "error" => Severity::Error,
            other => panic!("unknown severity {other}"),
        },
        Correlation {
            trace_id: optional("trace_id"),
            span_id: optional("span_id"),
            parent_span_id: optional("parent_span_id"),
            session_id: optional("session_id"),
            turn_id: optional("turn_id"),
            execution_id: optional("execution_id"),
            model_request_id: optional("model_request_id"),
            tool_invocation_id: optional("tool_invocation_id"),
            durable_position: correlation.get("durable_position").and_then(Value::as_u64),
        },
        attributes,
        measurements,
        match value["redaction_class"].as_str().expect("redaction class") {
            "public" => RedactionClass::Public,
            "operational" => RedactionClass::Operational,
            "restricted" => RedactionClass::Restricted,
            other => panic!("unknown redaction class {other}"),
        },
    )
    .map_err(|error| error.code())
}

#[test]
fn shared_fixture_is_the_exact_catalogue() {
    let fixture = fixture();
    for (category, values) in fixture["enum_values"].as_object().expect("enum values") {
        let expected: Vec<_> = values
            .as_array()
            .expect("category values")
            .iter()
            .map(|value| value.as_str().expect("enum value"))
            .collect();
        assert_eq!(attribute_enum_values(category), expected);
    }
    let mut fixture_names = Vec::new();
    for entry in fixture["catalogue"].as_array().expect("catalogue") {
        let name = entry["name"].as_str().expect("name");
        fixture_names.push(name);
        let schema = signal_schema(name).expect("catalogue schema");
        let attributes: Vec<_> = entry["attributes"]
            .as_object()
            .expect("attributes")
            .iter()
            .map(|(name, category)| (name.as_str(), category.as_str().expect("category")))
            .collect();
        let measurements: Vec<_> = entry["measurements"]
            .as_object()
            .expect("measurements")
            .iter()
            .map(|(name, raw_unit)| (name.as_str(), unit(raw_unit.as_str().expect("unit"))))
            .collect();
        assert_eq!(schema.attributes, attributes);
        assert_eq!(schema.measurements, measurements);
        assert_eq!(
            schema.minimum_redaction,
            match entry["minimum_redaction"].as_str().expect("redaction") {
                "operational" => RedactionClass::Operational,
                "restricted" => RedactionClass::Restricted,
                other => panic!("unknown redaction {other}"),
            }
        );
    }
    fixture_names.sort_unstable();
    assert_eq!(fixture_names, SIGNAL_NAMES);
}

#[test]
fn shared_valid_signals_bind_canonically() {
    let fixture = fixture();
    for entry in fixture["valid_signals"].as_array().expect("valid signals") {
        let parsed = signal(&entry["signal"]).expect("valid fixture signal");
        let binding = parsed.binding().expect("canonical binding");
        assert!(binding.inline_utf8.starts_with('{'));
        if let Some(expected) = entry.get("expected_digest").and_then(Value::as_str) {
            if !expected.is_empty() {
                assert_eq!(binding.digest, expected);
            } else {
                eprintln!("{} digest: {}", entry["name"], binding.digest);
            }
        }
    }
}

fn terminal() -> Value {
    fixture()["valid_signals"][0]["signal"].clone()
}

#[test]
fn fixture_mutations_return_stable_codes() {
    let fixture = fixture();
    let cases = fixture["invalid_cases"].as_array().expect("invalid cases");
    for case in cases {
        let mutation = case["mutation"].as_str().expect("mutation");
        let mut value = match mutation {
            "total_bytes_unknown" => json!({
                "signal_name":"agent.context.derived","schema_version":1,
                "observed_at_utc":"2026-08-29T00:00:00Z","severity":"info",
                "correlation":{},"attributes":[],
                "measurements":[{"name":"total_bytes","value":{"kind":"unknown"},"unit":"bytes"}],
                "redaction_class":"operational"
            }),
            "interaction_operational" => fixture["valid_signals"][1]["signal"].clone(),
            _ => terminal(),
        };
        match mutation {
            "unknown_signal" => value["signal_name"] = json!("agent.unknown"),
            "attribute_name_raw_error" => {
                value["attributes"] =
                    json!([{"name":"raw_error","value":{"kind":"string","value":"failed"}}])
            }
            "attribute_name_session_id" => {
                value["attributes"] =
                    json!([{"name":"session_id","value":{"kind":"string","value":"session"}}])
            }
            "attribute_count_9" => {
                value["attributes"] = json!((0..9)
                .map(
                    |index| json!({"name":format!("a{index}"),"value":{"kind":"bool","value":true}})
                )
                .collect::<Vec<_>>())
            }
            "input_tokens_bytes" => value["measurements"][1]["unit"] = json!("bytes"),
            "total_bytes_unknown" => {}
            "interaction_operational" => value["redaction_class"] = json!("operational"),
            "trace_uppercase" => {
                value["correlation"]["trace_id"] = json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            }
            "attributes_unsorted" => value["attributes"]
                .as_array_mut()
                .expect("attributes")
                .reverse(),
            other => panic!("unknown fixture mutation {other}"),
        }
        let actual = signal(&value).expect_err(mutation).wire_name();
        assert_eq!(
            actual,
            case["expected"].as_str().expect("expected"),
            "{mutation}"
        );
    }
}

#[test]
fn failure_catalogue_and_forbidden_labels_are_closed() {
    let fixture = fixture();
    let expected: Vec<_> = fixture["portable_failure_codes"]
        .as_array()
        .expect("failure codes")
        .iter()
        .map(|item| item.as_str().expect("failure code"))
        .collect();
    let actual = [
        AgentSignalErrorCode::InvalidSignal,
        AgentSignalErrorCode::UnknownSignal,
        AgentSignalErrorCode::AttributeNotAllowed,
        AgentSignalErrorCode::AttributeLimitExceeded,
        AgentSignalErrorCode::MeasurementInvalid,
        AgentSignalErrorCode::RedactionViolation,
    ]
    .map(AgentSignalErrorCode::wire_name);
    assert_eq!(actual.as_slice(), expected);

    for forbidden in [
        "credential",
        "endpoint",
        "prompt",
        "provider_model",
        "raw_error",
        "response",
        "session_id",
        "status",
        "turn_id",
    ] {
        let mut value = terminal();
        value["attributes"] = json!([{"name":forbidden,"value":{"kind":"bool","value":true}}]);
        assert_eq!(
            signal(&value),
            Err(AgentSignalErrorCode::AttributeNotAllowed)
        );
    }
}
