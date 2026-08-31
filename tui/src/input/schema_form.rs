use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SchemaControl {
    Editor,
    Choices(Vec<String>),
}

pub(crate) fn supports_response_schema(schema_json: &str) -> bool {
    response_schema_control(schema_json).is_some()
}

pub(crate) fn response_schema_control(schema_json: &str) -> Option<SchemaControl> {
    let Ok(schema) = serde_json::from_str::<Value>(schema_json) else {
        return None;
    };
    let object = schema.as_object()?;
    match schema.get("type").and_then(Value::as_str)? {
        "boolean" if keys_are(object, &["type"]) => {
            Some(SchemaControl::Choices(vec!["true".into(), "false".into()]))
        }
        "string" => match schema.get("enum") {
            Some(Value::Array(values))
                if keys_are(object, &["type", "enum", "minLength", "maxLength"])
                    && string_bounds_are_valid(&schema)
                    && !values.is_empty()
                    && values.len() <= 12 =>
            {
                let choices = values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .filter(|text| safe_choice(text))
                            .map(str::to_owned)
                    })
                    .collect::<Option<Vec<_>>>()?;
                if choices.iter().collect::<BTreeSet<_>>().len() != choices.len() {
                    return None;
                }
                Some(SchemaControl::Choices(choices))
            }
            Some(_) => None,
            None if keys_are(object, &["type", "minLength", "maxLength"])
                && string_bounds_are_valid(&schema) =>
            {
                Some(SchemaControl::Editor)
            }
            None => None,
        },
        "integer" | "number"
            if keys_are(object, &["type", "minimum", "maximum"])
                && schema.get("minimum").and_then(Value::as_f64).is_some()
                && schema.get("maximum").and_then(Value::as_f64).is_some()
                && schema["minimum"].as_f64()? <= schema["maximum"].as_f64()? =>
        {
            Some(SchemaControl::Editor)
        }
        _ => None,
    }
}

fn keys_are(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
    object.keys().all(|key| allowed.contains(&key.as_str()))
}

fn string_bounds_are_valid(schema: &Value) -> bool {
    let minimum = schema
        .get("minLength")
        .map(Value::as_u64)
        .unwrap_or(Some(0));
    let maximum = schema
        .get("maxLength")
        .map(Value::as_u64)
        .unwrap_or(Some(16 * 1_024));
    matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum <= maximum && maximum <= 16 * 1_024)
}

fn safe_choice(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 80
        && !value.chars().any(|character| character.is_control())
}

pub(crate) fn parse_schema_input(schema_json: &str, input: &str) -> Result<Value, &'static str> {
    if response_schema_control(schema_json).is_none() {
        return Err("unsupported_schema");
    }
    let schema: Value = serde_json::from_str(schema_json).map_err(|_| "unsupported_schema")?;
    let kind = schema
        .get("type")
        .and_then(Value::as_str)
        .ok_or("unsupported_schema")?;
    let value = match kind {
        "string" => Value::String(input.to_owned()),
        "boolean" => match input.trim() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return Err("expected_boolean"),
        },
        "integer" => input
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| "expected_integer")?,
        "number" => {
            let number = input.trim().parse::<f64>().map_err(|_| "expected_number")?;
            if !number.is_finite() {
                return Err("expected_number");
            }
            Value::from(number)
        }
        "object" => {
            let value: Value = serde_json::from_str(input).map_err(|_| "expected_json")?;
            if !value.is_object() {
                return Err("expected_json");
            }
            value
        }
        "array" => {
            let value: Value = serde_json::from_str(input).map_err(|_| "expected_json")?;
            if !value.is_array() {
                return Err("expected_json");
            }
            value
        }
        _ => return Err("unsupported_schema"),
    };
    validate_value(&schema, &value)?;
    Ok(value)
}

pub(crate) fn describe_schema(schema_json: &str) -> &'static str {
    let Ok(schema) = serde_json::from_str::<Value>(schema_json) else {
        return "Unsupported response schema; this request is read-only.";
    };
    match schema.get("type").and_then(Value::as_str) {
        Some("string") if schema.get("enum").is_some() => {
            "Enter one exact listed choice, without JSON quotes."
        }
        Some("string") => "Enter text; Garive will encode it as a JSON string.",
        Some("boolean") => "Enter true or false.",
        Some("integer") => "Enter a whole number within the displayed bounds.",
        Some("number") => "Enter a number within the displayed bounds.",
        _ => "Unsupported response schema; this request is read-only.",
    }
}

fn validate_value(schema: &Value, value: &Value) -> Result<(), &'static str> {
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            return Err("not_in_enum");
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|minimum| length < minimum)
            || schema
                .get("maxLength")
                .and_then(Value::as_u64)
                .is_some_and(|maximum| length > maximum)
        {
            return Err("string_bound");
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|minimum| number < minimum)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|maximum| number > maximum)
        {
            return Err("number_bound");
        }
    }
    Ok(())
}
