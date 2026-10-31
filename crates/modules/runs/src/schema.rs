//! The JSON schema check of tool arguments and finish summaries.
//!
//! The subset run kinds declare their schemas in: `type` (one or a list),
//! `properties`, `required`, `additionalProperties: false`, `enum`, `const`,
//! `minLength`/`maxLength`, `minimum`/`maximum`, `items`, `maxItems`.
//! Anything else in a schema is ignored. The first mismatch is reported
//! with its path (`$.text`), never the value.

use serde_json::Value;

/// Whether `value` matches `schema`; the first mismatch otherwise.
pub fn check(schema: &Value, value: &Value) -> Result<(), String> {
    at(schema, value, "$")
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_type(value: &Value, expected: &str) -> bool {
    let actual = type_name(value);
    actual == expected || (expected == "number" && actual == "integer")
}

fn at(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        // `true`, `{}` or anything not an object accepts everything.
        return Ok(());
    };
    match schema.get("type") {
        Some(Value::String(expected)) if !is_type(value, expected) => {
            return Err(format!(
                "{path}: expected {expected}, got {}",
                type_name(value)
            ));
        }
        Some(Value::Array(types))
            if !types
                .iter()
                .filter_map(Value::as_str)
                .any(|expected| is_type(value, expected)) =>
        {
            return Err(format!(
                "{path}: {} is not an allowed type",
                type_name(value)
            ));
        }
        _ => {}
    }
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        return Err(format!("{path}: not the expected value"));
    }
    if let Some(Value::Array(allowed)) = schema.get("enum")
        && !allowed.contains(value)
    {
        return Err(format!("{path}: not one of the allowed values"));
    }
    match value {
        Value::String(text) => {
            let len = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64)
                && len > max
            {
                return Err(format!("{path}: longer than {max} characters"));
            }
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64)
                && len < min
            {
                return Err(format!("{path}: shorter than {min} characters"));
            }
        }
        Value::Number(number) => {
            let n = number.as_f64().unwrap_or_default();
            if let Some(max) = schema.get("maximum").and_then(Value::as_f64)
                && n > max
            {
                return Err(format!("{path}: more than {max}"));
            }
            if let Some(min) = schema.get("minimum").and_then(Value::as_f64)
                && n < min
            {
                return Err(format!("{path}: less than {min}"));
            }
        }
        Value::Array(items) => {
            if let Some(max) = schema.get("maxItems").and_then(Value::as_u64)
                && u64::try_from(items.len()).unwrap_or(u64::MAX) > max
            {
                return Err(format!("{path}: more than {max} items"));
            }
            if let Some(item) = schema.get("items") {
                for (i, value) in items.iter().enumerate() {
                    at(item, value, &format!("{path}[{i}]"))?;
                }
            }
        }
        Value::Object(fields) => {
            if let Some(Value::Array(required)) = schema.get("required") {
                for name in required.iter().filter_map(Value::as_str) {
                    if !fields.contains_key(name) {
                        return Err(format!("{path}.{name}: required"));
                    }
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
            for (name, field) in fields {
                match properties.and_then(|p| p.get(name)) {
                    Some(property) => at(property, field, &format!("{path}.{name}"))?,
                    None if closed => return Err(format!("{path}.{name}: not allowed")),
                    None => {}
                }
            }
        }
        Value::Null | Value::Bool(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn echo() -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "maxLength": 5 },
                "n": { "type": "integer", "minimum": 1 },
                "tags": { "type": "array", "items": { "enum": ["a", "b"] }, "maxItems": 2 },
            },
            "required": ["text"],
            "additionalProperties": false,
        })
    }

    #[test]
    fn matching_values_pass() {
        assert!(check(&echo(), &json!({ "text": "hi" })).is_ok());
        assert!(check(&echo(), &json!({ "text": "hello", "n": 2, "tags": ["a"] })).is_ok());
        assert!(check(&json!({ "type": "number" }), &json!(3)).is_ok());
        assert!(check(&json!({ "type": ["string", "null"] }), &Value::Null).is_ok());
        assert!(check(&json!({}), &json!([1, "x"])).is_ok());
    }

    #[test]
    fn mismatches_name_their_path() {
        for (value, path) in [
            (json!("hi"), "$: expected object"),
            (json!({}), "$.text: required"),
            (json!({ "text": "too long" }), "$.text: longer"),
            (json!({ "text": "hi", "n": 0 }), "$.n: less"),
            (json!({ "text": "hi", "n": 1.5 }), "$.n: expected integer"),
            (
                json!({ "text": "hi", "tags": ["c"] }),
                "$.tags[0]: not one of",
            ),
            (
                json!({ "text": "hi", "tags": ["a", "a", "b"] }),
                "$.tags: more than 2",
            ),
            (json!({ "text": "hi", "other": 1 }), "$.other: not allowed"),
        ] {
            let err = check(&echo(), &value).unwrap_err();
            assert!(err.starts_with(path), "{value}: {err}");
        }
    }
}
