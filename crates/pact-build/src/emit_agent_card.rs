// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! Agent Card JSON generation for A2A (agent-to-agent) discovery.
//!
//! Converts PACT agent declarations into Agent Card JSON, a structured
//! format describing an agent's capabilities, tools, skills, compliance
//! posture, and available flows. This enables automated agent discovery
//! and interoperability in multi-agent systems.
//!
//! # Agent Card Format
//!
//! ```json
//! {
//!   "version": "1.0",
//!   "agent": {
//!     "name": "agent_name",
//!     "description": "Agent description",
//!     "model": "model-id",
//!     "capabilities": {
//!       "permissions": ["llm.query", "net.read"],
//!       "tools": [{ "name": "...", "description": "...", "parameters": {} }],
//!       "skills": ["skill_name"]
//!     },
//!     "compliance": { ... }
//!   },
//!   "flows": [{ "name": "...", "params": [...], "return_type": "..." }],
//!   "metadata": { "generated_by": "pact-build", "source": "file.pact" }
//! }
//! ```

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{AgentDecl, DeclKind, Program};
use pact_core::ast::types::TypeExprKind;
use serde::Serialize;
use serde_json::json;

use crate::emit_common::type_to_json_schema;

/// Top-level Agent Card structure for A2A discovery.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCard {
    /// Schema version for the agent card format.
    pub version: String,
    /// The agent definition.
    pub agent: AgentCardAgent,
    /// Flows that reference this agent.
    pub flows: Vec<AgentCardFlow>,
    /// Generation metadata.
    pub metadata: AgentCardMetadata,
}

/// Agent identity and capabilities within an agent card.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardAgent {
    /// Agent name (without the `@` prefix).
    pub name: String,
    /// Human-readable description extracted from the agent's prompt.
    pub description: String,
    /// Model identifier (e.g. `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// Agent capabilities: permissions, tools, and skills.
    pub capabilities: AgentCardCapabilities,
    /// Compliance metadata, if the agent references a compliance profile.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compliance: Option<AgentCardCompliance>,
}

/// Capabilities advertised by an agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardCapabilities {
    /// Permission capabilities the agent holds (e.g. `["llm.query", "net.read"]`).
    pub permissions: Vec<String>,
    /// Tools available to the agent, with parameter schemas.
    pub tools: Vec<AgentCardTool>,
    /// Skills available to the agent (by name).
    pub skills: Vec<String>,
}

/// A tool definition within an agent card.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardTool {
    /// Tool name (without the `#` prefix).
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub parameters: serde_json::Value,
}

/// Compliance metadata for an agent card.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardCompliance {
    /// Risk tier: `"low"`, `"medium"`, `"high"`, or `"critical"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_tier: Option<String>,
    /// Regulatory frameworks (e.g. `["pci_dss", "gdpr"]`).
    pub frameworks: Vec<String>,
    /// Separation-of-duty role assignments.
    pub roles: serde_json::Map<String, serde_json::Value>,
    /// Audit level: `"none"`, `"summary"`, or `"full"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audit_level: Option<String>,
    /// Data retention period (e.g. `"7y"`, `"90d"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
}

/// A flow that references an agent.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardFlow {
    /// Flow name.
    pub name: String,
    /// Flow parameters with types.
    pub params: Vec<AgentCardParam>,
    /// Return type as a string (e.g. `"String"`, `"List<Int>"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

/// A parameter in a flow definition.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardParam {
    /// Parameter name.
    pub name: String,
    /// Type as a string (e.g. `"String"`, `"Int"`).
    #[serde(rename = "type")]
    pub ty: String,
}

/// Generation metadata for an agent card.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCardMetadata {
    /// The tool that generated this agent card.
    pub generated_by: String,
    /// Source file name.
    pub source: String,
}

/// Format a PACT type expression as a human-readable string.
fn format_type(ty: &pact_core::ast::types::TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_type).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", format_type(inner)),
    }
}

