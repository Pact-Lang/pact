// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-04-08

//! Expression AST nodes.
//!
//! Expressions represent computations that produce values. They form the
//! body of flows and appear inside test blocks.

use super::types::TypeExpr;
use crate::span::Span;

/// An expression node in the PACT AST.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// All expression variants in the PACT language.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Integer literal, e.g. `42`.
    IntLit(i64),

    /// Float literal, e.g. `3.14`.
    FloatLit(f64),

    /// String literal, e.g. `"hello"`.
    StringLit(String),

    /// Prompt literal, e.g. `<<You are helpful>>`.
    PromptLit(String),

    /// Boolean literal: `true` or `false`.
    BoolLit(bool),

    /// Variable reference, e.g. `name`.
    Ident(String),

    /// Agent reference, e.g. `@greeter`.
    AgentRef(String),

    /// Tool reference, e.g. `#greet`.
    ToolRef(String),

    /// Memory reference, e.g. `~context`.
    MemoryRef(String),

    /// Permission reference, e.g. `!net.read`.
    PermissionRef(Vec<String>),

    /// Skill reference, e.g. `$age_verification`.
    SkillRef(String),

    /// Template reference, e.g. `%website_copy`.
    TemplateRef(String),

    /// Agent dispatch: `@agent -> #tool(args)`.
    AgentDispatch {
        agent: Box<Expr>,
        tool: Box<Expr>,
        args: Vec<Expr>,
    },

    /// Pipeline: `expr |> expr`.
    Pipeline { left: Box<Expr>, right: Box<Expr> },

    /// Fallback chain: `expr ?> expr`.
    FallbackChain {
        primary: Box<Expr>,
        fallback: Box<Expr>,
    },

    /// Parallel block: `parallel { a, b, c }`.
    Parallel(Vec<Expr>),

    /// Match expression: `match expr { pattern => body, ... }`.
    Match {
        subject: Box<Expr>,
        arms: Vec<MatchArm>,
    },

    /// Field access: `expr.field`.
    FieldAccess { object: Box<Expr>, field: String },

    /// Function / tool call: `name(args)`.
    FuncCall { callee: Box<Expr>, args: Vec<Expr> },

    /// Binary operation: `a + b`, `a == b`, etc.
    BinOp {
        left: Box<Expr>,
        op: BinOpKind,
        right: Box<Expr>,
    },

    /// Return statement: `return expr`.
    Return(Box<Expr>),

    /// Fail statement: `fail "message"`.
    Fail(Box<Expr>),

    /// Variable binding: `name = expr` (used as a statement-expression).
    Assign { name: String, value: Box<Expr> },

    /// Record literal used in test blocks: `record { ... }`.
    Record(Vec<Expr>),

    /// Assert expression used in test blocks: `assert expr`.
    Assert(Box<Expr>),

    /// Typed expression for parameter passing: `expr :: Type`.
    Typed { expr: Box<Expr>, ty: TypeExpr },

    /// List literal: `[a, b, c]`.
    ListLit(Vec<Expr>),

    /// Record literal with named fields: `{ key: expr, ... }`.
    RecordFields(Vec<(String, Expr)>),

    /// On-error handler: `expr on_error fallback_expr`.
    OnError {
        /// The primary expression to try.
        body: Box<Expr>,
        /// The fallback expression if body fails.
        fallback: Box<Expr>,
    },

    /// Environment variable lookup: `env("API_KEY")`.
    Env(String),

    /// Flow call: `run flow_name(arg1, arg2)`.
    RunFlow {
        flow_name: String,
        args: Vec<Expr>,
    },
}

/// A single arm in a `match` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// The pattern to match against (for v0.1, string/bool/ident literals).
    pub pattern: MatchPattern,
    /// The body expression to evaluate if the pattern matches.
    pub body: Expr,
    pub span: Span,
}

/// Patterns for match arms (kept simple for v0.1).
#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern {
    /// Match a specific string literal.
    StringLit(String),
    /// Match a specific boolean.
    BoolLit(bool),
    /// Match a specific integer.
    IntLit(i64),
    /// A named binding (catch-all or enum variant).
    Ident(String),
    /// Wildcard pattern `_`.
    Wildcard,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOpKind {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Neq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}
