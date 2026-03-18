// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Parser for agentflow text syntax → `AgentFlowGraph`.
//!
//! State machine approach: tracks whether we are inside a subgraph (agent),
//! inside an `@{...}` metadata block, or at the top level.

use crate::agentflow::*;
use crate::parser::MermaidError;
use std::collections::BTreeMap;

/// Parse agentflow text into an `AgentFlowGraph`.
pub fn parse_agentflow_text(input: &str) -> Result<AgentFlowGraph, MermaidError> {
    let mut parser = AgentFlowParser::new(input);
    parser.parse()
}

// ── Parser internals ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum ParserState {
    TopLevel,
    InAgent(String),           // agent id
    InMetadata(MetadataTarget), // collecting @{...} block lines
}

#[derive(Debug, Clone, PartialEq)]
enum MetadataTarget {
    /// Tool node inside an agent. (agent_id, node_id, label)
    Tool(String, String, String),
    /// Skill node inside an agent. (agent_id, node_id, label)
    Skill(String, String, String),
    /// Standalone schema. (node_id, label)
    Schema(String, String),
    /// Standalone template. (node_id, label)
    Template(String, String),
    /// Standalone directive. (node_id, label)
    Directive(String, String),
    /// Top-level tool (not inside an agent).
    TopTool(String, String),
}

struct AgentFlowParser<'a> {
    input: &'a str,
    graph: AgentFlowGraph,
    state: ParserState,
    meta_lines: Vec<String>,
    brace_depth: usize,
}

