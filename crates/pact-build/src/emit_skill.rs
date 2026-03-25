// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-24

//! Claude Code skill emitter for PACT declarations.
//!
//! Generates `.claude/skills/{name}/SKILL.md` files from PACT flow and skill
//! declarations. Each generated file follows the Claude Code custom slash
//! command format with YAML frontmatter and structured Markdown body.

use pact_core::ast::expr::{Expr, ExprKind};
use pact_core::ast::stmt::{
    AgentDecl, DeclKind, DirectiveDecl, FlowDecl, Program, SkillDecl, ToolDecl,
};
use pact_core::ast::types::{TypeExpr, TypeExprKind};

/// Convert a PACT name (snake_case) to kebab-case for file paths.
fn to_kebab(name: &str) -> String {
    name.replace('_', "-")
}

/// Extract the description text from a description expression.
fn extract_description(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// Format a type expression for display in Markdown.
fn format_type(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
}

/// Find a tool declaration by name in the program.
fn find_tool_decl<'a>(program: &'a Program, name: &str) -> Option<&'a ToolDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Tool(t) if t.name == name => Some(t),
        _ => None,
    })
}

/// Find an agent declaration by name in the program.
fn find_agent_decl<'a>(program: &'a Program, name: &str) -> Option<&'a AgentDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Agent(a) if a.name == name => Some(a),
        _ => None,
    })
}

/// Find a directive declaration by name in the program.
fn find_directive_decl<'a>(program: &'a Program, name: &str) -> Option<&'a DirectiveDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Directive(dir) if dir.name == name => Some(dir),
        _ => None,
    })
}

/// Format the argument-hint string from a parameter list.
fn format_argument_hint(params: &[pact_core::ast::stmt::Param]) -> String {
    if params.is_empty() {
        return String::new();
    }
    params
        .iter()
        .map(|p| format!("<{}>", p.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Extract agent names referenced in a list of expressions.
fn extract_agent_names(exprs: &[Expr]) -> Vec<String> {
    let mut names = Vec::new();
    for expr in exprs {
        collect_agent_names(expr, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

/// Recursively collect agent names from an expression tree.
fn collect_agent_names(expr: &Expr, out: &mut Vec<String>) {
    match &expr.kind {
        ExprKind::AgentRef(name) => {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        ExprKind::AgentDispatch { agent, tool, args } => {
            collect_agent_names(agent, out);
            collect_agent_names(tool, out);
            for arg in args {
                collect_agent_names(arg, out);
            }
        }
        ExprKind::Assign { value, .. } => {
            collect_agent_names(value, out);
        }
        ExprKind::Return(inner) | ExprKind::Fail(inner) => {
            collect_agent_names(inner, out);
        }
        ExprKind::Pipeline { left, right } | ExprKind::FallbackChain { primary: left, fallback: right } => {
            collect_agent_names(left, out);
            collect_agent_names(right, out);
        }
        ExprKind::Parallel(exprs) => {
            for e in exprs {
                collect_agent_names(e, out);
            }
        }
        _ => {}
    }
}

/// Format the permissions list for an agent.
fn format_permits(agent: &AgentDecl) -> String {
    let perms: Vec<String> = agent
        .permits
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(segs) => Some(format!("^{}", segs.join("."))),
            _ => None,
        })
        .collect();
    if perms.is_empty() {
        "none".to_string()
    } else {
        perms.join(", ")
    }
}

/// Format the tools list for an agent.
fn format_agent_tools(agent: &AgentDecl) -> String {
    let tools: Vec<String> = agent
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(format!("#{}", name)),
            _ => None,
        })
        .collect();
    if tools.is_empty() {
        "none".to_string()
    } else {
        tools.join(", ")
    }
}

/// Format a single flow body expression as a step description.
fn format_step(expr: &Expr) -> Option<String> {
    match &expr.kind {
        ExprKind::Assign { name, value } => {
            let rhs = format_expr(value);
            Some(format!("`{}` = {}", name, rhs))
        }
        ExprKind::Return(inner) => {
            let val = format_expr(inner);
            Some(format!("return {}", val))
        }
        ExprKind::Fail(inner) => {
            let val = format_expr(inner);
            Some(format!("fail {}", val))
        }
        _ => {
            let s = format_expr(expr);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        }
    }
}

/// Format an expression for human-readable step display.
fn format_expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::AgentDispatch { agent, tool, args } => {
            let agent_name = match &agent.kind {
                ExprKind::AgentRef(n) => format!("@{}", n),
                _ => "?".to_string(),
            };
            let tool_name = match &tool.kind {
                ExprKind::ToolRef(n) => format!("#{}", n),
                _ => "?".to_string(),
            };
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{} -> {}({})", agent_name, tool_name, arg_strs.join(", "))
        }
        ExprKind::AgentRef(name) => format!("@{}", name),
        ExprKind::ToolRef(name) => format!("#{}", name),
        ExprKind::SkillRef(name) => format!("${}", name),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::StringLit(s) => format!("\"{}\"", s),
        ExprKind::PromptLit(s) => format!("<<{}>>", s.trim()),
        ExprKind::IntLit(n) => n.to_string(),
        ExprKind::FloatLit(f) => f.to_string(),
        ExprKind::BoolLit(b) => b.to_string(),
        ExprKind::Pipeline { left, right } => {
            format!("{} |> {}", format_expr(left), format_expr(right))
        }
        ExprKind::FallbackChain { primary, fallback } => {
            format!("{} ?> {}", format_expr(primary), format_expr(fallback))
        }
        ExprKind::FieldAccess { object, field } => {
            format!("{}.{}", format_expr(object), field)
        }
        ExprKind::FuncCall { callee, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("{}({})", format_expr(callee), arg_strs.join(", "))
        }
        ExprKind::Parallel(exprs) => {
            let items: Vec<String> = exprs.iter().map(format_expr).collect();
            format!("parallel {{ {} }}", items.join(", "))
        }
        ExprKind::Return(inner) => format!("return {}", format_expr(inner)),
        ExprKind::Assign { name, value } => format!("{} = {}", name, format_expr(value)),
        ExprKind::RunFlow { flow_name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_expr).collect();
            format!("run {}({})", flow_name, arg_strs.join(", "))
        }
        _ => String::new(),
    }
}

