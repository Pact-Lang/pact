// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Integration tests for federation: parse → check → dispatch pipeline.
//!
//! Validates the full lifecycle of federation declarations, trust boundaries,
//! and federated dispatch without network calls.

use std::collections::HashMap;

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::DeclKind;
use pact_core::checker::Checker;
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Dispatcher;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

use pact_dispatch::federated::FederatedDispatcher;

// ── Helpers ──────────────────────────────────────────────────────

fn parse_program(src: &str) -> pact_core::ast::stmt::Program {
    let mut sm = SourceMap::new();
    let id = sm.add("test.pact", src);
    let tokens = Lexer::new(src, id).lex().unwrap();
    Parser::new(&tokens).parse().unwrap()
}

fn parse_and_check(src: &str) -> pact_core::ast::stmt::Program {
    let program = parse_program(src);
    let errors = Checker::new().check(&program);
    assert!(errors.is_empty(), "checker errors: {errors:?}");
    program
}

/// A simple mock dispatcher that returns tool_name + "_result".
struct MockDispatcher;

impl Dispatcher for MockDispatcher {
    fn dispatch(
        &self,
        _agent_name: &str,
        tool_name: &str,
        _args: &[Value],
        _agent_decl: &pact_core::ast::stmt::AgentDecl,
        _program: &pact_core::ast::stmt::Program,
    ) -> Result<Value, String> {
        Ok(Value::String(format!("{tool_name}_result")))
    }
}

// ═══════════════════════════════════════════════════════════════════
//  1. Parse + check: federation declarations
// ═══════════════════════════════════════════════════════════════════

const FEDERATION_SRC: &str = r#"
permit_tree {
    ^llm  { ^llm.query }
    ^net  { ^net.read }
    ^data { ^data.read, ^data.write }
}

federation {
    "https://agents.example.com/registry" trust: [^llm.query, ^net.read]
    "https://internal.corp.net/agents"    trust: [^data.read, ^data.write]
}

tool #search {
    description: <<Search.>>
    requires: [^net.read]
    params { query :: String }
    returns :: String
}

tool #store {
    description: <<Store findings.>>
    requires: [^data.write]
    params { data :: String }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search]
    prompt: <<Research agent.>>
}

agent @archivist {
    permits: [^data.read, ^data.write]
    tools: [#store]
    endpoint: "https://internal.corp.net/agents/archivist"
    prompt: <<Archive agent.>>
}

flow research(topic :: String) -> String {
    results = @researcher -> #search(topic)
    saved = @archivist -> #store(results)
    return saved
}
"#;

#[test]
fn parse_federation_declarations() {
    let program = parse_and_check(FEDERATION_SRC);

    // Find federation declaration.
    let fed_decls: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Federation(f) => Some(f),
            _ => None,
        })
        .collect();

    assert_eq!(fed_decls.len(), 1, "expected one federation block");
    let fed = fed_decls[0];
    assert_eq!(fed.registries.len(), 2, "expected two registries");

    assert_eq!(fed.registries[0].url, "https://agents.example.com/registry");
    assert_eq!(fed.registries[1].url, "https://internal.corp.net/agents");

    // Verify trust permissions on first registry.
    let trust_perms: Vec<String> = fed.registries[0]
        .trust
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(parts) => Some(parts.join(".")),
            _ => None,
        })
        .collect();
    assert_eq!(trust_perms, vec!["llm.query", "net.read"]);
}

#[test]
fn parse_agent_endpoint() {
    let program = parse_and_check(FEDERATION_SRC);

    let agents: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Agent(a) => Some(a),
            _ => None,
        })
        .collect();

    // researcher has no endpoint.
    let researcher = agents.iter().find(|a| a.name == "researcher").unwrap();
    assert!(researcher.endpoint.is_none());

    // archivist has endpoint.
    let archivist = agents.iter().find(|a| a.name == "archivist").unwrap();
    assert_eq!(
        archivist.endpoint.as_deref(),
        Some("https://internal.corp.net/agents/archivist")
    );
}

// ═══════════════════════════════════════════════════════════════════
//  2. Checker: federation validation errors
// ═══════════════════════════════════════════════════════════════════

#[test]
fn checker_rejects_invalid_federation_url() {
    let src = r#"
federation {
    "ftp://bad.example.com" trust: [^llm.query]
}
"#;
    let program = parse_program(src);
    let errors = Checker::new().check(&program);
    assert!(
        !errors.is_empty(),
        "expected checker error for invalid federation URL"
    );
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("InvalidFederationUrl") || msg.contains("http"),
        "expected URL validation error, got: {msg}"
    );
}

#[test]
fn checker_rejects_empty_trust() {
    let src = r#"
federation {
    "https://ok.example.com" trust: []
}
"#;
    let program = parse_program(src);
    let errors = Checker::new().check(&program);
    assert!(
        !errors.is_empty(),
        "expected checker error for empty trust list"
    );
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("EmptyFederationTrust") || msg.contains("trust"),
        "expected empty trust error, got: {msg}"
    );
}