impl<'a> AgentFlowParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            graph: AgentFlowGraph::new("LR"),
            state: ParserState::TopLevel,
            meta_lines: Vec::new(),
            brace_depth: 0,
        }
    }

    fn parse(&mut self) -> Result<AgentFlowGraph, MermaidError> {
        // First line must be `agentflow <DIR>`
        let mut found_header = false;

        for line in self.input.lines() {
            let trimmed = line.trim();

            // Skip empty and comments.
            if trimmed.is_empty() || trimmed.starts_with("%%") {
                continue;
            }

            // Skip `direction LR` inside subgraphs.
            if trimmed.starts_with("direction ") {
                continue;
            }

            // Header line.
            if trimmed.starts_with("agentflow") {
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 2 {
                    self.graph.direction = parts[1].to_string();
                }
                found_header = true;
                continue;
            }

            if !found_header {
                return Err(MermaidError::MissingDiagramType);
            }

            self.process_line(trimmed)?;
        }

        // Check for unclosed states.
        if let ParserState::InAgent(name) = &self.state {
            return Err(MermaidError::UnclosedSubgraph(name.clone()));
        }

        Ok(self.graph.clone())
    }

    fn process_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // If we're collecting metadata lines inside @{...}
        if let ParserState::InMetadata(_) = &self.state {
            return self.collect_metadata_line(trimmed);
        }

        // Subgraph start: `subgraph id["@name"]`
        if trimmed.starts_with("subgraph ") {
            let rest = trimmed.strip_prefix("subgraph ").unwrap().trim();
            let (id, label) = parse_subgraph_header(rest);
            self.state = ParserState::InAgent(id.clone());
            // Pre-create the agent entry.
            self.graph.agents.push(AgentFlowAgent {
                id,
                label,
                model: None,
                prompt: None,
                memory: vec![],
                nodes: vec![],
                skills: vec![],
            });
            return Ok(());
        }

        // Subgraph end.
        if trimmed == "end" {
            self.state = ParserState::TopLevel;
            return Ok(());
        }

        // Edge lines.
        if trimmed.contains("-->") || trimmed.contains("-.->") {
            return self.parse_edge_line(trimmed);
        }

        // Node definitions (with or without @{...}).
        self.parse_node_line(trimmed)
    }

    fn parse_node_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // Check if this line contains `@{` — start of metadata block.
        if let Some(at_pos) = trimmed.find("@{") {
            let node_part = trimmed[..at_pos].trim();
            let meta_start = &trimmed[at_pos + 2..]; // after `@{`

            let (id, label, node_type) = classify_node(node_part)?;
            let target = self.make_metadata_target(&id, &label, node_type)?;

            self.brace_depth = 1;
            self.meta_lines.clear();

            // Check if metadata closes on the same line.
            let remaining = meta_start.trim();
            if remaining.ends_with('}') && !remaining.contains('{') {
                // Single-line metadata: @{ key: value }
                let content = remaining.trim_end_matches('}').trim();
                if !content.is_empty() {
                    self.meta_lines.push(content.to_string());
                }
                self.finalize_metadata(target)?;
            } else {
                // Multi-line metadata block.
                if !remaining.is_empty() {
                    self.meta_lines.push(remaining.to_string());
                }
                self.state = ParserState::InMetadata(target);
            }

            return Ok(());
        }

        // Standalone node definition without metadata (bare node).
        // Could be a node reference inside a subgraph — skip for now.
        Ok(())
    }

    fn collect_metadata_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // Track brace depth for nested structures.
        for ch in trimmed.chars() {
            if ch == '{' {
                self.brace_depth += 1;
            } else if ch == '}' {
                self.brace_depth -= 1;
            }
        }

        if self.brace_depth == 0 {
            // Closing brace — don't include this line, finalize.
            // But include content before the final `}`.
            let content = trimmed.trim_end_matches('}').trim();
            if !content.is_empty() {
                self.meta_lines.push(content.to_string());
            }
            let target = if let ParserState::InMetadata(t) = &self.state {
                t.clone()
            } else {
                unreachable!()
            };
            // Restore state to what it was before metadata.
            self.finalize_metadata(target)?;
        } else {
            self.meta_lines.push(trimmed.to_string());
        }

        Ok(())
    }

    fn make_metadata_target(
        &self,
        id: &str,
        label: &str,
        node_type: NodeType,
    ) -> Result<MetadataTarget, MermaidError> {
        match node_type {
            NodeType::Tool => {
                if let ParserState::InAgent(agent_id) = &self.state {
                    Ok(MetadataTarget::Tool(
                        agent_id.clone(),
                        id.to_string(),
                        label.to_string(),
                    ))
                } else {
                    Ok(MetadataTarget::TopTool(id.to_string(), label.to_string()))
                }
            }
            NodeType::Skill => {
                if let ParserState::InAgent(agent_id) = &self.state {
                    Ok(MetadataTarget::Skill(
                        agent_id.clone(),
                        id.to_string(),
                        label.to_string(),
                    ))
                } else {
                    Err(MermaidError::MalformedNode(format!(
                        "skill node '{}' must be inside an agent subgraph",
                        id
                    )))
                }
            }
            NodeType::Schema => Ok(MetadataTarget::Schema(id.to_string(), label.to_string())),
            NodeType::Template => {
                Ok(MetadataTarget::Template(id.to_string(), label.to_string()))
            }
            NodeType::Directive => {
                Ok(MetadataTarget::Directive(id.to_string(), label.to_string()))
            }
        }
    }

    fn finalize_metadata(&mut self, target: MetadataTarget) -> Result<(), MermaidError> {
        let raw = self.meta_lines.join("\n");
        self.meta_lines.clear();

        match target {
            MetadataTarget::Tool(agent_id, node_id, label) => {
                let meta = parse_tool_metadata(&raw)?;
                if let Some(agent) = self.graph.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.nodes.push(AgentFlowToolNode {
                        id: node_id,
                        label,
                        shape: "roundedRect".to_string(),
                        metadata: meta,
                    });
                }
                self.state = ParserState::InAgent(agent_id);
            }
            MetadataTarget::TopTool(node_id, label) => {
                // Top-level tool without an agent — create a synthetic agent.
                let meta = parse_tool_metadata(&raw)?;
                let agent_id = format!("{}_agent", node_id);
                self.graph.agents.push(AgentFlowAgent {
                    id: agent_id,
                    label: format!("@{}_agent", node_id),
                    model: None,
                    prompt: None,
                    memory: vec![],
                    nodes: vec![AgentFlowToolNode {
                        id: node_id,
                        label,
                        shape: "roundedRect".to_string(),
                        metadata: meta,
                    }],
                    skills: vec![],
                });
                self.state = ParserState::TopLevel;
            }
            MetadataTarget::Skill(agent_id, node_id, label) => {
                let meta = parse_skill_metadata(&raw)?;
                if let Some(agent) = self.graph.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.skills.push(AgentFlowSkillNode {
                        id: node_id,
                        label,
                        shape: "stadium".to_string(),
                        metadata: meta,
                    });
                }
                self.state = ParserState::InAgent(agent_id);
            }
            MetadataTarget::Schema(node_id, label) => {
                let meta = parse_schema_metadata(&raw)?;
                self.graph.schemas.push(AgentFlowSchemaNode {
                    id: node_id,
                    label,
                    shape: "hexagon".to_string(),
                    metadata: meta,
                });
                self.state = ParserState::TopLevel;
            }
            MetadataTarget::Template(node_id, label) => {
                let meta = parse_template_metadata(&raw)?;
                self.graph.templates.push(AgentFlowTemplateNode {
                    id: node_id,
                    label,
                    shape: "subroutine".to_string(),
                    metadata: meta,
                });
                self.state = ParserState::TopLevel;
            }
            MetadataTarget::Directive(node_id, label) => {
                let meta = parse_directive_metadata(&raw)?;
                self.graph.directives.push(AgentFlowDirectiveNode {
                    id: node_id,
                    label,
                    shape: "trapezoid".to_string(),
                    metadata: meta,
                });
                self.state = ParserState::TopLevel;
            }
        }

        Ok(())
    }

    fn parse_edge_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // Dashed edge: `A -.-> B` (reference)
        if trimmed.contains("-.->") {
            let parts: Vec<&str> = trimmed.split("-.->").collect();
            if parts.len() == 2 {
                let from = parts[0].trim().to_string();
                let to = parts[1].trim().to_string();
                self.graph.edges.push(AgentFlowEdge {
                    from,
                    to,
                    label: None,
                    edge_type: EdgeType::Reference,
                });
            }
            return Ok(());
        }

        // Solid edge: `A --> B` or `A -->|"label"| B`
        let parts: Vec<&str> = trimmed.split("-->").collect();
        for i in 0..parts.len() - 1 {
            let left = parts[i].trim();
            let right = parts[i + 1].trim();

            let from = left.to_string();

            // Check for label: `|"label"| B` or `|label| B`
            let (to, label) = if let Some(rest) = right.strip_prefix('|') {
                if let Some(pipe_end) = rest.find('|') {
                    let lbl = rest[..pipe_end].trim().trim_matches('"').to_string();
                    let node = rest[pipe_end + 1..].trim().to_string();
                    (node, Some(lbl))
                } else {
                    (right.to_string(), None)
                }
            } else {
                (right.to_string(), None)
            };

            self.graph.edges.push(AgentFlowEdge {
                from,
                to,
                label,
                edge_type: EdgeType::Flow,
            });
        }

        Ok(())
    }
}

