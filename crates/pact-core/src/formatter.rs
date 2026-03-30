// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-12-18

//! PACT source-code formatter (AST pretty-printer).
//!
//! Takes a parsed [`Program`] and re-emits clean, consistently formatted PACT
//! source text.
//!
//! # Example
//!
//! ```ignore
//! use pact_core::formatter::format_program;
//!
//! let formatted = format_program(&program);
//! println!("{formatted}");
//! ```

use crate::ast::expr::{BinOpKind, Expr, ExprKind, MatchPattern};
use crate::ast::stmt::{
    AgentBundleDecl, AgentDecl, ComplianceDecl, Decl, DeclKind, DirectiveDecl, FlowDecl, Param,
    PermitNode, PermitTreeDecl, Program, SchemaDecl, SchemaField, SkillDecl, TemplateDecl,
    TemplateEntry, TestDecl, ToolDecl, TypeAliasDecl,
};
use crate::ast::types::{TypeExpr, TypeExprKind};

/// Indent width in spaces.
const INDENT: usize = 4;

/// Format a complete PACT program into a canonical string representation.
pub fn format_program(program: &Program) -> String {
    let mut f = Formatter::new();
    f.format_program(program);
    f.finish()
}

// ---------------------------------------------------------------------------
// Internal formatter state
// ---------------------------------------------------------------------------

struct Formatter {
    buf: String,
    indent: usize,
}

impl Formatter {
    fn new() -> Self {
        Self {
            buf: String::new(),
            indent: 0,
        }
    }

    fn finish(mut self) -> String {
        // Ensure trailing newline
        if !self.buf.ends_with('\n') {
            self.buf.push('\n');
        }
        self.buf
    }

    // -- helpers ------------------------------------------------------------

    fn push(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    fn push_indent(&mut self) {
        for _ in 0..self.indent {
            self.buf.push(' ');
        }
    }

    fn push_line(&mut self, s: &str) {
        self.push_indent();
        self.buf.push_str(s);
        self.buf.push('\n');
    }

    fn newline(&mut self) {
        self.buf.push('\n');
    }

    fn indent(&mut self) {
        self.indent += INDENT;
    }

    fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(INDENT);
    }

    // -- program / declarations ---------------------------------------------

