// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Parser for agentflow text syntax → `AgentFlowGraph`.
//!
//! State machine approach: tracks a container stack for nested `agent`/`flow`/
//! `task`/`subgraph` blocks, metadata blocks, and top-level declarations.

use crate::agentflow::*;
use crate::parser::MermaidError;
use std::collections::BTreeMap;

/// Parse agentflow text into an `AgentFlowGraph`.
pub fn parse_agentflow_text(input: &str) -> Result<AgentFlowGraph, MermaidError> {
    let mut parser = AgentFlowParser::new(input);
    parser.parse()
}

// ── Parser internals ───────────────────────────────────────────────────────

/// A container on the stack (agent, flow, task, or subgraph).
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct ContainerFrame {
    kind: ContainerKind,
    id: String,
    label: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ParserState {
    TopLevel,
    InMetadata(MetadataTarget),
    InTypeDecl(String, Vec<String>), // type name, accumulated lines
    InFrontmatter,
    InTemplateBlock(String, Vec<String>), // template name, accumulated lines
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
    /// Post-end metadata for a container. (container_id)
    PostEnd(String),
    /// Deferred node metadata. (node_id)
    DeferredNode(String),
}

struct AgentFlowParser<'a> {
    input: &'a str,
    graph: AgentFlowGraph,
    state: ParserState,
    meta_lines: Vec<String>,
    brace_depth: usize,
    /// Stack of open containers (for nested agent/flow/task/subgraph).
    container_stack: Vec<ContainerFrame>,
    /// Node labels discovered from edge definitions (id -> label).
    node_labels: std::collections::HashMap<String, String>,
}

