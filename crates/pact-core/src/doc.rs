// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-12-22

//! Markdown documentation generator for PACT programs.
//!
//! Produces a structured Markdown document from a parsed [`Program`],
//! covering permissions, schemas, tools, agents, flows, tests, and skills.

use crate::ast::expr::ExprKind;
use crate::ast::stmt::{
    AgentBundleDecl, AgentDecl, ComplianceDecl, DeclKind, DirectiveDecl, FlowDecl, Param,
    PermitNode, Program, SchemaDecl, SkillDecl, TemplateDecl, TemplateEntry, TestDecl, ToolDecl,
};
use crate::ast::types::{TypeExpr, TypeExprKind};

/// Generate a complete Markdown document from a parsed PACT program.
///
/// `title` is used as the top-level heading (typically the file name).
pub fn generate_docs(program: &Program, title: &str) -> String {
    let mut out = String::new();

    // Classify declarations
    let mut permit_trees = Vec::new();
    let mut schemas = Vec::new();
    let mut type_aliases = Vec::new();
    let mut tools = Vec::new();
    let mut agents = Vec::new();
    let mut bundles = Vec::new();
    let mut flows = Vec::new();
    let mut tests = Vec::new();
    let mut skills = Vec::new();
    let mut templates = Vec::new();
    let mut directives = Vec::new();
    let mut compliances = Vec::new();

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::PermitTree(pt) => permit_trees.push(pt),
            DeclKind::Schema(s) => schemas.push(s),
            DeclKind::TypeAlias(ta) => type_aliases.push(ta),
            DeclKind::Tool(t) => tools.push(t),
            DeclKind::Agent(a) => agents.push(a),
            DeclKind::AgentBundle(b) => bundles.push(b),
            DeclKind::Flow(f) => flows.push(f),
            DeclKind::Test(t) => tests.push(t),
            DeclKind::Skill(s) => skills.push(s),
            DeclKind::Template(t) => templates.push(t),
            DeclKind::Directive(d) => directives.push(d),
            DeclKind::Compliance(c) => compliances.push(c),
            DeclKind::Import(_) => {}     // imports resolved by loader
            DeclKind::Connect(_) => {}    // MCP connections are structural
            DeclKind::Lesson(_) => {}     // lessons handled separately
            DeclKind::Federation(_) => {} // federation is structural
        }
    }

    // Title
    out.push_str(&format!("# {title}\n\n"));

    // Table of contents
    out.push_str("## Table of Contents\n\n");
    if !permit_trees.is_empty() {
        out.push_str("- [Permissions](#permissions)\n");
    }
    if !schemas.is_empty() || !type_aliases.is_empty() {
        out.push_str("- [Schemas](#schemas)\n");
    }
    if !tools.is_empty() {
        out.push_str("- [Tools](#tools)\n");
    }
    if !agents.is_empty() {
        out.push_str("- [Agents](#agents)\n");
    }
    if !bundles.is_empty() {
        out.push_str("- [Agent Bundles](#agent-bundles)\n");
    }
    if !flows.is_empty() {
        out.push_str("- [Flows](#flows)\n");
    }
    if !tests.is_empty() {
        out.push_str("- [Tests](#tests)\n");
    }
    if !skills.is_empty() {
        out.push_str("- [Skills](#skills)\n");
    }
    if !templates.is_empty() {
        out.push_str("- [Templates](#templates)\n");
    }
    if !directives.is_empty() {
        out.push_str("- [Directives](#directives)\n");
    }
    if !compliances.is_empty() {
        out.push_str("- [Compliance](#compliance)\n");
    }
    out.push('\n');

    // Permissions
    if !permit_trees.is_empty() {
        out.push_str("## Permissions\n\n");
        for pt in &permit_trees {
            for node in &pt.nodes {
                render_permit_node(&mut out, node, 0);
            }
        }
        out.push('\n');
    }

    // Schemas (and type aliases)
    if !schemas.is_empty() || !type_aliases.is_empty() {
        out.push_str("## Schemas\n\n");
        for s in &schemas {
            render_schema(&mut out, s);
        }
        for ta in &type_aliases {
            out.push_str(&format!(
                "### Type `{}`\n\n`{}`\n\n",
                ta.name,
                ta.variants.join(" | ")
            ));
        }
    }

    // Tools
    if !tools.is_empty() {
        out.push_str("## Tools\n\n");
        for t in &tools {
            render_tool(&mut out, t);
        }
    }

    // Agents
    if !agents.is_empty() {
        out.push_str("## Agents\n\n");
        for a in &agents {
            render_agent(&mut out, a);
        }
    }

    // Agent Bundles
    if !bundles.is_empty() {
        out.push_str("## Agent Bundles\n\n");
        for b in &bundles {
            render_bundle(&mut out, b);
        }
    }

    // Flows
    if !flows.is_empty() {
        out.push_str("## Flows\n\n");
        for f in &flows {
            render_flow(&mut out, f);
        }
    }

    // Tests
    if !tests.is_empty() {
        out.push_str("## Tests\n\n");
        for t in &tests {
            render_test(&mut out, t);
        }
        out.push('\n');
    }

    // Skills
    if !skills.is_empty() {
        out.push_str("## Skills\n\n");
        for s in &skills {
            render_skill(&mut out, s);
        }
    }

    // Templates
    if !templates.is_empty() {
        out.push_str("## Templates\n\n");
        for t in &templates {
            render_template(&mut out, t);
        }
    }

    // Directives
    if !directives.is_empty() {
        out.push_str("## Directives\n\n");
        for d in &directives {
            render_directive_doc(&mut out, d);
        }
    }

    // Compliance
    if !compliances.is_empty() {
        out.push_str("## Compliance\n\n");
        for c in &compliances {
            render_compliance(&mut out, c);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Rendering helpers
// ---------------------------------------------------------------------------

fn render_permit_node(out: &mut String, node: &PermitNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let name = format!("^{}", node.path.join("."));
    out.push_str(&format!("{indent}- `{name}`\n"));
    for child in &node.children {
        render_permit_node(out, child, depth + 1);
    }
}

fn render_schema(out: &mut String, schema: &SchemaDecl) {
    out.push_str(&format!("### `{}`\n\n", schema.name));
    if schema.fields.is_empty() {
        out.push_str("_No fields._\n\n");
        return;
    }
    out.push_str("| Field | Type |\n|-------|------|\n");
    for f in &schema.fields {
        out.push_str(&format!("| `{}` | `{}` |\n", f.name, format_type(&f.ty)));
    }
    out.push('\n');
}

fn render_tool(out: &mut String, tool: &ToolDecl) {
    out.push_str(&format!("### `#{}`\n\n", tool.name));

    // Description
    if let ExprKind::PromptLit(desc) = &tool.description.kind {
        out.push_str(&format!("{desc}\n\n"));
    }

    // Required permissions
    if !tool.requires.is_empty() {
        let perms: Vec<String> = tool.requires.iter().map(format_permission_expr).collect();
        out.push_str(&format!("**Requires:** {}\n\n", perms.join(", ")));
    }

    // Handler
    if let Some(handler) = &tool.handler {
        out.push_str(&format!("**Handler:** `{handler}`\n\n"));
    }

    // Source (built-in provider)
    if let Some(source) = &tool.source {
        if source.args.is_empty() {
            out.push_str(&format!("**Source:** `^{}`\n\n", source.capability));
        } else {
            out.push_str(&format!(
                "**Source:** `^{}({})`\n\n",
                source.capability,
                source.args.join(", ")
            ));
        }
    }

    // Parameters
    if !tool.params.is_empty() {
        out.push_str("**Parameters:**\n\n");
        render_params(out, &tool.params);
    }

    // Return type
    if let Some(ret) = &tool.return_type {
        out.push_str(&format!("**Returns:** `{}`\n\n", format_type(ret)));
    }

    // Output template
    if let Some(output) = &tool.output {
        out.push_str(&format!("**Output Template:** `%{}`\n\n", output));
    }

    // Directives
    if !tool.directives.is_empty() {
        let refs: Vec<String> = tool
            .directives
            .iter()
            .map(|n| format!("`%{}`", n))
            .collect();
        out.push_str(&format!("**Directives:** {}\n\n", refs.join(", ")));
    }

    // Retry
    if let Some(retry) = tool.retry {
        out.push_str(&format!("**Retry:** {} attempts\n\n", retry));
    }

    // Validate
    if let Some(validate) = &tool.validate {
        out.push_str(&format!("**Validation:** `{}`\n\n", validate));
    }

    // Cache
    if let Some(cache) = &tool.cache {
        out.push_str(&format!("**Cache:** `{}`\n\n", cache));
    }
}

fn render_directive_doc(out: &mut String, directive: &DirectiveDecl) {
    out.push_str(&format!("### `%{}`\n\n", directive.name));

    // Prompt text
    let preview = if directive.text.len() > 80 {
        format!("{}...", &directive.text[..80])
    } else {
        directive.text.clone()
    };
    out.push_str(&format!("{}\n\n", preview.trim()));

    // Parameters
    if !directive.params.is_empty() {
        out.push_str("**Parameters:**\n\n");
        out.push_str("| Name | Type | Default |\n|------|------|---------|\n");
        for p in &directive.params {
            let ty_str = format_type(&p.ty);
            let default_str = match &p.default.kind {
                ExprKind::StringLit(s) => format!("\"{}\"", s),
                ExprKind::IntLit(n) => n.to_string(),
                ExprKind::FloatLit(f) => f.to_string(),
                ExprKind::BoolLit(b) => b.to_string(),
                _ => "_expr_".to_string(),
            };
            out.push_str(&format!(
                "| `{}` | `{}` | {} |\n",
                p.name, ty_str, default_str
            ));
        }
        out.push('\n');
    }
}

fn render_agent(out: &mut String, agent: &AgentDecl) {
    out.push_str(&format!("### `@{}`\n\n", agent.name));

    // Permits
    if !agent.permits.is_empty() {
        let perms: Vec<String> = agent.permits.iter().map(format_permission_expr).collect();
        out.push_str(&format!("**Permits:** {}\n\n", perms.join(", ")));
    }

    // Tools
    if !agent.tools.is_empty() {
        let refs: Vec<String> = agent.tools.iter().map(format_ref_expr).collect();
        out.push_str(&format!("**Tools:** {}\n\n", refs.join(", ")));
    }

    // Skills
    if !agent.skills.is_empty() {
        let refs: Vec<String> = agent.skills.iter().map(format_ref_expr).collect();
        out.push_str(&format!("**Skills:** {}\n\n", refs.join(", ")));
    }

    // Model
    if let Some(model_expr) = &agent.model {
        if let ExprKind::StringLit(m) = &model_expr.kind {
            out.push_str(&format!("**Model:** `{m}`\n\n"));
        }
    }

    // Prompt
    if let Some(prompt_expr) = &agent.prompt {
        if let ExprKind::PromptLit(text) = &prompt_expr.kind {
            out.push_str(&format!("**Prompt:**\n\n> {text}\n\n"));
        }
    }
}

fn render_bundle(out: &mut String, bundle: &AgentBundleDecl) {
    out.push_str(&format!("### `@{}`\n\n", bundle.name));
    if !bundle.agents.is_empty() {
        let refs: Vec<String> = bundle.agents.iter().map(format_ref_expr).collect();
        out.push_str(&format!("**Agents:** {}\n\n", refs.join(", ")));
    }
}

fn render_flow(out: &mut String, flow: &FlowDecl) {
    // Signature line
    let params_str = format_param_list(&flow.params);
    let ret_str = match &flow.return_type {
        Some(ty) => format!(" -> `{}`", format_type(ty)),
        None => String::new(),
    };
    out.push_str(&format!("### `{}`\n\n", flow.name));
    out.push_str(&format!(
        "**Signature:** `{}({})`{}\n\n",
        flow.name, params_str, ret_str
    ));

    // Parameters
    if !flow.params.is_empty() {
        out.push_str("**Parameters:**\n\n");
        render_params(out, &flow.params);
    }

    // Steps description
    if !flow.body.is_empty() {
        out.push_str("**Steps:**\n\n");
        for (i, expr) in flow.body.iter().enumerate() {
            let desc = describe_expr(expr);
            out.push_str(&format!("{}. {}\n", i + 1, desc));
        }
        out.push('\n');
    }
}

fn render_test(out: &mut String, test: &TestDecl) {
    out.push_str(&format!("- {}\n", test.description));
}

fn render_template(out: &mut String, template: &TemplateDecl) {
    out.push_str(&format!("### `%{}`\n\n", template.name));
    out.push_str("| Entry | Type | Description |\n");
    out.push_str("|-------|------|-------------|\n");
    for entry in &template.entries {
        match entry {
            TemplateEntry::Field {
                name,
                ty,
                description,
            } => {
                let desc = description.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "| `{}` | `{}` | {} |\n",
                    name,
                    format_type(ty),
                    desc
                ));
            }
            TemplateEntry::Repeat {
                name,
                ty,
                count,
                description,
            } => {
                let desc = description.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "| `{}` ({}) | `{}` | {} |\n",
                    name,
                    count,
                    format_type(ty),
                    desc
                ));
            }
            TemplateEntry::Section { name, description } => {
                let desc = description.as_deref().unwrap_or("");
                out.push_str(&format!("| section `{}` | — | {} |\n", name, desc));
            }
        }
    }
    out.push('\n');
}