    fn format_program(&mut self, program: &Program) {
        for (i, decl) in program.decls.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.format_decl(decl);
        }
    }

    fn format_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Agent(d) => self.format_agent(d),
            DeclKind::AgentBundle(d) => self.format_agent_bundle(d),
            DeclKind::Tool(d) => self.format_tool(d),
            DeclKind::Flow(d) => self.format_flow(d),
            DeclKind::Schema(d) => self.format_schema(d),
            DeclKind::TypeAlias(d) => self.format_type_alias(d),
            DeclKind::PermitTree(d) => self.format_permit_tree(d),
            DeclKind::Test(d) => self.format_test(d),
            DeclKind::Skill(d) => self.format_skill(d),
            DeclKind::Template(d) => self.format_template(d),
            DeclKind::Directive(d) => self.format_directive(d),
            DeclKind::Import(d) => {
                self.push_line(&format!("import \"{}\"", d.path));
            }
            DeclKind::Connect(c) => {
                self.push_line("connect {");
                self.indent();
                for entry in &c.servers {
                    self.push_indent();
                    self.buf
                        .push_str(&format!("{} \"{}\"\n", entry.name, entry.transport));
                }
                self.dedent();
                self.push_line("}");
            }
            DeclKind::Lesson(l) => {
                self.push_line(&format!("lesson \"{}\" {{", l.name));
                self.indent();
                if let Some(ctx) = &l.context {
                    self.push_line(&format!("context: <<{}>>", ctx));
                }
                if let Some(rule) = &l.rule {
                    self.push_line(&format!("rule: <<{}>>", rule));
                }
                if let Some(sev) = &l.severity {
                    self.push_line(&format!("severity: {}", sev));
                }
                self.dedent();
                self.push_line("}");
            }
            DeclKind::Compliance(c) => self.format_compliance(c),
            DeclKind::Federation(f) => {
                self.push_line("federation {");
                self.indent();
                for entry in &f.registries {
                    self.push_indent();
                    self.push(&format!("\"{}\" trust: [", entry.url));
                    for (i, perm) in entry.trust.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.write_expr(perm);
                    }
                    self.push("]\n");
                }
                self.dedent();
                self.push_line("}");
            }
        }
    }

    // -- agent --------------------------------------------------------------

    fn format_agent(&mut self, d: &AgentDecl) {
        self.push_line(&format!("agent @{} {{", d.name));
        self.indent();

        if !d.permits.is_empty() {
            self.push_indent();
            self.push("permits: [");
            self.push_expr_list(&d.permits);
            self.push("]\n");
        }

        if !d.tools.is_empty() {
            self.push_indent();
            self.push("tools: [");
            self.push_expr_list(&d.tools);
            self.push("]\n");
        }

        if !d.skills.is_empty() {
            self.push_indent();
            self.push("skills: [");
            self.push_expr_list(&d.skills);
            self.push("]\n");
        }

        if !d.memory.is_empty() {
            self.push_indent();
            self.push("memory: [");
            self.push_expr_list(&d.memory);
            self.push("]\n");
        }

        if let Some(model) = &d.model {
            self.push_indent();
            self.push("model: ");
            self.write_expr(model);
            self.newline();
        }

        if let Some(endpoint) = &d.endpoint {
            self.push_line(&format!("endpoint: \"{}\"", endpoint));
        }

        if let Some(prompt) = &d.prompt {
            self.push_indent();
            self.push("prompt: ");
            self.write_expr(prompt);
            self.newline();
        }

        self.dedent();
        self.push_line("}");
    }

    // -- agent_bundle -------------------------------------------------------

    fn format_agent_bundle(&mut self, d: &AgentBundleDecl) {
        self.push_line(&format!("agent_bundle @{} {{", d.name));
        self.indent();

        if !d.agents.is_empty() {
            self.push_indent();
            self.push("agents: [");
            self.push_expr_list(&d.agents);
            self.push("]\n");
        }

        if let Some(fallbacks) = &d.fallbacks {
            self.push_indent();
            self.push("fallbacks: ");
            self.write_expr(fallbacks);
            self.newline();
        }

        self.dedent();
        self.push_line("}");
    }

    // -- tool ---------------------------------------------------------------

    fn format_tool(&mut self, d: &ToolDecl) {
        self.push_line(&format!("tool #{} {{", d.name));
        self.indent();

        self.push_indent();
        self.push("description: ");
        self.write_expr(&d.description);
        self.newline();

        if !d.requires.is_empty() {
            self.push_indent();
            self.push("requires: [");
            self.push_expr_list(&d.requires);
            self.push("]\n");
        }

        if let Some(handler) = &d.handler {
            self.push_indent();
            self.push(&format!("handler: \"{}\"\n", handler));
        }

        if let Some(source) = &d.source {
            self.push_indent();
            if source.args.is_empty() {
                self.push(&format!("source: ^{}\n", source.capability));
            } else {
                self.push(&format!(
                    "source: ^{}({})\n",
                    source.capability,
                    source.args.join(", ")
                ));
            }
        }

        if !d.params.is_empty() {
            self.push_indent();
            self.push("params {\n");
            self.indent();
            for p in &d.params {
                self.format_param(p);
            }
            self.dedent();
            self.push_indent();
            self.push("}\n");
        }

        if let Some(rt) = &d.return_type {
            self.push_indent();
            self.push("returns :: ");
            self.write_type(rt);
            self.newline();
        }

        if !d.directives.is_empty() {
            self.push_indent();
            self.push("directives: [");
            for (i, name) in d.directives.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&format!("%{}", name));
            }
            self.push("]\n");
        }

        if let Some(output) = &d.output {
            self.push_indent();
            self.push(&format!("output: %{}\n", output));
        }

        if let Some(retry) = d.retry {
            self.push_indent();
            self.push(&format!("retry: {}\n", retry));
        }

        if let Some(validate) = &d.validate {
            self.push_indent();
            self.push(&format!("validate: {}\n", validate));
        }

        if let Some(cache) = &d.cache {
            self.push_indent();
            self.push(&format!("cache: \"{}\"\n", cache));
        }

        self.dedent();
        self.push_line("}");
    }

    // -- directive -----------------------------------------------------------

    fn format_directive(&mut self, d: &DirectiveDecl) {
        self.push_line(&format!("directive %{} {{", d.name));
        self.indent();

        self.push_indent();
        self.push(&format!("<<{}>>\n", d.text));

        if !d.params.is_empty() {
            self.push_indent();
            self.push("params {\n");
            self.indent();
            for p in &d.params {
                self.push_indent();
                self.push(&format!("{} :: ", p.name));
                self.write_type(&p.ty);
                self.push(" = ");
                self.write_expr(&p.default);
                self.newline();
            }
            self.dedent();
            self.push_indent();
            self.push("}\n");
        }

        self.dedent();
        self.push_line("}");
    }

    // -- skill --------------------------------------------------------------

    fn format_skill(&mut self, d: &SkillDecl) {
        self.push_line(&format!("skill ${} {{", d.name));
        self.indent();

        self.push_indent();
        self.push("description: ");
        self.write_expr(&d.description);
        self.newline();

        if !d.tools.is_empty() {
            self.push_indent();
            self.push("tools: [");
            self.push_expr_list(&d.tools);
            self.push("]\n");
        }

        if let Some(strategy) = &d.strategy {
            self.push_indent();
            self.push("strategy: ");
            self.write_expr(strategy);
            self.newline();
        }

        if !d.params.is_empty() {
            self.push_indent();
            self.push("params {\n");
            self.indent();
            for p in &d.params {
                self.format_param(p);
            }
            self.dedent();
            self.push_indent();
            self.push("}\n");
        }

        if let Some(rt) = &d.return_type {
            self.push_indent();
            self.push("returns :: ");
            self.write_type(rt);
            self.newline();
        }

        self.dedent();
        self.push_line("}");
    }

    // -- flow ---------------------------------------------------------------

    fn format_flow(&mut self, d: &FlowDecl) {
        self.push_indent();
        self.push(&format!("flow {}", d.name));

        if !d.params.is_empty() {
            self.push("(");
            for (i, p) in d.params.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(&p.name);
                if let Some(ty) = &p.ty {
                    self.push(" :: ");
                    self.write_type(ty);
                }
            }
            self.push(")");
        }

        if let Some(rt) = &d.return_type {
            self.push(" -> ");
            self.write_type(rt);
        }

        self.push(" {\n");
        self.indent();

        for expr in &d.body {
            self.push_indent();
            self.write_expr(expr);
            self.newline();
        }

        self.dedent();
        self.push_line("}");
    }

    // -- schema -------------------------------------------------------------

    fn format_schema(&mut self, d: &SchemaDecl) {
        self.push_line(&format!("schema {} {{", d.name));
        self.indent();
        for field in &d.fields {
            self.format_schema_field(field);
        }
        self.dedent();
        self.push_line("}");
    }

    fn format_schema_field(&mut self, f: &SchemaField) {
        self.push_indent();
        self.push(&f.name);
        self.push(" :: ");
        self.write_type(&f.ty);
        self.newline();
    }

    // -- type alias ---------------------------------------------------------

    fn format_type_alias(&mut self, d: &TypeAliasDecl) {
        self.push_indent();
        self.push(&format!("type {} = ", d.name));
        for (i, variant) in d.variants.iter().enumerate() {
            if i > 0 {
                self.push(" | ");
            }
            self.push(variant);
        }
        self.newline();
    }

    // -- permit_tree --------------------------------------------------------

    fn format_permit_tree(&mut self, d: &PermitTreeDecl) {
        self.push_line("permit_tree {");
        self.indent();
        for node in &d.nodes {
            self.format_permit_node(node);
        }
        self.dedent();
        self.push_line("}");
    }

    fn format_permit_node(&mut self, node: &PermitNode) {
        let perm_name = format!("^{}", node.path.join("."));
        if node.children.is_empty() {
            self.push_line(&perm_name);
        } else {
            self.push_line(&format!("{} {{", perm_name));
            self.indent();
            for child in &node.children {
                self.format_permit_node(child);
            }
            self.dedent();
            self.push_line("}");
        }
    }

    // -- test ---------------------------------------------------------------

    fn format_test(&mut self, d: &TestDecl) {
        self.push_line(&format!("test \"{}\" {{", d.description));
        self.indent();
        for expr in &d.body {
            self.push_indent();
            self.write_expr(expr);
            self.newline();
        }
        self.dedent();
        self.push_line("}");
    }

    // -- template -----------------------------------------------------------

    fn format_template(&mut self, d: &TemplateDecl) {
        self.push_line(&format!("template %{} {{", d.name));
        self.indent();
        for entry in &d.entries {
            match entry {
                TemplateEntry::Field {
                    name,
                    ty,
                    description,
                } => {
                    self.push_indent();
                    self.push(&format!("{} :: ", name));
                    self.write_type(ty);
                    if let Some(desc) = description {
                        self.push(&format!("  <<{}>>", desc));
                    }
                    self.newline();
                }
                TemplateEntry::Repeat {
                    name,
                    ty,
                    count,
                    description,
                } => {
                    self.push_indent();
                    self.push(&format!("{} :: ", name));
                    self.write_type(ty);
                    self.push(&format!(" * {}", count));
                    if let Some(desc) = description {
                        self.push(&format!("  <<{}>>", desc));
                    }
                    self.newline();
                }
                TemplateEntry::Section { name, description } => {
                    self.push_indent();
                    self.push(&format!("section {}", name));
                    if let Some(desc) = description {
                        self.push(&format!("  <<{}>>", desc));
                    }
                    self.newline();
                }
            }
        }
        self.dedent();
        self.push_line("}");
    }

    // -- compliance ---------------------------------------------------------

    fn format_compliance(&mut self, c: &ComplianceDecl) {
        self.push_line(&format!("compliance \"{}\" {{", c.name));
        self.indent();
        if let Some(risk) = &c.risk {
            self.push_line(&format!("risk: {}", risk));
        }
        if !c.frameworks.is_empty() {
            self.push_indent();
            self.push("frameworks: [");
            for (i, fw) in c.frameworks.iter().enumerate() {
                if i > 0 {
                    self.push(", ");
                }
                self.push(fw);
            }
            self.push("]\n");
        }
        if let Some(audit) = &c.audit {
            self.push_line(&format!("audit: {}", audit));
        }
        if let Some(retention) = &c.retention {
            self.push_line(&format!("retention: \"{}\"", retention));
        }
        if let Some(interval) = &c.review_interval {
            self.push_line(&format!("review_interval: \"{}\"", interval));
        }
        if !c.roles.is_empty() {
            self.push_line("roles {");
            self.indent();
            for role in &c.roles {
                self.push_line(&format!("{}: \"{}\"", role.role, role.assignee));
            }
            self.dedent();
            self.push_line("}");
        }
        self.dedent();
        self.push_line("}");
    }

    // -- params -------------------------------------------------------------

    fn format_param(&mut self, p: &Param) {
        self.push_indent();
        self.push(&p.name);
        if let Some(ty) = &p.ty {
            self.push(" :: ");
            self.write_type(ty);
        }
        self.newline();
    }

    // -- types --------------------------------------------------------------

    fn write_type(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeExprKind::Named(name) => self.push(name),
            TypeExprKind::Generic { name, args } => {
                self.push(name);
                self.push("<");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.write_type(arg);
                }
                self.push(">");
            }
            TypeExprKind::Optional(inner) => {
                self.write_type(inner);
                self.push("?");
            }
        }
    }

    // -- expressions --------------------------------------------------------

    fn write_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::IntLit(n) => self.push(&n.to_string()),
            ExprKind::FloatLit(f) => self.push(&format_float(*f)),
            ExprKind::StringLit(s) => {
                let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
                self.push(&format!("\"{}\"", escaped));
            }
            ExprKind::PromptLit(s) => self.push(&format!("<<{}>>", s)),
            ExprKind::BoolLit(b) => self.push(if *b { "true" } else { "false" }),
            ExprKind::Ident(name) => self.push(name),
            ExprKind::AgentRef(name) => self.push(&format!("@{}", name)),
            ExprKind::ToolRef(name) => self.push(&format!("#{}", name)),
            ExprKind::SkillRef(name) => self.push(&format!("${}", name)),
            ExprKind::MemoryRef(name) => self.push(&format!("~{}", name)),
            ExprKind::TemplateRef(name) => self.push(&format!("%{}", name)),
            ExprKind::PermissionRef(parts) => {
                self.push("^");
                self.push(&parts.join("."));
            }

            ExprKind::AgentDispatch { agent, tool, args } => {
                self.write_expr(agent);
                self.push(" -> ");
                self.write_expr(tool);
                if !args.is_empty() {
                    self.push("(");
                    self.push_expr_list(args);
                    self.push(")");
                }
            }

            ExprKind::Pipeline { left, right } => {
                self.write_expr(left);
                self.push(" |> ");
                self.write_expr(right);
            }

            ExprKind::FallbackChain { primary, fallback } => {
                self.write_expr(primary);
                self.push(" ?> ");
                self.write_expr(fallback);
            }

            ExprKind::Parallel(tasks) => {
                self.push("parallel {\n");
                self.indent();
                for (i, task) in tasks.iter().enumerate() {
                    self.push_indent();
                    self.write_expr(task);
                    if i + 1 < tasks.len() {
                        self.push(",");
                    }
                    self.newline();
                }
                self.dedent();
                self.push_indent();
                self.push("}");
            }

            ExprKind::Match { subject, arms } => {
                self.push("match ");
                self.write_expr(subject);
                self.push(" {\n");
                self.indent();
                for arm in arms {
                    self.push_indent();
                    self.write_match_pattern(&arm.pattern);
                    self.push(" => ");
                    self.write_expr(&arm.body);
                    self.newline();
                }
                self.dedent();
                self.push_indent();
                self.push("}");
            }

            ExprKind::FieldAccess { object, field } => {
                self.write_expr(object);
                self.push(".");
                self.push(field);
            }

            ExprKind::FuncCall { callee, args } => {
                self.write_expr(callee);
                self.push("(");
                self.push_expr_list(args);
                self.push(")");
            }

            ExprKind::BinOp { left, op, right } => {
                self.write_expr(left);
                self.push(&format!(" {} ", binop_str(*op)));
                self.write_expr(right);
            }

            ExprKind::Assign { name, value } => {
                self.push(name);
                self.push(" = ");
                self.write_expr(value);
            }

            ExprKind::Return(inner) => {
                self.push("return ");
                self.write_expr(inner);
            }

            ExprKind::Fail(inner) => {
                self.push("fail ");
                self.write_expr(inner);
            }

            ExprKind::Assert(inner) => {
                self.push("assert ");
                self.write_expr(inner);
            }

            ExprKind::Record(exprs) => {
                self.push("record {\n");
                self.indent();
                for e in exprs {
                    self.push_indent();
                    self.write_expr(e);
                    self.newline();
                }
                self.dedent();
                self.push_indent();
                self.push("}");
            }

            ExprKind::Typed { expr, ty } => {
                self.write_expr(expr);
                self.push(" :: ");
                self.write_type(ty);
            }

            ExprKind::ListLit(items) => {
                self.push("[");
                self.push_expr_list(items);
                self.push("]");
            }

            ExprKind::RecordFields(fields) => {
                self.push("{ ");
                for (i, (key, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.push(key);
                    self.push(": ");
                    self.write_expr(val);
                }
                self.push(" }");
            }

            ExprKind::OnError { body, fallback } => {
                self.write_expr(body);
                self.push(" on_error ");
                self.write_expr(fallback);
            }

            ExprKind::Env(key) => {
                self.push(&format!("env(\"{}\")", key));
            }

            ExprKind::RunFlow { flow_name, args } => {
                self.push(&format!("run {}", flow_name));
                self.push("(");
                self.push_expr_list(args);
                self.push(")");
            }
        }
    }

    fn write_match_pattern(&mut self, pat: &MatchPattern) {
        match pat {
            MatchPattern::StringLit(s) => self.push(&format!("\"{}\"", s)),
            MatchPattern::BoolLit(b) => self.push(if *b { "true" } else { "false" }),
            MatchPattern::IntLit(n) => self.push(&n.to_string()),
            MatchPattern::Ident(name) => self.push(name),
            MatchPattern::Wildcard => self.push("_"),
        }
    }

    /// Write a comma-separated list of expressions (no leading/trailing space).
    fn push_expr_list(&mut self, exprs: &[Expr]) {
        for (i, expr) in exprs.iter().enumerate() {
            if i > 0 {
                self.push(", ");
            }
            self.write_expr(expr);
        }
    }
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn binop_str(op: BinOpKind) -> &'static str {
    match op {
        BinOpKind::Add => "+",
        BinOpKind::Sub => "-",
        BinOpKind::Mul => "*",
        BinOpKind::Div => "/",
        BinOpKind::Eq => "==",
        BinOpKind::Neq => "!=",
        BinOpKind::Lt => "<",
        BinOpKind::Gt => ">",
        BinOpKind::LtEq => "<=",
        BinOpKind::GtEq => ">=",
    }
}

