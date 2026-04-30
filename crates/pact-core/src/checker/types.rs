// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-06-28

//! Type checking and inference for the PACT language.
//!
//! For v0.1, type checking is lightweight: it verifies that referenced types
//! exist (built-in or schema-defined) and that field accesses reference valid
//! schema fields.
//!
//! The [`TypeInference`] engine provides basic type inference for variables
//! assigned within flows, tracking their inferred types and detecting
//! incompatible usage.

use std::collections::HashMap;

use crate::ast::expr::{Expr, ExprKind};
use crate::ast::stmt::{DeclKind, Program};
use crate::checker::scope::SymbolKind;

/// Built-in type names recognized by the PACT type system.
pub const BUILTIN_TYPES: &[&str] = &[
    // Core types
    "String", "Int", "Float", "Bool", "List", "Map", "Optional", "Any", "Record",
    // Output format types
    "HTML", "Webpage", "Website",
    "PDF",
    "Markdown", "MD",
    "JSON",
    "CSV",
    "YAML", "YML",
    "SVG",
    "XML",
    "SQL",
    "Image", "PNG", "JPG", "JPEG", "Photo",
    "Audio", "MP3", "WAV", "Sound", "Voice",
    "Video", "MP4", "Clip", "Animation",
    "Code", "Project", "Archive", "ZIP",
    "Slides", "Presentation", "PPTX", "Deck",
    "Excel", "XLSX", "Spreadsheet", "Workbook",
];

/// Check if a type name is a built-in type.
pub fn is_builtin_type(name: &str) -> bool {
    BUILTIN_TYPES.contains(&name)
}

/// The inferred type of a variable.
#[derive(Debug, Clone, PartialEq)]
pub enum InferredType {
    /// A known concrete type.
    Known(String),
    /// Type could not be determined.
    Unknown,
}

impl InferredType {
    /// Check if two inferred types are compatible.
    /// `Any` and `Unknown` are compatible with everything.
    pub fn is_compatible(&self, other: &InferredType) -> bool {
        match (self, other) {
            (InferredType::Unknown, _) | (_, InferredType::Unknown) => true,
            (InferredType::Known(a), InferredType::Known(b)) => a == b || a == "Any" || b == "Any",
        }
    }
}

/// A warning produced during type inference.
#[derive(Debug, Clone)]
pub struct TypeWarning {
    /// The variable name.
    pub variable: String,
    /// The previously inferred type.
    pub expected: String,
    /// The new incompatible type.
    pub found: String,
    /// A human-readable message.
    pub message: String,
}

/// Basic type inference engine for PACT programs.
///
/// Walks flow bodies to infer variable types from assignments and detects
/// when a variable is reassigned with an incompatible type.
#[derive(Debug, Default)]
pub struct TypeInference {
    /// Map from variable name to its inferred type.
    pub inferred: HashMap<String, InferredType>,
    /// Warnings about type incompatibilities.
    pub warnings: Vec<TypeWarning>,
}

impl TypeInference {
    /// Create a new empty inference context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Run type inference on an entire program.
    pub fn infer_program(
        &mut self,
        program: &Program,
        symbols: &crate::checker::scope::SymbolTable,
    ) {
        for decl in &program.decls {
            if let DeclKind::Flow(f) = &decl.kind {
                // Reset per-flow inference
                self.inferred.clear();

                // Seed parameter types
                for param in &f.params {
                    if let Some(ty) = &param.ty {
                        let type_name = crate::checker::Checker::type_expr_to_string(ty);
                        self.inferred
                            .insert(param.name.clone(), InferredType::Known(type_name));
                    }
                }

                // Walk body expressions
                for expr in &f.body {
                    self.infer_expr(expr, symbols);
                }
            }
        }
    }