// ── Node classification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum NodeType {
    Tool,      // id["Label"]
    Schema,    // id{{"Label"}}
    Skill,     // id(["Label"])
    Template,  // id[["Label"]]
    Directive, // id[/"Label"/]
}

/// Parse a node definition string and return (id, label, type).
///
/// Supported shapes:
/// - `id["Label"]` → Tool (rounded rect)
/// - `id{{"Label"}}` → Schema (hexagon)
/// - `id(["Label"])` → Skill (stadium)
/// - `id[["Label"]]` → Template (subroutine)
/// - `id[/"Label"/]` → Directive (trapezoid)
fn classify_node(s: &str) -> Result<(String, String, NodeType), MermaidError> {
    let s = s.trim();

    // Find the first bracket-like char.
    let delim_pos = s
        .find(['[', '(', '{'])
        .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;

    let id = s[..delim_pos].trim().to_string();
    let rest = &s[delim_pos..];

    // Hexagon: {{"Label"}}
    if let Some(inner) = rest.strip_prefix("{{") {
        let inner = inner
            .strip_suffix("}}")
            .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;
        let label = unquote(inner.trim());
        return Ok((id, label, NodeType::Schema));
    }

    // Stadium/pill: (["Label"])
    if let Some(inner) = rest.strip_prefix("([") {
        let inner = inner
            .strip_suffix("])")
            .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;
        let label = unquote(inner.trim());
        return Ok((id, label, NodeType::Skill));
    }

    // Subroutine: [["Label"]]
    if let Some(inner) = rest.strip_prefix("[[") {
        let inner = inner
            .strip_suffix("]]")
            .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;
        let label = unquote(inner.trim());
        return Ok((id, label, NodeType::Template));
    }

    // Trapezoid: [/"Label"/]
    if let Some(inner) = rest.strip_prefix("[/") {
        let inner = inner
            .strip_suffix("/]")
            .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;
        let label = unquote(inner.trim());
        return Ok((id, label, NodeType::Directive));
    }

    // Rounded rect (tool): ["Label"]
    if let Some(inner) = rest.strip_prefix('[') {
        let inner = inner
            .strip_suffix(']')
            .ok_or_else(|| MermaidError::MalformedNode(s.to_string()))?;
        let label = unquote(inner.trim());
        return Ok((id, label, NodeType::Tool));
    }

    Err(MermaidError::MalformedNode(s.to_string()))
}

