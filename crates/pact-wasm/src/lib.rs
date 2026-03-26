// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! WebAssembly bindings for PACT ↔ Mermaid agentflow conversion.
//!
//! Provides two functions for browser/Node.js use:
//! - `pact_to_agentflow` — convert .pact source to Mermaid agentflow text
//! - `agentflow_to_pact` — convert Mermaid agentflow text to .pact source

use wasm_bindgen::prelude::*;

use pact_core::checker::Checker;
use pact_core::lexer::Lexer;
use pact_core::parser::Parser;
use pact_core::span::SourceMap;

/// Convert .pact source code to Mermaid agentflow diagram text.
///
/// Returns the agentflow string on success, or throws a JS error on failure.
#[wasm_bindgen]
pub fn pact_to_agentflow(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| JsError::new(&format!("Parse error: {e}")))?;

    Ok(pact_mermaid::agentflow_emit::pact_to_agentflow(&program))
}

/// Convert Mermaid agentflow diagram text to .pact source code.
///
/// Returns the PACT source string on success, or throws a JS error on failure.
#[wasm_bindgen]
pub fn agentflow_to_pact(source: &str) -> Result<String, JsError> {
    pact_mermaid::diagram_to_pact(source).map_err(|e| JsError::new(&format!("{e}")))
}

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

/// Convert .pact source to agentflow JSON AST.
///
/// Returns a JSON string representing the agentflow graph structure.
#[wasm_bindgen]
pub fn pact_to_agentflow_json(source: &str) -> Result<String, JsError> {
    let mut sm = SourceMap::new();
    let id = sm.add("input.pact", source);
    let tokens = Lexer::new(source, id)
        .lex()
        .map_err(|e| JsError::new(&format!("Lex error: {e}")))?;
    let program = Parser::new(&tokens)
        .parse()
        .map_err(|e| JsError::new(&format!("Parse error: {e}")))?;

    let json = pact_mermaid::agentflow_emit::pact_to_agentflow_json(&program);
    serde_json::to_string_pretty(&json)
        .map_err(|e| JsError::new(&format!("JSON serialization error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pact_to_agentflow() {
        let source = r#"
            agent @researcher {
                permits: [^llm.query]
                tools: [#search]
            }
            flow main(query :: String) -> String {
                result = @researcher -> #search(query)
                return result
            }
        "#;
        let result = pact_to_agentflow(source).unwrap();
        assert!(result.contains("agentflow"));
        assert!(result.contains("researcher"));
    }

    #[test]
    fn test_agentflow_to_pact() {
        let source = r#"agentflow TB
agent researcher["Researcher"]
    search
end
researcher@{
    model: "claude-sonnet-4-20250514"
    permits: "llm.query"
}
"#;
        let result = agentflow_to_pact(source).unwrap();
        assert!(result.contains("agent @researcher"));
    }

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
    fn test_pact_to_agentflow_json() {
        let source = "agent @g { permits: [^llm.query] tools: [#greet] }";
        let result = pact_to_agentflow_json(source).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(parsed.get("direction").is_some());
    }

    #[test]
    fn test_roundtrip() {
        let pact_source = r#"
            agent @writer {
                permits: [^llm.query]
                tools: [#draft]
            }
            flow write(topic :: String) -> String {
                result = @writer -> #draft(topic)
                return result
            }
        "#;
        let mermaid = pact_to_agentflow(pact_source).unwrap();
        let pact_back = agentflow_to_pact(&mermaid).unwrap();
        assert!(pact_back.contains("agent @writer"));
    }
}

#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn wasm_pact_to_agentflow() {
        let source = "agent @g { permits: [^llm.query] tools: [#greet] }";
        let result = pact_to_agentflow(source).unwrap();
        assert!(result.contains("agentflow"));
    }

    #[wasm_bindgen_test]
    fn wasm_agentflow_to_pact() {
        let source = "agentflow TB\nagent researcher[\"Researcher\"]\n    search\nend\nresearcher@{\n    model: \"claude-sonnet-4-20250514\"\n    permits: \"llm.query\"\n}\n";
        let result = agentflow_to_pact(source).unwrap();
        assert!(result.contains("agent @researcher"));
    }

    #[wasm_bindgen_test]
    fn wasm_pact_check() {
        let result = pact_check("agent @g { permits: [^llm.query] tools: [#greet] }").unwrap();
        assert_eq!(result, "OK");
    }

    #[wasm_bindgen_test]
    fn wasm_roundtrip() {
        let source = "agent @g { permits: [^llm.query] tools: [#greet] }";
        let mermaid = pact_to_agentflow(source).unwrap();
        let pact_back = agentflow_to_pact(&mermaid).unwrap();
        assert!(pact_back.contains("agent @g"));
    }
}
