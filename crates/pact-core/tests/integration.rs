// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-08-08

//! Integration tests for the full PACT pipeline: parse → check → interpret.

use pact_core::ast::stmt::DeclKind;
use pact_core::checker::Checker;
use pact_core::interpreter::value::Value;
use pact_core::interpreter::Interpreter;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

/// Helper: lex + parse source into a Program, panicking on failure.
fn parse(src: &str) -> pact_core::ast::stmt::Program {
    let mut sm = SourceMap::new();
    let id = sm.add("test.pact", src);
    let tokens = Lexer::new(src, id).lex().unwrap();
    Parser::new(&tokens).parse().unwrap()
}

/// Helper: lex + parse + check, returning checker errors.
fn check(src: &str) -> Vec<pact_core::checker::CheckError> {
    let program = parse(src);
    Checker::new().check(&program)
}

/// Helper: lex + parse + check + run a flow, returning the result value.
fn run(src: &str, flow_name: &str, args: Vec<Value>) -> Value {
    let program = parse(src);
    let errors = Checker::new().check(&program);
    assert!(errors.is_empty(), "expected no check errors, got: {errors:?}");

    let program = parse(src); // re-parse since checker consumes nothing but we need a fresh program
    let mut interp = Interpreter::new();
    interp.run(&program, flow_name, args).unwrap()
}

// ─────────────────────────────────────────────────────────────────────
// 1. hello_agent_end_to_end
// ─────────────────────────────────────────────────────────────────────

#[test]
fn hello_agent_end_to_end() {
    let src = r#"
        agent @greeter {
            permits: [^llm.query]
            tools: [#greet]
            model: "gpt-4"
            prompt: <<You are a friendly greeter>>
        }

        flow hello(name :: String) -> String {
            result = @greeter -> #greet(name)
            return result
        }
    "#;

    let result = run(src, "hello", vec![Value::String("world".into())]);

    // MockDispatcher returns Value::ToolResult("<tool_name>_result")
    assert_eq!(result, Value::ToolResult("greet_result".into()));
}

// ─────────────────────────────────────────────────────────────────────
// 2. research_flow_end_to_end
// ─────────────────────────────────────────────────────────────────────

#[test]
fn research_flow_end_to_end() {
    let src = r#"
        tool #search {
            description: <<Search the web for information>>
            requires: [^net.read]
            params {
                query :: String
            }
            returns :: String
        }

        tool #summarize {
            description: <<Summarize text into a concise report>>
            requires: [^llm.query]
            params {
                text :: String
            }
            returns :: String
        }

        agent @researcher {
            permits: [^net.read, ^llm.query]
            tools: [#search, #summarize]
            model: "claude-sonnet"
            prompt: <<You are a research assistant>>
        }

        flow research(topic :: String) -> String {
            raw = @researcher -> #search(topic)
            summary = @researcher -> #summarize(raw)
            return summary
        }
    "#;

    let result = run(src, "research", vec![Value::String("quantum computing".into())]);

    // The flow completes and returns the summarize tool result.
    assert_eq!(result, Value::ToolResult("summarize_result".into()));
}

// ─────────────────────────────────────────────────────────────────────
// 3. permission_violation_caught
// ─────────────────────────────────────────────────────────────────────

