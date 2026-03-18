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
    /// The kind of expression this node represents.
    pub kind: ExprKind,
    /// Source location of this expression.
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
        /// The agent being dispatched.
        agent: Box<Expr>,
        /// The tool the agent should invoke.
        tool: Box<Expr>,
        /// Arguments passed to the tool.
        args: Vec<Expr>,
    },

    /// Pipeline: `expr |> expr`.
    Pipeline {
        /// Left-hand side of the pipeline.
        left: Box<Expr>,
        /// Right-hand side of the pipeline.
        right: Box<Expr>,
    },

    /// Fallback chain: `expr ?> expr`.
    FallbackChain {
        /// The primary expression to attempt.
        primary: Box<Expr>,
        /// The fallback expression if the primary fails.
        fallback: Box<Expr>,
    },

    /// Parallel block: `parallel { a, b, c }`.
    Parallel(Vec<Expr>),

    /// Match expression: `match expr { pattern => body, ... }`.
    Match {
        /// The expression being matched on.
        subject: Box<Expr>,
        /// The match arms to evaluate in order.
        arms: Vec<MatchArm>,
    },

    /// Field access: `expr.field`.
    FieldAccess {
        /// The object expression to access.
        object: Box<Expr>,
        /// The field name being accessed.
        field: String,
    },

    /// Function / tool call: `name(args)`.
    FuncCall {
        /// The function or tool being called.
        callee: Box<Expr>,
        /// Arguments passed to the function.
        args: Vec<Expr>,
    },

    /// Binary operation: `a + b`, `a == b`, etc.
    BinOp {
        /// Left operand.
        left: Box<Expr>,
        /// The binary operator.
        op: BinOpKind,
        /// Right operand.
        right: Box<Expr>,
    },

    /// Return statement: `return expr`.
    Return(Box<Expr>),

    /// Fail statement: `fail "message"`.
    Fail(Box<Expr>),

    /// Variable binding: `name = expr` (used as a statement-expression).
    Assign {
        /// The variable name being assigned.
        name: String,
        /// The value expression to bind.
        value: Box<Expr>,
    },

    /// Record literal used in test blocks: `record { ... }`.
    Record(Vec<Expr>),

    /// Assert expression used in test blocks: `assert expr`.
    Assert(Box<Expr>),

    /// Typed expression for parameter passing: `expr :: Type`.
    Typed {
        /// The expression being annotated.
        expr: Box<Expr>,
        /// The type annotation.
        ty: TypeExpr,
    },

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
        /// The name of the flow to invoke.
        flow_name: String,
        /// Arguments passed to the flow.
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
    /// Source location of this match arm.
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
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Neq,
    /// Less than (`<`).
    Lt,
    /// Greater than (`>`).
    Gt,
    /// Less than or equal (`<=`).
    LtEq,
    /// Greater than or equal (`>=`).
    GtEq,
}