impl<'a> AgentFlowParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            graph: AgentFlowGraph::new("TB"),
            state: ParserState::TopLevel,
            meta_lines: Vec::new(),
            brace_depth: 0,
            container_stack: Vec::new(),
            node_labels: std::collections::HashMap::new(),
        }
    }

    fn parse(&mut self) -> Result<AgentFlowGraph, MermaidError> {
        let mut found_header = false;

        for line in self.input.lines() {
            let trimmed = line.trim();

            // Skip empty and comments.
            if trimmed.is_empty() || trimmed.starts_with("%%") {
                continue;
            }

            // Frontmatter handling.
            if trimmed == "---" {
                if let ParserState::InFrontmatter = &self.state {
                    self.state = ParserState::TopLevel;
                    continue;
                } else if !found_header {
                    self.state = ParserState::InFrontmatter;
                    continue;
                }
            }
            if let ParserState::InFrontmatter = &self.state {
                continue;
            }

            // Skip `direction TB/LR` inside containers.
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

        // Check for unclosed containers.
        if let Some(frame) = self.container_stack.last() {
            return Err(MermaidError::UnclosedSubgraph(frame.id.clone()));
        }

        // Post-process: reconstruct flow steps from parsed edges and node labels.
        self.reconstruct_flow_steps();

        Ok(self.graph.clone())
    }

    /// Get the current agent ID from the container stack (innermost agent).
    fn current_agent_id(&self) -> Option<String> {
        for frame in self.container_stack.iter().rev() {
            if frame.kind == ContainerKind::Agent || frame.kind == ContainerKind::Subgraph {
                return Some(frame.id.clone());
            }
        }
        None
    }

    fn process_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // If collecting metadata lines inside @{...}
        if let ParserState::InMetadata(_) = &self.state {
            return self.collect_metadata_line(trimmed);
        }

        // If collecting type declaration lines.
        if let ParserState::InTypeDecl(_, _) = &self.state {
            return self.collect_type_decl_line(trimmed);
        }

        // If collecting template block lines.
        if let ParserState::InTemplateBlock(_, _) = &self.state {
            return self.collect_template_block_line(trimmed);
        }

        // ── Container starts ──────────────────────────────────────────────

        // `agent id["Label"]` or `agent id["Label"]@{...}`
        if trimmed.starts_with("agent ") {
            return self.parse_container_start(trimmed, "agent", ContainerKind::Agent);
        }

        // `flow id["Label"]`
        if trimmed.starts_with("flow ") && !trimmed.contains("-->") {
            return self.parse_container_start(trimmed, "flow", ContainerKind::Flow);
        }

        // `task id` or `task id["Label"]`
        if trimmed.starts_with("task ") {
            return self.parse_container_start(trimmed, "task", ContainerKind::Task);
        }

        // `skill id["Label"]`
        if trimmed.starts_with("skill ") {
            return self.parse_container_start(trimmed, "skill", ContainerKind::Skill);
        }

        // `directive id["Label"]`
        if trimmed.starts_with("directive ") {
            return self.parse_container_start(trimmed, "directive", ContainerKind::Directive);
        }

        // `testCase id["Label"]`
        if trimmed.starts_with("testCase ") {
            return self.parse_container_start(trimmed, "testCase", ContainerKind::TestCase);
        }

        // Legacy `subgraph id["@name"]` or `subgraph id["@name"]@{...}`
        if trimmed.starts_with("subgraph ") {
            return self.parse_container_start(trimmed, "subgraph", ContainerKind::Subgraph);
        }

        // ── Container end ─────────────────────────────────────────────────
        if trimmed == "end" {
            if let Some(frame) = self.container_stack.pop() {
                // If top of stack was a flow container, create a flow def.
                if frame.kind == ContainerKind::Flow {
                    // Flow was already added when container started; nothing extra needed.
                }
                // No state change needed — we're still in whatever the parent container is.
                return Ok(());
            }
            // `end` without open container — ignore silently for compat.
            return Ok(());
        }

        // ── Post-end deferred metadata: `id@{ ... }` ──────────────────────
        if let Some(at_pos) = trimmed.find("@{") {
            let before = trimmed[..at_pos].trim();
            // If `before` is just an identifier (no shape brackets), it's deferred metadata.
            if !before.is_empty() && !before.contains(['[', '(', '{', '>', '"']) {
                let meta_start = &trimmed[at_pos + 2..];
                let id = before.to_string();

                self.brace_depth = 1;
                self.meta_lines.clear();

                let remaining = meta_start.trim();
                if remaining.ends_with('}') && !remaining.contains('{') {
                    let content = remaining.trim_end_matches('}').trim();
                    if !content.is_empty() {
                        self.meta_lines.push(content.to_string());
                    }
                    // Determine if this is for a container or a node.
                    let target = self.classify_deferred_target(&id);
                    self.finalize_metadata(target)?;
                } else {
                    if !remaining.is_empty() {
                        self.meta_lines.push(remaining.to_string());
                    }
                    let target = self.classify_deferred_target(&id);
                    self.state = ParserState::InMetadata(target);
                }
                return Ok(());
            }
        }

        // ── Type declarations: `type Name = Record { ... }` ───────────────
        if trimmed.starts_with("type ") {
            return self.parse_type_decl(trimmed);
        }

        // ── Template declarations: `template name { ... }` ────────────────
        if trimmed.starts_with("template ") {
            return self.parse_template_decl(trimmed);
        }

        // ── Edge lines ────────────────────────────────────────────────────
        if is_edge_line(trimmed) {
            return self.parse_edge_line(trimmed);
        }

        // ── Node definitions with @{...} ──────────────────────────────────
        if trimmed.contains("@{") {
            return self.parse_node_line(trimmed);
        }

        // Bare identifiers inside agent/subgraph containers — create placeholder tool node.
        // Deferred metadata (e.g. `id@{ shape: subroutine }`) will fill in details later.
        if !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            if let Some(agent_id) = self.current_agent_id() {
                if let Some(agent) = self.graph.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.nodes.push(AgentFlowToolNode {
                        id: trimmed.to_string(),
                        label: trimmed.to_string(),
                        shape: "subroutine".to_string(),
                        metadata: ToolMetadata::default(),
                    });
                }
            }
        }
        Ok(())
    }

    // ── Container parsing ──────────────────────────────────────────────────

    fn parse_container_start(
        &mut self,
        trimmed: &str,
        keyword: &str,
        kind: ContainerKind,
    ) -> Result<(), MermaidError> {
        let rest = trimmed.strip_prefix(keyword).unwrap().trim();

        // Strip off any inline @{...} metadata.
        let (header_part, inline_meta) = if let Some(at_pos) = rest.find("@{") {
            (&rest[..at_pos], Some(&rest[at_pos..]))
        } else {
            (rest, None)
        };

        let (id, label) = parse_container_header(header_part.trim());

        self.container_stack.push(ContainerFrame {
            kind: kind.clone(),
            id: id.clone(),
            label: label.clone(),
        });

        // Create the agent entry for agent/subgraph containers.
        if kind == ContainerKind::Agent || kind == ContainerKind::Subgraph {
            self.graph.agents.push(AgentFlowAgent {
                id: id.clone(),
                label,
                model: None,
                prompt: None,
                permits: vec![],
                memory: vec![],
                nodes: vec![],
                skills: vec![],
            });
        } else if kind == ContainerKind::Flow {
            self.graph.flows.push(AgentFlowDef {
                name: id.clone(),
                steps: vec![],
                params: BTreeMap::new(),
                returns: None,
                tasks: vec![],
            });
        } else if kind == ContainerKind::Directive {
            self.graph.directives.push(AgentFlowDirectiveNode {
                id: id.clone(),
                label: label.clone(),
                shape: "trapezoid".to_string(),
                metadata: DirectiveMetadata {
                    text: String::new(),
                    params: BTreeMap::new(),
                },
            });
        } else if kind == ContainerKind::TestCase {
            self.graph.tests.push(AgentFlowTestCase {
                id: id.clone(),
                label: label.clone(),
                assertions: vec![],
                metadata: TestMetadata {
                    assert_expr: None,
                    expects: None,
                },
            });
        }
        // Skill containers are inside agents — the stack handles nesting.
        // No separate IR entry needed since skills are part of the parent agent.

        // Handle inline metadata if present.
        if let Some(meta_str) = inline_meta {
            let meta_content = meta_str.trim_start_matches("@{");
            self.brace_depth = 1;
            self.meta_lines.clear();

            let remaining = meta_content.trim();
            if remaining.ends_with('}') && !remaining.contains('{') {
                let content = remaining.trim_end_matches('}').trim();
                if !content.is_empty() {
                    self.meta_lines.push(content.to_string());
                }
                let target = MetadataTarget::PostEnd(id);
                self.finalize_metadata(target)?;
            } else {
                if !remaining.is_empty() {
                    self.meta_lines.push(remaining.to_string());
                }
                self.state = ParserState::InMetadata(MetadataTarget::PostEnd(id));
            }
        }

        Ok(())
    }

    // ── Type declaration parsing ───────────────────────────────────────────

    fn parse_type_decl(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        let rest = trimmed.strip_prefix("type ").unwrap().trim();

        // `type Name` (opaque)
        if !rest.contains('=') && !rest.contains('{') {
            let name = rest.split_whitespace().next().unwrap_or(rest).to_string();
            self.graph.types.push(AgentFlowTypeDecl {
                name,
                kind: TypeDeclKind::Opaque,
            });
            return Ok(());
        }

        // `type Name = Record { ... }` or `type Name = SomeType`
        if let Some(eq_pos) = rest.find('=') {
            let name = rest[..eq_pos].trim().to_string();
            let rhs = rest[eq_pos + 1..].trim();

            if rhs.starts_with("Record") {
                // Multi-line record type.
                if rhs.contains('{') && rhs.contains('}') {
                    // Single-line record.
                    let inner = rhs
                        .split_once('{')
                        .map(|(_, r)| r.trim_end_matches('}').trim())
                        .unwrap_or("");
                    let fields = parse_record_fields(inner);
                    self.graph.types.push(AgentFlowTypeDecl {
                        name,
                        kind: TypeDeclKind::Record { fields },
                    });
                } else if rhs.contains('{') {
                    // Multi-line — collect until closing brace.
                    self.state = ParserState::InTypeDecl(name, Vec::new());
                } else {
                    self.graph.types.push(AgentFlowTypeDecl {
                        name,
                        kind: TypeDeclKind::Alias {
                            target: rhs.to_string(),
                        },
                    });
                }
            } else {
                // Simple alias.
                self.graph.types.push(AgentFlowTypeDecl {
                    name,
                    kind: TypeDeclKind::Alias {
                        target: rhs.to_string(),
                    },
                });
            }
        }

        Ok(())
    }

    fn collect_type_decl_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        if trimmed == "}" || trimmed.ends_with('}') {
            let content = trimmed.trim_end_matches('}').trim();
            let (name, lines) = if let ParserState::InTypeDecl(n, l) = &mut self.state {
                if !content.is_empty() {
                    l.push(content.to_string());
                }
                (n.clone(), l.clone())
            } else {
                unreachable!()
            };

            let fields = parse_record_fields(&lines.join("\n"));
            self.graph.types.push(AgentFlowTypeDecl {
                name,
                kind: TypeDeclKind::Record { fields },
            });
            self.state = ParserState::TopLevel;
        } else {
            if let ParserState::InTypeDecl(_, ref mut lines) = &mut self.state {
                lines.push(trimmed.to_string());
            }
        }
        Ok(())
    }

    // ── Template declaration parsing ───────────────────────────────────────

    fn parse_template_decl(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        let rest = trimmed.strip_prefix("template ").unwrap().trim();
        // Strip % prefix if present.
        let rest = rest.strip_prefix('%').unwrap_or(rest);

        // `template name { ... }`
        if let Some(brace_pos) = rest.find('{') {
            let name = rest[..brace_pos].trim().to_string();
            let after = rest[brace_pos + 1..].trim();
            if after.ends_with('}') {
                // Single-line template.
                let content = after.trim_end_matches('}').trim();
                let lines = if content.is_empty() {
                    vec![]
                } else {
                    vec![content.to_string()]
                };
                self.finalize_template_block(&name, &lines)?;
            } else {
                let mut lines = Vec::new();
                if !after.is_empty() {
                    lines.push(after.to_string());
                }
                self.state = ParserState::InTemplateBlock(name, lines);
            }
        }
        Ok(())
    }

    fn collect_template_block_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        if trimmed == "}" || trimmed.ends_with('}') {
            let content = trimmed.trim_end_matches('}').trim();
            let (name, lines) = if let ParserState::InTemplateBlock(n, l) = &mut self.state {
                if !content.is_empty() {
                    l.push(content.to_string());
                }
                (n.clone(), l.clone())
            } else {
                unreachable!()
            };
            self.finalize_template_block(&name, &lines)?;
            self.state = ParserState::TopLevel;
        } else {
            if let ParserState::InTemplateBlock(_, ref mut lines) = &mut self.state {
                lines.push(trimmed.to_string());
            }
        }
        Ok(())
    }

    fn finalize_template_block(
        &mut self,
        name: &str,
        lines: &[String],
    ) -> Result<(), MermaidError> {
        let mut fields = BTreeMap::new();
        let mut sections = Vec::new();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("section ") {
                sections.push(trimmed.strip_prefix("section ").unwrap().trim().to_string());
            } else if let Some((key, val)) = parse_kv_line(trimmed) {
                // Strip <<description>> from the value.
                let clean_val = if let Some(desc_pos) = val.find("<<") {
                    val[..desc_pos].trim().to_string()
                } else {
                    val
                };
                fields.insert(key, clean_val);
            }
        }

        self.graph.templates.push(AgentFlowTemplateNode {
            id: name.to_string(),
            label: name.to_string(),
            shape: "subroutine".to_string(),
            metadata: TemplateMetadata { fields, sections },
        });
        Ok(())
    }

    // ── Node line parsing ──────────────────────────────────────────────────

    fn parse_node_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        if let Some(at_pos) = trimmed.find("@{") {
            let node_part = trimmed[..at_pos].trim();
            let meta_start = &trimmed[at_pos + 2..];

            let (id, label, node_type) = classify_node(node_part)?;
            let target = self.make_metadata_target(&id, &label, node_type)?;

            self.brace_depth = 1;
            self.meta_lines.clear();

            let remaining = meta_start.trim();
            if remaining.ends_with('}') && !remaining.contains('{') {
                let content = remaining.trim_end_matches('}').trim();
                if !content.is_empty() {
                    self.meta_lines.push(content.to_string());
                }
                self.finalize_metadata(target)?;
            } else {
                if !remaining.is_empty() {
                    self.meta_lines.push(remaining.to_string());
                }
                self.state = ParserState::InMetadata(target);
            }

            return Ok(());
        }

        Ok(())
    }

    fn collect_metadata_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        for ch in trimmed.chars() {
            if ch == '{' {
                self.brace_depth += 1;
            } else if ch == '}' {
                self.brace_depth -= 1;
            }
        }

        if self.brace_depth == 0 {
            let content = trimmed.trim_end_matches('}').trim();
            if !content.is_empty() {
                self.meta_lines.push(content.to_string());
            }
            let target = if let ParserState::InMetadata(t) = &self.state {
                t.clone()
            } else {
                unreachable!()
            };
            self.finalize_metadata(target)?;
        } else {
            self.meta_lines.push(trimmed.to_string());
        }

        Ok(())
    }

    fn classify_deferred_target(&self, id: &str) -> MetadataTarget {
        // Check if id matches any agent.
        if self.graph.agents.iter().any(|a| a.id == id) {
            return MetadataTarget::PostEnd(id.to_string());
        }
        // Check if id matches any flow.
        if self.graph.flows.iter().any(|f| f.name == id) {
            return MetadataTarget::PostEnd(id.to_string());
        }
        // Check if id matches any directive.
        if self.graph.directives.iter().any(|d| d.id == id) {
            return MetadataTarget::PostEnd(id.to_string());
        }
        // Check if id matches any test case.
        if self.graph.tests.iter().any(|t| t.id == id) {
            return MetadataTarget::PostEnd(id.to_string());
        }
        // Check if id matches any skill inside an agent.
        for agent in &self.graph.agents {
            if agent.skills.iter().any(|s| s.id == id) {
                return MetadataTarget::PostEnd(id.to_string());
            }
        }
        MetadataTarget::DeferredNode(id.to_string())
    }

    fn make_metadata_target(
        &self,
        id: &str,
        label: &str,
        node_type: NodeType,
    ) -> Result<MetadataTarget, MermaidError> {
        match node_type {
            NodeType::Tool => {
                if let Some(agent_id) = self.current_agent_id() {
                    Ok(MetadataTarget::Tool(
                        agent_id,
                        id.to_string(),
                        label.to_string(),
                    ))
                } else {
                    Ok(MetadataTarget::TopTool(id.to_string(), label.to_string()))
                }
            }
            NodeType::Skill => {
                if let Some(agent_id) = self.current_agent_id() {
                    Ok(MetadataTarget::Skill(
                        agent_id,
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
            NodeType::Template => Ok(MetadataTarget::Template(id.to_string(), label.to_string())),
            NodeType::Directive => Ok(MetadataTarget::Directive(id.to_string(), label.to_string())),
            NodeType::Diamond => {
                // Diamond nodes are type references — treat as schema.
                Ok(MetadataTarget::Schema(id.to_string(), label.to_string()))
            }
        }
    }

    fn finalize_metadata(&mut self, target: MetadataTarget) -> Result<(), MermaidError> {
        let raw = self.meta_lines.join("\n");
        self.meta_lines.clear();

        match target {
            MetadataTarget::Tool(agent_id, node_id, label) => {
                let meta = parse_tool_metadata(&raw)?;
                // Determine shape from metadata or default to subroutine.
                let shape = extract_kv(&raw, "shape").unwrap_or_else(|| "subroutine".to_string());
                if let Some(agent) = self.graph.agents.iter_mut().find(|a| a.id == agent_id) {
                    agent.nodes.push(AgentFlowToolNode {
                        id: node_id,
                        label,
                        shape,
                        metadata: meta,
                    });
                }
                self.state = ParserState::TopLevel;
            }
            MetadataTarget::TopTool(node_id, label) => {
                let meta = parse_tool_metadata(&raw)?;
                let shape = extract_kv(&raw, "shape").unwrap_or_else(|| "subroutine".to_string());
                let agent_id = format!("{}_agent", node_id);
                self.graph.agents.push(AgentFlowAgent {
                    id: agent_id,
                    label: format!("@{}_agent", node_id),
                    model: None,
                    prompt: None,
                    permits: vec![],
                    memory: vec![],
                    nodes: vec![AgentFlowToolNode {
                        id: node_id,
                        label,
                        shape,
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
                self.state = ParserState::TopLevel;
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
            MetadataTarget::PostEnd(container_id) => {
                // Apply metadata to the matching agent or flow.
                if let Some(agent) = self.graph.agents.iter_mut().find(|a| a.id == container_id) {
                    if let Some(model) = extract_kv(&raw, "model") {
                        agent.model = Some(unquote(&model));
                    }
                    if let Some(permits_str) = extract_kv(&raw, "permits") {
                        agent.permits = parse_string_array_or_csv(&permits_str);
                    }
                    if let Some(prompt) = extract_kv(&raw, "prompt") {
                        agent.prompt = Some(unquote(&prompt));
                    }
                    if let Some(memory_str) = extract_kv(&raw, "memory") {
                        agent.memory = parse_string_array_or_csv(&memory_str)
                            .iter()
                            .map(|m| {
                                if m.starts_with('~') {
                                    m.clone()
                                } else {
                                    format!("~{}", m)
                                }
                            })
                            .collect();
                    }
                    if let Some(fallbacks_str) = extract_kv(&raw, "fallbacks") {
                        // Store fallbacks on matching bundle.
                        let fb = unquote(&fallbacks_str);
                        for bundle in &mut self.graph.bundles {
                            if bundle.agents.contains(&container_id) || bundle.id == container_id {
                                bundle.fallbacks = Some(fb.clone());
                            }
                        }
                    }
                }
                if let Some(flow) = self.graph.flows.iter_mut().find(|f| f.name == container_id) {
                    if let Some(params_str) = extract_kv(&raw, "params") {
                        let unquoted = unquote(&params_str);
                        for pair in unquoted.split(',') {
                            let pair = pair.trim();
                            if let Some(colon) = pair.find(':') {
                                let k = pair[..colon].trim().to_string();
                                let v = pair[colon + 1..].trim().to_string();
                                flow.params.insert(k, v);
                            }
                        }
                    }
                    if let Some(ret) = extract_kv(&raw, "returns") {
                        flow.returns = Some(unquote(&ret));
                    }
                }
                // Skill metadata (strategy, params, returns).
                for agent in &mut self.graph.agents {
                    for skill in &mut agent.skills {
                        if skill.id == container_id {
                            if let Some(strategy) = extract_kv(&raw, "strategy") {
                                skill.metadata.strategy = Some(unquote(&strategy));
                            }
                            if let Some(params_str) = extract_kv(&raw, "params") {
                                let unquoted = unquote(&params_str);
                                for pair in unquoted.split(',') {
                                    let pair = pair.trim();
                                    if let Some(sep) = pair.find("::") {
                                        let k = pair[..sep].trim().to_string();
                                        let v = pair[sep + 2..].trim().to_string();
                                        skill.metadata.params.insert(k, v);
                                    }
                                }
                            }
                            if let Some(ret) = extract_kv(&raw, "returns") {
                                skill.metadata.returns = Some(unquote(&ret));
                            }
                        }
                    }
                }
                // Test case metadata (assert, expects).
                if let Some(test) = self.graph.tests.iter_mut().find(|t| t.id == container_id) {
                    if let Some(assert_expr) = extract_kv(&raw, "assert") {
                        test.metadata.assert_expr = Some(unquote(&assert_expr));
                    }
                    if let Some(expects) = extract_kv(&raw, "expects") {
                        test.metadata.expects = Some(unquote(&expects));
                    }
                }
                // Directive metadata (params).
                if let Some(dir) = self
                    .graph
                    .directives
                    .iter_mut()
                    .find(|d| d.id == container_id)
                {
                    if let Some(params_str) = extract_kv(&raw, "params") {
                        let unquoted = unquote(&params_str);
                        for pair in unquoted.split(',') {
                            let pair = pair.trim();
                            if let Some(sep) = pair.find("::") {
                                let k = pair[..sep].trim().to_string();
                                let v = pair[sep + 2..].trim().to_string();
                                dir.metadata.params.insert(k, v);
                            }
                        }
                    }
                }
                self.state = ParserState::TopLevel;
            }
            MetadataTarget::DeferredNode(node_id) => {
                // Apply deferred @{ shape: ... } to matching node in any agent.
                let shape = extract_kv(&raw, "shape");
                let returns = extract_kv(&raw, "returns");
                let requires = extract_kv(&raw, "requires");
                let cache = extract_kv(&raw, "cache");

                for agent in &mut self.graph.agents {
                    for tool in &mut agent.nodes {
                        if tool.id == node_id {
                            if let Some(s) = &shape {
                                tool.shape = unquote(s);
                            }
                            if let Some(r) = &returns {
                                tool.metadata.returns = Some(unquote(r));
                            }
                            if let Some(r) = &requires {
                                tool.metadata.requires = parse_string_array_or_csv(r);
                            }
                            if let Some(c) = &cache {
                                tool.metadata.cache = Some(unquote(c));
                            }
                        }
                    }
                    for skill in &mut agent.skills {
                        if skill.id == node_id {
                            if let Some(s) = &shape {
                                skill.shape = unquote(s);
                            }
                        }
                    }
                }
                self.state = ParserState::TopLevel;
            }
        }

        Ok(())
    }

    // ── Flow step reconstruction ─────────────────────────────────────────

    /// Reconstruct flow steps from parsed edges and node labels.
    ///
    /// The emitter generates task edges with the pattern:
    ///   `{flow}_agent_s{n}["agent_name"] --- {flow}_{tool}_s{n}["tool_name"] --> {flow}_{output}_s{n}["output_name"]`
    ///
    /// This produces:
    ///   - Association edge: {flow}_agent_s{n} --- {flow}_{tool}_s{n}
    ///   - Flow edge: {flow}_{tool}_s{n} --> {flow}_{output}_s{n}
    ///
    /// We reconstruct each step by matching `_s{n}` suffixed nodes.
    fn reconstruct_flow_steps(&mut self) {
        use std::collections::BTreeMap;

        for flow in &mut self.graph.flows {
            let prefix = format!("{}_", flow.name);

            // Collect association edges within this flow (agent --- tool).
            let mut agent_for_step: BTreeMap<usize, String> = BTreeMap::new();
            let mut tool_for_step: BTreeMap<usize, String> = BTreeMap::new();
            let mut output_for_step: BTreeMap<usize, String> = BTreeMap::new();

            // Find edges belonging to this flow by checking if node IDs start with the flow prefix.
            for edge in &self.graph.edges {
                if !edge.from.starts_with(&prefix) && !edge.to.starts_with(&prefix) {
                    continue;
                }

                // Extract step number from _s{n} suffix.
                let extract_step_num = |id: &str| -> Option<usize> {
                    if let Some(pos) = id.rfind("_s") {
                        id[pos + 2..].parse::<usize>().ok()
                    } else {
                        None
                    }
                };

                if edge.edge_type == EdgeType::Association {
                    // agent --- tool pattern
                    if let Some(n) = extract_step_num(&edge.from) {
                        if edge.from.contains("_agent_s") {
                            // Agent node — use its label as the agent name.
                            if let Some(label) = self.node_labels.get(&edge.from) {
                                agent_for_step.insert(n, label.clone());
                            }
                        }
                    }
                    if let Some(n) = extract_step_num(&edge.to) {
                        if !edge.to.contains("_agent_s") {
                            // Tool node — use its label as the tool name.
                            if let Some(label) = self.node_labels.get(&edge.to) {
                                tool_for_step.insert(n, label.clone());
                            }
                        }
                    }
                } else if edge.edge_type == EdgeType::Flow {
                    // tool --> output pattern (within a task)
                    if let Some(from_n) = extract_step_num(&edge.from) {
                        if let Some(to_n) = extract_step_num(&edge.to) {
                            if from_n == to_n {
                                // Same step: tool --> output
                                if let Some(label) = self.node_labels.get(&edge.to) {
                                    output_for_step.insert(to_n, label.clone());
                                }
                                // Also capture tool from the from side if not already.
                                if !tool_for_step.contains_key(&from_n) {
                                    if let Some(label) = self.node_labels.get(&edge.from) {
                                        tool_for_step.insert(from_n, label.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Build steps from the collected data.
            let max_step = agent_for_step
                .keys()
                .chain(tool_for_step.keys())
                .chain(output_for_step.keys())
                .copied()
                .max()
                .unwrap_or(0);

            for n in 1..=max_step {
                let agent = agent_for_step
                    .get(&n)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string());
                let tool = tool_for_step
                    .get(&n)
                    .cloned()
                    .unwrap_or_else(|| format!("step_{}", n));
                let output_var = output_for_step
                    .get(&n)
                    .cloned()
                    .unwrap_or_else(|| format!("result_{}", n));

                // Collect args from previous step outputs.
                let args = if n == 1 {
                    flow.params.keys().cloned().collect()
                } else {
                    vec![output_for_step
                        .get(&(n - 1))
                        .cloned()
                        .unwrap_or_else(|| format!("result_{}", n - 1))]
                };

                flow.steps.push(AgentFlowStep {
                    output_var,
                    agent,
                    tool,
                    args,
                });
            }
        }
    }

    // ── Edge parsing ───────────────────────────────────────────────────────

    fn parse_edge_line(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        // Try each edge type in order of specificity (longest match first).
        // `o--o` bidirectional
        if trimmed.contains("o--o") {
            return self.parse_simple_edge(
                trimmed,
                "o--o",
                EdgeType::Bidirectional,
                EdgeStroke::Normal,
            );
        }
        // `==>` pipeline (thick)
        if trimmed.contains("==>") {
            return self.parse_simple_edge(trimmed, "==>", EdgeType::Pipeline, EdgeStroke::Thick);
        }
        // `-.->` reference (dashed, legacy)
        if trimmed.contains("-.->") {
            return self.parse_simple_edge(
                trimmed,
                "-.->",
                EdgeType::Reference,
                EdgeStroke::Dotted,
            );
        }
        // `-.-` reference/association (dashed, no arrow)
        if trimmed.contains("-.-") && !trimmed.contains("-.->") {
            return self.parse_simple_edge(
                trimmed,
                "-.-",
                EdgeType::Association,
                EdgeStroke::Dotted,
            );
        }
        // `-->>` delegation
        if trimmed.contains("-->>") {
            return self.parse_simple_edge(
                trimmed,
                "-->>",
                EdgeType::Delegation,
                EdgeStroke::Normal,
            );
        }
        // `--x` error
        if trimmed.contains("--x") {
            return self.parse_simple_edge(trimmed, "--x", EdgeType::Error, EdgeStroke::Normal);
        }
        // `--o` output binding
        if trimmed.contains("--o") && !trimmed.contains("o--o") {
            return self.parse_simple_edge(
                trimmed,
                "--o",
                EdgeType::OutputBinding,
                EdgeStroke::Normal,
            );
        }
        // `---` association
        if trimmed.contains("---")
            && !trimmed.contains("-->>")
            && !trimmed.contains("--x")
            && !trimmed.contains("--o")
        {
            return self.parse_simple_edge(
                trimmed,
                "---",
                EdgeType::Association,
                EdgeStroke::Normal,
            );
        }
        // `-->` flow (default)
        if trimmed.contains("-->") {
            return self.parse_flow_edge(trimmed);
        }

        Ok(())
    }

    fn parse_simple_edge(
        &mut self,
        trimmed: &str,
        separator: &str,
        edge_type: EdgeType,
        stroke: EdgeStroke,
    ) -> Result<(), MermaidError> {
        let parts: Vec<&str> = trimmed.splitn(2, separator).collect();
        if parts.len() == 2 {
            let left_raw = parts[0].trim();
            let from = extract_node_id(left_raw);
            if let Some(lbl) = extract_node_label(left_raw) {
                self.node_labels.insert(from.clone(), lbl);
            }
            let right = parts[1].trim();

            // Check if the right side contains another edge operator (compound edge).
            // e.g., `A --- B["label"] --> C["label"]`
            if let Some(arrow_pos) = right.find("-->") {
                let mid_raw = right[..arrow_pos].trim();
                let rest = right[arrow_pos + 3..].trim();
                let mid_id = extract_node_id(mid_raw);
                if let Some(lbl) = extract_node_label(mid_raw) {
                    self.node_labels.insert(mid_id.clone(), lbl);
                }
                let (to, label) = parse_edge_target(rest);
                // Record label from the target node too.
                if let Some(lbl) = extract_node_label(rest) {
                    self.node_labels.insert(to.clone(), lbl);
                }

                // First edge: from --- mid (current edge type)
                self.graph.edges.push(AgentFlowEdge {
                    from: from.clone(),
                    to: mid_id.clone(),
                    label: None,
                    edge_type: edge_type.clone(),
                    stroke: stroke.clone(),
                });
                // Second edge: mid --> to (flow)
                self.graph.edges.push(AgentFlowEdge {
                    from: mid_id,
                    to,
                    label,
                    edge_type: EdgeType::Flow,
                    stroke: EdgeStroke::Normal,
                });
            } else {
                let (to, label) = parse_edge_target(right);
                if let Some(lbl) = extract_node_label(right) {
                    self.node_labels.insert(to.clone(), lbl);
                }

                self.graph.edges.push(AgentFlowEdge {
                    from,
                    to,
                    label,
                    edge_type,
                    stroke,
                });
            }
        }
        Ok(())
    }

    fn parse_flow_edge(&mut self, trimmed: &str) -> Result<(), MermaidError> {
        let parts: Vec<&str> = trimmed.split("-->").collect();
        for i in 0..parts.len() - 1 {
            let left_raw = parts[i].trim();
            let right_raw = parts[i + 1].trim();

            let from = extract_node_id(left_raw);
            if let Some(lbl) = extract_node_label(left_raw) {
                self.node_labels.insert(from.clone(), lbl);
            }
            let (to, label) = parse_edge_target(right_raw);
            if let Some(lbl) = extract_node_label(right_raw) {
                self.node_labels.insert(to.clone(), lbl);
            }

            self.graph.edges.push(AgentFlowEdge {
                from,
                to,
                label,
                edge_type: EdgeType::Flow,
                stroke: EdgeStroke::Normal,
            });
        }
        Ok(())
    }
}

// ── Edge target parsing ────────────────────────────────────────────────────

fn parse_edge_target(right: &str) -> (String, Option<String>) {
    if let Some(rest) = right.strip_prefix('|') {
        if let Some(pipe_end) = rest.find('|') {
            let lbl = rest[..pipe_end].trim().trim_matches('"').to_string();
            let node = rest[pipe_end + 1..].trim().to_string();
            (extract_node_id(&node), Some(lbl))
        } else {
            (extract_node_id(right), None)
        }
    } else {
        (extract_node_id(right), None)
    }
}

/// Extract the bare node ID from a possibly-labeled node reference like `id["label"]`.
fn extract_node_id(s: &str) -> String {
    let s = s.trim();
    if let Some(pos) = s.find(['[', '(', '{']) {
        s[..pos].trim().to_string()
    } else {
        s.to_string()
    }
}

/// Extract the label from a possibly-labeled node reference like `id["label"]`.
/// Returns None if no label bracket is present.
fn extract_node_label(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(start) = s.find('[') {
        let rest = &s[start..];
        // Handle ["label"] format
        if let Some(inner) = rest.strip_prefix("[\"") {
            if let Some(end) = inner.find("\"]") {
                return Some(inner[..end].to_string());
            }
        }
        // Handle [label] format
        if let Some(inner) = rest.strip_prefix('[') {
            if let Some(end) = inner.find(']') {
                return Some(inner[..end].trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Check if a line contains any edge operator.
fn is_edge_line(trimmed: &str) -> bool {
    trimmed.contains("-->")
        || trimmed.contains("-.->")
        || trimmed.contains("-.-")
        || trimmed.contains("==>")
        || trimmed.contains("--o")
        || trimmed.contains("--x")
        || trimmed.contains("-->>")
        || trimmed.contains("o--o")
        || trimmed.contains("---")
}

// ── Node classification ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
enum NodeType {
    Tool,      // id["Label"]
    Schema,    // id{{"Label"}}
    Skill,     // id(["Label"])
    Template,  // id[["Label"]]
    Directive, // id[/"Label"/]
    Diamond,   // id{"Label"} or id{Label}
}

fn classify_node(s: &str) -> Result<(String, String, NodeType), MermaidError> {
    let s = s.trim();

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

    // Diamond: {Label} or {"Label"}
    if let Some(inner) = rest.strip_prefix('{') {
        if let Some(inner) = inner.strip_suffix('}') {
            let label = unquote(inner.trim());
            return Ok((id, label, NodeType::Diamond));
        }
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

    // Lean-right (flow): >"Label"]
    if let Some(inner) = rest.strip_prefix(">") {
        if let Some(inner) = inner.strip_suffix(']') {
            let label = unquote(inner.trim());
            return Ok((id, label, NodeType::Tool));
        }
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

/// Parse `id["Label"]` from container header.
fn parse_container_header(rest: &str) -> (String, String) {
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
        let id = rest.to_string();
        let label = format!("@{}", rest);
        (id, label)
    }
}

// ── Metadata parsers ───────────────────────────────────────────────────────

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

        // Detect start of params block (case-insensitive).
        let trimmed_lower = trimmed.to_lowercase();
        if trimmed_lower == "params:" {
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
            let key_lower = key.to_lowercase();
            match key_lower.as_str() {
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
                "shape" | "agent" => {} // skip shape/agent keys — handled elsewhere
                _ => {}
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

        if trimmed.to_lowercase() == "params:" {
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
            match key.to_lowercase().as_str() {
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

        if trimmed.to_lowercase() == "fields:" {
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

        let trimmed_lower = trimmed.to_lowercase();
        if trimmed_lower == "fields:" {
            in_fields = true;
            continue;
        }

        if let Some((key, val)) = parse_kv_line(trimmed) {
            if key.to_lowercase() == "sections" {
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

        if trimmed.to_lowercase() == "params:" {
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
            if key.to_lowercase() == "text" {
                text = unquote(&val);
            }
        }
    }

    Ok(DirectiveMetadata { text, params })
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn parse_kv_line(line: &str) -> Option<(String, String)> {
    let colon_pos = line.find(':')?;
    let key = line[..colon_pos].trim().to_string();
    let val = line[colon_pos + 1..].trim().to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

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

/// Parse either `["a", "b"]` array syntax or `"a, b"` CSV syntax.
fn parse_string_array_or_csv(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.starts_with('[') {
        parse_string_array(s)
    } else {
        let inner = s.trim_matches('"');
        inner
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    }
}

/// Extract a single key's value from raw metadata text.
///
/// Handles both multi-line (`key: value` per line) and single-line
/// comma-separated (`key1: "val1", key2: "val2"`) formats.
fn extract_kv(raw: &str, key: &str) -> Option<String> {
    let line_count = raw.lines().count();

    // For multi-line metadata (one key: value per line), parse each line independently.
    // This avoids corruption from joining lines that use newlines as separators.
    // Only use this path when there are multiple lines AND each line has a single KV pair.
    if line_count > 1 {
        for line in raw.lines() {
            let trimmed = line.trim();
            if !has_unquoted_comma(trimmed) {
                if let Some((k, v)) = parse_kv_line(trimmed) {
                    if k.to_lowercase() == key.to_lowercase() {
                        return Some(v);
                    }
                }
            }
        }
    }

    // Comma-separated format: split on commas respecting quotes, then match key.
    let joined = raw.lines().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
    for part in split_kv_pairs(&joined) {
        if let Some((k, v)) = parse_kv_line(part.trim()) {
            if k.to_lowercase() == key.to_lowercase() {
                return Some(v);
            }
        }
    }

    None
}

/// Check if a string contains a comma outside of quoted strings.
fn has_unquoted_comma(s: &str) -> bool {
    let mut in_quotes = false;
    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => return true,
            _ => {}
        }
    }
    false
}

/// Split `key1: "val1", key2: "val2"` into individual key-value pair strings,
/// respecting quoted commas and bracket-enclosed arrays.
fn split_kv_pairs(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut bracket_depth = 0;

    for ch in s.chars() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if !in_quotes && ch == '[' {
            bracket_depth += 1;
            current.push(ch);
        } else if !in_quotes && ch == ']' {
            bracket_depth -= 1;
            current.push(ch);
        } else if ch == ',' && !in_quotes && bracket_depth == 0 {
            parts.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(ch);
        }
    }
    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Parse record fields from `name: Type` lines.
fn parse_record_fields(s: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((key, val)) = parse_kv_line(trimmed) {
            fields.insert(key, val.trim().to_string());
        }
    }
    fields
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
        assert_eq!(graph.agents[0].nodes[0].metadata.description, "Do research");
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
    fn parse_new_agent_keyword() {
        let input = r#"
agentflow TB
    agent researcher["Researcher"]
        research["Research"]@{
            description: "Do research"
            requires: ["^net.read"]
            returns: "String"
        }
    end
    researcher@{ model: "claude-3", permits: "net.read, llm.query" }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.direction, "TB");
        assert_eq!(graph.agents.len(), 1);
        assert_eq!(graph.agents[0].id, "researcher");
        assert_eq!(graph.agents[0].model.as_deref(), Some("claude-3"));
        assert_eq!(graph.agents[0].permits.len(), 2);
        assert!(graph.agents[0].permits.contains(&"net.read".to_string()));
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
    fn parse_new_edge_types() {
        let input = r#"
agentflow TB
    a --o b
    c --x d
    e -->> f
    g --- h
    i o--o j
    k ==> l
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.edges.len(), 6);
        assert_eq!(graph.edges[0].edge_type, EdgeType::OutputBinding);
        assert_eq!(graph.edges[1].edge_type, EdgeType::Error);
        assert_eq!(graph.edges[2].edge_type, EdgeType::Delegation);
        assert_eq!(graph.edges[3].edge_type, EdgeType::Association);
        assert_eq!(graph.edges[4].edge_type, EdgeType::Bidirectional);
        assert_eq!(graph.edges[5].edge_type, EdgeType::Pipeline);
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
    fn parse_type_declarations() {
        let input = r#"
agentflow TB
    type Opaque
    type Alias = String
    type Report = Record {
        title: String
        body: String
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.types.len(), 3);
        assert!(matches!(graph.types[0].kind, TypeDeclKind::Opaque));
        assert!(matches!(graph.types[1].kind, TypeDeclKind::Alias { .. }));
        assert!(matches!(graph.types[2].kind, TypeDeclKind::Record { .. }));
        if let TypeDeclKind::Record { fields } = &graph.types[2].kind {
            assert_eq!(fields.len(), 2);
            assert_eq!(fields.get("title"), Some(&"String".to_string()));
        }
    }

    #[test]
    fn parse_template_declaration() {
        let input = r#"
agentflow TB
    template website_copy {
        HERO_TAGLINE: String           <<main tagline>>
        section ENGLISH
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.templates.len(), 1);
        assert_eq!(graph.templates[0].id, "website_copy");
        assert_eq!(
            graph.templates[0].metadata.fields.get("HERO_TAGLINE"),
            Some(&"String".to_string())
        );
        assert_eq!(graph.templates[0].metadata.sections, vec!["ENGLISH"]);
    }

    #[test]
    fn parse_frontmatter() {
        let input = r#"
---
config:
  layout: elk
---
agentflow TB
    agent researcher["Researcher"]
    end
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.direction, "TB");
        assert_eq!(graph.agents.len(), 1);
    }

    #[test]
    fn parse_nested_containers() {
        let input = r#"
agentflow TB
    flow main["Main"]
        task Step1
        end
        task Step2
        end
    end
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.flows.len(), 1);
        assert_eq!(graph.flows[0].name, "main");
    }

    #[test]
    fn parse_deferred_node_metadata() {
        let input = r#"
agentflow TB
    agent researcher["Researcher"]
        search["Search"]@{
            description: "Search"
            returns: "String"
        }
    end
    search@{ shape: subroutine, requires: "^net.read", cache: "5m" }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.agents[0].nodes[0].shape, "subroutine");
        assert_eq!(
            graph.agents[0].nodes[0].metadata.requires,
            vec!["^net.read"]
        );
        assert_eq!(
            graph.agents[0].nodes[0].metadata.cache.as_deref(),
            Some("5m")
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
    fn parse_diamond_node() {
        let input = r#"
agentflow TB
    Result{"Result"}@{
        fields:
            status: "String"
    }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.schemas.len(), 1);
        assert_eq!(graph.schemas[0].id, "Result");
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

    #[test]
    fn parse_edge_with_output_binding_label() {
        let input = r#"
agentflow TB
    a --o|"result"| b
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].edge_type, EdgeType::OutputBinding);
        assert_eq!(graph.edges[0].label.as_deref(), Some("result"));
    }

    #[test]
    fn parse_flow_container_with_metadata() {
        let input = r#"
agentflow TB
    flow main["Main"]
        task Step1
        end
    end
    main@{ params: "topic: String", returns: "String" }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.flows.len(), 1);
        assert_eq!(graph.flows[0].name, "main");
        assert_eq!(
            graph.flows[0].params.get("topic"),
            Some(&"String".to_string())
        );
        assert_eq!(graph.flows[0].returns.as_deref(), Some("String"));
    }

    #[test]
    fn parse_agent_with_inline_metadata() {
        let input = r#"
agentflow TB
    subgraph researcher["@researcher"]@{
        permits: [netRead, llmQuery]
        prompt: "You are a researcher"
    }
        search["Search"]@{
            description: "Search"
            returns: "String"
        }
    end
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.agents.len(), 1);
        assert_eq!(graph.agents[0].permits.len(), 2);
        assert_eq!(
            graph.agents[0].prompt.as_deref(),
            Some("You are a researcher")
        );
    }

    #[test]
    fn roundtrip_text_to_graph_to_pact() {
        // Parse agentflow text → graph → PACT text → verify key elements.
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
    researcher@{ model: "claude-3", permits: "net.read" }
"#;
        let graph = parse_agentflow_text(input).unwrap();
        let pact = crate::agentflow_convert::agentflow_graph_to_pact(&graph);
        assert!(pact.contains("tool #search"), "No tool in PACT:\n{}", pact);
        assert!(
            pact.contains("agent @researcher"),
            "No agent in PACT:\n{}",
            pact
        );
        assert!(pact.contains("description: <<Search the web>>"));
        assert!(pact.contains("requires: [^net.read]"));
    }

    #[test]
    fn deeply_nested_containers() {
        let input = r#"
agentflow TB
    agent outer["Outer"]
        flow inner["Inner"]
            task Step1
            end
            task Step2
            end
            task Step3
            end
        end
    end
"#;
        let graph = parse_agentflow_text(input).unwrap();
        assert_eq!(graph.agents.len(), 1);
        assert_eq!(graph.flows.len(), 1);
    }
}
