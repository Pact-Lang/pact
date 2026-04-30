// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-11-18

//! Conversion between PACT runtime values and JSON.

use pact_core::interpreter::value::Value;
use serde_json::Value as JsonValue;

/// Convert a PACT runtime value to a JSON value.
pub fn value_to_json(val: &Value) -> JsonValue {
    match val {
        Value::String(s) => JsonValue::String(s.clone()),
        Value::Int(n) => serde_json::json!(*n),
        Value::Float(n) => serde_json::json!(*n),
        Value::Bool(b) => JsonValue::Bool(*b),
        Value::List(items) => JsonValue::Array(items.iter().map(value_to_json).collect()),
        Value::Record(fields) => {
            let map: serde_json::Map<String, JsonValue> = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            JsonValue::Object(map)
        }
        Value::AgentRef(name) => JsonValue::String(format!("@{name}")),
        Value::ToolResult(s) => JsonValue::String(s.clone()),
        Value::Null => JsonValue::Null,
    }
}

/// Convert a JSON value to a PACT runtime value.
pub fn json_to_value(json: &JsonValue) -> Value {
    match json {
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Array(items) => Value::List(items.iter().map(json_to_value).collect()),
        JsonValue::Object(map) => {
            let fields = map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_value(v)))
                .collect();
            Value::Record(fields)
        }
        JsonValue::Null => Value::Null,
    }
}

/// Format PACT values as a human-readable user message for a tool call.
pub fn format_tool_call_message(tool_name: &str, args: &[Value]) -> String {
    if args.is_empty() {
        return format!(
            "Call tool #{tool_name}. Determine the required arguments from the user's request and the tool's parameter schema."
        );
    }

    let args_json: Vec<String> = args.iter().map(|a| value_to_json(a).to_string()).collect();
    format!(
        "Call tool #{tool_name} with these EXACT values as arguments (pass them verbatim to the tool's required parameters, do NOT modify or replace them): {}",
        args_json.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn roundtrip_string() {
        let val = Value::String("hello".into());
        assert_eq!(json_to_value(&value_to_json(&val)), val);
    }

    #[test]
    fn roundtrip_int() {
        let val = Value::Int(42);
        assert_eq!(json_to_value(&value_to_json(&val)), val);
    }

    #[test]
    fn roundtrip_list() {
        let val = Value::List(vec![Value::String("a".into()), Value::Int(1)]);
        assert_eq!(json_to_value(&value_to_json(&val)), val);
    }

    #[test]
    fn roundtrip_record() {
        let mut map = HashMap::new();
        map.insert("name".into(), Value::String("Alice".into()));
        map.insert("age".into(), Value::Int(30));
        let val = Value::Record(map);
        assert_eq!(json_to_value(&value_to_json(&val)), val);
    }

    #[test]
    fn null_roundtrip() {
        assert_eq!(json_to_value(&value_to_json(&Value::Null)), Value::Null);
    }

    #[test]
    fn format_tool_call() {
        let msg = format_tool_call_message("search", &[Value::String("rust".into())]);
        assert!(msg.contains("#search"));
        assert!(msg.contains("rust"));
    }

    #[test]
    fn format_tool_call_no_args() {
        let msg = format_tool_call_message("status", &[]);
        assert!(msg.contains("#status"));
        assert!(msg.contains("Determine the required arguments"));
    }
}
