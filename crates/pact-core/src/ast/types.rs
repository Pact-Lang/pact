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
    /// The kind of type expression.
    pub kind: TypeExprKind,
    /// Source location of this type expression.
    pub span: Span,
}

/// The different forms a type expression can take.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExprKind {
    /// A simple named type, e.g. `String`, `Int`.
    Named(String),

    /// A generic type, e.g. `List<String>`, `Map<String, Int>`.
    Generic {
        /// The generic type name.
        name: String,
        /// The type arguments.
        args: Vec<TypeExpr>,
    },

    /// An optional type, e.g. `String?`.
    Optional(Box<TypeExpr>),
}