/// Generate an Agent Card JSON string for a single agent.
///
/// Resolves the agent's tools, skills, permissions, compliance profile,
/// and any flows that dispatch to this agent within the program.
pub fn generate_agent_card(agent: &AgentDecl, program: &Program, source_name: &str) -> String {
    // Extract model
    let model = agent
        .model
        .as_ref()
        .and_then(|e| match &e.kind {
            ExprKind::StringLit(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    // Extract description from prompt
    let description = agent
        .prompt
        .as_ref()
        .map(crate::emit_common::extract_prompt_text)
        .unwrap_or_default();

    // Collect permission strings
    let permissions: Vec<String> = agent
        .permits
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(parts) => Some(parts.join(".")),
            _ => None,
        })
        .collect();

    // Collect tool names referenced by the agent
    let tool_names: Vec<&str> = agent
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    // Resolve tool declarations and build card tools
    let tools: Vec<AgentCardTool> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) if tool_names.contains(&t.name.as_str()) => {
                let desc = match &t.description.kind {
                    ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
                    _ => String::new(),
                };

                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for param in &t.params {
                    let type_schema = param
                        .ty
                        .as_ref()
                        .map(type_to_json_schema)
                        .unwrap_or_else(|| json!({}));

                    let mut prop = type_schema;
                    if let Some(obj) = prop.as_object_mut() {
                        obj.insert(
                            "description".to_string(),
                            json!(format!("{} parameter", param.name)),
                        );
                    }
                    properties.insert(param.name.clone(), prop);
                    required.push(json!(param.name));
                }

                let parameters = json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                });

                Some(AgentCardTool {
                    name: t.name.clone(),
                    description: desc,
                    parameters,
                })
            }
            _ => None,
        })
        .collect();

    // Collect skill names
    let skills: Vec<String> = agent
        .skills
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::SkillRef(name) => Some(name.clone()),
            _ => None,
        })
        .collect();

    // Resolve compliance profile if referenced
    let compliance = agent.compliance.as_ref().and_then(|comp_name| {
        program.decls.iter().find_map(|d| match &d.kind {
            DeclKind::Compliance(c) if c.name == *comp_name => {
                let mut roles = serde_json::Map::new();
                for role in &c.roles {
                    roles.insert(
                        role.role.clone(),
                        serde_json::Value::String(role.assignee.clone()),
                    );
                }
                Some(AgentCardCompliance {
                    risk_tier: c.risk.clone(),
                    frameworks: c.frameworks.clone(),
                    roles,
                    audit_level: c.audit.clone(),
                    retention: c.retention.clone(),
                })
            }
            _ => None,
        })
    });

    // Find flows that reference this agent
    let flows: Vec<AgentCardFlow> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Flow(f) if flow_references_agent(f, &agent.name) => {
                let params: Vec<AgentCardParam> = f
                    .params
                    .iter()
                    .map(|p| AgentCardParam {
                        name: p.name.clone(),
                        ty: p
                            .ty
                            .as_ref()
                            .map(format_type)
                            .unwrap_or_else(|| "Any".to_string()),
                    })
                    .collect();

                let return_type = f.return_type.as_ref().map(format_type);

                Some(AgentCardFlow {
                    name: f.name.clone(),
                    params,
                    return_type,
                })
            }
            _ => None,
        })
        .collect();

    let card = AgentCard {
        version: "1.0".to_string(),
        agent: AgentCardAgent {
            name: agent.name.clone(),
            description,
            model,
            capabilities: AgentCardCapabilities {
                permissions,
                tools,
                skills,
            },
            compliance,
        },
        flows,
        metadata: AgentCardMetadata {
            generated_by: "pact-build".to_string(),
            source: source_name.to_string(),
        },
    };

    serde_json::to_string_pretty(&card).expect("JSON serialization should not fail")
}

/// Check whether a flow body contains a reference to the given agent name.
fn flow_references_agent(flow: &pact_core::ast::stmt::FlowDecl, agent_name: &str) -> bool {
    flow.body
        .iter()
        .any(|expr| expr_references_agent(expr, agent_name))
}

