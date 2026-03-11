// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-06-14

//! Symbol table and name resolution for semantic analysis.
//!
//! The [`SymbolTable`] collects all top-level declarations during name
//! collection and provides lookup methods for the type checker and
//! permission validator.

use std::collections::{HashMap, HashSet};

/// Information about a declared symbol.
#[derive(Debug, Clone)]
pub enum SymbolKind {
    Agent {
        permits: Vec<Vec<String>>,
        tools: Vec<String>,
    },
    AgentBundle {
        agents: Vec<String>,
    },
    Flow {
        param_count: usize,
    },
    Schema {
        fields: Vec<(String, String)>,
    },
    TypeAlias {
        variants: Vec<String>,
    },
    Tool {
        /// Permission paths required by this tool.
        requires: Vec<Vec<String>>,
        /// Parameter names with (name, type_name, is_required).
        params: Vec<(String, String, bool)>,
        /// Return type name.
        return_type: Option<String>,
    },
    Skill {
        /// Tools this skill uses.
        tools: Vec<String>,
        /// Parameter names with (name, type_name, is_required).
        params: Vec<(String, String, bool)>,
        /// Return type name.
        return_type: Option<String>,
    },
    Template {
        /// Entry names defined in this template.
        entries: Vec<String>,
    },
    Directive {
        /// Parameter names defined in this directive.
        params: Vec<String>,
    },
}

/// The symbol table for a PACT program.
#[derive(Debug, Default)]
pub struct SymbolTable {
    /// Map from symbol name to its kind.
    symbols: HashMap<String, SymbolKind>,
    /// Set of all known permission paths (from permit_tree declarations).
    permissions: HashSet<String>,
}

impl SymbolTable {
    /// Create an empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a symbol. Returns `false` if it was already defined.
    pub fn define(&mut self, name: String, kind: SymbolKind) -> bool {
        if let std::collections::hash_map::Entry::Vacant(e) = self.symbols.entry(name) {
            e.insert(kind);
            true
        } else {
            false
        }
    }

    /// Look up a symbol by name.
    pub fn lookup(&self, name: &str) -> Option<&SymbolKind> {
        self.symbols.get(name)
    }

    /// Register a permission path (e.g. "net.read").
    pub fn define_permission(&mut self, path: String) {
        self.permissions.insert(path);
    }

    /// Check if a permission path is defined.
    pub fn has_permission(&self, path: &str) -> bool {
        self.permissions.contains(path)
    }

    /// Get all defined permission paths.
    pub fn all_permissions(&self) -> impl Iterator<Item = &String> {
        self.permissions.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_lookup() {
        let mut st = SymbolTable::new();
        assert!(st.define(
            "greeter".to_string(),
            SymbolKind::Agent {
                permits: vec![vec!["llm".into(), "query".into()]],
                tools: vec!["greet".into()],
            },
        ));
        assert!(st.lookup("greeter").is_some());
        assert!(st.lookup("nonexistent").is_none());
    }

    #[test]
    fn duplicate_define() {
        let mut st = SymbolTable::new();
        st.define("x".to_string(), SymbolKind::Flow { param_count: 0 });
        assert!(!st.define("x".to_string(), SymbolKind::Flow { param_count: 1 },));
    }

    #[test]
    fn permissions() {
        let mut st = SymbolTable::new();
        st.define_permission("net.read".to_string());
        st.define_permission("llm.query".to_string());
        assert!(st.has_permission("net.read"));
        assert!(!st.has_permission("fs.write"));
    }
}