/// Format a float, ensuring it always has a decimal point.
fn format_float(f: f64) -> String {
    let s = f.to_string();
    if s.contains('.') {
        s
    } else {
        format!("{}.0", s)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::span::SourceId;

    /// Helper: parse source text and format the resulting AST.
    fn roundtrip(src: &str) -> String {
        let id = SourceId(0);
        let tokens = Lexer::new(src, id).lex().expect("lex failed");
        let mut parser = Parser::new(&tokens);
        let program = parser.parse().expect("parse failed");
        format_program(&program)
    }

    #[test]
    fn format_permit_tree() {
        let src = r#"permit_tree {
    ^net {
        ^net.read
        ^net.write
    }
    ^llm {
        ^llm.query
    }
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("permit_tree {"));
        assert!(out.contains("    ^net {"));
        assert!(out.contains("        ^net.read"));
        assert!(out.contains("        ^net.write"));
        assert!(out.contains("    ^llm {"));
        assert!(out.contains("        ^llm.query"));
    }

    #[test]
    fn format_schema() {
        let src = r#"schema Greeting {
    message :: String
    tone :: String
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("schema Greeting {"));
        assert!(out.contains("    message :: String"));
        assert!(out.contains("    tone :: String"));
        assert!(out.contains("}"));
    }

    #[test]
    fn format_type_alias() {
        let src = "type Tone = Formal | Casual | Friendly\n";
        let out = roundtrip(src);
        assert!(out.contains("type Tone = Formal | Casual | Friendly"));
    }

    #[test]
    fn format_tool_declaration() {
        let src = r#"tool #greet {
    description: <<Greet someone>>
    requires: [^net.read]
    params {
        name :: String
    }
    returns :: Greeting
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("tool #greet {"));
        assert!(out.contains("    description: <<Greet someone>>"));
        assert!(out.contains("    requires: [^net.read]"));
        assert!(out.contains("    params {"));
        assert!(out.contains("        name :: String"));
        assert!(out.contains("    returns :: Greeting"));
    }

    #[test]
    fn format_agent_declaration() {
        let src = r#"agent @greeter {
    permits: [^net.read]
    tools: [#greet]
    model: "gpt-4"
    prompt: <<You are a friendly greeter>>
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("agent @greeter {"));
        assert!(out.contains("    permits: [^net.read]"));
        assert!(out.contains("    tools: [#greet]"));
        assert!(out.contains("    prompt: <<You are a friendly greeter>>"));
    }

    #[test]
    fn format_flow_declaration() {
        let src = r#"flow greet_user(name :: String) -> Greeting {
    result = @greeter -> #greet(name)
    return result
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("flow greet_user(name :: String) -> Greeting {"));
        assert!(out.contains("    result = @greeter -> #greet(name)"));
        assert!(out.contains("    return result"));
    }

    #[test]
    fn format_test_block() {
        let src = r#"test "greet test" {
    result = greet_user("Alice")
    assert result.message
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("test \"greet test\" {"));
        assert!(out.contains("    result = greet_user(\"Alice\")"));
        assert!(out.contains("    assert result.message"));
    }

    #[test]
    fn format_pipeline_expression() {
        let src = r#"flow pipeline_example() -> String {
    result = @a -> #t1() |> @b -> #t2()
    return result
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("|>"));
    }

    #[test]
    fn format_fallback_expression() {
        let src = r#"flow fallback_example() -> String {
    result = @a -> #t1() ?> @b -> #t2()
    return result
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("?>"));
    }

    #[test]
    fn format_blank_lines_between_decls() {
        let src = r#"schema A {
    x :: String
}