    /// Infer the type produced by an expression.
    pub fn infer_expr(
        &mut self,
        expr: &Expr,
        symbols: &crate::checker::scope::SymbolTable,
    ) -> InferredType {
        match &expr.kind {
            ExprKind::StringLit(_) | ExprKind::PromptLit(_) => InferredType::Known("String".into()),
            ExprKind::IntLit(_) => InferredType::Known("Int".into()),
            ExprKind::FloatLit(_) => InferredType::Known("Float".into()),
            ExprKind::BoolLit(_) => InferredType::Known("Bool".into()),
            ExprKind::ListLit(_) => InferredType::Known("List".into()),
            ExprKind::RecordFields(_) => InferredType::Known("Record".into()),

            ExprKind::Ident(name) => self
                .inferred
                .get(name)
                .cloned()
                .unwrap_or(InferredType::Unknown),

            ExprKind::Assign { name, value } => {
                let val_type = self.infer_expr(value, symbols);

                // Check for incompatible reassignment
                if let Some(existing) = self.inferred.get(name) {
                    if !existing.is_compatible(&val_type) {
                        if let (InferredType::Known(exp), InferredType::Known(found)) =
                            (existing, &val_type)
                        {
                            self.warnings.push(TypeWarning {
                                variable: name.clone(),
                                expected: exp.clone(),
                                found: found.clone(),
                                message: format!(
                                    "variable '{}' was inferred as {} but is being assigned {}",
                                    name, exp, found
                                ),
                            });
                        }
                    }
                }

                self.inferred.insert(name.clone(), val_type.clone());
                val_type
            }

            ExprKind::AgentDispatch { tool, .. } => {
                // If the tool is declared and has a return type, use it
                if let ExprKind::ToolRef(tool_name) = &tool.kind {
                    if let Some(SymbolKind::Tool {
                        return_type: Some(rt),
                        ..
                    }) = symbols.lookup(tool_name)
                    {
                        return InferredType::Known(rt.clone());
                    }
                }
                InferredType::Unknown
            }

            ExprKind::BinOp { left, op, right } => {
                let l = self.infer_expr(left, symbols);
                let _r = self.infer_expr(right, symbols);
                match op {
                    crate::ast::expr::BinOpKind::Eq
                    | crate::ast::expr::BinOpKind::Neq
                    | crate::ast::expr::BinOpKind::Lt
                    | crate::ast::expr::BinOpKind::Gt
                    | crate::ast::expr::BinOpKind::LtEq
                    | crate::ast::expr::BinOpKind::GtEq => InferredType::Known("Bool".into()),
                    _ => l, // arithmetic preserves the type of the left operand
                }
            }

            ExprKind::Return(inner) | ExprKind::Fail(inner) | ExprKind::Assert(inner) => {
                self.infer_expr(inner, symbols)
            }

            ExprKind::FuncCall { callee, .. } => {
                if let ExprKind::Ident(name) = &callee.kind {
                    if let Some(SymbolKind::Flow { .. }) = symbols.lookup(name) {
                        // We don't track flow return types in SymbolKind::Flow yet
                        return InferredType::Unknown;
                    }
                }
                InferredType::Unknown
            }

            ExprKind::FieldAccess { .. } => InferredType::Unknown,
            ExprKind::Pipeline { right, .. } => self.infer_expr(right, symbols),
            ExprKind::FallbackChain { primary, .. } => self.infer_expr(primary, symbols),

            _ => InferredType::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_types_recognized() {
        assert!(is_builtin_type("String"));
        assert!(is_builtin_type("Int"));
        assert!(is_builtin_type("List"));
        assert!(is_builtin_type("Record"));
        assert!(!is_builtin_type("CustomType"));

        // Output format types
        assert!(is_builtin_type("HTML"));
        assert!(is_builtin_type("PDF"));
        assert!(is_builtin_type("JSON"));
        assert!(is_builtin_type("CSV"));
        assert!(is_builtin_type("Image"));
        assert!(is_builtin_type("Slides"));
        assert!(is_builtin_type("Excel"));
    }

    #[test]
    fn inferred_type_compatibility() {
        let string_ty = InferredType::Known("String".into());
        let int_ty = InferredType::Known("Int".into());
        let any_ty = InferredType::Known("Any".into());
        let unknown = InferredType::Unknown;

        assert!(string_ty.is_compatible(&string_ty));
        assert!(!string_ty.is_compatible(&int_ty));
        assert!(string_ty.is_compatible(&any_ty));
        assert!(any_ty.is_compatible(&int_ty));
        assert!(unknown.is_compatible(&string_ty));
        assert!(string_ty.is_compatible(&unknown));
    }

    #[test]
    fn infer_literal_types() {
        use crate::span::{SourceId, Span};

        let symbols = crate::checker::scope::SymbolTable::new();
        let mut inference = TypeInference::new();

        let dummy_span = Span::new(SourceId(0), 0, 1);

        let string_expr = Expr {
            kind: ExprKind::StringLit("hello".into()),
            span: dummy_span,
        };
        assert_eq!(
            inference.infer_expr(&string_expr, &symbols),
            InferredType::Known("String".into())
        );

        let int_expr = Expr {
            kind: ExprKind::IntLit(42),
            span: dummy_span,
        };
        assert_eq!(
            inference.infer_expr(&int_expr, &symbols),
            InferredType::Known("Int".into())
        );

        let bool_expr = Expr {
            kind: ExprKind::BoolLit(true),
            span: dummy_span,
        };
        assert_eq!(
            inference.infer_expr(&bool_expr, &symbols),
            InferredType::Known("Bool".into())
        );
    }

    #[test]
    fn infer_assignment_tracks_type() {
        use crate::span::{SourceId, Span};

        let symbols = crate::checker::scope::SymbolTable::new();
        let mut inference = TypeInference::new();

        let dummy_span = Span::new(SourceId(0), 0, 1);

        let assign_expr = Expr {
            kind: ExprKind::Assign {
                name: "x".into(),
                value: Box::new(Expr {
                    kind: ExprKind::StringLit("hello".into()),
                    span: dummy_span,
                }),
            },
            span: dummy_span,
        };

        inference.infer_expr(&assign_expr, &symbols);
        assert_eq!(
            inference.inferred.get("x"),
            Some(&InferredType::Known("String".into()))
        );
    }

    #[test]
    fn infer_incompatible_reassignment_warns() {
        use crate::span::{SourceId, Span};

        let symbols = crate::checker::scope::SymbolTable::new();
        let mut inference = TypeInference::new();

        let dummy_span = Span::new(SourceId(0), 0, 1);

        // First assignment: x = "hello" (String)
        let assign1 = Expr {
            kind: ExprKind::Assign {
                name: "x".into(),
                value: Box::new(Expr {
                    kind: ExprKind::StringLit("hello".into()),
                    span: dummy_span,
                }),
            },
            span: dummy_span,
        };
        inference.infer_expr(&assign1, &symbols);

        // Second assignment: x = 42 (Int) — should warn
        let assign2 = Expr {
            kind: ExprKind::Assign {
                name: "x".into(),
                value: Box::new(Expr {
                    kind: ExprKind::IntLit(42),
                    span: dummy_span,
                }),
            },
            span: dummy_span,
        };
        inference.infer_expr(&assign2, &symbols);

        assert_eq!(inference.warnings.len(), 1);
        assert_eq!(inference.warnings[0].variable, "x");
        assert_eq!(inference.warnings[0].expected, "String");
        assert_eq!(inference.warnings[0].found, "Int");
    }

    #[test]
    fn infer_dispatch_return_type() {
        use crate::span::{SourceId, Span};

        let mut symbols = crate::checker::scope::SymbolTable::new();
        symbols.define(
            "search".into(),
            SymbolKind::Tool {
                requires: vec![],
                params: vec![],
                return_type: Some("String".into()),
            },
        );

        let mut inference = TypeInference::new();
        let dummy_span = Span::new(SourceId(0), 0, 1);

        let dispatch_expr = Expr {
            kind: ExprKind::AgentDispatch {
                agent: Box::new(Expr {
                    kind: ExprKind::AgentRef("bot".into()),
                    span: dummy_span,
                }),
                tool: Box::new(Expr {
                    kind: ExprKind::ToolRef("search".into()),
                    span: dummy_span,
                }),
                args: vec![],
            },
            span: dummy_span,
        };

        let result = inference.infer_expr(&dispatch_expr, &symbols);
        assert_eq!(result, InferredType::Known("String".into()));
    }
}
