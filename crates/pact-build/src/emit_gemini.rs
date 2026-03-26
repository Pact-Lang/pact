// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! Google Gemini function declarations generation.
//!
//! Converts PACT tool declarations into the Gemini `function_declarations`
//! format. Gemini uses uppercase type names (`STRING`, `INTEGER`, `NUMBER`,
//! `BOOLEAN`, `ARRAY`, `OBJECT`) unlike OpenAI/Claude which use lowercase.
//!
//! # Gemini Function Declaration Format
//!
//! ```json
//! [
//!   {
//!     "name": "tool_name",
//!     "description": "description",
//!     "parameters": {
//!       "type": "OBJECT",
//!       "properties": { ... },
//!       "required": [...]
//!     }
//!   }
//! ]
//! ```

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};
use serde_json::json;

/// Generate Gemini function declarations JSON from a PACT program.
///
/// Iterates over all tool declarations and produces a JSON array of
/// Gemini-compatible function declarations with uppercase type names.
pub fn generate_gemini_tools_json(program: &Program) -> String {
    let tools: Vec<serde_json::Value> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) => Some(tool_to_gemini(t)),
            _ => None,
        })
        .collect();

    serde_json::to_string_pretty(&tools).expect("JSON serialization should not fail")
}

/// Convert a single PACT tool declaration to a Gemini function declaration.
fn tool_to_gemini(tool: &pact_core::ast::stmt::ToolDecl) -> serde_json::Value {
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
            .map(type_to_gemini_schema)
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

    json!({
        "name": tool.name,
        "description": description,
        "parameters": {
            "type": "OBJECT",
            "properties": properties,
            "required": required,
        }
    })
}

/// Convert a PACT type expression to a Gemini schema with uppercase type names.
///
/// Gemini uses `STRING`, `INTEGER`, `NUMBER`, `BOOLEAN`, `ARRAY`, `OBJECT`
/// rather than the lowercase variants used by OpenAI and Claude.
fn type_to_gemini_schema(ty: &pact_core::ast::types::TypeExpr) -> serde_json::Value {
    use pact_core::ast::types::TypeExprKind;

    match &ty.kind {
        TypeExprKind::Named(name) => match name.as_str() {
            "String" => json!({"type": "STRING"}),
            "Int" => json!({"type": "INTEGER"}),
            "Float" => json!({"type": "NUMBER"}),
            "Bool" => json!({"type": "BOOLEAN"}),
            "Any" => json!({}),
            _ => json!({"type": "STRING"}),
        },
        TypeExprKind::Generic { name, args } => match name.as_str() {
            "List" => {
                let items = args
                    .first()
                    .map(type_to_gemini_schema)
                    .unwrap_or_else(|| json!({}));
                json!({"type": "ARRAY", "items": items})
            }
            "Map" => {
                let value_type = args
                    .get(1)
                    .map(type_to_gemini_schema)
                    .unwrap_or_else(|| json!({}));
                json!({"type": "OBJECT", "additionalProperties": value_type})
            }
            _ => json!({"type": "OBJECT"}),
        },
        TypeExprKind::Optional(inner) => type_to_gemini_schema(inner),
    }
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
    fn gemini_tool_basic() {
        let src = r#"tool #greet {
            description: <<Generate a greeting message.>>
            requires: [^llm.query]
            params {
                name :: String
            }
            returns :: String
        }"#;
        let program = parse_program(src);
        let json_str = generate_gemini_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["name"], "greet");
        assert_eq!(parsed[0]["description"], "Generate a greeting message.");
        assert_eq!(parsed[0]["parameters"]["type"], "OBJECT");
        assert_eq!(
            parsed[0]["parameters"]["properties"]["name"]["type"],
            "STRING"
        );
        assert_eq!(parsed[0]["parameters"]["required"][0], "name");
    }

    #[test]
    fn gemini_tool_multiple_params() {
        let src = r#"tool #search {
            description: <<Search for things.>>
            requires: [^net.read]
            params {
                query :: String
                limit :: Int
                verbose :: Bool
            }
            returns :: List<String>
        }"#;
        let program = parse_program(src);
        let json_str = generate_gemini_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

        let props = &parsed[0]["parameters"]["properties"];
        assert_eq!(props["query"]["type"], "STRING");
        assert_eq!(props["limit"]["type"], "INTEGER");
        assert_eq!(props["verbose"]["type"], "BOOLEAN");
        assert_eq!(
            parsed[0]["parameters"]["required"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn gemini_type_mapping_uppercase() {
        use pact_core::ast::types::{TypeExpr, TypeExprKind};
        use pact_core::span::{SourceId, Span};

        let span = Span::new(SourceId(0), 0, 0);

        let string_ty = TypeExpr {
            kind: TypeExprKind::Named("String".into()),
            span,
        };
        assert_eq!(type_to_gemini_schema(&string_ty), json!({"type": "STRING"}));

        let int_ty = TypeExpr {
            kind: TypeExprKind::Named("Int".into()),
            span,
        };
        assert_eq!(type_to_gemini_schema(&int_ty), json!({"type": "INTEGER"}));

        let float_ty = TypeExpr {
            kind: TypeExprKind::Named("Float".into()),
            span,
        };
        assert_eq!(type_to_gemini_schema(&float_ty), json!({"type": "NUMBER"}));

        let bool_ty = TypeExpr {
            kind: TypeExprKind::Named("Bool".into()),
            span,
        };
        assert_eq!(type_to_gemini_schema(&bool_ty), json!({"type": "BOOLEAN"}));

        let list_ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "List".into(),
                args: vec![TypeExpr {
                    kind: TypeExprKind::Named("String".into()),
                    span,
                }],
            },
            span,
        };
        assert_eq!(
            type_to_gemini_schema(&list_ty),
            json!({"type": "ARRAY", "items": {"type": "STRING"}})
        );

        let map_ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "Map".into(),
                args: vec![
                    TypeExpr {
                        kind: TypeExprKind::Named("String".into()),
                        span,
                    },
                    TypeExpr {
                        kind: TypeExprKind::Named("Int".into()),
                        span,
                    },
                ],
            },
            span,
        };
        assert_eq!(
            type_to_gemini_schema(&map_ty),
            json!({"type": "OBJECT", "additionalProperties": {"type": "INTEGER"}})
        );
    }

    #[test]
    fn gemini_multiple_tools() {
        let src = r#"
            tool #a { description: <<Tool A>> requires: [] params { x :: String } }
            tool #b { description: <<Tool B>> requires: [] params { y :: Int } }
            tool #c { description: <<Tool C>> requires: [] params { z :: Float } }
        "#;
        let program = parse_program(src);
        let json_str = generate_gemini_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0]["name"], "a");
        assert_eq!(parsed[1]["name"], "b");
        assert_eq!(parsed[2]["name"], "c");
        assert_eq!(parsed[0]["parameters"]["properties"]["x"]["type"], "STRING");
        assert_eq!(
            parsed[1]["parameters"]["properties"]["y"]["type"],
            "INTEGER"
        );
        assert_eq!(parsed[2]["parameters"]["properties"]["z"]["type"], "NUMBER");
    }

    #[test]
    fn gemini_empty_program() {
        let program = Program { decls: vec![] };
        let json_str = generate_gemini_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();

        assert!(parsed.is_empty());
    }
}
