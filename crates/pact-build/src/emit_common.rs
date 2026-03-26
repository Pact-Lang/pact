// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! Shared helpers for PACT build emitters.
//!
//! Functions in this module are used by multiple emitters (Claude JSON,
//! Markdown, skill files, etc.) to avoid duplicating conversion logic.

use pact_core::ast::expr::ExprKind;
use pact_core::ast::types::{TypeExpr, TypeExprKind};
use serde_json::{json, Value as JsonValue};

/// Convert a PACT type expression to a JSON Schema type.
///
/// Maps PACT's built-in types to their JSON Schema equivalents:
///
/// | PACT type      | JSON Schema                                    |
/// |----------------|------------------------------------------------|
/// | `String`       | `{"type": "string"}`                           |
/// | `Int`          | `{"type": "integer"}`                          |
/// | `Float`        | `{"type": "number"}`                           |
/// | `Bool`         | `{"type": "boolean"}`                          |
/// | `Any`          | `{}`                                           |
/// | `List<T>`      | `{"type": "array", "items": <T>}`              |
/// | `Map<K, V>`    | `{"type": "object", "additionalProperties": <V>}` |
/// | `Optional<T>`  | schema of `T` (nullable not emitted)           |
pub fn type_to_json_schema(ty: &TypeExpr) -> JsonValue {
    match &ty.kind {
        TypeExprKind::Named(name) => match name.as_str() {
            "String" => json!({"type": "string"}),
            "Int" => json!({"type": "integer"}),
            "Float" => json!({"type": "number"}),
            "Bool" => json!({"type": "boolean"}),
            "Any" => json!({}),
            _ => json!({"type": "string"}),
        },
        TypeExprKind::Generic { name, args } => match name.as_str() {
            "List" => {
                let items = args
                    .first()
                    .map(type_to_json_schema)
                    .unwrap_or_else(|| json!({}));
                json!({"type": "array", "items": items})
            }
            "Map" => {
                let value_type = args
                    .get(1)
                    .map(type_to_json_schema)
                    .unwrap_or_else(|| json!({}));
                json!({"type": "object", "additionalProperties": value_type})
            }
            _ => json!({"type": "object"}),
        },
        TypeExprKind::Optional(inner) => type_to_json_schema(inner),
    }
}

/// Extract the text content from a `PromptLit` or `StringLit` expression.
///
/// Returns the trimmed string value for prompt and string literals,
/// or an empty string for any other expression kind.
pub fn extract_prompt_text(expr: &pact_core::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::ast::expr::Expr;
    use pact_core::ast::types::TypeExpr;
    use pact_core::span::{SourceId, Span};

    fn span() -> Span {
        Span::new(SourceId(0), 0, 0)
    }

    #[test]
    fn named_types() {
        let cases = [
            ("String", json!({"type": "string"})),
            ("Int", json!({"type": "integer"})),
            ("Float", json!({"type": "number"})),
            ("Bool", json!({"type": "boolean"})),
            ("Any", json!({})),
            ("Unknown", json!({"type": "string"})),
        ];
        for (name, expected) in cases {
            let ty = TypeExpr {
                kind: TypeExprKind::Named(name.into()),
                span: span(),
            };
            assert_eq!(type_to_json_schema(&ty), expected, "failed for {name}");
        }
    }

    #[test]
    fn generic_list() {
        let ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "List".into(),
                args: vec![TypeExpr {
                    kind: TypeExprKind::Named("Int".into()),
                    span: span(),
                }],
            },
            span: span(),
        };
        assert_eq!(
            type_to_json_schema(&ty),
            json!({"type": "array", "items": {"type": "integer"}})
        );
    }

    #[test]
    fn generic_map() {
        let ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "Map".into(),
                args: vec![
                    TypeExpr {
                        kind: TypeExprKind::Named("String".into()),
                        span: span(),
                    },
                    TypeExpr {
                        kind: TypeExprKind::Named("Bool".into()),
                        span: span(),
                    },
                ],
            },
            span: span(),
        };
        assert_eq!(
            type_to_json_schema(&ty),
            json!({"type": "object", "additionalProperties": {"type": "boolean"}})
        );
    }

    #[test]
    fn optional_unwraps() {
        let ty = TypeExpr {
            kind: TypeExprKind::Optional(Box::new(TypeExpr {
                kind: TypeExprKind::Named("Float".into()),
                span: span(),
            })),
            span: span(),
        };
        assert_eq!(type_to_json_schema(&ty), json!({"type": "number"}));
    }

    #[test]
    fn extract_prompt_lit() {
        let expr = Expr {
            kind: ExprKind::PromptLit("  Hello world  ".into()),
            span: span(),
        };
        assert_eq!(extract_prompt_text(&expr), "Hello world");
    }

    #[test]
    fn extract_string_lit() {
        let expr = Expr {
            kind: ExprKind::StringLit("  test  ".into()),
            span: span(),
        };
        assert_eq!(extract_prompt_text(&expr), "test");
    }

    #[test]
    fn extract_other_expr_empty() {
        let expr = Expr {
            kind: ExprKind::IntLit(42),
            span: span(),
        };
        assert_eq!(extract_prompt_text(&expr), "");
    }
}
