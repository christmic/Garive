#![allow(dead_code, unused_imports)]

#[path = "../src/input/schema_form.rs"]
mod schema_form;

use serde_json::json;

#[test]
fn scalar_inputs_are_natural_and_strictly_bounded() {
    assert_eq!(
        schema_form::parse_schema_input(
            r#"{"type":"string","enum":["approve","deny"]}"#,
            "approve"
        ),
        Ok(json!("approve"))
    );
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"boolean"}"#, "true"),
        Ok(json!(true))
    );
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"integer","minimum":2,"maximum":9}"#, "1"),
        Err("number_bound")
    );
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"string","maxLength":2}"#, "界界界"),
        Err("string_bound")
    );
}

#[test]
fn structured_inputs_require_the_declared_json_shape() {
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"object"}"#, r#"{"ok":true}"#),
        Err("unsupported_schema")
    );
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"object"}"#, "[]"),
        Err("unsupported_schema")
    );
    assert_eq!(
        schema_form::parse_schema_input(r#"{"type":"null"}"#, "null"),
        Err("unsupported_schema")
    );
}

#[test]
fn response_schema_support_is_explicit_and_rejects_missing_or_unknown_types() {
    for schema in [
        r#"{"type":"string"}"#,
        r#"{"type":"boolean"}"#,
        r#"{"type":"integer","minimum":0,"maximum":10}"#,
        r#"{"type":"number","minimum":0,"maximum":10}"#,
    ] {
        assert!(schema_form::supports_response_schema(schema), "{schema}");
    }
    for schema in [
        r#"{}"#,
        r#"{"type":"null"}"#,
        r#"{"type":"object","required":[]}"#,
        r#"{"type":"array"}"#,
        r#"{"type":"string","pattern":".*"}"#,
        r#"{"type":"number","minimum":0,"maximum":10,"multipleOf":2}"#,
        r#"{"type":"string","enum":["same","same"]}"#,
        "not-json",
    ] {
        assert!(!schema_form::supports_response_schema(schema), "{schema}");
    }
}
