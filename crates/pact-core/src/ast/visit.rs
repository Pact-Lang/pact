// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-04-15

//! Visitor trait for AST traversal.
//!
//! The [`Visitor`] trait provides a mechanism for walking the AST without
//! modifying it. Each method has a default implementation that recurses
//! into child nodes, so visitors only need to override the methods they
//! care about.

use super::expr::{Expr, ExprKind, MatchArm};
use super::stmt::{Decl, DeclKind, Program};

/// A visitor that walks the PACT AST.
///
/// Override any method to perform custom logic at that node. The default
/// implementations simply recurse into children.
pub trait Visitor {
    /// Visit a complete program.
    fn visit_program(&mut self, program: &Program) {
        for decl in &program.decls {
            self.visit_decl(decl);
        }
    }

    /// Visit a top-level declaration.
    fn visit_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Agent(a) => {
                for p in &a.permits {
                    self.visit_expr(p);
                }
                for t in &a.tools {
                    self.visit_expr(t);
                }
                if let Some(m) = &a.model {
                    self.visit_expr(m);
                }
                if let Some(p) = &a.prompt {
                    self.visit_expr(p);
                }
                for m in &a.memory {
                    self.visit_expr(m);
                }
            }
            DeclKind::AgentBundle(ab) => {
                for a in &ab.agents {
                    self.visit_expr(a);
                }
                if let Some(f) = &ab.fallbacks {
                    self.visit_expr(f);
                }
            }
            DeclKind::Flow(f) => {
                for expr in &f.body {
                    self.visit_expr(expr);
                }
            }
            DeclKind::Tool(t) => {
                self.visit_expr(&t.description);
                for r in &t.requires {
                    self.visit_expr(r);
                }
            }
            DeclKind::Skill(s) => {
                self.visit_expr(&s.description);
                for t in &s.tools {
                    self.visit_expr(t);
                }
                if let Some(st) = &s.strategy {
                    self.visit_expr(st);
                }
            }
            DeclKind::Template(_) => {}
            DeclKind::Directive(_) => {}
            DeclKind::Schema(_) => {}
            DeclKind::TypeAlias(_) => {}
            DeclKind::PermitTree(_) => {}
            DeclKind::Test(t) => {
                for expr in &t.body {
                    self.visit_expr(expr);
                }
            }
            DeclKind::Import(_) => {}
            DeclKind::Connect(_) => {}
        }
    }

    /// Visit an expression.
    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::AgentDispatch { agent, tool, args } => {
                self.visit_expr(agent);
                self.visit_expr(tool);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::Pipeline { left, right } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::FallbackChain { primary, fallback } => {
                self.visit_expr(primary);
                self.visit_expr(fallback);
            }
            ExprKind::Parallel(exprs) => {
                for e in exprs {
                    self.visit_expr(e);
                }
            }
            ExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    self.visit_match_arm(arm);
                }
            }
            ExprKind::FieldAccess { object, .. } => {
                self.visit_expr(object);
            }
            ExprKind::FuncCall { callee, args } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::Return(e) | ExprKind::Fail(e) | ExprKind::Assert(e) => {
                self.visit_expr(e);
            }
            ExprKind::Assign { value, .. } => {
                self.visit_expr(value);
            }
            ExprKind::Record(exprs) => {
                for e in exprs {
                    self.visit_expr(e);
                }
            }
            ExprKind::Typed { expr, .. } => {
                self.visit_expr(expr);
            }
            ExprKind::ListLit(items) => {
                for item in items {
                    self.visit_expr(item);
                }
            }
            ExprKind::RecordFields(fields) => {
                for (_, expr) in fields {
                    self.visit_expr(expr);
                }
            }
            ExprKind::OnError { body, fallback } => {
                self.visit_expr(body);
                self.visit_expr(fallback);
            }
            ExprKind::RunFlow { args, .. } => {
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            // Leaf nodes — nothing to recurse into
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::StringLit(_)
            | ExprKind::PromptLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::Ident(_)
            | ExprKind::AgentRef(_)
            | ExprKind::ToolRef(_)
            | ExprKind::MemoryRef(_)
            | ExprKind::PermissionRef(_)
            | ExprKind::SkillRef(_)
            | ExprKind::TemplateRef(_)
            | ExprKind::Env(_) => {}
        }
    }

    /// Visit a match arm.
    fn visit_match_arm(&mut self, arm: &MatchArm) {
        self.visit_expr(&arm.body);
    }
}