/// Recursively check whether an expression references the given agent name.
fn expr_references_agent(expr: &pact_core::ast::expr::Expr, agent_name: &str) -> bool {
    match &expr.kind {
        ExprKind::AgentRef(name) if name == agent_name => true,
        ExprKind::AgentDispatch { agent, tool, args } => {
            expr_references_agent(agent, agent_name)
                || expr_references_agent(tool, agent_name)
                || args.iter().any(|e| expr_references_agent(e, agent_name))
        }
        ExprKind::Pipeline { left, right } => {
            expr_references_agent(left, agent_name) || expr_references_agent(right, agent_name)
        }
        ExprKind::FallbackChain { primary, fallback } => {
            expr_references_agent(primary, agent_name)
                || expr_references_agent(fallback, agent_name)
        }
        ExprKind::Assign { value, .. } => expr_references_agent(value, agent_name),
        ExprKind::Parallel(exprs) => exprs.iter().any(|e| expr_references_agent(e, agent_name)),
        ExprKind::Return(inner) | ExprKind::Fail(inner) => expr_references_agent(inner, agent_name),
        ExprKind::OnError { body, fallback } => {
            expr_references_agent(body, agent_name) || expr_references_agent(fallback, agent_name)
        }
        ExprKind::FuncCall { callee, args } => {
            expr_references_agent(callee, agent_name)
                || args.iter().any(|e| expr_references_agent(e, agent_name))
        }
        _ => false,
    }
}

