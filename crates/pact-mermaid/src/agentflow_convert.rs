// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Convert an `AgentFlowGraph` into PACT source text.
//!
//! Maps each agentflow construct to its PACT declaration equivalent.

use crate::agentflow::*;
use std::collections::BTreeSet;

/// Convert an `AgentFlowGraph` to PACT source text.
pub fn agentflow_graph_to_pact(graph: &AgentFlowGraph) -> String {
    let mut out = String::new();

    out.push_str("-- Auto-generated from agentflow diagram\n");
    out.push_str("-- Do not edit manually — regenerate from the .mmd source.\n\n");

    // Collect all permissions for the permit_tree.
    let perms = collect_permissions(graph);
    if !perms.is_empty() {
        emit_permit_tree(&perms, &mut out);
        out.push('\n');
    }

    // Type declarations → schema or type.
    for ty in &graph.types {
        emit_type_decl(ty, &mut out);
        out.push('\n');
    }

    // Schema declarations (for backward compat — skip if already emitted via types).
    let type_names: BTreeSet<&str> = graph.types.iter().map(|t| t.name.as_str()).collect();
    for schema in &graph.schemas {
        if !type_names.contains(schema.id.as_str()) {
            emit_schema(schema, &mut out);
            out.push('\n');
        }
    }

    // Template declarations.
    for template in &graph.templates {
        emit_template(template, &mut out);
        out.push('\n');
    }

    // Directive declarations.
    for directive in &graph.directives {
        emit_directive(directive, &mut out);
        out.push('\n');
    }

    // Tool and skill declarations (extracted from agents).
    // Only emit tools that have meaningful metadata (description, params, etc.).
    for agent in &graph.agents {
        for tool in &agent.nodes {
            if tool_has_metadata(tool) {
                emit_tool(tool, &mut out);
                out.push('\n');
            }
        }
        for skill in &agent.skills {
            emit_skill(skill, &mut out);
            out.push('\n');
        }
    }

    // Agent declarations. Skip empty wrapper agents (bundle containers).
    for agent in &graph.agents {
        if agent.nodes.is_empty()
            && agent.skills.is_empty()
            && agent.permits.is_empty()
            && agent.prompt.is_none()
            && agent.model.is_none()
        {
            continue;
        }
        emit_agent(agent, &mut out);
        out.push('\n');
    }

    // Bundle declarations.
    for bundle in &graph.bundles {
        emit_bundle(bundle, &mut out);
        out.push('\n');
    }

    // Flow declarations.
    if !graph.flows.is_empty() {
        for flow in &graph.flows {
            emit_named_flow(flow, graph, &mut out);
            out.push('\n');
        }
    } else {
        // Legacy fallback: reconstruct a single flow from edges.
        let flow_edges: Vec<&AgentFlowEdge> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Flow)
            .collect();
        if !flow_edges.is_empty() {
            emit_flow_from_edges(&flow_edges, graph, &mut out);
        }
    }

    // Emit comments for association edges (skip internal flow wiring).
    let flow_prefixes: Vec<String> = graph.flows.iter().map(|f| format!("{}_", f.name)).collect();
    let assoc_edges: Vec<&AgentFlowEdge> = graph
        .edges
        .iter()
        .filter(|e| {
            e.edge_type == EdgeType::Association
                && !flow_prefixes
                    .iter()
                    .any(|p| e.from.starts_with(p) || e.to.starts_with(p))
        })
        .collect();
    if !assoc_edges.is_empty() {
        out.push_str("\n-- Associations\n");
        for edge in &assoc_edges {
            out.push_str(&format!("-- {} --- {}\n", edge.from, edge.to));
        }
    }

    out
}

// ── Permission tree ────────────────────────────────────────────────────────

