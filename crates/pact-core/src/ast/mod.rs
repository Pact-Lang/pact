// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-04-03

//! Abstract Syntax Tree definitions for the PACT language.
//!
//! The AST is the structured representation of a parsed `.pact` file.
//! It is produced by the [parser](crate::parser) and consumed by the
//! [checker](crate::checker) and [interpreter](crate::interpreter).

pub mod expr;
pub mod stmt;
pub mod types;
pub mod visit;

pub use expr::{BinOpKind, Expr, ExprKind, MatchArm, MatchPattern};
pub use stmt::{
    AgentBundleDecl, AgentDecl, Decl, DeclKind, FlowDecl, Param, PermitNode, PermitTreeDecl,
    Program, SchemaDecl, SchemaField, TestDecl, ToolDecl, TypeAliasDecl,
};
pub use types::{TypeExpr, TypeExprKind};
pub use visit::Visitor;
