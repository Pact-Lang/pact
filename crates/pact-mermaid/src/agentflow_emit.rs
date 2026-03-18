// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! PACT `Program` → agentflow text and JSON.
//!
//! Converts a parsed PACT program into both agentflow text syntax
//! and JSON AST representation.

use crate::agentflow::*;
use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, FlowDecl, Program, TemplateEntry};
use std::collections::BTreeMap;

/// Convert a PACT `Program` into agentflow text.
pub fn pact_to_agentflow(program: &Program) -> String {
    let graph = pact_to_agentflow_graph(program);
    emit_agentflow_text(&graph)
}

/// Convert a PACT `Program` into a JSON value.
pub fn pact_to_agentflow_json(program: &Program) -> serde_json::Value {
    let graph = pact_to_agentflow_graph(program);
    serde_json::to_value(&graph).expect("AgentFlowGraph should always serialize")
}

/// Convert a PACT `Program` into an `AgentFlowGraph`.
pub fn pact_to_agentflow_graph(program: &Program) -> AgentFlowGraph {
    let mut graph = AgentFlowGraph::new("LR");

    // First pass: collect all tools and skills by name for later lookup.
    let mut tool_nodes: BTreeMap<String, AgentFlowToolNode> = BTreeMap::new();
    let mut skill_nodes: BTreeMap<String, AgentFlowSkillNode> = BTreeMap::new();

    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Tool(t) => {
                let node = tool_decl_to_node(t);
                tool_nodes.insert(t.name.clone(), node);
            }
            DeclKind::Skill(s) => {
                let node = skill_decl_to_node(s);
                skill_nodes.insert(s.name.clone(), node);
            }
            _ => {}
        }
    }

    // Second pass: build agents, schemas, templates, directives, bundles.
    for decl in &program.decls {
        match &decl.kind {
            DeclKind::Agent(a) => {
                let mut agent = AgentFlowAgent {
                    id: a.name.clone(),
                    label: format!("@{}", a.name),
                    model: a.model.as_ref().and_then(|e| match &e.kind {
                        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => Some(s.clone()),
                        _ => None,
                    }),
                    prompt: a.prompt.as_ref().and_then(|e| match &e.kind {
                        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => Some(s.clone()),
                        _ => None,
                    }),
                    memory: a
                        .memory
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::MemoryRef(name) => Some(format!("~{}", name)),
                            _ => None,
                        })
                        .collect(),
                    nodes: vec![],
                    skills: vec![],
                };

                // Match tools to this agent.
                for tool_expr in &a.tools {
                    if let ExprKind::ToolRef(tool_name) = &tool_expr.kind {
                        if let Some(node) = tool_nodes.get(tool_name) {
                            agent.nodes.push(node.clone());
                        }
                    }
                }

                // Match skills to this agent.
                for skill_expr in &a.skills {
                    if let ExprKind::SkillRef(skill_name) = &skill_expr.kind {
                        if let Some(node) = skill_nodes.get(skill_name) {
                            agent.skills.push(node.clone());
                        }
                    }
                }

                graph.agents.push(agent);
            }
            DeclKind::Schema(s) => {
                graph.schemas.push(AgentFlowSchemaNode {
                    id: s.name.clone(),
                    label: s.name.clone(),
                    shape: "hexagon".to_string(),
                    metadata: SchemaMetadata {
                        fields: s
                            .fields
                            .iter()
                            .map(|f| (f.name.clone(), type_expr_to_string(&f.ty)))
                            .collect(),
                    },
                });
            }
            DeclKind::Template(t) => {
                let mut fields = BTreeMap::new();
                let mut sections = Vec::new();
                for entry in &t.entries {
                    match entry {
                        TemplateEntry::Field {
                            name, ty, ..
                        } => {
                            fields.insert(name.clone(), type_expr_to_string(ty));
                        }
                        TemplateEntry::Repeat {
                            name, ty, count, ..
                        } => {
                            fields.insert(
                                name.clone(),
                                format!("{} * {}", type_expr_to_string(ty), count),
                            );
                        }
                        TemplateEntry::Section { name, .. } => {
                            sections.push(name.clone());
                        }
                    }
                }
                graph.templates.push(AgentFlowTemplateNode {
                    id: t.name.clone(),
                    label: t.name.clone(),
                    shape: "subroutine".to_string(),
                    metadata: TemplateMetadata { fields, sections },
                });
            }
            DeclKind::Directive(d) => {
                let params: BTreeMap<String, String> = d
                    .params
                    .iter()
                    .map(|p| {
                        let default_str = match &p.default.kind {
                            ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.clone(),
                            _ => String::new(),
                        };
                        let ty_str = type_expr_to_string(&p.ty);
                        if default_str.is_empty() {
                            (p.name.clone(), ty_str)
                        } else {
                            (p.name.clone(), format!("{} = {}", ty_str, default_str))
                        }
                    })
                    .collect();
                graph.directives.push(AgentFlowDirectiveNode {
                    id: d.name.clone(),
                    label: d.name.clone(),
                    shape: "trapezoid".to_string(),
                    metadata: DirectiveMetadata {
                        text: d.text.clone(),
                        params,
                    },
                });
            }
            DeclKind::AgentBundle(ab) => {
                let agents: Vec<String> = ab
                    .agents
                    .iter()
                    .filter_map(|e| match &e.kind {
                        ExprKind::AgentRef(name) => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                let fallbacks = ab.fallbacks.as_ref().map(|_| {
                    // Emit a simplified fallback string.
                    agents.join(" ?> ")
                });
                graph.bundles.push(AgentFlowBundle {
                    id: ab.name.clone(),
                    label: format!("@{}", ab.name),
                    agents,
                    fallbacks,
                });
            }
            DeclKind::TypeAlias(ta) => {
                graph.type_aliases.push(AgentFlowTypeAlias {
                    name: ta.name.clone(),
                    variants: ta.variants.clone(),
                });
            }
            DeclKind::Flow(f) => {
                // Extract edges from flow body.
                let mut flow_edges = Vec::new();
                extract_flow_edges(&f.body, &mut flow_edges);
                graph.edges.extend(flow_edges);

                // Extract flow steps for task-based emission.
                let flow_def = extract_flow_def(f);
                graph.flows.push(flow_def);
            }
            _ => {}
        }
    }

    // Add reference edges from tool output/directives.
    let mut ref_edges = Vec::new();
    for agent in &graph.agents {
        for tool in &agent.nodes {
            if let Some(output) = &tool.metadata.output {
                let tpl_name = output.strip_prefix('%').unwrap_or(output);
                ref_edges.push(AgentFlowEdge {
                    from: tool.id.clone(),
                    to: tpl_name.to_string(),
                    label: None,
                    edge_type: EdgeType::Reference,
                });
            }
            for dir in &tool.metadata.directives {
                let dir_name = dir.strip_prefix('%').unwrap_or(dir);
                ref_edges.push(AgentFlowEdge {
                    from: tool.id.clone(),
                    to: dir_name.to_string(),
                    label: None,
                    edge_type: EdgeType::Reference,
                });
            }
        }
    }
    graph.edges.extend(ref_edges);

    graph
}