fn render_skill(out: &mut String, skill: &SkillDecl) {
    out.push_str(&format!("### `${}`\n\n", skill.name));

    // Description
    if let ExprKind::PromptLit(desc) = &skill.description.kind {
        out.push_str(&format!("{desc}\n\n"));
    }

    // Tools
    if !skill.tools.is_empty() {
        let refs: Vec<String> = skill.tools.iter().map(format_ref_expr).collect();
        out.push_str(&format!("**Tools:** {}\n\n", refs.join(", ")));
    }

    // Strategy
    if let Some(strategy_expr) = &skill.strategy {
        if let ExprKind::PromptLit(text) = &strategy_expr.kind {
            out.push_str(&format!("**Strategy:**\n\n> {text}\n\n"));
        }
    }

    // Parameters
    if !skill.params.is_empty() {
        out.push_str("**Parameters:**\n\n");
        render_params(out, &skill.params);
    }

    // Return type
    if let Some(ret) = &skill.return_type {
        out.push_str(&format!("**Returns:** `{}`\n\n", format_type(ret)));
    }
}

fn render_compliance(out: &mut String, c: &ComplianceDecl) {
    out.push_str(&format!("### `{}`\n\n", c.name));
    if let Some(risk) = &c.risk {
        out.push_str(&format!("**Risk:** {}\n\n", risk));
    }
    if !c.frameworks.is_empty() {
        let fws: Vec<String> = c.frameworks.iter().map(|f| format!("`{}`", f)).collect();
        out.push_str(&format!("**Frameworks:** {}\n\n", fws.join(", ")));
    }
    if let Some(audit) = &c.audit {
        out.push_str(&format!("**Audit:** {}\n\n", audit));
    }
    if let Some(retention) = &c.retention {
        out.push_str(&format!("**Retention:** {}\n\n", retention));
    }
    if let Some(interval) = &c.review_interval {
        out.push_str(&format!("**Review Interval:** {}\n\n", interval));
    }
    if !c.roles.is_empty() {
        out.push_str("**Roles:**\n\n");
        out.push_str("| Role | Assignee |\n|------|----------|\n");
        for role in &c.roles {
            out.push_str(&format!("| {} | {} |\n", role.role, role.assignee));
        }
        out.push('\n');
    }
}

