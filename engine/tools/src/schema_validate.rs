use serde_json::{Map, Value};

use crate::SchemaFailure;

pub(crate) fn validate_arguments(schema: &Value, instance: &Value) -> Vec<SchemaFailure> {
    let mut failures = Vec::new();
    validate_node(schema, instance, "", "", &mut failures);
    failures.sort();
    failures
}

fn validate_node(
    schema: &Value,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    let object = schema
        .as_object()
        .expect("definition validation admitted object");
    if let Some(kind) = object.get("type").and_then(Value::as_str) {
        if !matches_type(instance, kind) {
            out.push(failure(ip, sp, "type"));
            return;
        }
    }
    if object
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|values| !values.contains(instance))
    {
        out.push(failure(ip, sp, "enum"));
    }
    if object.get("const").is_some_and(|value| value != instance) {
        out.push(failure(ip, sp, "const"));
    }
    validate_object(object, instance, ip, sp, out);
    validate_array(object, instance, ip, sp, out);
    validate_string(object, instance, ip, sp, out);
    validate_number(object, instance, ip, sp, out);
    validate_composition(object, instance, ip, sp, out);
}

fn validate_object(
    schema: &Map<String, Value>,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    let Some(value) = instance.as_object() else {
        return;
    };
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !value.contains_key(name) {
                out.push(failure(ip, sp, "required"));
            }
        }
    }
    let properties = schema.get("properties").and_then(Value::as_object);
    if let Some(properties) = properties {
        for (name, child) in properties {
            if let Some(item) = value.get(name) {
                validate_node(
                    child,
                    item,
                    &join(ip, name),
                    &join(&join(sp, "properties"), name),
                    out,
                );
            }
        }
    }
    for (name, item) in value {
        if properties.is_some_and(|known| known.contains_key(name)) {
            continue;
        }
        match schema.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                out.push(failure(&join(ip, name), sp, "additionalProperties"))
            }
            Some(Value::Object(_)) => validate_node(
                &schema["additionalProperties"],
                item,
                &join(ip, name),
                &join(sp, "additionalProperties"),
                out,
            ),
            _ => {}
        }
    }
}

fn validate_array(
    schema: &Map<String, Value>,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    let Some(value) = instance.as_array() else {
        return;
    };
    boundary(
        schema,
        "minItems",
        value.len() as u64,
        |a, b| a >= b,
        ip,
        sp,
        out,
    );
    boundary(
        schema,
        "maxItems",
        value.len() as u64,
        |a, b| a <= b,
        ip,
        sp,
        out,
    );
    if let Some(items) = schema.get("items") {
        for (index, item) in value.iter().enumerate() {
            validate_node(
                items,
                item,
                &join(ip, &index.to_string()),
                &join(sp, "items"),
                out,
            );
        }
    }
}

fn validate_string(
    schema: &Map<String, Value>,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    let Some(value) = instance.as_str() else {
        return;
    };
    let len = value.chars().count() as u64;
    boundary(schema, "minLength", len, |a, b| a >= b, ip, sp, out);
    boundary(schema, "maxLength", len, |a, b| a <= b, ip, sp, out);
}

fn validate_number(
    schema: &Map<String, Value>,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    let Some(value) = instance.as_f64() else {
        return;
    };
    let checks = [
        (
            "minimum",
            schema
                .get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|bound| value < bound),
        ),
        (
            "maximum",
            schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|bound| value > bound),
        ),
        (
            "exclusiveMinimum",
            schema
                .get("exclusiveMinimum")
                .and_then(Value::as_f64)
                .is_some_and(|bound| value <= bound),
        ),
        (
            "exclusiveMaximum",
            schema
                .get("exclusiveMaximum")
                .and_then(Value::as_f64)
                .is_some_and(|bound| value >= bound),
        ),
    ];
    for (key, failed) in checks {
        if failed {
            out.push(failure(ip, sp, key));
        }
    }
    if let Some(unit) = schema.get("multipleOf").and_then(Value::as_f64) {
        let quotient = value / unit;
        if (quotient - quotient.round()).abs() > f64::EPSILON * quotient.abs().max(1.0) * 4.0 {
            out.push(failure(ip, sp, "multipleOf"));
        }
    }
}

fn validate_composition(
    schema: &Map<String, Value>,
    instance: &Value,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    for keyword in ["allOf", "anyOf", "oneOf"] {
        let Some(branches) = schema.get(keyword).and_then(Value::as_array) else {
            continue;
        };
        let results: Vec<Vec<SchemaFailure>> = branches
            .iter()
            .map(|branch| {
                let mut failures = Vec::new();
                validate_node(branch, instance, ip, &join(sp, keyword), &mut failures);
                failures
            })
            .collect();
        let successes = results.iter().filter(|result| result.is_empty()).count();
        if keyword == "allOf" {
            for result in results {
                out.extend(result);
            }
        } else if (keyword == "anyOf" && successes == 0) || (keyword == "oneOf" && successes != 1) {
            out.push(failure(ip, sp, keyword));
        }
    }
    if let Some(child) = schema.get("not") {
        let mut inner = Vec::new();
        validate_node(child, instance, ip, &join(sp, "not"), &mut inner);
        if inner.is_empty() {
            out.push(failure(ip, sp, "not"));
        }
    }
}

fn matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|number| number.fract() == 0.0),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn boundary<F: Fn(u64, u64) -> bool>(
    schema: &Map<String, Value>,
    key: &str,
    actual: u64,
    check: F,
    ip: &str,
    sp: &str,
    out: &mut Vec<SchemaFailure>,
) {
    if schema
        .get(key)
        .and_then(Value::as_u64)
        .is_some_and(|bound| !check(actual, bound))
    {
        out.push(failure(ip, sp, key));
    }
}
fn failure(ip: &str, sp: &str, keyword: &str) -> SchemaFailure {
    SchemaFailure::new(ip, &join(sp, keyword), keyword)
}
fn join(base: &str, token: &str) -> String {
    format!("{base}/{}", token.replace('~', "~0").replace('/', "~1"))
}
