// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-07-10

//! Scoped variable environment for the interpreter.
//!
//! The [`Environment`] maintains a stack of scopes. Each scope is a map
//! from variable names to values. Variable lookup walks up the stack,
//! and new bindings are always added to the topmost scope.

use super::value::Value;
use std::collections::HashMap;

/// A stack-based scoped environment for variable bindings.
#[derive(Debug)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    /// Create an environment with a single empty global scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Push a new scope onto the stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the topmost scope from the stack.
    ///
    /// # Panics
    ///
    /// Panics if called when only the global scope remains.
    pub fn pop_scope(&mut self) {
        assert!(self.scopes.len() > 1, "cannot pop the global scope");
        self.scopes.pop();
    }

    /// Define (or shadow) a variable in the current scope.
    pub fn define(&mut self, name: String, value: Value) {
        self.scopes.last_mut().unwrap().insert(name, value);
    }

    /// Look up a variable by name, searching from the innermost scope outward.
    pub fn lookup(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Some(val);
            }
        }
        None
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_lookup() {
        let mut env = Environment::new();
        env.define("x".into(), Value::Int(42));
        assert_eq!(env.lookup("x"), Some(&Value::Int(42)));
        assert_eq!(env.lookup("y"), None);
    }

    #[test]
    fn scoped_lookup() {
        let mut env = Environment::new();
        env.define("x".into(), Value::Int(1));
        env.push_scope();
        env.define("x".into(), Value::Int(2));
        assert_eq!(env.lookup("x"), Some(&Value::Int(2)));
        env.pop_scope();
        assert_eq!(env.lookup("x"), Some(&Value::Int(1)));
    }

    #[test]
    fn inner_scope_sees_outer() {
        let mut env = Environment::new();
        env.define("outer".into(), Value::String("hello".into()));
        env.push_scope();
        assert_eq!(env.lookup("outer"), Some(&Value::String("hello".into())));
        env.pop_scope();
    }

    #[test]
    #[should_panic(expected = "cannot pop the global scope")]
    fn pop_global_panics() {
        let mut env = Environment::new();
        env.pop_scope();
    }
}