// ---------------------------------------------------------------------------
// Formatting utilities
// ---------------------------------------------------------------------------

fn format_type(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let args_str: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, args_str.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
}

fn format_permission_expr(expr: &crate::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::PermissionRef(segments) => format!("`^{}`", segments.join(".")),
        _ => format!("`{:?}`", expr.kind),
    }
}

fn format_ref_expr(expr: &crate::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::ToolRef(name) => format!("`#{name}`"),
        ExprKind::AgentRef(name) => format!("`@{name}`"),
        ExprKind::SkillRef(name) => format!("`${name}`"),
        ExprKind::MemoryRef(name) => format!("`~{name}`"),
        ExprKind::TemplateRef(name) => format!("`%{name}`"),
        _ => format!("`{:?}`", expr.kind),
    }
}

fn format_param_list(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| match &p.ty {
            Some(ty) => format!("{} :: {}", p.name, format_type(ty)),
            None => p.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_params(out: &mut String, params: &[Param]) {
    out.push_str("| Name | Type |\n|------|------|\n");
    for p in params {
        let ty_str = match &p.ty {
            Some(ty) => format!("`{}`", format_type(ty)),
            None => "_untyped_".to_string(),
        };
        out.push_str(&format!("| `{}` | {} |\n", p.name, ty_str));
    }
    out.push('\n');
}

/// Produce a short human-readable description of an expression (for flow step summaries).
fn describe_expr(expr: &crate::ast::expr::Expr) -> String {
    match &expr.kind {
        ExprKind::Assign { name, value } => {
            format!("Assign `{}` = {}", name, describe_expr(value))
        }
        ExprKind::AgentDispatch { agent, tool, args } => {
            let agent_name = describe_expr(agent);
            let tool_name = describe_expr(tool);
            let args_str: Vec<String> = args.iter().map(describe_expr).collect();
            if args_str.is_empty() {
                format!("Dispatch {} -> {}()", agent_name, tool_name)
            } else {
                format!(
                    "Dispatch {} -> {}({})",
                    agent_name,
                    tool_name,
                    args_str.join(", ")
                )
            }
        }
        ExprKind::Return(inner) => format!("Return {}", describe_expr(inner)),
        ExprKind::Fail(inner) => format!("Fail with {}", describe_expr(inner)),
        ExprKind::Pipeline { left, right } => {
            format!("{} |> {}", describe_expr(left), describe_expr(right))
        }
        ExprKind::FallbackChain { primary, fallback } => {
            format!(
                "{} with fallback {}",
                describe_expr(primary),
                describe_expr(fallback)
            )
        }
        ExprKind::Parallel(exprs) => {
            let items: Vec<String> = exprs.iter().map(describe_expr).collect();
            format!("Parallel [{}]", items.join(", "))
        }
        ExprKind::FuncCall { callee, args } => {
            let args_str: Vec<String> = args.iter().map(describe_expr).collect();
            format!("Call {}({})", describe_expr(callee), args_str.join(", "))
        }
        ExprKind::Match { subject, .. } => {
            format!("Match on {}", describe_expr(subject))
        }
        ExprKind::Ident(name) => format!("`{name}`"),
        ExprKind::AgentRef(name) => format!("`@{name}`"),
        ExprKind::ToolRef(name) => format!("`#{name}`"),
        ExprKind::SkillRef(name) => format!("`${name}`"),
        ExprKind::MemoryRef(name) => format!("`~{name}`"),
        ExprKind::TemplateRef(name) => format!("`%{name}`"),
        ExprKind::PermissionRef(segs) => format!("`^{}`", segs.join(".")),
        ExprKind::StringLit(s) => format!("\"{}\"", s),
        ExprKind::IntLit(n) => format!("{n}"),
        ExprKind::FloatLit(f) => format!("{f}"),
        ExprKind::BoolLit(b) => format!("{b}"),
        ExprKind::PromptLit(text) => {
            let preview = if text.len() > 40 {
                format!("{}...", &text[..40])
            } else {
                text.clone()
            };
            format!("<<{preview}>>")
        }
        ExprKind::Assert(inner) => format!("Assert {}", describe_expr(inner)),
        ExprKind::BinOp { left, op, right } => {
            let op_str = match op {
                crate::ast::BinOpKind::Add => "+",
                crate::ast::BinOpKind::Sub => "-",
                crate::ast::BinOpKind::Mul => "*",
                crate::ast::BinOpKind::Div => "/",
                crate::ast::BinOpKind::Eq => "==",
                crate::ast::BinOpKind::Neq => "!=",
                crate::ast::BinOpKind::Lt => "<",
                crate::ast::BinOpKind::Gt => ">",
                crate::ast::BinOpKind::LtEq => "<=",
                crate::ast::BinOpKind::GtEq => ">=",
            };
            format!(
                "{} {} {}",
                describe_expr(left),
                op_str,
                describe_expr(right)
            )
        }
        ExprKind::FieldAccess { object, field } => {
            format!("{}.{}", describe_expr(object), field)
        }
        ExprKind::ListLit(items) => {
            let items_str: Vec<String> = items.iter().map(describe_expr).collect();
            format!("[{}]", items_str.join(", "))
        }
        ExprKind::RecordFields(_) => "Record { ... }".to_string(),
        ExprKind::Record(_) => "record { ... }".to_string(),
        ExprKind::Typed { expr, ty } => {
            format!("{} :: {}", describe_expr(expr), format_type(ty))
        }
        ExprKind::OnError { body, fallback } => {
            format!(
                "{} on_error {}",
                describe_expr(body),
                describe_expr(fallback)
            )
        }
        ExprKind::Env(key) => format!("env(\"{}\")", key),
        ExprKind::RunFlow { flow_name, args } => {
            let args_str: Vec<String> = args.iter().map(describe_expr).collect();
            format!("run {}({})", flow_name, args_str.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::span::SourceMap;

    fn parse_program(source: &str) -> Program {
        let mut sm = SourceMap::new();
        let sid = sm.add("test.pact", source);
        let tokens = Lexer::new(source, sid).lex().expect("lex failed");
        let (program, errors) = Parser::new(&tokens).parse_collecting_errors();
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        program
    }

    #[test]
    fn test_generates_title_and_toc() {
        let source = r#"
tool #hello {
    description: <<Say hello.>>
    requires: [^llm.query]
    params { name :: String }
    returns :: String
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "test.pact");
        assert!(doc.starts_with("# test.pact\n"));
        assert!(doc.contains("## Table of Contents"));
        assert!(doc.contains("- [Tools](#tools)"));
    }

    #[test]
    fn test_permissions_section() {
        let source = r#"
permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
        ^net.write
    }
}

tool #noop {
    description: <<No-op.>>
    requires: [^llm.query]
    params { x :: String }
    returns :: String
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "perms.pact");
        assert!(doc.contains("## Permissions"));
        assert!(doc.contains("`^llm`"));
        assert!(doc.contains("`^llm.query`"));
        assert!(doc.contains("`^net`"));
        assert!(doc.contains("`^net.read`"));
        assert!(doc.contains("`^net.write`"));
    }

    #[test]
    fn test_schemas_section() {
        let source = r#"
schema Report {
    title :: String
    body :: String
    tags :: List<String>
}

tool #noop {
    description: <<No-op.>>
    requires: [^llm.query]
    params { x :: String }
    returns :: String
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "schemas.pact");
        assert!(doc.contains("## Schemas"));
        assert!(doc.contains("### `Report`"));
        assert!(doc.contains("| `title` | `String` |"));
        assert!(doc.contains("| `tags` | `List<String>` |"));
    }

    #[test]
    fn test_tools_section() {
        let source = r#"
tool #search {
    description: <<Search for information.>>
    requires: [^net.read]
    params {
        query :: String
    }
    returns :: List<String>
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "tools.pact");
        assert!(doc.contains("## Tools"));
        assert!(doc.contains("### `#search`"));
        assert!(doc.contains("Search for information."));
        assert!(doc.contains("`^net.read`"));
        assert!(doc.contains("| `query` | `String` |"));
        assert!(doc.contains("`List<String>`"));
    }

    #[test]
    fn test_agents_section() {
        let source = r#"
tool #greet {
    description: <<Greet.>>
    requires: [^llm.query]
    params { name :: String }
    returns :: String
}