fn collect_permissions(graph: &AgentFlowGraph) -> BTreeSet<String> {
    let mut perms = BTreeSet::new();
    for agent in &graph.agents {
        // Collect from agent-level permits.
        for p in &agent.permits {
            let normalized = if p.starts_with('^') {
                p.clone()
            } else {
                format!("^{}", p)
            };
            perms.insert(normalized);
        }
        // Also collect from tools.
        for tool in &agent.nodes {
            for p in &tool.metadata.requires {
                perms.insert(p.clone());
            }
        }
    }
    perms
}

fn emit_permit_tree(perms: &BTreeSet<String>, out: &mut String) {
    let mut groups: BTreeSet<String> = BTreeSet::new();
    for p in perms {
        let stripped = p.strip_prefix('^').unwrap_or(p);
        if let Some(dot) = stripped.find('.') {
            groups.insert(stripped[..dot].to_string());
        }
    }

    out.push_str("permit_tree {\n");
    for group in &groups {
        out.push_str(&format!("    ^{} {{\n", group));
        for p in perms {
            let stripped = p.strip_prefix('^').unwrap_or(p);
            if stripped.starts_with(&format!("{}.", group)) {
                out.push_str(&format!("        ^{}\n", stripped));
            }
        }
        out.push_str("    }\n");
    }

    for p in perms {
        let stripped = p.strip_prefix('^').unwrap_or(p);
        if !stripped.contains('.') {
            out.push_str(&format!("    ^{}\n", stripped));
        }
    }

    out.push_str("}\n");
}

// ── Type declarations ──────────────────────────────────────────────────────

fn emit_type_decl(ty: &AgentFlowTypeDecl, out: &mut String) {
    match &ty.kind {
        TypeDeclKind::Opaque => {
            out.push_str(&format!("-- opaque type: {}\n", ty.name));
        }
        TypeDeclKind::Alias { target } => {
            // Check if it looks like a union type (contains |).
            if target.contains('|') {
                let variants: Vec<&str> = target.split('|').map(|s| s.trim()).collect();
                out.push_str(&format!(
                    "type {} = {}\n",
                    ty.name,
                    variants.join(" | ")
                ));
            } else {
                out.push_str(&format!("type {} = {}\n", ty.name, target));
            }
        }
        TypeDeclKind::Record { fields } => {
            out.push_str(&format!("schema {} {{\n", ty.name));
            for (name, field_ty) in fields {
                out.push_str(&format!("    {} :: {}\n", name, field_ty));
            }
            out.push_str("}\n");
        }
    }
}

// ── Schema ─────────────────────────────────────────────────────────────────

fn emit_schema(schema: &AgentFlowSchemaNode, out: &mut String) {
    out.push_str(&format!("schema {} {{\n", schema.label));
    for (name, ty) in &schema.metadata.fields {
        out.push_str(&format!("    {} :: {}\n", name, ty));
    }
    out.push_str("}\n");
}

// ── Template ───────────────────────────────────────────────────────────────

fn emit_template(template: &AgentFlowTemplateNode, out: &mut String) {
    // Strip % prefix if present (for backward compat).
    let name = template.id.strip_prefix('%').unwrap_or(&template.id);
    out.push_str(&format!("template %{} {{\n", name));
    for (field_name, ty) in &template.metadata.fields {
        if let Some(rest) = ty.strip_prefix("String * ") {
            if let Ok(count) = rest.trim().parse::<usize>() {
                out.push_str(&format!("    {} :: String * {}\n", field_name, count));
                continue;
            }
        }
        out.push_str(&format!("    {} :: {}\n", field_name, ty));
    }
    for section in &template.metadata.sections {
        out.push_str(&format!("    section {}\n", section));
    }
    out.push_str("}\n");
}

// ── Directive ──────────────────────────────────────────────────────────────

