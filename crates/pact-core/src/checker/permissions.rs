// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-06-20

//! Permission validation.
//!
//! This module implements PACT's core safety feature: verifying that agents
//! have the required permissions for the tools they use. In v0.1, the
//! tool→permission mapping is hardcoded; it becomes declarative in v0.3.

use std::collections::HashMap;

/// Hardcoded registry mapping tool names to their required permissions.
///
/// In future versions this will be loaded from tool declarations.
pub fn tool_permission_registry() -> HashMap<&'static str, Vec<&'static str>> {
    let mut map = HashMap::new();
    map.insert("web_search", vec!["net.read"]);
    map.insert("http_get", vec!["net.read"]);
    map.insert("http_post", vec!["net.read", "net.write"]);
    map.insert("llm_query", vec!["llm.query"]);
    map.insert("greet", vec!["llm.query"]);
    map.insert("summarize", vec!["llm.query"]);
    map.insert("write", vec!["llm.query"]);
    map.insert("analyze", vec!["llm.query"]);
    map.insert("draft_report", vec!["llm.query"]);
    map.insert("file_read", vec!["fs.read"]);
    map.insert("file_write", vec!["fs.read", "fs.write"]);
    // Security scanning tools
    map.insert("scan_headers", vec!["scan.passive"]);
    map.insert("scan_ssl", vec!["scan.passive"]);
    map.insert("scan_dns", vec!["scan.passive"]);
    map.insert("scan_technologies", vec!["scan.passive"]);
    map.insert("scan_ports", vec!["scan.active"]);
    map.insert("scan_http", vec!["scan.active"]);
    map.insert("exploit_validate", vec!["scan.exploit"]);
    map
}

/// Check if a set of granted permissions satisfies a required permission.
///
/// A permission `net` grants `net.read` and `net.write` (parent covers children).
pub fn permission_satisfies(granted: &[Vec<String>], required: &str) -> bool {
    let req_parts: Vec<&str> = required.split('.').collect();
    for grant in granted {
        // Check if the granted permission is a prefix of (or equal to) the required one
        if grant.len() <= req_parts.len() && grant.iter().zip(req_parts.iter()).all(|(g, r)| g == r)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let granted = vec![vec!["net".into(), "read".into()]];
        assert!(permission_satisfies(&granted, "net.read"));
    }

    #[test]
    fn parent_grants_child() {
        let granted = vec![vec!["net".into()]];
        assert!(permission_satisfies(&granted, "net.read"));
        assert!(permission_satisfies(&granted, "net.write"));
    }

    #[test]
    fn no_match() {
        let granted = vec![vec!["llm".into(), "query".into()]];
        assert!(!permission_satisfies(&granted, "net.read"));
    }

    #[test]
    fn registry_has_entries() {
        let reg = tool_permission_registry();
        assert!(reg.contains_key("web_search"));
        assert!(reg.contains_key("greet"));
    }
}