agent @assistant {
    permits: [^llm.query]
    tools: [#greet]
    model: "gpt-4"
    prompt: <<You are a helpful assistant.>>
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "agents.pact");
        assert!(doc.contains("## Agents"));
        assert!(doc.contains("### `@assistant`"));
        assert!(doc.contains("`^llm.query`"));
        assert!(doc.contains("`#greet`"));
        assert!(doc.contains("`gpt-4`"));
        assert!(doc.contains("You are a helpful assistant."));
    }

    #[test]
    fn test_flows_section() {
        let source = r#"
tool #greet {
    description: <<Greet.>>
    requires: [^llm.query]
    params { name :: String }
    returns :: String
}

agent @assistant {
    permits: [^llm.query]
    tools: [#greet]
    prompt: <<You are helpful.>>
}

flow main(input :: String) -> String {
    result = @assistant -> #greet(input)
    return result
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "flows.pact");
        assert!(doc.contains("## Flows"));
        assert!(doc.contains("### `main`"));
        assert!(doc.contains("input :: String"));
        assert!(doc.contains("-> `String`"));
        assert!(doc.contains("**Steps:**"));
    }

    #[test]
    fn test_tests_section() {
        let source = r#"
tool #greet {
    description: <<Greet.>>
    requires: [^llm.query]
    params { name :: String }
    returns :: String
}

