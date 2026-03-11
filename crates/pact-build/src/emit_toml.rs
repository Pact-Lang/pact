// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-10-02

//! TOML generation for pact build artifacts.
//!
//! This module converts the checked AST into TOML configuration files:
//! - `pact.toml` — project manifest
//! - `agents/<name>.toml` — agent configuration
//! - `tools/<name>.toml` — tool definitions
//! - `flows/<name>.toml` — flow orchestration specs
//! - `permissions.toml` — flattened permission tree

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{
    AgentDecl, DeclKind, FlowDecl, PermitNode, Program, SkillDecl, ToolDecl,
};
use pact_core::ast::types::TypeExprKind;
use serde::Serialize;

use crate::config::BuildConfig;

// ── Serializable structures ────────────────────────────────────────

/// Top-level project manifest.
#[derive(Serialize)]
pub struct Manifest {
    pub pact: ManifestMeta,
    pub agents: ManifestList,
    pub tools: ManifestList,
    #[serde(skip_serializing_if = "ManifestList::is_empty")]
    pub skills: ManifestList,
    pub flows: ManifestList,
}

#[derive(Serialize)]
pub struct ManifestMeta {
    pub version: String,
    pub source: String,
    pub target: String,
}

#[derive(Serialize)]
pub struct ManifestList {
    pub list: Vec<String>,
}

impl ManifestList {
    fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// Agent TOML config.
#[derive(Serialize)]
pub struct AgentConfig {
    pub agent: AgentConfigInner,
}

#[derive(Serialize)]
pub struct AgentConfigInner {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub prompt_file: String,
    pub permissions: AgentPermissions,
    pub tools: AgentToolList,
}

#[derive(Serialize)]
pub struct AgentPermissions {
    pub granted: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentToolList {
    pub list: Vec<String>,
}

/// Tool TOML config.
#[derive(Serialize)]
pub struct ToolConfig {
    pub tool: ToolConfigInner,
}

#[derive(Serialize)]
pub struct ToolConfigInner {
    pub name: String,
    pub description: String,
    pub permissions: ToolPermissions,
    pub params: Vec<ToolParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}

#[derive(Serialize)]
pub struct ToolPermissions {
    pub requires: Vec<String>,
}

#[derive(Serialize)]
pub struct ToolParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub required: bool,
}

/// Flow TOML config.
#[derive(Serialize)]
pub struct FlowConfig {
    pub flow: FlowConfigInner,
}

#[derive(Serialize)]
pub struct FlowConfigInner {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub params: Vec<FlowParam>,
    pub steps: Vec<FlowStep>,
}

#[derive(Serialize)]
pub struct FlowParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Serialize)]
pub struct FlowStep {
    pub variable: String,
    pub agent: String,
    pub tool: String,
    pub args: Vec<String>,
}

/// Skill TOML config.
#[derive(Serialize)]
pub struct SkillConfig {
    pub skill: SkillConfigInner,
}

#[derive(Serialize)]
pub struct SkillConfigInner {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    pub params: Vec<ToolParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}

/// Permissions TOML config.
#[derive(Serialize)]
pub struct PermissionsConfig {
    pub permissions: Vec<PermissionEntry>,
}

#[derive(Serialize)]
pub struct PermissionEntry {
    pub path: String,
    pub children: Vec<String>,
}

// ── Generation functions ───────────────────────────────────────────

/// Generate the project manifest TOML.
pub fn generate_manifest(program: &Program, config: &BuildConfig) -> String {
    let mut agents = Vec::new();
    let mut tools = Vec::new();
    let mut skills = Vec::new();
    let mut flows = Vec::new();

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Agent(a) => agents.push(a.name.clone()),
            DeclKind::Tool(t) => tools.push(t.name.clone()),
            DeclKind::Skill(s) => skills.push(s.name.clone()),
            DeclKind::Flow(f) => flows.push(f.name.clone()),
            _ => {}
        }
    }

    let manifest = Manifest {
        pact: ManifestMeta {
            version: "0.2".to_string(),
            source: config.source_name().to_string(),
            target: config.target.as_str().to_string(),
        },
        agents: ManifestList { list: agents },
        tools: ManifestList { list: tools },
        skills: ManifestList { list: skills },
        flows: ManifestList { list: flows },
    };

    toml::to_string_pretty(&manifest).expect("manifest serialization should not fail")
}

