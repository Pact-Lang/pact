// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Integration tests for pact-dispatch.
//!
//! Covers executor, providers, mediation, cache, convert roundtrips,
//! and error paths — all without network calls.

use std::collections::HashMap;
use std::time::Duration;

use pact_core::ast::stmt::DeclKind;
use pact_core::interpreter::value::Value;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

use pact_dispatch::cache::ToolCache;
use pact_dispatch::convert::{json_to_value, value_to_json};
use pact_dispatch::executor::{execute_handler, parse_handler, HandlerSpec};
use pact_dispatch::mediation::{MediationError, RuntimeMediator};
use pact_dispatch::providers::{execute_provider, ProviderRegistry};
use pact_dispatch::types::ContentBlock;

// ── Helpers ──────────────────────────────────────────────────────

fn parse_program(src: &str) -> pact_core::ast::stmt::Program {
    let mut sm = SourceMap::new();
    let id = sm.add("test.pact", src);
    let tokens = Lexer::new(src, id).lex().unwrap();
    Parser::new(&tokens).parse().unwrap()
}

fn make_tool_use(name: &str, input: serde_json::Value) -> ContentBlock {
    ContentBlock::ToolUse {
        id: "tu_integration".to_string(),
        name: name.to_string(),
        input,
    }
}

// ═══════════════════════════════════════════════════════════════════
//  1. Executor integration
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn executor_shell_echo() {
    let spec = parse_handler("sh echo hello").unwrap();
    let params = HashMap::new();
    let result = execute_handler(&spec, &params).await.unwrap();
    assert_eq!(result.trim(), "hello");
}

#[tokio::test]
async fn executor_shell_with_interpolation() {
    let spec = parse_handler("sh echo {msg}").unwrap();
    let mut params = HashMap::new();
    params.insert("msg".to_string(), "world".to_string());
    let result = execute_handler(&spec, &params).await.unwrap();
    assert_eq!(result.trim(), "world");
}

#[tokio::test]
async fn executor_builtin_echo() {
    let spec = HandlerSpec::Builtin {
        name: "echo".to_string(),
    };
    let mut params = HashMap::new();
    params.insert("message".to_string(), "integration test".to_string());
    let result = execute_handler(&spec, &params).await.unwrap();
    assert_eq!(result, "integration test");
}

#[tokio::test]
async fn executor_builtin_echo_text_key() {
    let spec = HandlerSpec::Builtin {
        name: "echo".to_string(),
    };
    let mut params = HashMap::new();
    params.insert("text".to_string(), "via text key".to_string());
    let result = execute_handler(&spec, &params).await.unwrap();
    assert_eq!(result, "via text key");
}

#[tokio::test]
async fn executor_builtin_json() {
    let spec = HandlerSpec::Builtin {
        name: "json".to_string(),
    };
    let mut params = HashMap::new();
    params.insert("name".to_string(), "Alice".to_string());
    params.insert("age".to_string(), "30".to_string());
    let result = execute_handler(&spec, &params).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["name"], "Alice");
    assert_eq!(parsed["age"], "30");
}

#[tokio::test]
async fn executor_builtin_noop() {
    let spec = HandlerSpec::Builtin {
        name: "noop".to_string(),
    };
    let params = HashMap::new();
    let result = execute_handler(&spec, &params).await.unwrap();
    assert_eq!(result, "ok");
}