fn unquote(s: &str) -> String {
    s.trim_matches('"').to_string()
}

/// Parse `id["@name"]` from subgraph header like `researcher["@researcher"]`.
fn parse_subgraph_header(rest: &str) -> (String, String) {
    if let Some(bracket_pos) = rest.find('[') {
        let id = rest[..bracket_pos].trim().to_string();
        let label_part = &rest[bracket_pos..];
        let label = label_part
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim_matches('"')
            .to_string();
        (id, label)
    } else {
        // No brackets — use the rest as both id and label.
        let id = rest.to_string();
        let label = format!("@{}", rest);
        (id, label)
    }
}

// ── Metadata parsers ───────────────────────────────────────────────────────

/// Parse the YAML-like content of a tool `@{...}` block.
fn parse_tool_metadata(raw: &str) -> Result<ToolMetadata, MermaidError> {
    let mut meta = ToolMetadata {
        description: String::new(),
        requires: vec![],
        deny: vec![],
        source: None,
        handler: None,
        output: None,
        directives: vec![],
        params: BTreeMap::new(),
        returns: None,
        retry: None,
        cache: None,
        validate: None,
    };

    let mut in_params = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Detect start of params block.
        if trimmed == "params:" {
            in_params = true;
            continue;
        }

        // If we're in the params sub-block, lines are indented key: "Type" pairs.
        if in_params {
            if let Some((key, val)) = parse_kv_line(trimmed) {
                meta.params.insert(key, unquote(&val));
                continue;
            } else {
                // No longer in params — fall through to normal parsing.
                in_params = false;
            }
        }

        if let Some((key, val)) = parse_kv_line(trimmed) {
            match key.as_str() {
                "description" => meta.description = unquote(&val),
                "requires" => meta.requires = parse_string_array(&val),
                "deny" => meta.deny = parse_string_array(&val),
                "source" => meta.source = Some(unquote(&val)),
                "handler" => meta.handler = Some(unquote(&val)),
                "output" => meta.output = Some(unquote(&val)),
                "directives" => meta.directives = parse_string_array(&val),
                "returns" => meta.returns = Some(unquote(&val)),
                "retry" => meta.retry = val.trim().trim_matches('"').parse().ok(),
                "cache" => meta.cache = Some(unquote(&val)),
                "validate" => meta.validate = Some(unquote(&val)),
                _ => {} // ignore unknown keys
            }
        }
    }

    if meta.description.is_empty() {
        return Err(MermaidError::MalformedMetadata(
            "tool metadata requires a 'description' field".to_string(),
        ));
    }

    Ok(meta)
}

fn parse_skill_metadata(raw: &str) -> Result<SkillMetadata, MermaidError> {
    let mut meta = SkillMetadata {
        description: String::new(),
        tools: vec![],
        strategy: None,
        params: BTreeMap::new(),
        returns: None,
    };

    let mut in_params = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "params:" {
            in_params = true;
            continue;
        }

        if in_params {
            if let Some((key, val)) = parse_kv_line(trimmed) {
                meta.params.insert(key, unquote(&val));
                continue;
            } else {
                in_params = false;
            }
        }

        if let Some((key, val)) = parse_kv_line(trimmed) {
            match key.as_str() {
                "description" => meta.description = unquote(&val),
                "tools" => meta.tools = parse_string_array(&val),
                "strategy" => meta.strategy = Some(unquote(&val)),
                "returns" => meta.returns = Some(unquote(&val)),
                _ => {}
            }
        }
    }

    if meta.description.is_empty() {
        return Err(MermaidError::MalformedMetadata(
            "skill metadata requires a 'description' field".to_string(),
        ));
    }

    Ok(meta)
}