/// Generate an agent's TOML config.
pub fn generate_agent_toml(agent: &AgentDecl) -> String {
    let permissions: Vec<String> = agent
        .permits
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(segs) => Some(segs.join(".")),
            _ => None,
        })
        .collect();

    let tools: Vec<String> = agent
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.clone()),
            _ => None,
        })
        .collect();

    let model = agent.model.as_ref().and_then(|e| match &e.kind {
        ExprKind::StringLit(s) => Some(s.clone()),
        _ => None,
    });

    let config = AgentConfig {
        agent: AgentConfigInner {
            name: agent.name.clone(),
            model,
            prompt_file: format!("{}.prompt.md", agent.name),
            permissions: AgentPermissions {
                granted: permissions,
            },
            tools: AgentToolList { list: tools },
        },
    };

    toml::to_string_pretty(&config).expect("agent serialization should not fail")
}

/// Generate a tool's TOML config.
pub fn generate_tool_toml(tool: &ToolDecl) -> String {
    let requires: Vec<String> = tool
        .requires
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(segs) => Some(segs.join(".")),
            _ => None,
        })
        .collect();

    let params: Vec<ToolParam> = tool
        .params
        .iter()
        .map(|p| ToolParam {
            name: p.name.clone(),
            ty: p
                .ty
                .as_ref()
                .map(type_expr_to_string)
                .unwrap_or_else(|| "Any".to_string()),
            required: true,
        })
        .collect();

    let returns = tool.return_type.as_ref().map(type_expr_to_string);

    let description = match &tool.description.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.clone(),
        _ => String::new(),
    };

    let config = ToolConfig {
        tool: ToolConfigInner {
            name: tool.name.clone(),
            description,
            permissions: ToolPermissions { requires },
            params,
            returns,
        },
    };

    toml::to_string_pretty(&config).expect("tool serialization should not fail")
}

/// Generate a flow's TOML config.
pub fn generate_flow_toml(flow: &FlowDecl) -> String {
    let params: Vec<FlowParam> = flow
        .params
        .iter()
        .map(|p| FlowParam {
            name: p.name.clone(),
            ty: p
                .ty
                .as_ref()
                .map(type_expr_to_string)
                .unwrap_or_else(|| "Any".to_string()),
        })
        .collect();

    let return_type = flow.return_type.as_ref().map(type_expr_to_string);

    // Extract steps from flow body (agent dispatches and assignments)
    let steps = extract_flow_steps(flow);

    let config = FlowConfig {
        flow: FlowConfigInner {
            name: flow.name.clone(),
            return_type,
            params,
            steps,
        },
    };

    toml::to_string_pretty(&config).expect("flow serialization should not fail")
}

/// Generate a skill's TOML config.
pub fn generate_skill_toml(skill: &SkillDecl) -> String {
    let tools: Vec<String> = skill
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.clone()),
            _ => None,
        })
        .collect();

    let description = match &skill.description.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.clone(),
        _ => String::new(),
    };

    let strategy = skill.strategy.as_ref().and_then(|e| match &e.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => Some(s.clone()),
        _ => None,
    });

    let params: Vec<ToolParam> = skill
        .params
        .iter()
        .map(|p| ToolParam {
            name: p.name.clone(),
            ty: p
                .ty
                .as_ref()
                .map(type_expr_to_string)
                .unwrap_or_else(|| "Any".to_string()),
            required: true,
        })
        .collect();

    let returns = skill.return_type.as_ref().map(type_expr_to_string);

    let config = SkillConfig {
        skill: SkillConfigInner {
            name: skill.name.clone(),
            description,
            tools,
            strategy,
            params,
            returns,
        },
    };

    toml::to_string_pretty(&config).expect("skill serialization should not fail")
}

/// Generate the permissions TOML from permit tree declarations.
pub fn generate_permissions_toml(program: &Program) -> String {
    let mut entries = Vec::new();

    for decl in &program.decls {
        if let DeclKind::PermitTree(pt) = &decl.kind {
            collect_permission_entries(&pt.nodes, &mut entries);
        }
    }

    let config = PermissionsConfig {
        permissions: entries,
    };

    toml::to_string_pretty(&config).expect("permissions serialization should not fail")
}