// ═══════════════════════════════════════════════════════════════════
//  2. Provider integration
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn provider_fs_read_temp_file() {
    let dir = std::env::temp_dir().join("pact_dispatch_test_read");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("hello.txt");
    std::fs::write(&path, "hello from file").unwrap();

    let mut params = HashMap::new();
    params.insert("path".to_string(), path.display().to_string());
    let result = execute_provider("fs.read", &params).await.unwrap();
    assert_eq!(result, "hello from file");

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[tokio::test]
async fn provider_fs_write_temp_file() {
    let dir = std::env::temp_dir().join("pact_dispatch_test_write");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("output.txt");

    let mut params = HashMap::new();
    params.insert("path".to_string(), path.display().to_string());
    params.insert("content".to_string(), "written by test".to_string());
    let result = execute_provider("fs.write", &params).await.unwrap();
    assert!(result.contains("15 bytes"));

    let contents = std::fs::read_to_string(&path).unwrap();
    assert_eq!(contents, "written by test");

    std::fs::remove_file(&path).ok();
    std::fs::remove_dir(&dir).ok();
}

#[tokio::test]
async fn provider_fs_glob_pattern() {
    let dir = std::env::temp_dir().join("pact_dispatch_test_glob");
    std::fs::create_dir_all(&dir).ok();
    std::fs::write(dir.join("a.txt"), "a").unwrap();
    std::fs::write(dir.join("b.txt"), "b").unwrap();
    std::fs::write(dir.join("c.log"), "c").unwrap();

    let mut params = HashMap::new();
    params.insert("pattern".to_string(), format!("{}/*.txt", dir.display()));
    let result = execute_provider("fs.glob", &params).await.unwrap();
    let files: Vec<String> = serde_json::from_str(&result).unwrap();
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|f| f.contains("a.txt")));
    assert!(files.iter().any(|f| f.contains("b.txt")));

    std::fs::remove_file(dir.join("a.txt")).ok();
    std::fs::remove_file(dir.join("b.txt")).ok();
    std::fs::remove_file(dir.join("c.log")).ok();
    std::fs::remove_dir(&dir).ok();
}

#[tokio::test]
async fn provider_time_now() {
    let params = HashMap::new();
    let result = execute_provider("time.now", &params).await.unwrap();
    let secs: u64 = result.parse().unwrap();
    assert!(secs > 1_700_000_000, "timestamp should be after 2023");
}

#[tokio::test]
async fn provider_json_parse() {
    let mut params = HashMap::new();
    params.insert(
        "input".to_string(),
        r#"{"key":"value","num":42}"#.to_string(),
    );
    let result = execute_provider("json.parse", &params).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["key"], "value");
    assert_eq!(parsed["num"], 42);
}

// ═══════════════════════════════════════════════════════════════════
//  3. Mediation integration
// ═══════════════════════════════════════════════════════════════════