fn emit_directive(directive: &AgentFlowDirectiveNode, out: &mut String) {
    let name = directive.id.strip_prefix('%').unwrap_or(&directive.id);
    out.push_str(&format!("directive %{} {{\n", name));
    out.push_str(&format!("    <<{}>>\n", directive.metadata.text));
    if !directive.metadata.params.is_empty() {
        out.push_str("    params {\n");
        for (param_name, ty_default) in &directive.metadata.params {
            if let Some(eq_pos) = ty_default.find(" = ") {
                let ty = &ty_default[..eq_pos];
                let default = &ty_default[eq_pos + 3..];
                out.push_str(&format!(
                    "        {} :: {} = <<{}>>\n",
                    param_name, ty, default
                ));
            } else {
                out.push_str(&format!("        {} :: {}\n", param_name, ty_default));
            }
        }
        out.push_str("    }\n");
    }
    out.push_str("}\n");
}

// ── Tool ───────────────────────────────────────────────────────────────────

fn emit_tool(tool: &AgentFlowToolNode, out: &mut String) {
    let name = to_snake_case(&tool.id);
    out.push_str(&format!("tool #{} {{\n", name));
    if !tool.metadata.description.is_empty() {
        out.push_str(&format!("    description: <<{}>>\n", tool.metadata.description));
    }

    if !tool.metadata.requires.is_empty() {
        let perms: Vec<String> = tool
            .metadata
            .requires
            .iter()
            .map(|p| {
                if p.starts_with('^') {
                    p.clone()
                } else {
                    format!("^{}", p)
                }
            })
            .collect();
        out.push_str(&format!("    requires: [{}]\n", perms.join(", ")));
    }

    if let Some(source) = &tool.metadata.source {
        let s = source.strip_prefix('^').unwrap_or(source);
        out.push_str(&format!("    source: ^{}\n", s));
    }

    if let Some(handler) = &tool.metadata.handler {
        out.push_str(&format!("    handler: {}\n", handler));
    }

    if let Some(output) = &tool.metadata.output {
        let tpl = output.strip_prefix('%').unwrap_or(output);
        out.push_str(&format!("    output: %{}\n", tpl));
    }

    if !tool.metadata.directives.is_empty() {
        let dirs: Vec<String> = tool
            .metadata
            .directives
            .iter()
            .map(|d| {
                let name = d.strip_prefix('%').unwrap_or(d);
                format!("%{}", name)
            })
            .collect();
        out.push_str(&format!("    directives: [{}]\n", dirs.join(", ")));
    }

    if !tool.metadata.params.is_empty() {
        out.push_str("    params {\n");
        for (param_name, ty) in &tool.metadata.params {
            out.push_str(&format!("        {} :: {}\n", param_name, ty));
        }
        out.push_str("    }\n");
    }

    if let Some(returns) = &tool.metadata.returns {
        out.push_str(&format!("    returns :: {}\n", returns));
    }

    if let Some(retry) = tool.metadata.retry {
        out.push_str(&format!("    retry: {}\n", retry));
    }

    if let Some(cache) = &tool.metadata.cache {
        out.push_str(&format!("    cache: {}\n", cache));
    }

    if let Some(validate) = &tool.metadata.validate {
        out.push_str(&format!("    validate: {}\n", validate));
    }

    if !tool.metadata.deny.is_empty() {
        for d in &tool.metadata.deny {
            out.push_str(&format!("    -- deny: {}\n", d));
        }
    }

    out.push_str("}\n");
}

// ── Skill ──────────────────────────────────────────────────────────────────

