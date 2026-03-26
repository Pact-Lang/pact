// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! OpenAI function-calling JSON generation.
//!
//! Converts PACT tool declarations into the OpenAI function-calling format.
//! This is used by:
//! - `pact build` to generate OpenAI-compatible tool definitions
//! - `pact run --dispatch openai` to construct API requests
//!
//! # OpenAI Tool Format
//!
//! ```json
//! {
//!   "type": "function",
//!   "function": {
//!     "name": "tool_name",
//!     "description": "What this tool does",
//!     "strict": true,
//!     "parameters": {
//!       "type": "object",
//!       "properties": { ... },
//!       "required": [...]
//!     }
//!   }
//! }
//! ```

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program, ToolDecl};
use serde_json::json;

use crate::emit_common::type_to_json_schema;

/// Convert a PACT tool declaration to an OpenAI function-calling tool definition.
///
/// Returns a `serde_json::Value` in the OpenAI tool format with `"type": "function"`
/// and a nested `"function"` object containing name, description, strict mode, and
/// parameter schema.
pub fn tool_to_openai(tool: &ToolDecl) -> serde_json::Value {
    let description = match &tool.description.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
        _ => String::new(),
    };

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in &tool.params {
        let type_schema = param
            .ty
            .as_ref()
            .map(type_to_json_schema)
            .unwrap_or_else(|| json!({}));

        let mut prop = type_schema;
        if let Some(obj) = prop.as_object_mut() {
            obj.insert(
                "description".to_string(),
                json!(format!("{} parameter", param.name)),
            );
        }

        properties.insert(param.name.clone(), prop);
        required.push(json!(param.name));
    }

    let parameters = json!({
        "type": "object",
        "properties": properties,
        "required": required,
    });

    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": description,
            "strict": true,
            "parameters": parameters,
        }
    })
}

/// Generate the OpenAI function-calling tools JSON for all tools in a program.
///
/// Returns a pretty-printed JSON array where each element is an OpenAI tool
/// definition wrapping the PACT tool's name, description, and parameter schema.
pub fn generate_openai_tools_json(program: &Program) -> String {
    let tools: Vec<serde_json::Value> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) => Some(tool_to_openai(t)),
            _ => None,
        })
        .collect();

    serde_json::to_string_pretty(&tools).expect("JSON serialization should not fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse_program(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn tool_to_openai_basic() {
        let src = r#"tool #greet {
            description: <<Generate a greeting message.>>
            requires: [^llm.query]
            params {
                name :: String
            }
            returns :: String
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let openai_tool = tool_to_openai(tool);
            assert_eq!(openai_tool["type"], "function");
            assert_eq!(openai_tool["function"]["name"], "greet");
            assert_eq!(
                openai_tool["function"]["description"],
                "Generate a greeting message."
            );
            assert_eq!(openai_tool["function"]["strict"], true);

            let params = &openai_tool["function"]["parameters"];
            assert_eq!(params["type"], "object");
            assert_eq!(params["properties"]["name"]["type"], "string");
            assert_eq!(params["required"][0], "name");
        }
    }

    #[test]
    fn tool_with_multiple_params() {
        let src = r#"tool #search {
            description: <<Search for things.>>
            requires: [^net.read]
            params {
                query :: String
                limit :: Int
            }
            returns :: List<String>
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let openai_tool = tool_to_openai(tool);
            let params = &openai_tool["function"]["parameters"];
            assert_eq!(params["properties"]["query"]["type"], "string");
            assert_eq!(params["properties"]["limit"]["type"], "integer");
            assert_eq!(params["required"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn generate_openai_tools_json_output() {
        let src = r#"
            tool #a { description: <<Tool A>> requires: [] params { x :: String } }
            tool #b { description: <<Tool B>> requires: [] params { y :: Int } }
        "#;
        let program = parse_program(src);
        let json_str = generate_openai_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["type"], "function");
        assert_eq!(parsed[0]["function"]["name"], "a");
        assert_eq!(parsed[0]["function"]["strict"], true);
        assert_eq!(parsed[1]["type"], "function");
        assert_eq!(parsed[1]["function"]["name"], "b");
        assert_eq!(parsed[1]["function"]["strict"], true);
    }

    #[test]
    fn tool_with_no_params() {
        let src = r#"tool #ping {
            description: <<Ping the server.>>
            requires: [^net.read]
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let openai_tool = tool_to_openai(tool);
            let params = &openai_tool["function"]["parameters"];
            assert_eq!(params["type"], "object");
            assert!(params["properties"].as_object().unwrap().is_empty());
            assert!(params["required"].as_array().unwrap().is_empty());
        }
    }
}
