use std::collections::BTreeSet;

use serde_json::Value;

use crate::{PreparationError, PreparationErrorCode};

pub(crate) use crate::schema_validate::validate_arguments;
pub(crate) use crate::unique_json::parse_arguments;

const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";
const KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "format",
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "allOf",
    "anyOf",
    "oneOf",
    "not",
];

pub(crate) fn validate_definition(schema: &Value) -> Result<(), PreparationError> {
    let object = schema.as_object().ok_or_else(invalid_definition)?;
    if object.get("type").and_then(Value::as_str) != Some("object")
        || !matches!(object.get("properties"), Some(Value::Object(_)))
        || !matches!(
            object.get("additionalProperties"),
            Some(Value::Bool(_)) | Some(Value::Object(_))
        )
    {
        return Err(invalid_definition());
    }
    validate_schema_node(schema)
}

pub(crate) fn validate_value_definition(schema: &Value) -> Result<(), PreparationError> {
    validate_schema_node(schema)
}

/// Validates any root type in Garive's portable JSON Schema subset.
pub fn validate_portable_value_schema(schema: &Value) -> Result<(), PreparationError> {
    validate_value_definition(schema)
}

/// Validates one JSON value against an already portable value schema.
pub fn validate_portable_value(
    schema: &Value,
    value: &Value,
) -> Result<Vec<crate::SchemaFailure>, PreparationError> {
    validate_value_definition(schema)?;
    Ok(validate_arguments(schema, value))
}

fn validate_schema_node(schema: &Value) -> Result<(), PreparationError> {
    let object = schema.as_object().ok_or_else(invalid_definition)?;
    if object
        .keys()
        .any(|keyword| !KEYWORDS.contains(&keyword.as_str()))
    {
        return Err(PreparationError::new(
            PreparationErrorCode::UnsupportedSchemaKeyword,
        ));
    }
    if object
        .get("$schema")
        .is_some_and(|value| value.as_str() != Some(DIALECT))
    {
        return Err(invalid_definition());
    }
    if let Some(kind) = object.get("type") {
        let admitted = matches!(
            kind.as_str(),
            Some("object" | "array" | "string" | "number" | "integer" | "boolean" | "null")
        );
        if !admitted {
            return Err(invalid_definition());
        }
    }
    validate_required(object)?;
    validate_keyword_values(object)?;
    if let Some(properties) = object.get("properties") {
        for child in properties
            .as_object()
            .ok_or_else(invalid_definition)?
            .values()
        {
            validate_schema_node(child)?;
        }
    }
    if let Some(additional) = object.get("additionalProperties") {
        match additional {
            Value::Object(_) => validate_schema_node(additional)?,
            Value::Bool(_) => {}
            _ => return Err(invalid_definition()),
        }
    }
    if let Some(items) = object.get("items") {
        validate_schema_node(items)?;
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = object.get(keyword) {
            let branches = branches
                .as_array()
                .filter(|items| !items.is_empty())
                .ok_or_else(invalid_definition)?;
            for child in branches {
                validate_schema_node(child)?;
            }
        }
    }
    if let Some(child) = object.get("not") {
        validate_schema_node(child)?;
    }
    Ok(())
}

fn validate_required(object: &serde_json::Map<String, Value>) -> Result<(), PreparationError> {
    let Some(required) = object.get("required") else {
        return Ok(());
    };
    let values = required.as_array().ok_or_else(invalid_definition)?;
    let mut unique = BTreeSet::new();
    if values.iter().any(|value| match value.as_str() {
        Some(name) => !unique.insert(name),
        None => true,
    }) {
        return Err(invalid_definition());
    }
    Ok(())
}

fn validate_keyword_values(
    object: &serde_json::Map<String, Value>,
) -> Result<(), PreparationError> {
    for keyword in ["minItems", "maxItems", "minLength", "maxLength"] {
        if object
            .get(keyword)
            .is_some_and(|value| value.as_u64().is_none())
        {
            return Err(invalid_definition());
        }
    }
    for keyword in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
        if object
            .get(keyword)
            .is_some_and(|value| value.as_f64().is_none())
        {
            return Err(invalid_definition());
        }
    }
    if object
        .get("multipleOf")
        .is_some_and(|value| match value.as_f64() {
            Some(number) => number <= 0.0,
            None => true,
        })
    {
        return Err(invalid_definition());
    }
    if let Some(values) = object.get("enum") {
        let values = values
            .as_array()
            .filter(|items| !items.is_empty())
            .ok_or_else(invalid_definition)?;
        if values
            .iter()
            .enumerate()
            .any(|(index, value)| values[..index].contains(value))
        {
            return Err(invalid_definition());
        }
    }
    Ok(())
}

fn invalid_definition() -> PreparationError {
    PreparationError::new(PreparationErrorCode::InvalidToolDefinition)
}