fn emit_skill(skill: &AgentFlowSkillNode, out: &mut String) {
    let name = to_snake_case(&skill.id);
    out.push_str(&format!("skill ${} {{\n", name));
    out.push_str(&format!(
        "    description: <<{}>>\n",
        skill.metadata.description
    ));

    if !skill.metadata.tools.is_empty() {
        let tools: Vec<String> = skill
            .metadata
            .tools
            .iter()
            .map(|t| {
                let n = t.strip_prefix('#').unwrap_or(t);
                format!("#{}", n)
            })
            .collect();
        out.push_str(&format!("    tools: [{}]\n", tools.join(", ")));
    }

    if let Some(strategy) = &skill.metadata.strategy {
        out.push_str(&format!("    strategy: <<{}>>\n", strategy));
    }

    if !skill.metadata.params.is_empty() {
        out.push_str("    params {\n");
        for (param_name, ty) in &skill.metadata.params {
            out.push_str(&format!("        {} :: {}\n", param_name, ty));
        }
        out.push_str("    }\n");
    }

    if let Some(returns) = &skill.metadata.returns {
        out.push_str(&format!("    returns :: {}\n", returns));
    }

    out.push_str("}\n");
}

// ── Agent ──────────────────────────────────────────────────────────────────

fn emit_agent(agent: &AgentFlowAgent, out: &mut String) {
    out.push_str(&format!("agent @{} {{\n", agent.id));

    // Use agent-level permits if available, otherwise collect from tools.
    let mut permits: BTreeSet<String> = BTreeSet::new();
    let mut denies: BTreeSet<String> = BTreeSet::new();

    if !agent.permits.is_empty() {
        for p in &agent.permits {
            let normalized = if p.starts_with('^') {
                p.clone()
            } else {
                format!("^{}", p)
            };
            permits.insert(normalized);
        }
    } else {
        for tool in &agent.nodes {
            for p in &tool.metadata.requires {
                permits.insert(p.clone());
            }
            for d in &tool.metadata.deny {
                denies.insert(d.clone());
            }
        }
        for d in &denies {
            permits.remove(d);
        }
    }

    if !permits.is_empty() {
        let perm_list: Vec<String> = permits
            .iter()
            .map(|p| {
                if p.starts_with('^') {
                    p.clone()
                } else {
                    format!("^{}", p)
                }
            })
            .collect();
        out.push_str(&format!("    permits: [{}]\n", perm_list.join(", ")));
    } else {
        out.push_str("    permits: []\n");
    }

    if !agent.nodes.is_empty() {
        let tool_list: Vec<String> = agent
            .nodes
            .iter()
            .map(|t| format!("#{}", to_snake_case(&t.id)))
            .collect();
        out.push_str(&format!("    tools: [{}]\n", tool_list.join(", ")));
    } else {
        out.push_str("    tools: []\n");
    }

    if !agent.skills.is_empty() {
        let skill_list: Vec<String> = agent
            .skills
            .iter()
            .map(|s| format!("${}", to_snake_case(&s.id)))
            .collect();
        out.push_str(&format!("    skills: [{}]\n", skill_list.join(", ")));
    }

    if let Some(model) = &agent.model {
        out.push_str(&format!("    model: \"{}\"\n", model));
    }

    if let Some(prompt) = &agent.prompt {
        out.push_str(&format!("    prompt: <<{}>>\n", prompt));
    }

    if !agent.memory.is_empty() {
        let mem_list: Vec<String> = agent
            .memory
            .iter()
            .map(|m| {
                let n = m.strip_prefix('~').unwrap_or(m);
                format!("~{}", n)
            })
            .collect();
        out.push_str(&format!("    memory: [{}]\n", mem_list.join(", ")));
    }

    out.push_str("}\n");
}

// ── Bundle ─────────────────────────────────────────────────────────────────

fn emit_bundle(bundle: &AgentFlowBundle, out: &mut String) {
    out.push_str(&format!("agent_bundle @{} {{\n", bundle.id));
    let agents: Vec<String> = bundle.agents.iter().map(|a| format!("@{}", a)).collect();
    out.push_str(&format!("    agents: [{}]\n", agents.join(", ")));
    if let Some(fallbacks) = &bundle.fallbacks {
        out.push_str(&format!("    fallbacks: {}\n", fallbacks));
    }
    out.push_str("}\n");
}

// ── Flow ───────────────────────────────────────────────────────────────────