#[test]
fn mediation_full_program_agent_with_tools() {
    let src = r#"
        tool #search {
            description: <<Search the web.>>
            requires: [^net.read]
            params { query :: String }
            returns :: String
        }
        tool #summarize {
            description: <<Summarize text.>>
            requires: [^llm.query]
            params { text :: String }
            returns :: String
        }
        agent @researcher {
            permits: [^net.read, ^llm.query]
            tools: [#search, #summarize]
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[2].kind {
        let mediator = RuntimeMediator::new(agent, &program);

        // Both tools should pass
        let tu1 = make_tool_use("search", serde_json::json!({"query": "rust"}));
        assert!(mediator.validate_tool_use(&tu1, &program).is_ok());

        let tu2 = make_tool_use("summarize", serde_json::json!({"text": "long text..."}));
        assert!(mediator.validate_tool_use(&tu2, &program).is_ok());
    } else {
        panic!("expected agent decl at index 2");
    }
}

#[test]
fn mediation_unauthorized_tool_caught() {
    let src = r#"
        tool #search {
            description: <<Search.>>
            requires: [^net.read]
            params { query :: String }
            returns :: String
        }
        agent @limited {
            permits: [^llm.query]
            tools: []
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[1].kind {
        let mediator = RuntimeMediator::new(agent, &program);
        let tu = make_tool_use("search", serde_json::json!({"query": "hack"}));
        let err = mediator.validate_tool_use(&tu, &program).unwrap_err();
        match err {
            MediationError::UnauthorizedTool {
                tool_name,
                agent_name,
                ..
            } => {
                assert_eq!(tool_name, "search");
                assert_eq!(agent_name, "limited");
            }
            other => panic!("expected UnauthorizedTool, got {:?}", other),
        }
    } else {
        panic!("expected agent decl");
    }
}

#[test]
fn mediation_missing_permission_caught() {
    let src = r#"
        tool #fetch {
            description: <<Fetch data.>>
            requires: [^net.read]
            params { url :: String }
            returns :: String
        }
        agent @worker {
            permits: [^llm.query]
            tools: [#fetch]
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[1].kind {
        let mediator = RuntimeMediator::new(agent, &program);
        let tu = make_tool_use("fetch", serde_json::json!({"url": "https://example.com"}));
        let err = mediator.validate_tool_use(&tu, &program).unwrap_err();
        match err {
            MediationError::MissingPermission {
                permission,
                tool_name,
                ..
            } => {
                assert_eq!(permission, "net.read");
                assert_eq!(tool_name, "fetch");
            }
            other => panic!("expected MissingPermission, got {:?}", other),
        }
    } else {
        panic!("expected agent decl");
    }
}

#[test]
fn mediation_invalid_input_type_caught() {
    let src = r#"
        tool #search {
            description: <<Search.>>
            requires: [^net.read]
            params { query :: String }
            returns :: String
        }
        agent @bot {
            permits: [^net.read, ^llm.query]
            tools: [#search]
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[1].kind {
        let mediator = RuntimeMediator::new(agent, &program);
        // Pass a number where String is expected
        let tu = make_tool_use("search", serde_json::json!({"query": 12345}));
        let err = mediator.validate_tool_use(&tu, &program).unwrap_err();
        match err {
            MediationError::InvalidToolInput {
                param, expected, ..
            } => {
                assert_eq!(param, "query");
                assert_eq!(expected, "String");
            }
            other => panic!("expected InvalidToolInput, got {:?}", other),
        }
    } else {
        panic!("expected agent decl");
    }
}

#[test]
fn mediation_sensitive_data_leak_detected() {
    let src = r#"
        tool #greet {
            description: <<Greet someone.>>
            requires: [^llm.query]
            params { name :: String }
            returns :: String
        }
        agent @greeter {
            permits: [^llm.query]
            tools: [#greet]
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[1].kind {
        let mediator = RuntimeMediator::new(agent, &program);
        let output = "Here is the card: 4111111111111111 for processing.";
        let err = mediator
            .validate_output(output, "greet", &program)
            .unwrap_err();
        assert!(matches!(err, MediationError::SensitiveDataLeak { .. }));
    } else {
        panic!("expected agent decl");
    }
}

#[test]
fn mediation_scope_violation_detected() {
    let src = r#"
        tool #lookup {
            description: <<Look up info.>>
            requires: [^net.read]
            params { query :: String }
            returns :: String
        }
        agent @reader {
            permits: [^net.read, ^llm.query]
            tools: [#lookup]
        }
    "#;
    let program = parse_program(src);
    if let DeclKind::Agent(agent) = &program.decls[1].kind {
        let mediator = RuntimeMediator::new(agent, &program);

        // Storage violation: agent claims to have saved data without write perms
        let output = "I have saved your preferences to the database.";
        let err = mediator
            .validate_output(output, "lookup", &program)
            .unwrap_err();
        assert!(matches!(err, MediationError::ScopeViolation { .. }));

        // Sending violation: agent claims to have sent data without net.write
        let output2 = "I have sent the report to your email address.";
        let err2 = mediator
            .validate_output(output2, "lookup", &program)
            .unwrap_err();
        assert!(matches!(err2, MediationError::ScopeViolation { .. }));
    } else {
        panic!("expected agent decl");
    }
}

// ═══════════════════════════════════════════════════════════════════
//  4. Cache integration
// ═══════════════════════════════════════════════════════════════════

#[test]
fn cache_set_and_get() {
    let cache = ToolCache::new();
    cache.set(
        "tool:search:rust".into(),
        "cached result".into(),
        Duration::from_secs(60),
    );
    assert_eq!(
        cache.get("tool:search:rust"),
        Some("cached result".to_string())
    );
}

#[test]
fn cache_ttl_expiry() {
    let cache = ToolCache::new();
    cache.set(
        "ephemeral_key".into(),
        "temp_value".into(),
        Duration::from_millis(50),
    );
    assert_eq!(cache.get("ephemeral_key"), Some("temp_value".to_string()));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(cache.get("ephemeral_key"), None);
}

#[test]
fn cache_miss_returns_none() {
    let cache = ToolCache::new();
    assert_eq!(cache.get("nonexistent_key"), None);
}

#[test]
fn cache_overwrite_value() {
    let cache = ToolCache::new();
    cache.set("key".into(), "first".into(), Duration::from_secs(60));
    cache.set("key".into(), "second".into(), Duration::from_secs(60));
    assert_eq!(cache.get("key"), Some("second".to_string()));
}

#[test]
fn cache_clear_removes_all() {
    let cache = ToolCache::new();
    cache.set("x".into(), "1".into(), Duration::from_secs(60));
    cache.set("y".into(), "2".into(), Duration::from_secs(60));
    cache.clear();
    assert_eq!(cache.get("x"), None);
    assert_eq!(cache.get("y"), None);
}

// ═══════════════════════════════════════════════════════════════════
//  5. Convert roundtrip
// ═══════════════════════════════════════════════════════════════════

#[test]
fn convert_roundtrip_string() {
    let val = Value::String("hello world".into());
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_int() {
    let val = Value::Int(-42);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_float() {
    let val = Value::Float(2.72);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_bool() {
    let val = Value::Bool(true);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_null() {
    let val = Value::Null;
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_list() {
    let val = Value::List(vec![
        Value::String("a".into()),
        Value::Int(1),
        Value::Bool(false),
    ]);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_record() {
    let mut fields = HashMap::new();
    fields.insert("name".into(), Value::String("Alice".into()));
    fields.insert("age".into(), Value::Int(30));
    fields.insert("active".into(), Value::Bool(true));
    let val = Value::Record(fields);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_roundtrip_nested_list_in_record() {
    let mut fields = HashMap::new();
    fields.insert(
        "tags".into(),
        Value::List(vec![
            Value::String("rust".into()),
            Value::String("pact".into()),
        ]),
    );
    fields.insert("count".into(), Value::Int(2));
    let val = Value::Record(fields);
    let json = value_to_json(&val);
    let back = json_to_value(&json);
    assert_eq!(back, val);
}

#[test]
fn convert_agent_ref_to_json() {
    let val = Value::AgentRef("worker".into());
    let json = value_to_json(&val);
    // AgentRef serializes as "@worker"
    assert_eq!(json, serde_json::Value::String("@worker".to_string()));
}

#[test]
fn convert_tool_result_to_json() {
    let val = Value::ToolResult("some result".into());
    let json = value_to_json(&val);
    assert_eq!(json, serde_json::Value::String("some result".to_string()));
}

// ═══════════════════════════════════════════════════════════════════
//  6. Error paths
// ═══════════════════════════════════════════════════════════════════

#[tokio::test]
async fn error_provider_fs_read_missing_path_param() {
    let params = HashMap::new();
    let err = execute_provider("fs.read", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("path"),
        "error should mention missing 'path': {msg}"
    );
}

#[tokio::test]
async fn error_provider_fs_write_missing_path_param() {
    let params = HashMap::new();
    let err = execute_provider("fs.write", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("path"),
        "error should mention missing 'path': {msg}"
    );
}

#[tokio::test]
async fn error_provider_fs_write_missing_content_param() {
    let mut params = HashMap::new();
    params.insert("path".to_string(), "/tmp/pact_test_dummy.txt".to_string());
    let err = execute_provider("fs.write", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("content"),
        "error should mention missing 'content': {msg}"
    );
}

#[tokio::test]
async fn error_provider_fs_glob_missing_pattern() {
    let params = HashMap::new();
    let err = execute_provider("fs.glob", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("pattern"),
        "error should mention missing 'pattern': {msg}"
    );
}

#[tokio::test]
async fn error_provider_json_parse_missing_input() {
    let params = HashMap::new();
    let err = execute_provider("json.parse", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("input"),
        "error should mention missing 'input': {msg}"
    );
}

#[tokio::test]
async fn error_unknown_provider_capability() {
    let params = HashMap::new();
    let err = execute_provider("search.bing", &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown provider capability"),
        "error should say unknown provider: {msg}"
    );
}

#[test]
fn error_unknown_provider_not_in_registry() {
    let reg = ProviderRegistry::new();
    assert!(!reg.exists("search.bing"));
    assert!(reg.get("search.bing").is_none());
}

#[tokio::test]
async fn error_unknown_builtin_handler() {
    let spec = HandlerSpec::Builtin {
        name: "nonexistent_builtin".to_string(),
    };
    let params = HashMap::new();
    let err = execute_handler(&spec, &params).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown builtin handler"),
        "error should mention unknown builtin: {msg}"
    );
}

#[test]
fn error_invalid_handler_format() {
    let err = parse_handler("ftp://example.com/file").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unknown handler format"),
        "error should mention unknown format: {msg}"
    );
}

#[test]
fn error_http_handler_missing_url() {
    let err = parse_handler("http GET").unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("METHOD and URL"),
        "error should mention missing URL: {msg}"
    );
}