schema B {
    y :: Int
}
"#;
        let out = roundtrip(src);
        // There should be a blank line between the two schema declarations
        assert!(out.contains("}\n\nschema B"));
    }

    #[test]
    fn format_multiple_params() {
        let src = r#"flow multi(a :: String, b :: Int) -> String {
    return a
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("flow multi(a :: String, b :: Int) -> String {"));
    }

    #[test]
    fn format_list_literal() {
        let src = r#"flow list_example() -> String {
    items = [1, 2, 3]
    return items
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("[1, 2, 3]"));
    }

    #[test]
    fn format_binop() {
        let src = r#"flow math() -> Int {
    result = 1 + 2
    return result
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("1 + 2"));
    }

    #[test]
    fn format_idempotent() {
        // Formatting an already-formatted program should produce the same output.
        let src = r#"schema Greeting {
    message :: String
    tone :: String
}

type Tone = Formal | Casual | Friendly
"#;
        let first = roundtrip(src);
        let second = roundtrip(&first);
        assert_eq!(first, second, "Formatter is not idempotent");
    }

    #[test]
    fn format_compliance_roundtrip() {
        let src = r#"compliance "payment_processing" {
    risk: high
    frameworks: [pci_dss, gdpr, sox]
    audit: full
    retention: "7y"
    review_interval: "90d"
    roles {
        approver: "finance_lead"
        executor: "payment_agent"
        auditor: "compliance_team"
    }
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("compliance \"payment_processing\" {"));
        assert!(out.contains("risk: high"));
        assert!(out.contains("frameworks: [pci_dss, gdpr, sox]"));
        assert!(out.contains("audit: full"));
        assert!(out.contains("retention: \"7y\""));
        assert!(out.contains("review_interval: \"90d\""));
        assert!(out.contains("roles {"));
        assert!(out.contains("approver: \"finance_lead\""));
        assert!(out.contains("executor: \"payment_agent\""));
        assert!(out.contains("auditor: \"compliance_team\""));
    }

    #[test]
    fn format_all_examples_idempotent() {
        let examples_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
        let entries: Vec<_> = std::fs::read_dir(&examples_dir)
            .expect("examples dir should exist")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "pact"))
            .collect();
        assert!(!entries.is_empty(), "no .pact examples found");

        for entry in entries {
            let path = entry.path();
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            let first = roundtrip(&src);
            let second = roundtrip(&first);
            assert_eq!(
                first,
                second,
                "Formatter not idempotent for {}",
                path.file_name().unwrap().to_string_lossy()
            );
        }
    }

    #[test]
    fn format_federation_roundtrip() {
        let src = r#"federation {
    "https://agents.example.com" trust: [^llm.query, ^net.read]
    "https://internal.corp.net" trust: [^data.read, ^data.write]
}
"#;
        let out = roundtrip(src);
        assert!(out.contains("federation {"));
        assert!(out.contains("\"https://agents.example.com\" trust: [^llm.query, ^net.read]"));
        assert!(out.contains("\"https://internal.corp.net\" trust: [^data.read, ^data.write]"));
        let second = roundtrip(&out);
        assert_eq!(out, second, "federation formatting not idempotent");
    }
}