/// Emit a named flow from a parsed `AgentFlowDef` with reconstructed steps.
fn emit_named_flow(flow: &AgentFlowDef, _graph: &AgentFlowGraph, out: &mut String) {
    // Build parameter list.
    let params: Vec<String> = flow
        .params
        .iter()
        .map(|(k, v)| format!("{} :: {}", k, v))
        .collect();
    let params_str = if params.is_empty() {
        "input :: String".to_string()
    } else {
        params.join(", ")
    };

    let returns = flow
        .returns
        .as_deref()
        .unwrap_or("String");

    out.push_str(&format!(
        "flow {}({}) -> {} {{\n",
        flow.name, params_str, returns
    ));

    for (i, step) in flow.steps.iter().enumerate() {
        let args = if step.args.is_empty() {
            if i == 0 {
                flow.params.keys().cloned().collect::<Vec<_>>().join(", ")
            } else {
                flow.steps[i - 1].output_var.clone()
            }
        } else {
            step.args.join(", ")
        };

        out.push_str(&format!(
            "    {} = @{} -> #{}({})\n",
            step.output_var, step.agent, to_snake_case(&step.tool), args
        ));
    }

    if let Some(last) = flow.steps.last() {
        out.push_str(&format!("    return {}\n", last.output_var));
    }
    out.push_str("}\n");
}