agent @assistant {
    permits: [^llm.query]
    tools: [#greet]
    prompt: <<You are helpful.>>
}

test "greeting works" {
    result = @assistant -> #greet("world")
    assert result == "greet_result"
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "tests.pact");
        assert!(doc.contains("## Tests"));
        assert!(doc.contains("greeting works"));
    }

    #[test]
    fn test_full_document() {
        let source = r#"
permit_tree {
    ^llm {
        ^llm.query
    }
    ^net {
        ^net.read
    }
}

schema Report {
    title :: String
    body :: String
}

tool #search {
    description: <<Search for info.>>
    requires: [^net.read]
    params { query :: String }
    returns :: List<String>
}

tool #analyze {
    description: <<Analyze content.>>
    requires: [^llm.query]
    params { content :: String }
    returns :: String
}

agent @researcher {
    permits: [^net.read, ^llm.query]
    tools: [#search, #analyze]
    prompt: <<You are a research assistant.>>
}

flow research(topic :: String) -> String {
    results = @researcher -> #search(topic)
    analysis = @researcher -> #analyze(results)
    return analysis
}

test "research works" {
    result = @researcher -> #search("test")
    assert result == "search_result"
}
"#;
        let program = parse_program(source);
        let doc = generate_docs(&program, "full.pact");

        // All major sections should be present
        assert!(doc.contains("# full.pact"));
        assert!(doc.contains("## Table of Contents"));
        assert!(doc.contains("## Permissions"));
        assert!(doc.contains("## Schemas"));
        assert!(doc.contains("## Tools"));
        assert!(doc.contains("## Agents"));
        assert!(doc.contains("## Flows"));
        assert!(doc.contains("## Tests"));
    }

    #[test]
    fn test_optional_type_formatting() {
        let ty = TypeExpr {
            kind: TypeExprKind::Optional(Box::new(TypeExpr {
                kind: TypeExprKind::Named("String".to_string()),
                span: crate::span::Span::new(crate::span::SourceId(0), 0, 0),
            })),
            span: crate::span::Span::new(crate::span::SourceId(0), 0, 0),
        };
        assert_eq!(format_type(&ty), "String?");
    }

    #[test]
    fn test_generic_type_formatting() {
        let ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "Map".to_string(),
                args: vec![
                    TypeExpr {
                        kind: TypeExprKind::Named("String".to_string()),
                        span: crate::span::Span::new(crate::span::SourceId(0), 0, 0),
                    },
                    TypeExpr {
                        kind: TypeExprKind::Named("Int".to_string()),
                        span: crate::span::Span::new(crate::span::SourceId(0), 0, 0),
                    },
                ],
            },
            span: crate::span::Span::new(crate::span::SourceId(0), 0, 0),
        };
        assert_eq!(format_type(&ty), "Map<String, Int>");
    }
}