#[test]
fn checker_rejects_invalid_agent_endpoint() {
    let src = r#"
tool #test_tool {
    description: <<Test.>>
    requires: [^llm.query]
    params { x :: String }
    returns :: String
}

agent @bad_agent {
    permits: [^llm.query]
    tools: [#test_tool]
    endpoint: "not-a-url"
    prompt: <<Bad.>>
}
"#;
    let program = parse_program(src);
    let errors = Checker::new().check(&program);
    assert!(
        !errors.is_empty(),
        "expected checker error for invalid agent endpoint"
    );
    let msg = format!("{:?}", errors[0]);
    assert!(
        msg.contains("InvalidAgentEndpoint") || msg.contains("endpoint"),
        "expected endpoint validation error, got: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════
//  3. Federated dispatcher: trust + local fallback
// ═══════════════════════════════════════════════════════════════════

#[test]
fn federated_dispatcher_local_fallback() {
    let program = parse_and_check(FEDERATION_SRC);

    let mut trust_map = HashMap::new();
    trust_map.insert(
        "https://internal.corp.net/agents".to_string(),
        vec!["data.read".to_string(), "data.write".to_string()],
    );

    let dispatcher =
        FederatedDispatcher::new(trust_map, Box::new(MockDispatcher)).unwrap();

    // Dispatch to local agent (no endpoint) — should use fallback.
    let researcher = program
        .decls
        .iter()
        .find_map(|d| match &d.kind {
            DeclKind::Agent(a) if a.name == "researcher" => Some(a),
            _ => None,
        })
        .unwrap();

    let result = dispatcher
        .dispatch(
            "researcher",
            "search",
            &[Value::String("test".into())],
            researcher,
            &program,
        )
        .unwrap();

    assert_eq!(result, Value::String("search_result".into()));
}

#[test]
fn federated_dispatcher_trust_validation_pass() {
    let program = parse_and_check(FEDERATION_SRC);

    let mut trust_map = HashMap::new();
    trust_map.insert(
        "https://internal.corp.net/agents".to_string(),
        vec!["data.read".to_string(), "data.write".to_string()],
    );

    let dispatcher =
        FederatedDispatcher::new(trust_map, Box::new(MockDispatcher)).unwrap();

    // Dispatch to remote agent — trust boundary covers data.read + data.write.
    // This will fail at the HTTP level (no server), but trust validation should pass.
    let archivist = program
        .decls
        .iter()
        .find_map(|d| match &d.kind {
            DeclKind::Agent(a) if a.name == "archivist" => Some(a),
            _ => None,
        })
        .unwrap();

    let result = dispatcher.dispatch(
        "archivist",
        "store",
        &[Value::String("test".into())],
        archivist,
        &program,
    );

    // Should fail with connection error, NOT trust error.
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        !err.contains("trust boundary"),
        "should pass trust check but fail on HTTP. Got: {err}"
    );
}

#[test]
fn federated_dispatcher_trust_validation_fail() {
    let program = parse_and_check(FEDERATION_SRC);

    // Empty trust map — no permissions are trusted.
    let trust_map = HashMap::new();

    let dispatcher =
        FederatedDispatcher::new(trust_map, Box::new(MockDispatcher)).unwrap();

    let archivist = program
        .decls
        .iter()
        .find_map(|d| match &d.kind {
            DeclKind::Agent(a) if a.name == "archivist" => Some(a),
            _ => None,
        })
        .unwrap();

    let result = dispatcher.dispatch(
        "archivist",
        "store",
        &[Value::String("test".into())],
        archivist,
        &program,
    );

    // Should fail with trust boundary error.
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.contains("trust boundary"),
        "expected trust boundary error. Got: {err}"
    );
}

// ═══════════════════════════════════════════════════════════════════
//  4. Full pipeline: parse → check → build for federation example
// ═══════════════════════════════════════════════════════════════════

#[test]
fn federation_example_file_passes_check() {
    let src = std::fs::read_to_string("../../examples/federation.pact")
        .expect("federation.pact should exist");
    let program = parse_program(&src);
    let errors = Checker::new().check(&program);
    assert!(errors.is_empty(), "federation.pact has checker errors: {errors:?}");

    // Verify key declarations are present.
    let has_federation = program
        .decls
        .iter()
        .any(|d| matches!(&d.kind, DeclKind::Federation(_)));
    assert!(has_federation, "expected federation declaration");

    let agents: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Agent(a) => Some(a.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(agents.contains(&"researcher"), "expected @researcher");
    assert!(agents.contains(&"archivist"), "expected @archivist");

    let flows: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f.name.as_str()),
            _ => None,
        })
        .collect();
    assert!(flows.contains(&"research_and_archive"));
    assert!(flows.contains(&"contextual_research"));
}