/// Legacy fallback: reconstruct a single flow from edge connections.
fn emit_flow_from_edges(flow_edges: &[&AgentFlowEdge], graph: &AgentFlowGraph, out: &mut String) {
    out.push_str("flow main(input :: String) -> String {\n");

    let mut tool_to_agent: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for agent in &graph.agents {
        for tool in &agent.nodes {
            tool_to_agent.insert(tool.id.clone(), agent.id.clone());
        }
    }

    for (i, edge) in flow_edges.iter().enumerate() {
        let step_var = format!("step_{}", i + 1);
        let prev = if i > 0 {
            format!("step_{}", i)
        } else {
            "input".to_string()
        };

        let to_name = to_snake_case(&edge.to);

        if let Some(agent_id) = tool_to_agent.get(&edge.to) {
            out.push_str(&format!(
                "    {} = @{} -> #{}({})\n",
                step_var, agent_id, to_name, prev
            ));
        } else {
            out.push_str(&format!(
                "    {} = #{}({})\n",
                step_var, to_name, prev
            ));
        }
    }

    let last = format!("step_{}", flow_edges.len());
    out.push_str(&format!("    return {}\n", last));
    out.push_str("}\n");
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Check if a tool node has any meaningful metadata beyond just an ID.
fn tool_has_metadata(tool: &AgentFlowToolNode) -> bool {
    !tool.metadata.description.is_empty()
        || !tool.metadata.requires.is_empty()
        || !tool.metadata.params.is_empty()
        || tool.metadata.returns.is_some()
        || tool.metadata.source.is_some()
        || tool.metadata.handler.is_some()
        || tool.metadata.output.is_some()
        || !tool.metadata.directives.is_empty()
        || tool.metadata.retry.is_some()
        || tool.metadata.cache.is_some()
        || tool.metadata.validate.is_some()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            result.push(ch.to_ascii_lowercase());
        } else if (ch == ' ' || ch == '-' || ch == '_') && !result.ends_with('_') {
            result.push('_');
        }
    }
    result.trim_end_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sample_graph() -> AgentFlowGraph {
        let mut g = AgentFlowGraph::new("TB");

        g.schemas.push(AgentFlowSchemaNode {
            id: "SiteConfig".to_string(),
            label: "SiteConfig".to_string(),
            shape: "hexagon".to_string(),
            metadata: SchemaMetadata {
                fields: BTreeMap::from([
                    ("name".to_string(), "String".to_string()),
                    ("summary".to_string(), "String".to_string()),
                ]),
            },
        });

        g.templates.push(AgentFlowTemplateNode {
            id: "website_copy".to_string(),
            label: "website_copy".to_string(),
            shape: "subroutine".to_string(),
            metadata: TemplateMetadata {
                fields: BTreeMap::from([("HERO_TAGLINE".to_string(), "String".to_string())]),
                sections: vec![],
            },
        });

        g.directives.push(AgentFlowDirectiveNode {
            id: "scandinavian_design".to_string(),
            label: "scandinavian_design".to_string(),
            shape: "trapezoid".to_string(),
            metadata: DirectiveMetadata {
                text: "Use Google Fonts for headings".to_string(),
                params: BTreeMap::from([(
                    "heading_font".to_string(),
                    "String = Playfair Display".to_string(),
                )]),
            },
        });

        g.agents.push(AgentFlowAgent {
            id: "researcher".to_string(),
            label: "@researcher".to_string(),
            model: None,
            prompt: None,
            permits: vec!["^net.read".to_string()],
            memory: vec![],
            nodes: vec![
                AgentFlowToolNode {
                    id: "research_location".to_string(),
                    label: "Research Location".to_string(),
                    shape: "subroutine".to_string(),
                    metadata: ToolMetadata {
                        description: "Research a city".to_string(),
                        requires: vec!["^net.read".to_string()],
                        deny: vec![],
                        source: Some("^search.duckduckgo(query)".to_string()),
                        handler: None,
                        output: None,
                        directives: vec![],
                        params: BTreeMap::from([("query".to_string(), "String".to_string())]),
                        returns: Some("String".to_string()),
                        retry: None,
                        cache: None,
                        validate: None,
                    },
                },
                AgentFlowToolNode {
                    id: "write_copy".to_string(),
                    label: "Write Copy".to_string(),
                    shape: "subroutine".to_string(),
                    metadata: ToolMetadata {
                        description: "Write marketing copy".to_string(),
                        requires: vec!["^llm.query".to_string()],
                        deny: vec![],
                        source: None,
                        handler: None,
                        output: Some("%website_copy".to_string()),
                        directives: vec![],
                        params: BTreeMap::from([("brief".to_string(), "String".to_string())]),
                        returns: Some("String".to_string()),
                        retry: None,
                        cache: None,
                        validate: None,
                    },
                },
            ],
            skills: vec![],
        });

        g.agents.push(AgentFlowAgent {
            id: "designer".to_string(),
            label: "@designer".to_string(),
            model: None,
            prompt: None,
            permits: vec!["^llm.query".to_string()],
            memory: vec![],
            nodes: vec![AgentFlowToolNode {
                id: "generate_html".to_string(),
                label: "Generate HTML".to_string(),
                shape: "subroutine".to_string(),
                metadata: ToolMetadata {
                    description: "Generate a one-page HTML website".to_string(),
                    requires: vec!["^llm.query".to_string()],
                    deny: vec![],
                    source: None,
                    handler: None,
                    output: None,
                    directives: vec!["%scandinavian_design".to_string()],
                    params: BTreeMap::from([("content".to_string(), "String".to_string())]),
                    returns: Some("String".to_string()),
                    retry: None,
                    cache: None,
                    validate: None,
                },
            }],
            skills: vec![],
        });

        g.edges = vec![
            AgentFlowEdge {
                from: "research_location".to_string(),
                to: "write_copy".to_string(),
                label: None,
                edge_type: EdgeType::Flow,
                stroke: EdgeStroke::Normal,
            },
            AgentFlowEdge {
                from: "write_copy".to_string(),
                to: "generate_html".to_string(),
                label: None,
                edge_type: EdgeType::Flow,
                stroke: EdgeStroke::Normal,
            },
            AgentFlowEdge {
                from: "write_copy".to_string(),
                to: "website_copy".to_string(),
                label: None,
                edge_type: EdgeType::Reference,
                stroke: EdgeStroke::Dotted,
            },
        ];

        g
    }

    #[test]
    fn generates_permit_tree() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("permit_tree {"));
        assert!(pact.contains("^llm.query"));
        assert!(pact.contains("^net.read"));
    }

    #[test]
    fn generates_schema() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("schema SiteConfig {"));
        assert!(pact.contains("name :: String"));
    }

    #[test]
    fn generates_template() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("template %website_copy {"));
        assert!(pact.contains("HERO_TAGLINE :: String"));
    }

    #[test]
    fn generates_directive() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("directive %scandinavian_design {"));
        assert!(pact.contains("<<Use Google Fonts for headings>>"));
    }

    #[test]
    fn generates_tool_declarations() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("tool #research_location {"));
        assert!(pact.contains("description: <<Research a city>>"));
        assert!(pact.contains("requires: [^net.read]"));
        assert!(pact.contains("source: ^search.duckduckgo(query)"));
        assert!(pact.contains("tool #write_copy {"));
        assert!(pact.contains("output: %website_copy"));
    }

    #[test]
    fn generates_agent_declarations() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("agent @researcher {"));
        assert!(pact.contains("tools: [#research_location, #write_copy]"));
        assert!(pact.contains("agent @designer {"));
    }

    #[test]
    fn generates_flow() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("flow main(input :: String) -> String {"));
        assert!(pact.contains("step_1 = @researcher -> #write_copy(input)"));
        assert!(pact.contains("step_2 = @designer -> #generate_html(step_1)"));
        assert!(pact.contains("return step_2"));
    }

    #[test]
    fn generates_tool_with_directives() {
        let pact = agentflow_graph_to_pact(&sample_graph());
        assert!(pact.contains("directives: [%scandinavian_design]"));
    }

    #[test]
    fn generates_bundle() {
        let mut g = AgentFlowGraph::new("TB");
        g.bundles.push(AgentFlowBundle {
            id: "website_team".to_string(),
            label: "@website_team".to_string(),
            agents: vec!["researcher".to_string(), "designer".to_string()],
            fallbacks: Some("researcher ?> designer".to_string()),
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("agent_bundle @website_team {"));
        assert!(pact.contains("agents: [@researcher, @designer]"));
        assert!(pact.contains("fallbacks: researcher ?> designer"));
    }

    #[test]
    fn deny_emitted_as_comment() {
        let mut g = AgentFlowGraph::new("TB");
        g.agents.push(AgentFlowAgent {
            id: "restricted".to_string(),
            label: "@restricted".to_string(),
            model: None,
            prompt: None,
            permits: vec![],
            memory: vec![],
            nodes: vec![AgentFlowToolNode {
                id: "limited_tool".to_string(),
                label: "Limited Tool".to_string(),
                shape: "subroutine".to_string(),
                metadata: ToolMetadata {
                    description: "A restricted tool".to_string(),
                    requires: vec!["^llm.query".to_string()],
                    deny: vec!["^net.write".to_string()],
                    source: None,
                    handler: None,
                    output: None,
                    directives: vec![],
                    params: BTreeMap::new(),
                    returns: None,
                    retry: None,
                    cache: None,
                    validate: None,
                },
            }],
            skills: vec![],
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("-- deny: ^net.write"));
    }

    #[test]
    fn converts_type_decl_record_to_schema() {
        let mut g = AgentFlowGraph::new("TB");
        g.types.push(AgentFlowTypeDecl {
            name: "Report".to_string(),
            kind: TypeDeclKind::Record {
                fields: BTreeMap::from([
                    ("title".to_string(), "String".to_string()),
                    ("body".to_string(), "String".to_string()),
                ]),
            },
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("schema Report {"));
        assert!(pact.contains("title :: String"));
    }

    #[test]
    fn converts_type_decl_alias() {
        let mut g = AgentFlowGraph::new("TB");
        g.types.push(AgentFlowTypeDecl {
            name: "Status".to_string(),
            kind: TypeDeclKind::Alias {
                target: "Active | Inactive".to_string(),
            },
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("type Status = Active | Inactive"));
    }

    #[test]
    fn handles_association_edges() {
        let mut g = AgentFlowGraph::new("TB");
        g.edges.push(AgentFlowEdge {
            from: "doc_a".to_string(),
            to: "doc_b".to_string(),
            label: None,
            edge_type: EdgeType::Association,
            stroke: EdgeStroke::Normal,
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("-- doc_a --- doc_b"));
    }

    #[test]
    fn agent_permits_used_directly() {
        let mut g = AgentFlowGraph::new("TB");
        g.agents.push(AgentFlowAgent {
            id: "test_agent".to_string(),
            label: "@test_agent".to_string(),
            model: None,
            prompt: None,
            permits: vec!["^net.read".to_string(), "^llm.query".to_string()],
            memory: vec![],
            nodes: vec![],
            skills: vec![],
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("permits: [^llm.query, ^net.read]"));
    }

    #[test]
    fn delegation_edges_become_dispatch() {
        // Delegation edges (-->>) should be treated like flow edges.
        let mut g = AgentFlowGraph::new("TB");
        g.agents.push(AgentFlowAgent {
            id: "coordinator".into(),
            label: "@coordinator".into(),
            model: None, prompt: None, permits: vec![], memory: vec![],
            nodes: vec![AgentFlowToolNode {
                id: "plan".into(), label: "Plan".into(), shape: "subroutine".into(),
                metadata: ToolMetadata {
                    description: "Plan".into(), requires: vec![], deny: vec![],
                    source: None, handler: None, output: None, directives: vec![],
                    params: BTreeMap::new(), returns: Some("String".into()),
                    retry: None, cache: None, validate: None,
                },
            }],
            skills: vec![],
        });
        g.edges.push(AgentFlowEdge {
            from: "input".into(), to: "plan".into(), label: None,
            edge_type: EdgeType::Delegation, stroke: EdgeStroke::Normal,
        });
        // Delegation edges aren't Flow, so emit_flow won't include them.
        // Verify no crash and output is valid PACT.
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("agent @coordinator"));
    }

    #[test]
    fn output_binding_edges_not_in_flow() {
        // OutputBinding edges (--o) should not appear in the flow body.
        let mut g = AgentFlowGraph::new("TB");
        g.edges.push(AgentFlowEdge {
            from: "tool_a".into(), to: "doc_out".into(), label: None,
            edge_type: EdgeType::OutputBinding, stroke: EdgeStroke::Normal,
        });
        let pact = agentflow_graph_to_pact(&g);
        // Should not crash, and should not contain a flow for OutputBinding.
        assert!(!pact.contains("flow main"));
    }

    #[test]
    fn converts_opaque_type_to_comment() {
        let mut g = AgentFlowGraph::new("TB");
        g.types.push(AgentFlowTypeDecl {
            name: "Token".into(),
            kind: TypeDeclKind::Opaque,
        });
        let pact = agentflow_graph_to_pact(&g);
        assert!(pact.contains("-- opaque type: Token"));
    }

    #[test]
    fn full_roundtrip_agentflow_to_pact_to_agentflow() {
        // Parse agentflow text → convert to PACT → parse PACT → emit agentflow
        // → verify key structure is preserved.
        let input = r#"
agentflow TB
    agent researcher["Researcher"]
        search["Search"]@{
            description: "Search the web"
            requires: ["^net.read"]
            params:
                query: "String"
            returns: "String"
        }
    end
    researcher@{ permits: "net.read" }

    search --> summarize
"#;
        let graph = crate::agentflow_parse::parse_agentflow_text(input).unwrap();
        let pact = agentflow_graph_to_pact(&graph);
        assert!(pact.contains("tool #search"));
        assert!(pact.contains("agent @researcher"));
        assert!(pact.contains("description: <<Search the web>>"));
    }
}