/// Generate Agent Card JSON for every agent in a program.
///
/// Returns a vector of `(filename, json_content)` pairs, where the
/// filename is `{agent_name}.agent_card.json`.
pub fn generate_all_agent_cards(program: &Program, source_name: &str) -> Vec<(String, String)> {
    program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Agent(agent) => {
                let json = generate_agent_card(agent, program, source_name);
                let filename = format!("{}.agent_card.json", agent.name);
                Some((filename, json))
            }
            _ => None,
        })
        .collect()
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
    fn basic_agent_card() {
        let src = r#"
            tool #greet {
                description: <<Generate a greeting message.>>
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
        "#;
        let program = parse_program(src);
        let cards = generate_all_agent_cards(&program, "test.pact");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].0, "greeter.agent_card.json");

        let parsed: serde_json::Value = serde_json::from_str(&cards[0].1).unwrap();
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["agent"]["name"], "greeter");
        assert_eq!(
            parsed["agent"]["description"],
            "You are a friendly greeter."
        );
        assert_eq!(parsed["agent"]["model"], "claude-sonnet-4-20250514");
        assert_eq!(
            parsed["agent"]["capabilities"]["permissions"][0],
            "llm.query"
        );
        assert_eq!(parsed["agent"]["capabilities"]["tools"][0]["name"], "greet");
        assert_eq!(
            parsed["agent"]["capabilities"]["tools"][0]["parameters"]["properties"]["name"]["type"],
            "string"
        );
        assert!(parsed["agent"]["compliance"].is_null());
        assert_eq!(parsed["metadata"]["generated_by"], "pact-build");
        assert_eq!(parsed["metadata"]["source"], "test.pact");
    }

    #[test]
    fn agent_card_with_compliance() {
        let src = r#"
            compliance "payment_processing" {
                risk: high
                frameworks: [pci_dss, gdpr]
                audit: full
                retention: "7y"
                roles {
                    approver: "finance_lead"
                    executor: "payment_agent"
                }
            }
            tool #charge {
                description: <<Process a payment.>>
                requires: [^net.write]
                params { amount :: Float }
                returns :: Bool
            }
            agent @payments {
                permits: [^net.write]
                tools: [#charge]
                model: "claude-sonnet-4-20250514"
                prompt: <<You handle payments securely.>>
                compliance: "payment_processing"
            }
        "#;
        let program = parse_program(src);
        let cards = generate_all_agent_cards(&program, "billing.pact");
        assert_eq!(cards.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&cards[0].1).unwrap();
        let compliance = &parsed["agent"]["compliance"];
        assert_eq!(compliance["risk_tier"], "high");
        assert_eq!(compliance["frameworks"][0], "pci_dss");
        assert_eq!(compliance["frameworks"][1], "gdpr");
        assert_eq!(compliance["roles"]["approver"], "finance_lead");
        assert_eq!(compliance["roles"]["executor"], "payment_agent");
        assert_eq!(compliance["audit_level"], "full");
        assert_eq!(compliance["retention"], "7y");
    }

    #[test]
    fn agent_card_with_flows() {
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
                model: "claude-sonnet-4-20250514"
                prompt: <<You greet people.>>
            }
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let program = parse_program(src);
        let cards = generate_all_agent_cards(&program, "test.pact");
        let parsed: serde_json::Value = serde_json::from_str(&cards[0].1).unwrap();

        assert_eq!(parsed["flows"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["flows"][0]["name"], "hello");
        assert_eq!(parsed["flows"][0]["params"][0]["name"], "name");
        assert_eq!(parsed["flows"][0]["params"][0]["type"], "String");
        assert_eq!(parsed["flows"][0]["return_type"], "String");
    }

    #[test]
    fn multiple_agents_generate_multiple_cards() {
        let src = r#"
            tool #search {
                description: <<Search the web.>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            tool #summarize {
                description: <<Summarize text.>>
                requires: [^llm.query]
                params { text :: String }
                returns :: String
            }
            agent @searcher {
                permits: [^net.read]
                tools: [#search]
                model: "claude-sonnet-4-20250514"
                prompt: <<You search the web.>>
            }
            agent @writer {
                permits: [^llm.query]
                tools: [#summarize]
                model: "claude-sonnet-4-20250514"
                prompt: <<You summarize content.>>
            }
        "#;
        let program = parse_program(src);
        let cards = generate_all_agent_cards(&program, "multi.pact");
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].0, "searcher.agent_card.json");
        assert_eq!(cards[1].0, "writer.agent_card.json");

        let card0: serde_json::Value = serde_json::from_str(&cards[0].1).unwrap();
        let card1: serde_json::Value = serde_json::from_str(&cards[1].1).unwrap();
        assert_eq!(card0["agent"]["capabilities"]["tools"][0]["name"], "search");
        assert_eq!(
            card1["agent"]["capabilities"]["tools"][0]["name"],
            "summarize"
        );
    }

    #[test]
    fn agent_card_no_tools_no_compliance() {
        let src = r#"
            agent @minimal {
                permits: [^llm.query]
                tools: []
                prompt: <<A minimal agent.>>
            }
        "#;
        let program = parse_program(src);
        let cards = generate_all_agent_cards(&program, "minimal.pact");
        assert_eq!(cards.len(), 1);

        let parsed: serde_json::Value = serde_json::from_str(&cards[0].1).unwrap();
        assert_eq!(parsed["agent"]["name"], "minimal");
        assert_eq!(parsed["agent"]["model"], "");
        assert!(parsed["agent"]["capabilities"]["tools"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(parsed["agent"]["compliance"].is_null());
    }

    #[test]
    fn format_type_display() {
        use pact_core::ast::types::{TypeExpr, TypeExprKind};
        use pact_core::span::{SourceId, Span};

        let span = Span::new(SourceId(0), 0, 0);

        let simple = TypeExpr {
            kind: TypeExprKind::Named("String".into()),
            span,
        };
        assert_eq!(format_type(&simple), "String");

        let generic = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "List".into(),
                args: vec![TypeExpr {
                    kind: TypeExprKind::Named("Int".into()),
                    span,
                }],
            },
            span,
        };
        assert_eq!(format_type(&generic), "List<Int>");

        let optional = TypeExpr {
            kind: TypeExprKind::Optional(Box::new(TypeExpr {
                kind: TypeExprKind::Named("Float".into()),
                span,
            })),
            span,
        };
        assert_eq!(format_type(&optional), "Float?");
    }
}