#[test]
fn permission_violation_caught() {
    let src = r#"
        tool #web_fetch {
            description: <<Fetch a web page>>
            requires: [^net.read]
            params {
                url :: String
            }
            returns :: String
        }

        agent @unprivileged {
            permits: []
            tools: [#web_fetch]
        }
    "#;

    let errors = check(src);

    assert_eq!(errors.len(), 1, "expected exactly one error, got: {errors:?}");
    match &errors[0] {
        pact_core::checker::CheckError::MissingPermission {
            agent,
            tool,
            permission,
            ..
        } => {
            assert_eq!(agent, "unprivileged");
            assert_eq!(tool, "web_fetch");
            assert_eq!(permission, "net.read");
        }
        other => panic!("expected MissingPermission, got: {other:?}"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// 4. tool_with_handler_parses
// ─────────────────────────────────────────────────────────────────────

#[test]
fn tool_with_handler_parses() {
    let src = r#"
        tool #web_search {
            description: <<Search the web>>
            requires: [^net.read]
            handler: "http GET https://example.com/{query}"
            params {
                query :: String
            }
            returns :: String
        }
    "#;

    let program = parse(src);

    assert_eq!(program.decls.len(), 1);
    match &program.decls[0].kind {
        DeclKind::Tool(t) => {
            assert_eq!(t.name, "web_search");
            assert_eq!(
                t.handler.as_deref(),
                Some("http GET https://example.com/{query}")
            );
            assert_eq!(t.params.len(), 1);
            assert_eq!(t.params[0].name, "query");
        }
        other => panic!("expected Tool declaration, got: {other:?}"),
    }

    // Also verify it passes the checker
    let errors = Checker::new().check(&program);
    assert!(errors.is_empty(), "expected no check errors, got: {errors:?}");
}

// ─────────────────────────────────────────────────────────────────────
// 5. multiple_errors_collected
// ─────────────────────────────────────────────────────────────────────

/// The checker collects all semantic errors in a single pass rather than
/// stopping at the first one. Verify that multiple distinct errors are
/// returned together.
#[test]
fn multiple_errors_collected() {
    let src = r#"
        tool #fetch {
            description: <<Fetch data>>
            requires: [^net.read]
            params {
                url :: String
            }
        }

        tool #write {
            description: <<Write data>>
            requires: [^fs.write]
            params {
                path :: String
            }
        }

        agent @bad_agent {
            permits: []
            tools: [#fetch, #write]
        }

        flow broken(x :: UnknownType) -> AnotherUnknown {
            return x
        }
    "#;

    let errors = check(src);

    // We expect at least 4 errors:
    //   - MissingPermission for #fetch (net.read)
    //   - MissingPermission for #write (fs.write)
    //   - UnknownType for UnknownType
    //   - UnknownType for AnotherUnknown
    assert!(
        errors.len() >= 4,
        "expected at least 4 errors, got {}: {errors:?}",
        errors.len()
    );

    let missing_perm_count = errors
        .iter()
        .filter(|e| matches!(e, pact_core::checker::CheckError::MissingPermission { .. }))
        .count();
    let unknown_type_count = errors
        .iter()
        .filter(|e| matches!(e, pact_core::checker::CheckError::UnknownType { .. }))
        .count();

    assert!(
        missing_perm_count >= 2,
        "expected at least 2 MissingPermission errors, got {missing_perm_count}"
    );
    assert!(
        unknown_type_count >= 2,
        "expected at least 2 UnknownType errors, got {unknown_type_count}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 6. type_checking_catches_unknown_types
// ─────────────────────────────────────────────────────────────────────

#[test]
fn type_checking_catches_unknown_types() {
    let src = r#"
        flow transform(input :: NonExistentType) -> AlsoMissing {
            return input
        }
    "#;

    let errors = check(src);

    // Both the parameter type and return type are unknown.
    let unknown_types: Vec<_> = errors
        .iter()
        .filter_map(|e| match e {
            pact_core::checker::CheckError::UnknownType { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        unknown_types.contains(&"NonExistentType"),
        "expected UnknownType for 'NonExistentType', got: {unknown_types:?}"
    );
    assert!(
        unknown_types.contains(&"AlsoMissing"),
        "expected UnknownType for 'AlsoMissing', got: {unknown_types:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 7. full_program_with_all_constructs
// ─────────────────────────────────────────────────────────────────────

#[test]
fn full_program_with_all_constructs() {
    let src = r#"
        permit_tree {
            ^net {
                ^net.read
                ^net.write
            }
            ^llm {
                ^llm.query
            }
            ^fs {
                ^fs.read
                ^fs.write
            }
        }

        type Status = Success | Failure | Pending

        schema Report {
            title :: String
            body :: String
            score :: Float
        }

        tool #search {
            description: <<Search the web for information>>
            requires: [^net.read]
            params {
                query :: String
            }
            returns :: String
        }

        agent @researcher {
            permits: [^net.read, ^llm.query]
            tools: [#search]
            model: "claude-sonnet"
            prompt: <<You are a research assistant>>
        }

        flow investigate(topic :: String) -> String {
            result = @researcher -> #search(topic)
            return result
        }

        test "investigation returns a result" {
            result = @researcher -> #search("test topic")
            assert result == "search_result"
        }
    "#;

    let program = parse(src);

    // Count each declaration kind.
    let mut permit_trees = 0;
    let mut type_aliases = 0;
    let mut schemas = 0;
    let mut tools = 0;
    let mut agents = 0;
    let mut flows = 0;
    let mut tests = 0;

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::PermitTree(_) => permit_trees += 1,
            DeclKind::TypeAlias(_) => type_aliases += 1,
            DeclKind::Schema(_) => schemas += 1,
            DeclKind::Tool(_) => tools += 1,
            DeclKind::Agent(_) => agents += 1,
            DeclKind::Flow(_) => flows += 1,
            DeclKind::Test(_) => tests += 1,
            DeclKind::Import(_) => {} // imports resolved by loader
            _ => {}
        }
    }

    assert_eq!(permit_trees, 1, "expected 1 permit_tree");
    assert_eq!(type_aliases, 1, "expected 1 type alias");
    assert_eq!(schemas, 1, "expected 1 schema");
    assert_eq!(tools, 1, "expected 1 tool");
    assert_eq!(agents, 1, "expected 1 agent");
    assert_eq!(flows, 1, "expected 1 flow");
    assert_eq!(tests, 1, "expected 1 test");

    // The whole program should pass the checker with no errors.
    let errors = Checker::new().check(&program);
    assert!(errors.is_empty(), "expected no check errors, got: {errors:?}");

    // Run the flow end-to-end.
    let program = parse(src);
    let mut interp = Interpreter::new();
    let result = interp
        .run(
            &program,
            "investigate",
            vec![Value::String("quantum".into())],
        )
        .unwrap();
    assert_eq!(result, Value::ToolResult("search_result".into()));

    // Run the test declarations and verify they pass.
    let program = parse(src);
    let mut interp = Interpreter::new();
    let test_results = interp.run_tests(&program);
    assert_eq!(test_results.len(), 1);
    assert!(
        test_results[0].1,
        "test '{}' should have passed",
        test_results[0].0
    );
}
