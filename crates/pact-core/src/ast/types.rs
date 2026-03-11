// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-04-05

//! Type expression AST nodes.
//!
//! Type expressions appear in parameter annotations (`name :: Type`),
//! return types (`-> Type`), and schema field definitions.

use crate::span::Span;

/// A type expression in the PACT language.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub kind: TypeExprKind,
    pub span: Span,
}

/// The different forms a type expression can take.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    /// A simple named type, e.g. `String`, `Int`.
    Named(String),

    /// A generic type, e.g. `List<String>`, `Map<String, Int>`.
    Generic { name: String, args: Vec<TypeExpr> },

    /// An optional type, e.g. `String?`.
    Optional(Box<TypeExpr>),
}
