// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-07-05

//! Runtime value representation.
//!
//! [`Value`] is the runtime representation of all data in the PACT
//! interpreter. The interpreter is tree-walking and dynamically typed,
//! so all values are boxed in this enum.

use std::collections::HashMap;
use std::fmt;

/// A runtime value in the PACT interpreter.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string value.
    String(String),
    /// A 64-bit integer.
    Int(i64),
    /// A 64-bit float.
    Float(f64),
    /// A boolean.
    Bool(bool),
    /// An ordered list of values.
    List(Vec<Value>),
    /// A key-value record (used for schema instances).
    Record(HashMap<String, Value>),
    /// A reference to an agent (by name, without `@`).
    AgentRef(String),
    /// The result of a tool invocation.
    ToolResult(String),
    /// The null/unit value.
    Null,
}

impl Value {
    /// Return a human-readable type name for this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "String",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::Bool(_) => "Bool",
            Value::List(_) => "List",
            Value::Record(_) => "Record",
            Value::AgentRef(_) => "AgentRef",
            Value::ToolResult(_) => "ToolResult",
            Value::Null => "Null",
        }
    }

    /// Attempt to interpret this value as a truthy boolean.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::List(l) => !l.is_empty(),
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{s}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Record(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::AgentRef(name) => write!(f, "@{name}"),
            Value::ToolResult(s) => write!(f, "ToolResult(\"{s}\")"),
            Value::Null => write!(f, "null"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness() {
        assert!(Value::Bool(true).is_truthy());
        assert!(!Value::Bool(false).is_truthy());
        assert!(!Value::Null.is_truthy());
        assert!(Value::String("hello".into()).is_truthy());
        assert!(!Value::String("".into()).is_truthy());
        assert!(Value::Int(1).is_truthy());
        assert!(!Value::Int(0).is_truthy());
    }

    #[test]
    fn display() {
        assert_eq!(Value::String("hello".into()).to_string(), "hello");
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Null.to_string(), "null");
        assert_eq!(
            Value::ToolResult("result".into()).to_string(),
            "ToolResult(\"result\")"
        );
    }
}