/// Generate a Claude Code SKILL.md file from a PACT flow declaration.
///
/// The output includes YAML frontmatter with the skill metadata, followed by
/// a structured Markdown body describing the workflow agents, steps, output
/// schema, directives, and permission boundaries.
pub fn generate_flow_skill(flow: &FlowDecl, program: &Program) -> String {
    let kebab = to_kebab(&flow.name);
    let hint = format_argument_hint(&flow.params);

    let mut md = String::new();

    // YAML frontmatter
    md.push_str("---\n");
    md.push_str(&format!("name: {}\n", kebab));
    md.push_str(&format!(
        "description: \"Executes the {} workflow\"\n",
        flow.name
    ));
    md.push_str("user-invocable: true\n");
    md.push_str("allowed-tools: Read, Write, Edit, Bash, Grep, Glob\n");
    if !hint.is_empty() {
        md.push_str(&format!("argument-hint: \"{}\"\n", hint));
    }
    md.push_str("---\n\n");

    // Workflow header
    md.push_str(&format!("## Workflow: {}\n\n", flow.name));

    // Agents section — discover all agents referenced in the flow body
    let agent_names = extract_agent_names(&flow.body);
    if !agent_names.is_empty() {
        md.push_str("### Agents\n\n");
        for agent_name in &agent_names {
            if let Some(agent) = find_agent_decl(program, agent_name) {
                let prompt_text = agent
                    .prompt
                    .as_ref()
                    .map(extract_description)
                    .unwrap_or_default();
                if prompt_text.is_empty() {
                    md.push_str(&format!("**@{}**\n", agent_name));
                } else {
                    md.push_str(&format!("**@{}** — {}\n", agent_name, prompt_text));
                }
                md.push_str(&format!("- Capabilities: {}\n", format_permits(agent)));
                md.push_str(&format!("- Tools: {}\n", format_agent_tools(agent)));
                md.push('\n');
            } else {
                md.push_str(&format!("**@{}**\n\n", agent_name));
            }
        }
    }

    // Steps section
    let steps: Vec<String> = flow.body.iter().filter_map(format_step).collect();
    if !steps.is_empty() {
        md.push_str("### Steps\n\n");
        for (i, step) in steps.iter().enumerate() {
            md.push_str(&format!("{}. {}\n", i + 1, step));
        }
        md.push('\n');
    }

    // Output schema section
    if let Some(ref ret_ty) = flow.return_type {
        md.push_str("### Output Schema\n\n");
        md.push_str(&format!("Return type: {}\n\n", format_type(ret_ty)));
    }

    // Directives section — collect directives from tools used by agents in this flow
    let mut directive_names: Vec<String> = Vec::new();
    for agent_name in &agent_names {
        if let Some(agent) = find_agent_decl(program, agent_name) {
            for tool_expr in &agent.tools {
                if let ExprKind::ToolRef(tool_name) = &tool_expr.kind {
                    if let Some(tool) = find_tool_decl(program, tool_name) {
                        for dir_name in &tool.directives {
                            if !directive_names.contains(dir_name) {
                                directive_names.push(dir_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    if !directive_names.is_empty() {
        md.push_str("### Directives\n\n");
        for dir_name in &directive_names {
            if let Some(directive) = find_directive_decl(program, dir_name) {
                md.push_str(&format!(
                    "**{}**: {}\n\n",
                    directive.name,
                    directive.text.trim()
                ));
            } else {
                md.push_str(&format!("**{}**\n\n", dir_name));
            }
        }
    }

    // Permission boundaries
    if !agent_names.is_empty() {
        md.push_str("### Permission Boundaries\n\n");
        for agent_name in &agent_names {
            if let Some(agent) = find_agent_decl(program, agent_name) {
                let permits = format_permits(agent);
                md.push_str(&format!("- @{}: {}\n", agent_name, permits));
            }
        }
        md.push('\n');
    }

    md
}

/// Generate a Claude Code SKILL.md file from a PACT skill declaration.
///
/// The output includes YAML frontmatter with the skill metadata, followed by
/// a structured Markdown body describing the skill strategy, tools, and
/// parameters.
pub fn generate_skill_skill(skill: &SkillDecl, program: &Program) -> String {
    let kebab = to_kebab(&skill.name);
    let description = extract_description(&skill.description);
    let hint = format_argument_hint(&skill.params);

    let mut md = String::new();

    // YAML frontmatter
    md.push_str("---\n");
    md.push_str(&format!("name: {}\n", kebab));
    md.push_str(&format!("description: \"{}\"\n", description));
    md.push_str("user-invocable: true\n");
    md.push_str("allowed-tools: Read, Write, Edit, Bash, Grep, Glob\n");
    if !hint.is_empty() {
        md.push_str(&format!("argument-hint: \"{}\"\n", hint));
    }
    md.push_str("---\n\n");

    // Skill header
    md.push_str(&format!("## Skill: {}\n\n", skill.name));

    // Strategy text
    if let Some(strategy_expr) = &skill.strategy {
        let strategy = extract_description(strategy_expr);
        if !strategy.is_empty() {
            md.push_str(&strategy);
            md.push_str("\n\n");
        }
    }

    // Tools section
    let tool_refs: Vec<&str> = skill
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    if !tool_refs.is_empty() {
        md.push_str("### Tools\n\n");
        for tool_name in &tool_refs {
            if let Some(tool) = find_tool_decl(program, tool_name) {
                let desc = extract_description(&tool.description);
                md.push_str(&format!("- **#{}**: {}\n", tool_name, desc));
            } else {
                md.push_str(&format!("- **#{}**\n", tool_name));
            }
        }
        md.push('\n');
    }

    // Parameters section
    if !skill.params.is_empty() {
        md.push_str("### Parameters\n\n");
        for param in &skill.params {
            let ty = param
                .ty
                .as_ref()
                .map(format_type)
                .unwrap_or_else(|| "Any".to_string());
            md.push_str(&format!("- `{}` ({})\n", param.name, ty));
        }
        md.push('\n');
    }

    md
}

/// Generate all Claude Code skill files from a PACT program.
///
/// Returns a list of `(path, content)` pairs where path is relative to the
/// project root (e.g. `.claude/skills/build-site/SKILL.md`).
pub fn generate_all_skills(program: &Program) -> Vec<(String, String)> {
    let mut skills = Vec::new();

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Flow(flow) => {
                let kebab = to_kebab(&flow.name);
                let path = format!(".claude/skills/{}/SKILL.md", kebab);
                let content = generate_flow_skill(flow, program);
                skills.push((path, content));
            }
            DeclKind::Skill(skill) => {
                let kebab = to_kebab(&skill.name);
                let path = format!(".claude/skills/{}/SKILL.md", kebab);
                let content = generate_skill_skill(skill, program);
                skills.push((path, content));
            }
            _ => {}
        }
    }

    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse_program(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn flow_to_skill_frontmatter() {
        let src = r#"
            tool #greet {
                description: <<Generate a greeting.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                model: "claude-sonnet-4-20250514"
                prompt: <<You are a friendly greeter.>>
            }
            flow build_site(domain :: String, style :: String) -> String {
                result = @greeter -> #greet(domain)
                return result
            }
        "#;
        let program = parse_program(src);
        let flow = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f),
            _ => None,
        }).unwrap();

        let output = generate_flow_skill(flow, &program);

        // Check YAML frontmatter fields
        assert!(output.contains("name: build-site"));
        assert!(output.contains("description: \"Executes the build_site workflow\""));
        assert!(output.contains("allowed-tools: Read, Write, Edit, Bash, Grep, Glob"));
        assert!(output.contains("argument-hint: \"<domain>, <style>\""));
        assert!(output.contains("user-invocable: true"));
    }

    #[test]
    fn flow_to_skill_steps() {
        let src = r#"
            tool #search {
                description: <<Search the web.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            tool #summarize {
                description: <<Summarize content.>>
                requires: [^llm.query]
                params { content :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read, ^llm.query]
                tools: [#search, #summarize]
                prompt: <<You are a research assistant.>>
            }
            flow research(topic :: String) -> String {
                raw = @researcher -> #search(topic)
                summary = @researcher -> #summarize(raw)
                return summary
            }
        "#;
        let program = parse_program(src);
        let flow = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f),
            _ => None,
        }).unwrap();

        let output = generate_flow_skill(flow, &program);

        // Verify step listing
        assert!(output.contains("### Steps"));
        assert!(output.contains("1. `raw` = @researcher -> #search(topic)"));
        assert!(output.contains("2. `summary` = @researcher -> #summarize(raw)"));
        assert!(output.contains("3. return summary"));

        // Verify agents section
        assert!(output.contains("### Agents"));
        assert!(output.contains("**@researcher** — You are a research assistant."));
        assert!(output.contains("- Capabilities: ^net.read, ^llm.query"));
        assert!(output.contains("- Tools: #search, #summarize"));

        // Verify output schema
        assert!(output.contains("### Output Schema"));
        assert!(output.contains("Return type: String"));

        // Verify permission boundaries
        assert!(output.contains("### Permission Boundaries"));
        assert!(output.contains("- @researcher: ^net.read, ^llm.query"));
    }

    #[test]
    fn skill_decl_to_claude_skill() {
        let src = r#"
            tool #lint {
                description: <<Run linter on code.>>
                requires: [^llm.query]
                params { code :: String }
                returns :: String
            }
            tool #format_code {
                description: <<Auto-format source code.>>
                requires: [^llm.query]
                params { code :: String }
                returns :: String
            }
            skill $code_review {
                description: <<Review code for quality and style.>>
                tools: [#lint, #format_code]
                strategy: <<First lint the code to find issues, then auto-format. Report both results.>>
                params { input :: String, language :: String }
                returns :: String
            }
        "#;
        let program = parse_program(src);
        let skill = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Skill(s) => Some(s),
            _ => None,
        }).unwrap();

        let output = generate_skill_skill(skill, &program);

        // Verify frontmatter
        assert!(output.contains("name: code-review"));
        assert!(output.contains("description: \"Review code for quality and style.\""));
        assert!(output.contains("argument-hint: \"<input>, <language>\""));

        // Verify strategy becomes body
        assert!(output.contains("## Skill: code_review"));
        assert!(output.contains("First lint the code to find issues, then auto-format. Report both results."));

        // Verify tools section
        assert!(output.contains("### Tools"));
        assert!(output.contains("- **#lint**: Run linter on code."));
        assert!(output.contains("- **#format_code**: Auto-format source code."));

        // Verify parameters section
        assert!(output.contains("### Parameters"));
        assert!(output.contains("- `input` (String)"));
        assert!(output.contains("- `language` (String)"));
    }

    #[test]
    fn kebab_naming() {
        assert_eq!(to_kebab("build_site"), "build-site");
        assert_eq!(to_kebab("hello_world_flow"), "hello-world-flow");
        assert_eq!(to_kebab("simple"), "simple");
        assert_eq!(to_kebab("a_b_c"), "a-b-c");
    }

    #[test]
    fn generate_all_skills_collects_flows_and_skills() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                prompt: <<Be friendly.>>
            }
            skill $quick_lint {
                description: <<Lint code quickly.>>
                tools: [#greet]
                params { code :: String }
            }
            flow say_hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let program = parse_program(src);
        let all = generate_all_skills(&program);

        assert_eq!(all.len(), 2);

        // Skill comes first in declaration order
        let (skill_path, skill_content) = &all[0];
        assert_eq!(skill_path, ".claude/skills/quick-lint/SKILL.md");
        assert!(skill_content.contains("name: quick-lint"));

        // Flow comes second
        let (flow_path, flow_content) = &all[1];
        assert_eq!(flow_path, ".claude/skills/say-hello/SKILL.md");
        assert!(flow_content.contains("name: say-hello"));
    }

    #[test]
    fn flow_with_no_agents_or_return_type() {
        let src = r#"
            flow empty_flow() {
            }
        "#;
        let program = parse_program(src);
        let flow = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f),
            _ => None,
        }).unwrap();

        let output = generate_flow_skill(flow, &program);

        // Should have frontmatter but no agents, steps, or output schema sections
        assert!(output.contains("name: empty-flow"));
        assert!(output.contains("description: \"Executes the empty_flow workflow\""));
        assert!(!output.contains("### Agents"));
        assert!(!output.contains("### Steps"));
        assert!(!output.contains("### Output Schema"));
        assert!(!output.contains("### Permission Boundaries"));
    }

    #[test]
    fn skill_without_strategy() {
        let src = r#"
            tool #fetch {
                description: <<Fetch a URL.>>
                requires: [^net.read]
                params { url :: String }
                returns :: String
            }
            skill $fetcher {
                description: <<Fetch web pages.>>
                tools: [#fetch]
            }
        "#;
        let program = parse_program(src);
        let skill = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Skill(s) => Some(s),
            _ => None,
        }).unwrap();

        let output = generate_skill_skill(skill, &program);

        assert!(output.contains("name: fetcher"));
        assert!(output.contains("## Skill: fetcher"));
        assert!(output.contains("- **#fetch**: Fetch a URL."));
        // No strategy text, no parameters section
        assert!(!output.contains("### Parameters"));
    }

    #[test]
    fn flow_with_directives() {
        let src = r#"
            directive %style_guide {
                <<Use consistent naming conventions and follow the project style guide.>>
            }
            tool #write_code {
                description: <<Write source code.>>
                requires: [^fs.write]
                directives: [%style_guide]
                params { spec :: String }
                returns :: String
            }
            agent @coder {
                permits: [^fs.write]
                tools: [#write_code]
                prompt: <<You are a code generator.>>
            }
            flow generate(spec :: String) -> String {
                code = @coder -> #write_code(spec)
                return code
            }
        "#;
        let program = parse_program(src);
        let flow = program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Flow(f) => Some(f),
            _ => None,
        }).unwrap();

        let output = generate_flow_skill(flow, &program);

        assert!(output.contains("### Directives"));
        assert!(output.contains("**style_guide**: Use consistent naming conventions and follow the project style guide."));
    }
}