/// Recursively collect permission tree entries.
fn collect_permission_entries(nodes: &[PermitNode], entries: &mut Vec<PermissionEntry>) {
    for node in nodes {
        let path = node.path.join(".");
        let children: Vec<String> = node.children.iter().map(|c| c.path.join(".")).collect();
        if !children.is_empty() {
            entries.push(PermissionEntry { path, children });
        }
        collect_permission_entries(&node.children, entries);
    }
}

/// Extract agent dispatch steps from a flow body for TOML serialization.
fn extract_flow_steps(flow: &FlowDecl) -> Vec<FlowStep> {
    let mut steps = Vec::new();
    for expr in &flow.body {
        if let ExprKind::Assign { name, value } = &expr.kind {
            if let ExprKind::AgentDispatch { agent, tool, args } = &value.kind {
                let agent_name = match &agent.kind {
                    ExprKind::AgentRef(n) => n.clone(),
                    _ => continue,
                };
                let tool_name = match &tool.kind {
                    ExprKind::ToolRef(n) => n.clone(),
                    _ => continue,
                };
                let arg_names: Vec<String> = args
                    .iter()
                    .map(|a| match &a.kind {
                        ExprKind::Ident(n) => n.clone(),
                        ExprKind::StringLit(s) => format!("\"{}\"", s),
                        _ => "_".to_string(),
                    })
                    .collect();
                steps.push(FlowStep {
                    variable: name.clone(),
                    agent: agent_name,
                    tool: tool_name,
                    args: arg_names,
                });
            }
        }
    }
    steps
}

/// Convert a type expression to its string representation.
fn type_expr_to_string(ty: &pact_core::ast::types::TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(type_expr_to_string).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => {
            format!("{}?", type_expr_to_string(inner))
        }
    }
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
    fn manifest_generation() {
        let src = r#"
            tool #greet { description: <<Greet>> requires: [^llm.query] params { name :: String } }
            agent @greeter { permits: [^llm.query] tools: [#greet] }
            flow hello(name :: String) -> String { result = @greeter -> #greet(name) return result }
        "#;
        let program = parse_program(src);
        let config = BuildConfig::new("test.pact", "./out", crate::config::Target::Claude);
        let toml = generate_manifest(&program, &config);
        assert!(toml.contains("version = \"0.2\""));
        assert!(toml.contains("greeter"));
        assert!(toml.contains("greet"));
        assert!(toml.contains("hello"));
    }

    #[test]
    fn agent_toml_generation() {
        let src = r#"agent @writer {
            permits: [^llm.query]
            tools: [#write]
            model: "claude-sonnet-4-20250514"
            prompt: <<You are a writer>>
        }"#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[0].kind {
            let toml = generate_agent_toml(agent);
            assert!(toml.contains("name = \"writer\""));
            assert!(toml.contains("model = \"claude-sonnet-4-20250514\""));
            assert!(toml.contains("writer.prompt.md"));
            assert!(toml.contains("llm.query"));
        }
    }

    #[test]
    fn tool_toml_generation() {
        let src = r#"tool #web_search {
            description: <<Search the web for information.>>
            requires: [^net.read]
            params {
                query :: String
                max_results :: Int
            }
            returns :: List<String>
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let toml = generate_tool_toml(tool);
            assert!(toml.contains("name = \"web_search\""));
            assert!(toml.contains("Search the web"));
            assert!(toml.contains("net.read"));
            assert!(toml.contains("query"));
            assert!(toml.contains("List<String>"));
        }
    }

    #[test]
    fn flow_toml_generation() {
        let src = r#"
            agent @g { permits: [^llm.query] tools: [#greet] }
            flow hello(name :: String) -> String {
                result = @g -> #greet(name)
                return result
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Flow(flow) = &program.decls[1].kind {
            let toml = generate_flow_toml(flow);
            assert!(toml.contains("name = \"hello\""));
            assert!(toml.contains("return_type = \"String\""));
            assert!(toml.contains("variable = \"result\""));
            assert!(toml.contains("agent = \"g\""));
            assert!(toml.contains("tool = \"greet\""));
        }
    }

    #[test]
    fn permissions_toml_generation() {
        let src = r#"permit_tree {
            ^net {
                ^net.read
                ^net.write
            }
        }"#;
        let program = parse_program(src);
        let toml = generate_permissions_toml(&program);
        assert!(toml.contains("path = \"net\""));
        assert!(toml.contains("net.read"));
        assert!(toml.contains("net.write"));
    }
}
