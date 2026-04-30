// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! WebAssembly bindings for PACT — check, format, build, and document .pact programs.
//!
//! Provides browser/Node.js functions:
//! - `pact_check` — validate syntax and semantics
//! - `pact_fmt` — format .pact source to canonical style
//! - `pact_doc` — generate Markdown documentation
//! - `pact_agent_cards` — generate A2A agent card JSON

use wasm_bindgen::prelude::*;

use pact_core::checker::Checker;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

/// Validate .pact source code for syntax and semantic errors.
///
/// Returns "OK" if valid, or a newline-separated list of errors.
#[wasm_bindgen]
pub fn pact_check(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let (program, parse_errors) = Parser::new(&tokens).parse_collecting_errors();
    let check_errors = Checker::new().check(&program);

    if parse_errors.is_empty() && check_errors.is_empty() {
        return Ok("OK".to_string());
    }

    let mut messages = Vec::new();
    for e in &parse_errors {
        messages.push(format!("Parse error: {e}"));
    }
    for e in &check_errors {
        messages.push(format!("Check error: {e}"));
    }
    Ok(messages.join("\n"))
}

/// Format .pact source code to canonical style.
///
/// Returns the formatted source string on success, or throws a JS error on failure.
#[wasm_bindgen]
pub fn pact_fmt(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| JsError::new(&format!("Parse error: {e}")))?;

    Ok(pact_core::formatter::format_program(&program))
}

/// Generate Markdown documentation from .pact source.
///
/// Returns a Markdown string documenting all declarations in the program.
#[wasm_bindgen]
pub fn pact_doc(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| JsError::new(&format!("Parse error: {e}")))?;

    Ok(pact_core::doc::generate_docs(&program, "input.pact"))
}

/// Generate Agent Card JSON for all agents in .pact source.
///
/// Returns a JSON object mapping agent names to their A2A agent card JSON.
#[wasm_bindgen]
pub fn pact_agent_cards(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| JsError::new(&format!("Parse error: {e}")))?;

    let cards = pact_build::emit_agent_card::generate_all_agent_cards(&program, "input.pact");
    let map: serde_json::Map<String, serde_json::Value> = cards
        .into_iter()
        .map(|(name, json_str)| {
            let val =
                serde_json::from_str(&json_str).unwrap_or(serde_json::Value::String(json_str));
            (name, val)
        })
        .collect();

    serde_json::to_string_pretty(&serde_json::Value::Object(map))
        .map_err(|e| JsError::new(&format!("JSON serialization error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pact_check_valid() {
        let result = pact_check("agent @g { permits: [^llm.query] tools: [#greet] }").unwrap();
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_pact_check_invalid() {
        let result = pact_check("agent { }").unwrap();
        assert!(result.contains("error") || result.contains("Error"));
    }

    #[test]
    fn test_pact_fmt() {
        let source = "agent @g{permits:[^llm.query] tools:[#greet]}";
        let result = pact_fmt(source).unwrap();
        assert!(result.contains("agent @g"));
        let result2 = pact_fmt(&result).unwrap();
        assert_eq!(result, result2, "formatter should be idempotent");
    }

    #[test]
    fn test_pact_doc() {
        let source = r#"
            tool #search {
                description: <<Search the web.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
                prompt: <<Research agent.>>
            }
        "#;
        let result = pact_doc(source).unwrap();
        assert!(result.contains("# input.pact"), "should have title");
        assert!(result.contains("researcher"), "should document agent");
        assert!(result.contains("search"), "should document tool");
    }

    #[test]
    fn test_pact_agent_cards() {
        let source = r#"
            tool #search {
                description: <<Search the web.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
                model: "claude-sonnet-4-20250514"
                prompt: <<Research agent.>>
            }
        "#;
        let result = pact_agent_cards(source).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.is_object(), "should return a JSON object");
        let keys: Vec<&String> = parsed.as_object().unwrap().keys().collect();
        assert!(!keys.is_empty(), "should have at least one agent card");
    }

    #[test]
    fn test_pact_check_federation() {
        let source = r#"
            federation {
                "https://agents.example.com" trust: [^llm.query]
            }
            tool #greet {
                description: <<Greet.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @remote {
                permits: [^llm.query]
                tools: [#greet]
                endpoint: "https://agents.example.com/remote"
                prompt: <<Remote agent.>>
            }
        "#;
        let result = pact_check(source).unwrap();
        assert_eq!(result, "OK", "federation should pass check: {result}");
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn wasm_pact_check() {
        let result = pact_check("agent @g { permits: [^llm.query] tools: [#greet] }").unwrap();
        assert_eq!(result, "OK");
    }
}