// ── Flow edge extraction ───────────────────────────────────────────────────

/// Extract flow edges from a PACT flow body with proper variable-binding tracking.
///
/// For each `name = @agent -> #tool(arg1, arg2, ...)`, we:
/// 1. Record that `name` was produced by `#tool`
/// 2. For each argument, find which tool produced it and create a labeled edge
///
/// Extracts flow edges from a flow body, using implicit linear chaining.
///
/// Rules:
/// - Each step emits **one** edge from the previous step, labeled with the
///   previous step's output variable name.
/// - At fan-in points (tool consuming multiple prior outputs), additional
///   labeled edges are emitted only for inputs from **non-immediate**
///   predecessors (skip edges).
///
/// This avoids redundant edges: the linear chain is implicit, only true
/// fan-in gets extra arrows.
///
/// ```pact
/// triage = @monitor -> #triage_alert(alert)
/// investigation = @investigator -> #analyze_incident(triage)
/// root_cause = @investigator -> #root_cause_analysis(investigation)
/// runbook = @responder -> #generate_runbook(root_cause)
/// dashboard = @reporter -> #create_report(triage, investigation, root_cause, runbook)
/// ```
/// Edges: triage_alert →|triage| analyze_incident →|investigation| root_cause_analysis
///        →|root_cause| generate_runbook →|runbook| create_report
///        Plus skip edges: triage_alert →|triage| create_report,
///        analyze_incident →|investigation| create_report,
///        root_cause_analysis →|root_cause| create_report
fn extract_flow_edges(
    body: &[pact_core::ast::expr::Expr],
    edges: &mut Vec<AgentFlowEdge>,
) {
    use std::collections::HashMap;

    // Maps variable name -> tool that produced it
    let mut var_to_tool: HashMap<String, String> = HashMap::new();
    // The most recently seen tool (for linear chain edges)
    let mut prev_tool: Option<(String, String)> = None; // (var_name, tool_name)

    for expr in body {
        let dispatch = match &expr.kind {
            ExprKind::Assign { name, value } => {
                extract_dispatch_info(value).map(|(tool, args)| (Some(name.clone()), tool, args))
            }
            ExprKind::AgentDispatch { tool, args, .. } => {
                if let ExprKind::ToolRef(tool_name) = &tool.kind {
                    Some((None, tool_name.clone(), extract_arg_names(args)))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((var_name, tool_name, args)) = dispatch {
            // 1. Emit the linear chain edge from the previous step
            if let Some((ref prev_var, ref prev_tool_name)) = prev_tool {
                edges.push(AgentFlowEdge {
                    from: prev_tool_name.clone(),
                    to: tool_name.clone(),
                    label: Some(prev_var.clone()),
                    edge_type: EdgeType::Flow,
                });
            }

            // 2. Emit skip edges for fan-in args from non-immediate predecessors
            for arg_name in &args {
                if let Some(source_tool) = var_to_tool.get(arg_name) {
                    // Skip if this is the immediate predecessor (already covered above)
                    let is_immediate = prev_tool
                        .as_ref()
                        .is_some_and(|(pv, _)| pv == arg_name);
                    if !is_immediate {
                        edges.push(AgentFlowEdge {
                            from: source_tool.clone(),
                            to: tool_name.clone(),
                            label: Some(arg_name.clone()),
                            edge_type: EdgeType::Flow,
                        });
                    }
                }
            }

            // Track this step
            if let Some(name) = var_name {
                var_to_tool.insert(name.clone(), tool_name.clone());
                prev_tool = Some((name, tool_name));
            }
        }
    }
}

/// Extract a flow definition with its steps from a FlowDecl.
fn extract_flow_def(f: &FlowDecl) -> AgentFlowDef {
    let mut steps = Vec::new();
    for expr in &f.body {
        if let ExprKind::Assign { name, value } = &expr.kind {
            if let Some((agent_name, tool_name, args)) = extract_full_dispatch_info(value) {
                steps.push(AgentFlowStep {
                    output_var: name.clone(),
                    agent: agent_name,
                    tool: tool_name,
                    args,
                });
            }
        }
    }
    AgentFlowDef {
        name: f.name.clone(),
        steps,
    }
}

/// Extract agent name, tool name, and argument names from a dispatch expression.
fn extract_full_dispatch_info(
    expr: &pact_core::ast::expr::Expr,
) -> Option<(String, String, Vec<String>)> {
    match &expr.kind {
        ExprKind::AgentDispatch { agent, tool, args } => {
            let agent_name = match &agent.kind {
                ExprKind::AgentRef(name) => name.clone(),
                _ => return None,
            };
            let tool_name = match &tool.kind {
                ExprKind::ToolRef(name) => name.clone(),
                _ => return None,
            };
            Some((agent_name, tool_name, extract_arg_names(args)))
        }
        ExprKind::Pipeline { left, right } => {
            extract_full_dispatch_info(right).or_else(|| extract_full_dispatch_info(left))
        }
        _ => None,
    }
}

/// Extract tool name and argument names from a dispatch expression.
fn extract_dispatch_info(
    expr: &pact_core::ast::expr::Expr,
) -> Option<(String, Vec<String>)> {
    match &expr.kind {
        ExprKind::AgentDispatch { tool, args, .. } => {
            if let ExprKind::ToolRef(name) = &tool.kind {
                Some((name.clone(), extract_arg_names(args)))
            } else {
                None
            }
        }
        ExprKind::Pipeline { left, right } => {
            extract_dispatch_info(right).or_else(|| extract_dispatch_info(left))
        }
        _ => None,
    }
}

/// Extract variable names from a list of argument expressions.
fn extract_arg_names(args: &[pact_core::ast::expr::Expr]) -> Vec<String> {
    args.iter()
        .filter_map(|arg| match &arg.kind {
            ExprKind::Ident(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}


// ── Tool/Skill decl → node ─────────────────────────────────────────────────

fn tool_decl_to_node(t: &pact_core::ast::stmt::ToolDecl) -> AgentFlowToolNode {
    let description = match &t.description.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.clone(),
        _ => String::new(),
    };

    let requires: Vec<String> = t
        .requires
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::PermissionRef(parts) => Some(format!("^{}", parts.join("."))),
            _ => None,
        })
        .collect();

    let source = t.source.as_ref().map(|s| {
        if s.args.is_empty() {
            format!("^{}", s.capability)
        } else {
            format!("^{}({})", s.capability, s.args.join(", "))
        }
    });

    let output = t.output.as_ref().map(|o| format!("%{}", o));

    let directives: Vec<String> = t.directives.iter().map(|d| format!("%{}", d)).collect();

    let params: BTreeMap<String, String> = t
        .params
        .iter()
        .map(|p| {
            let ty = p
                .ty
                .as_ref()
                .map(type_expr_to_string)
                .unwrap_or_else(|| "String".to_string());
            (p.name.clone(), ty)
        })
        .collect();

    let returns = t.return_type.as_ref().map(type_expr_to_string);

    AgentFlowToolNode {
        id: t.name.clone(),
        label: to_title_case(&t.name),
        shape: "roundedRect".to_string(),
        metadata: ToolMetadata {
            description,
            requires,
            deny: vec![],
            source,
            handler: t.handler.clone(),
            output,
            directives,
            params,
            returns,
            retry: t.retry,
            cache: t.cache.clone(),
            validate: t.validate.clone(),
        },
    }
}

fn skill_decl_to_node(s: &pact_core::ast::stmt::SkillDecl) -> AgentFlowSkillNode {
    let description = match &s.description.kind {
        ExprKind::PromptLit(str_val) | ExprKind::StringLit(str_val) => str_val.clone(),
        _ => String::new(),
    };

    let tools: Vec<String> = s
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(format!("#{}", name)),
            _ => None,
        })
        .collect();

    let strategy = s.strategy.as_ref().and_then(|e| match &e.kind {
        ExprKind::PromptLit(str_val) | ExprKind::StringLit(str_val) => Some(str_val.clone()),
        _ => None,
    });

    let params: BTreeMap<String, String> = s
        .params
        .iter()
        .map(|p| {
            let ty = p
                .ty
                .as_ref()
                .map(type_expr_to_string)
                .unwrap_or_else(|| "String".to_string());
            (p.name.clone(), ty)
        })
        .collect();

    let returns = s.return_type.as_ref().map(type_expr_to_string);

    AgentFlowSkillNode {
        id: s.name.clone(),
        label: to_title_case(&s.name),
        shape: "stadium".to_string(),
        metadata: SkillMetadata {
            description,
            tools,
            strategy,
            params,
            returns,
        },
    }
}

// ── Text emitter ───────────────────────────────────────────────────────────

fn emit_agentflow_text(graph: &AgentFlowGraph) -> String {
    let mut out = String::new();
    out.push_str(&format!("agentflow {}\n", graph.direction));

    // ── Types: schemas as Record types ─────────────────────────────────────
    if !graph.schemas.is_empty() || !graph.type_aliases.is_empty() {
        for schema in &graph.schemas {
            out.push_str(&format!("  type {} = Record {{\n", schema.id));
            for (name, ty) in &schema.metadata.fields {
                out.push_str(&format!("    {}: {}\n", name, ty));
            }
            out.push_str("  }\n\n");
        }

        for ta in &graph.type_aliases {
            out.push_str(&format!(
                "  type {} = {}\n",
                ta.name,
                ta.variants.join(" | ")
            ));
        }
        out.push('\n');
    }

    // ── Templates (TBD: Mermaid syntax pending) ─────────────────────────────
    for tpl in &graph.templates {
        out.push_str(&format!(
            "  %% template {}: {}\n",
            tpl.id,
            tpl.metadata
                .fields
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !graph.templates.is_empty() {
        out.push('\n');
    }

    // ── Directives (TBD: Mermaid syntax pending) ──────────────────────────
    for dir in &graph.directives {
        out.push_str(&format!("  %% directive {}\n", dir.id));
    }
    if !graph.directives.is_empty() {
        out.push('\n');
    }

    // ── Tools: flat top-level definitions ──────────────────────────────────
    for agent in &graph.agents {
        for tool in &agent.nodes {
            out.push_str(&format!("  {}@{{\n", tool.id));
            out.push_str("    toolDefinition: true\n");
            out.push_str("    shape: subroutine\n");
            out.push_str(&format!(
                "    description: \"{}\"\n",
                tool.metadata.description.replace('"', "\\\"")
            ));
            if !tool.metadata.requires.is_empty() {
                let perms: Vec<String> = tool
                    .metadata
                    .requires
                    .iter()
                    .map(|r| r.strip_prefix('^').unwrap_or(r).to_string())
                    .collect();
                out.push_str(&format!("    requires: \"{}\"\n", perms.join(", ")));
            }
            if let Some(output) = &tool.metadata.output {
                out.push_str(&format!("    output: \"{}\"\n", output));
            }
            if !tool.metadata.directives.is_empty() {
                out.push_str(&format!(
                    "    directives: \"{}\"\n",
                    tool.metadata.directives.join(", ")
                ));
            }
            if !tool.metadata.params.is_empty() {
                out.push_str("    params:\n");
                for (name, ty) in &tool.metadata.params {
                    out.push_str(&format!("      {}: {}\n", name, ty));
                }
            }
            if let Some(returns) = &tool.metadata.returns {
                out.push_str(&format!("    returns: \"{}\"\n", returns));
            }
            out.push_str("  }\n\n");
        }
    }

    // ── Agents: grouped under agent bundle or standalone ───────────────────
    // Emit bundles first — agents inside bundles.
    let bundled_agents: Vec<&str> = graph
        .bundles
        .iter()
        .flat_map(|b| b.agents.iter().map(|s| s.as_str()))
        .collect();

    for bundle in &graph.bundles {
        out.push_str(&format!(
            "  agent {}[\"{}\"]\n",
            bundle.id,
            to_title_case(&bundle.id)
        ));

        for agent in &graph.agents {
            if bundle.agents.contains(&agent.id) {
                emit_agent_definition(&mut out, agent);
            }
        }

        out.push_str("  end\n\n");
    }

    // Emit unbundled agents.
    for agent in &graph.agents {
        if !bundled_agents.contains(&agent.id.as_str()) {
            emit_agent_definition(&mut out, agent);
        }
    }

    // ── Flows: task-based blocks ──────────────────────────────────────────
    for flow in &graph.flows {
        emit_flow_tasks(&mut out, flow, graph);
    }

    out
}

/// Emit an agent definition in the new `@{ agentDefinition: true }` format.
fn emit_agent_definition(out: &mut String, agent: &AgentFlowAgent) {
    out.push_str(&format!("\n    {}@{{\n", agent.id));
    out.push_str("      agentDefinition: true\n");
    out.push_str("      shape: hex\n");
    if let Some(model) = &agent.model {
        out.push_str(&format!("      model: \"{}\"\n", model));
    }

    // Collect permissions from tools.
    let mut permits = Vec::new();
    for tool in &agent.nodes {
        for perm in &tool.metadata.requires {
            let p = perm.strip_prefix('^').unwrap_or(perm);
            if !permits.contains(&p) {
                permits.push(p);
            }
        }
    }
    if !permits.is_empty() {
        out.push_str(&format!("      permits: \"{}\"\n", permits.join(", ")));
    }

    let tool_names: Vec<&str> = agent.nodes.iter().map(|t| t.id.as_str()).collect();
    if !tool_names.is_empty() {
        out.push_str(&format!("      tools: \"{}\"\n", tool_names.join(", ")));
    }

    if let Some(prompt) = &agent.prompt {
        out.push_str(&format!(
            "      prompt: \"{}\"\n",
            prompt.replace('"', "\\\"")
        ));
    }

    out.push_str("    }\n");
}

/// Emit flow definitions as task blocks with agent/tool/output triplets.
fn emit_flow_tasks(out: &mut String, flow: &AgentFlowDef, _graph: &AgentFlowGraph) {
    use std::collections::HashMap;

    out.push_str(&format!("  flow {}\n", flow.name));

    // Build var->step index for fan-in detection.
    let mut var_to_step: HashMap<String, usize> = HashMap::new();
    for (i, step) in flow.steps.iter().enumerate() {
        var_to_step.insert(step.output_var.clone(), i);
    }

    // Emit each step as a task block.
    for (i, step) in flow.steps.iter().enumerate() {
        let step_label = format!("Step{}", i + 1);
        out.push_str(&format!("    task {}\n", step_label));
        out.push_str("      direction TB\n");
        out.push_str(&format!(
            "      a_{}{{{{{}}}}}@{{ agent: {} }} --- {}@{{ shape: subroutine }}\n",
            step.agent, step.agent, step.agent, step.tool
        ));
        out.push_str(&format!(
            "      a_{} --o {}@{{ shape: doc }}\n",
            step.agent, step.output_var
        ));
        out.push_str("    end\n\n");
    }

    // Emit linear chain edges between steps.
    for i in 0..flow.steps.len().saturating_sub(1) {
        let label = &flow.steps[i].output_var;
        out.push_str(&format!(
            "    Step{} -->|\"{}\"| Step{}\n",
            i + 1,
            label,
            i + 2
        ));
    }

    // Emit fan-in edges: find steps that consume non-immediate-predecessor variables.
    for (i, step) in flow.steps.iter().enumerate() {
        let mut fan_in_args: Vec<&String> = Vec::new();
        for arg in &step.args {
            if let Some(&src_idx) = var_to_step.get(arg) {
                // Skip if it's the immediate predecessor (already covered by linear chain).
                if i > 0 && src_idx == i - 1 {
                    continue;
                }
                // Skip if it's a flow parameter (not produced by any step).
                fan_in_args.push(arg);
            }
        }

        if !fan_in_args.is_empty() {
            out.push_str(&format!("\n    %% Fan-in for Step{}\n", i + 1));
            // The immediate predecessor step carries all fan-in data.
            let prev_step = format!("Step{}", i);
            for arg in fan_in_args {
                out.push_str(&format!(
                    "    {} -->|\"{}\"| Step{}\n",
                    prev_step, arg, i + 1
                ));
            }
        }
    }

    out.push('\n');
}


// ── Helpers ────────────────────────────────────────────────────────────────

fn type_expr_to_string(ty: &pact_core::ast::types::TypeExpr) -> String {
    use pact_core::ast::types::TypeExprKind;
    match &ty.kind {
        TypeExprKind::Named(name) => name.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(type_expr_to_string).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => format!("{}?", type_expr_to_string(inner)),
    }
}

fn to_title_case(s: &str) -> String {
    s.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    fn agent_with_tools_to_agentflow() {
        let src = r#"
            tool #search {
                description: <<Search the web>>
                requires: [^net.read]
                params { query :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        let text = pact_to_agentflow(&program);
        assert!(text.starts_with("agentflow LR\n"));
        assert!(text.contains("search@{"));
        assert!(text.contains("toolDefinition: true"));
        assert!(text.contains("researcher@{"));
        assert!(text.contains("agentDefinition: true"));
        assert!(text.contains("shape: hex"));
    }

    #[test]
    fn schema_to_agentflow() {
        let src = "schema Report { title :: String body :: String }";
        let program = parse_program(src);
        let text = pact_to_agentflow(&program);
        assert!(text.contains("type Report = Record {"));
        assert!(text.contains("title: String"));
        assert!(text.contains("body: String"));
    }

    #[test]
    fn template_to_agentflow() {
        let src = r#"
            template %website_copy {
                HERO_TAGLINE :: String
                MENU_ITEM :: String * 6
                section ENGLISH
            }
        "#;
        let program = parse_program(src);
        let text = pact_to_agentflow(&program);
        assert!(text.contains("%% template website_copy: HERO_TAGLINE"));
    }

    #[test]
    fn directive_to_agentflow() {
        let src = r#"
            directive %scandinavian_design {
                <<Use Google Fonts for headings>>
                params {
                    heading_font :: String = <<Playfair Display>>
                }
            }
        "#;
        let program = parse_program(src);
        let text = pact_to_agentflow(&program);
        assert!(text.contains("%% directive scandinavian_design"));
    }

    #[test]
    fn bundle_to_agentflow() {
        let src = r#"
            agent @a { permits: [] tools: [] }
            agent @b { permits: [] tools: [] }
            agent_bundle @team {
                agents: [@a, @b]
            }
        "#;
        let program = parse_program(src);
        let graph = pact_to_agentflow_graph(&program);
        assert_eq!(graph.bundles.len(), 1);
        assert_eq!(graph.bundles[0].id, "team");
        assert_eq!(graph.bundles[0].agents, vec!["a", "b"]);
    }

    #[test]
    fn flow_creates_edges() {
        let src = r#"
            tool #search {
                description: <<Search>>
                requires: [^net.read]
                params { q :: String }
                returns :: String
            }
            tool #summarize {
                description: <<Summarize>>
                requires: [^llm.query]
                params { content :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read, ^llm.query]
                tools: [#search, #summarize]
            }
            flow research(topic :: String) -> String {
                results = @researcher -> #search(topic)
                summary = @researcher -> #summarize(results)
                return summary
            }
        "#;
        let program = parse_program(src);
        let graph = pact_to_agentflow_graph(&program);

        let flow_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Flow)
            .collect();
        assert_eq!(flow_edges.len(), 1);
        assert_eq!(flow_edges[0].from, "search");
        assert_eq!(flow_edges[0].to, "summarize");
        assert_eq!(flow_edges[0].label.as_deref(), Some("results"));
    }

    #[test]
    fn flow_fan_in_edges() {
        let src = r#"
            tool #triage {
                description: <<Triage>>
                requires: [^llm.query]
                params { alert :: String }
                returns :: String
            }
            tool #investigate {
                description: <<Investigate>>
                requires: [^net.read]
                params { info :: String }
                returns :: String
            }
            tool #find_root_cause {
                description: <<Root cause>>
                requires: [^llm.query]
                params { data :: String }
                returns :: String
            }
            tool #create_report {
                description: <<Report>>
                requires: [^fs.write]
                params { a :: String b :: String c :: String }
                returns :: String
            }
            agent @responder {
                permits: [^llm.query, ^net.read, ^fs.write]
                tools: [#triage, #investigate, #find_root_cause, #create_report]
            }
            flow respond(alert :: String) -> String {
                triage = @responder -> #triage(alert)
                investigation = @responder -> #investigate(triage)
                root_cause = @responder -> #find_root_cause(investigation)
                report = @responder -> #create_report(triage, investigation, root_cause)
                return report
            }
        "#;
        let program = parse_program(src);
        let graph = pact_to_agentflow_graph(&program);

        let flow_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Flow)
            .collect();

        // Linear chain (implicit): triage->investigate, investigate->find_root_cause,
        //   find_root_cause->create_report (labeled "root_cause", immediate predecessor)
        // Skip edges (fan-in): triage->create_report, investigate->create_report
        assert_eq!(flow_edges.len(), 5);

        // The linear chain edges
        assert!(flow_edges.iter().any(|e| e.from == "triage"
            && e.to == "investigate"
            && e.label.as_deref() == Some("triage")));
        assert!(flow_edges.iter().any(|e| e.from == "investigate"
            && e.to == "find_root_cause"
            && e.label.as_deref() == Some("investigation")));
        // Immediate predecessor edge to create_report
        assert!(flow_edges.iter().any(|e| e.from == "find_root_cause"
            && e.to == "create_report"
            && e.label.as_deref() == Some("root_cause")));

        // Skip (fan-in) edges to create_report
        let skip_edges: Vec<_> = flow_edges
            .iter()
            .filter(|e| e.to == "create_report" && e.from != "find_root_cause")
            .collect();
        assert_eq!(skip_edges.len(), 2);
        let labels: Vec<_> = skip_edges.iter().filter_map(|e| e.label.as_deref()).collect();
        assert!(labels.contains(&"triage"));
        assert!(labels.contains(&"investigation"));
    }

    #[test]
    fn reference_edges_from_output() {
        let src = r#"
            template %website_copy {
                HERO :: String
            }
            tool #write_copy {
                description: <<Write copy>>
                requires: [^llm.query]
                output: %website_copy
                params { brief :: String }
                returns :: String
            }
            agent @writer {
                permits: [^llm.query]
                tools: [#write_copy]
            }
        "#;
        let program = parse_program(src);
        let graph = pact_to_agentflow_graph(&program);

        let ref_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Reference)
            .collect();
        assert_eq!(ref_edges.len(), 1);
        assert_eq!(ref_edges[0].from, "write_copy");
        assert_eq!(ref_edges[0].to, "website_copy");
    }

    #[test]
    fn json_output() {
        let src = r#"
            tool #search {
                description: <<Search>>
                requires: [^net.read]
                params { q :: String }
                returns :: String
            }
            agent @researcher {
                permits: [^net.read]
                tools: [#search]
            }
        "#;
        let program = parse_program(src);
        let json = pact_to_agentflow_json(&program);
        assert_eq!(json["type"], "agentflow");
        assert_eq!(json["direction"], "LR");
        assert!(json["agents"].is_array());
        assert_eq!(json["agents"][0]["id"], "researcher");
    }
}