fn parse_schema_metadata(raw: &str) -> Result<SchemaMetadata, MermaidError> {
    let mut fields = BTreeMap::new();
    let mut in_fields = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "fields:" {
            in_fields = true;
            continue;
        }

        if in_fields {
            if let Some((key, val)) = parse_kv_line(trimmed) {
                fields.insert(key, unquote(&val));
            }
        }
    }

    Ok(SchemaMetadata { fields })
}

fn parse_template_metadata(raw: &str) -> Result<TemplateMetadata, MermaidError> {
    let mut fields = BTreeMap::new();
    let mut sections = Vec::new();
    let mut in_fields = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "fields:" {
            in_fields = true;
            continue;
        }

        if let Some((key, val)) = parse_kv_line(trimmed) {
            if key == "sections" {
                sections = parse_string_array(&val);
                in_fields = false;
                continue;
            }
        }

        if in_fields {
            if let Some((key, val)) = parse_kv_line(trimmed) {
                fields.insert(key, unquote(&val));
            }
        }
    }

    Ok(TemplateMetadata { fields, sections })
}

fn parse_directive_metadata(raw: &str) -> Result<DirectiveMetadata, MermaidError> {
    let mut text = String::new();
    let mut params = BTreeMap::new();
    let mut in_params = false;

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed == "params:" {
            in_params = true;
            continue;
        }

        if in_params {
            if let Some((key, val)) = parse_kv_line(trimmed) {
                params.insert(key, unquote(&val));
                continue;
            }
        }

        if let Some((key, val)) = parse_kv_line(trimmed) {
            if key == "text" {
                text = unquote(&val);
            }
        }
    }

    Ok(DirectiveMetadata { text, params })
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Parse a `key: value` line. Handles `key: "value"` and `key: value`.
fn parse_kv_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let key = line[..colon_pos].trim().to_string();
    let val = line[colon_pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

/// Parse a `["item1", "item2"]` string into a vec of strings.
fn parse_string_array(s: &str) -> Vec<String> {
    let s = s.trim();
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return vec![];
    }
    inner
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_agentflow() {
        let input = r#"
agentflow LR
    subgraph researcher["@researcher"]
        direction LR

        research["Research"]@{
            description: "Do research"
            requires: ["^net.read"]
            params:
                query: "String"
            returns: "String"
        }
    end
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.direction, "LR");
        assert_eq!(graph.agents.len(), 1);
        assert_eq!(graph.agents[0].id, "researcher");
        assert_eq!(graph.agents[0].nodes.len(), 1);
        assert_eq!(graph.agents[0].nodes[0].id, "research");
        assert_eq!(
            graph.agents[0].nodes[0].metadata.description,
            "Do research"
        );
        assert_eq!(
            graph.agents[0].nodes[0].metadata.requires,
            vec!["^net.read"]
        );
        assert_eq!(
            graph.agents[0].nodes[0].metadata.params.get("query"),
            Some(&"String".to_string())
        );
    }

    #[test]
    fn parse_schema_node() {
        let input = r#"
agentflow LR
    SiteConfig{{"SiteConfig"}}@{
        fields:
            name: "String"
            summary: "String"
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.schemas.len(), 1);
        assert_eq!(graph.schemas[0].id, "SiteConfig");
        assert_eq!(graph.schemas[0].metadata.fields.len(), 2);
    }

    #[test]
    fn parse_template_node() {
        let input = r#"
agentflow LR
    website_copy[["website_copy"]]@{
        fields:
            HERO_TAGLINE: "String"
            HERO_SUBTITLE: "String"
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.templates.len(), 1);
        assert_eq!(graph.templates[0].id, "website_copy");
        assert_eq!(graph.templates[0].metadata.fields.len(), 2);
    }

    #[test]
    fn parse_template_with_sections() {
        let input = r#"
agentflow LR
    bilingual[["bilingual"]]@{
        sections: ["ENGLISH", "SWEDISH"]
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.templates.len(), 1);
        assert_eq!(
            graph.templates[0].metadata.sections,
            vec!["ENGLISH", "SWEDISH"]
        );
    }

    #[test]
    fn parse_directive_node() {
        let input = r#"
agentflow LR
    scandinavian_design[/"scandinavian_design"/]@{
        text: "Use Google Fonts for headings"
        params:
            heading_font: "String = Playfair Display"
            body_font: "String = Inter"
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.directives.len(), 1);
        assert_eq!(graph.directives[0].id, "scandinavian_design");
        assert_eq!(
            graph.directives[0].metadata.text,
            "Use Google Fonts for headings"
        );
        assert_eq!(graph.directives[0].metadata.params.len(), 2);
    }

    #[test]
    fn parse_flow_and_reference_edges() {
        let input = r#"
agentflow LR
    subgraph researcher["@researcher"]
        research["Research"]@{
            description: "Research"
            returns: "String"
        }
    end

    research --> write_copy
    research -.-> website_copy
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.edges.len(), 2);
        assert_eq!(graph.edges[0].edge_type, EdgeType::Flow);
        assert_eq!(graph.edges[0].from, "research");
        assert_eq!(graph.edges[0].to, "write_copy");
        assert_eq!(graph.edges[1].edge_type, EdgeType::Reference);
    }

    #[test]
    fn parse_edge_with_label() {
        let input = r#"
agentflow LR
    research -->|"step_1"| write_copy
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].label.as_deref(), Some("step_1"));
    }

    #[test]
    fn parse_skill_node() {
        let input = "
agentflow LR
    subgraph researcher[\"@researcher\"]
        deep_research([\"Deep Research\"])@{
            description: \"Thorough research strategy\"
            tools: [\"#web_search\", \"#summarize\"]
            strategy: \"Always cross-reference multiple sources\"
        }
    end
";
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.agents[0].skills.len(), 1);
        assert_eq!(graph.agents[0].skills[0].id, "deep_research");
        assert_eq!(
            graph.agents[0].skills[0].metadata.tools,
            vec!["#web_search", "#summarize"]
        );
    }

    #[test]
    fn missing_header_is_error() {
        let input = "subgraph foo\nend\n";
        assert!(parse_agentflow_text(input).is_err());
    }

    #[test]
    fn unclosed_subgraph_is_error() {
        let input = "agentflow LR\n    subgraph foo\n";
        assert!(parse_agentflow_text(input).is_err());
    }

    #[test]
    fn parse_full_example() {
        let input = r#"
agentflow LR
    %% ── Schemas ──
    SiteConfig{{"SiteConfig"}}@{
        fields:
            name: "String"
            summary: "String"
    }

    %% ── Templates ──
    website_copy[["website_copy"]]@{
        fields:
            HERO_TAGLINE: "String"
    }

    %% ── Directives ──
    scandinavian_design[/"scandinavian_design"/]@{
        text: "Use clean design"
        params:
            heading_font: "String = Playfair Display"
    }

    %% ── Agent: researcher ──
    subgraph researcher["@researcher"]
        direction LR

        research_location["Research Location"]@{
            description: "Research a city"
            requires: ["^net.read"]
            source: "^search.duckduckgo(query)"
            params:
                query: "String"
            returns: "String"
        }

        write_copy["Write Copy"]@{
            description: "Write marketing copy"
            requires: ["^llm.query"]
            output: "%website_copy"
            params:
                brief: "String"
            returns: "String"
        }
    end

    %% ── Agent: designer ──
    subgraph designer["@designer"]
        generate_html["Generate HTML"]@{
            description: "Generate a one-page HTML website"
            requires: ["^llm.query"]
            directives: ["%scandinavian_design"]
            params:
                content: "String"
            returns: "String"
        }
    end

    %% ── Reference edges ──
    write_copy -.-> website_copy
    generate_html -.-> scandinavian_design

    %% ── Flow edges ──
    research_location --> write_copy
    write_copy --> generate_html
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.schemas.len(), 1);
        assert_eq!(graph.templates.len(), 1);
        assert_eq!(graph.directives.len(), 1);
        assert_eq!(graph.agents.len(), 2);
        assert_eq!(graph.agents[0].nodes.len(), 2);
        assert_eq!(graph.agents[1].nodes.len(), 1);
        assert_eq!(graph.edges.len(), 4);

        // Check edge types.
        let flow_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Flow)
            .collect();
        let ref_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Reference)
            .collect();
        assert_eq!(flow_edges.len(), 2);
        assert_eq!(ref_edges.len(), 2);
    }
}
