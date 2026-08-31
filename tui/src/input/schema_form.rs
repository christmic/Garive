use serde_json::Value;

pub(crate) fn supports_response_schema(schema_json: &str) -> bool {
    let Ok(schema) = serde_json::from_str::<Value>(schema_json) else {
        return false;
    };
    matches!(
        schema.get("type").and_then(Value::as_str),
        Some("string" | "boolean" | "integer" | "number" | "object" | "array")
    )
}

pub(crate) fn parse_schema_input(schema_json: &str, input: &str) -> Result<Value, &'static str> {
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
        Some("object") => "Enter one JSON object matching the public schema.",
        Some("array") => "Enter one JSON array matching the public schema.",
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
