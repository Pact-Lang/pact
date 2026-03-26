// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Core IR types for the `agentflow` diagram format.
//!
//! Every PACT construct maps to a distinct node type with its own metadata
//! schema. Agents are represented as subgraphs containing tool and skill nodes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ── Tool metadata ──────────────────────────────────────────────────────────

/// Metadata carried by a tool node's `@{...}` block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolMetadata {
    /// Human-readable description of the tool.
    pub description: String,
    /// Capability permissions the tool requires (e.g. `^net.read`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Capability permissions explicitly denied to the tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
    /// Source expression for the tool invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Handler function or endpoint for the tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handler: Option<String>,
    /// Expected output format or destination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Inline directives attached to this tool.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<String>,
    /// Named parameters as `name -> type` pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    /// Return type of the tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    /// Number of retry attempts on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
    /// Cache strategy or TTL expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<String>,
    /// Validation expression applied to tool output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
}

// ── Schema metadata ────────────────────────────────────────────────────────

/// Metadata for a schema node — fields as key:type pairs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Schema fields as `name -> type` pairs.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

// ── Template metadata ──────────────────────────────────────────────────────

/// Metadata for a template node — fields and/or sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateMetadata {
    /// Template fields as `name -> type` pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    /// Named sections within the template.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
}

// ── Directive metadata ─────────────────────────────────────────────────────

/// Metadata for a directive node — prompt text with optional params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectiveMetadata {
    /// The directive prompt text.
    pub text: String,
    /// Optional parameters as `name -> value` pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

// ── Skill metadata ─────────────────────────────────────────────────────────

/// Metadata for a skill node inside an agent subgraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Human-readable description of the skill.
    pub description: String,
    /// Tool IDs that this skill composes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Execution strategy (e.g. sequential, parallel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    /// Named parameters as `name -> type` pairs.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    /// Return type of the skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
}

// ── Node types ─────────────────────────────────────────────────────────────

/// A tool node inside an agent subgraph (rounded rect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowToolNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label shown inside the node shape.
    pub label: String,
    /// Mermaid shape descriptor (e.g. `rounded-rect`).
    pub shape: String,
    /// Tool-specific metadata from the `@{...}` block.
    pub metadata: ToolMetadata,
}

/// A skill node inside an agent subgraph (stadium/pill).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowSkillNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label shown inside the node shape.
    pub label: String,
    /// Mermaid shape descriptor (e.g. `stadium`).
    pub shape: String,
    /// Skill-specific metadata.
    pub metadata: SkillMetadata,
}

/// A standalone schema node (hexagon).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowSchemaNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label shown inside the node shape.
    pub label: String,
    /// Mermaid shape descriptor (e.g. `hexagon`).
    pub shape: String,
    /// Schema-specific metadata (field definitions).
    pub metadata: SchemaMetadata,
}

/// A standalone template node (subroutine / double-bordered rect).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowTemplateNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label shown inside the node shape.
    pub label: String,
    /// Mermaid shape descriptor (e.g. `subroutine`).
    pub shape: String,
    /// Template-specific metadata (fields and sections).
    pub metadata: TemplateMetadata,
}

/// A standalone directive node (trapezoid).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowDirectiveNode {
    /// Unique node identifier.
    pub id: String,
    /// Display label shown inside the node shape.
    pub label: String,
    /// Mermaid shape descriptor (e.g. `trapezoid`).
    pub shape: String,
    /// Directive-specific metadata (prompt text and params).
    pub metadata: DirectiveMetadata,
}

// ── Lesson metadata ───────────────────────────────────────────────────────

/// Metadata for a lesson node (learned operational knowledge).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonMetadata {
    /// Context: what happened that prompted this lesson.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    /// Rule: what to do in similar situations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// Severity level (error, warning).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

/// A lesson node (lin-doc shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowLessonNode {
    pub id: String,
    pub label: String,
    pub shape: String,
    pub metadata: LessonMetadata,
}

// ── Test metadata ─────────────────────────────────────────────────────────

/// Metadata for a testCase container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMetadata {
    /// Assertion expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assert_expr: Option<String>,
    /// Human-readable expected behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expects: Option<String>,
}

/// A test case container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowTestCase {
    pub id: String,
    pub label: String,
    /// Assertion node IDs inside the test.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<String>,
    pub metadata: TestMetadata,
}

// ── Agent & Bundle ─────────────────────────────────────────────────────────

/// An agent container (subgraph) holding tool and skill nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowAgent {
    /// Unique agent identifier (used as subgraph ID).
    pub id: String,
    /// Display label for the agent subgraph.
    pub label: String,
    /// LLM model name the agent uses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// System prompt assigned to the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// Permission capabilities granted to this agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permits: Vec<String>,
    /// Memory backends configured for the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory: Vec<String>,
    /// Tool nodes contained within the agent subgraph.
    pub nodes: Vec<AgentFlowToolNode>,
    /// Skill nodes contained within the agent subgraph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<AgentFlowSkillNode>,
}

/// An agent bundle grouping multiple agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowBundle {
    /// Unique bundle identifier.
    pub id: String,
    /// Display label for the bundle.
    pub label: String,
    /// Agent IDs grouped by this bundle.
    pub agents: Vec<String>,
    /// Fallback strategy expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<String>,
}

// ── Edges ──────────────────────────────────────────────────────────────────

/// Distinguishes execution flow from structural references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum EdgeType {
    /// Execution-flow edge (solid arrow `-->`). This is the default.
    #[default]
    Flow,
    /// Structural reference edge (dashed arrow `-.->`).
    Reference,
    /// Output binding edge (`--o`).
    OutputBinding,
    /// Error/cancel edge (`--x`).
    Error,
    /// Delegation edge (`-->>`).
    Delegation,
    /// Association edge (`---`).
    Association,
    /// Bidirectional edge (`o--o`).
    Bidirectional,
    /// Pipeline edge (`==>`).
    Pipeline,
}

/// Edge stroke style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EdgeStroke {
    #[default]
    Normal,
    Thick,
    Dotted,
}

/// An edge connecting two nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowEdge {
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Optional label rendered on the edge arrow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Whether this edge represents execution flow or a structural reference.
    #[serde(default, rename = "type")]
    pub edge_type: EdgeType,
    /// Stroke style for the edge line.
    #[serde(default, skip_serializing_if = "is_default_stroke")]
    pub stroke: EdgeStroke,
}

fn is_default_stroke(s: &EdgeStroke) -> bool {
    *s == EdgeStroke::Normal
}

/// The kind of container block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerKind {
    Agent,
    Flow,
    Task,
    Subgraph,
    Skill,
    Directive,
    TestCase,
    Group,
}

/// A first-class type declaration in agentflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowTypeDecl {
    /// Type name.
    pub name: String,
    /// The kind of type declaration.
    pub kind: TypeDeclKind,
}

/// Variants of type declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TypeDeclKind {
    /// Opaque type with no visible structure.
    Opaque,
    /// Type alias: `type Foo = Bar`
    Alias { target: String },
    /// Record type: `type Foo = Record { field: Type, ... }`
    Record { fields: BTreeMap<String, String> },
}

/// A task block inside a flow container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowTask {
    /// Task identifier.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Node IDs contained in this task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<String>,
    /// Edges within this task.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<AgentFlowEdge>,
}

// ── Top-level graph ────────────────────────────────────────────────────────

/// The complete agentflow graph — full PACT model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowGraph {
    /// Diagram type identifier (always `"agentflow"`).
    #[serde(rename = "type")]
    pub diagram_type: String,
    /// Graph layout direction (e.g. `"LR"`, `"TB"`).
    pub direction: String,
    /// Top-level schema nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schemas: Vec<AgentFlowSchemaNode>,
    /// Top-level template nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<AgentFlowTemplateNode>,
    /// Top-level directive nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub directives: Vec<AgentFlowDirectiveNode>,
    /// Agent subgraphs containing tool and skill nodes.
    pub agents: Vec<AgentFlowAgent>,
    /// Agent bundles grouping multiple agents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bundles: Vec<AgentFlowBundle>,
    /// Lesson nodes (lin-doc shape with operational knowledge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lessons: Vec<AgentFlowLessonNode>,
    /// Test case containers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<AgentFlowTestCase>,
    /// All edges (flow and reference) in the graph.
    pub edges: Vec<AgentFlowEdge>,
    /// Flow definitions with their steps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flows: Vec<AgentFlowDef>,
    /// Type alias declarations (union types). Kept for backward compat.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_aliases: Vec<AgentFlowTypeAlias>,
    /// First-class type declarations (records, aliases, opaques).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub types: Vec<AgentFlowTypeDecl>,
}

/// A flow definition containing ordered steps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowDef {
    /// Flow name.
    pub name: String,
    /// Ordered steps in the flow.
    pub steps: Vec<AgentFlowStep>,
    /// Named parameters for this flow.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    /// Return type of this flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    /// Task blocks within this flow.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<AgentFlowTask>,
}

/// A single step in a flow: an agent dispatching a tool, producing an output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowStep {
    /// Output variable name.
    pub output_var: String,
    /// Agent name (without @).
    pub agent: String,
    /// Tool name (without #).
    pub tool: String,
    /// Argument variable names.
    pub args: Vec<String>,
    /// Skill name (without $), if the tool belongs to a skill on this agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

/// A type alias (union type) declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFlowTypeAlias {
    /// Alias name.
    pub name: String,
    /// Variant names.
    pub variants: Vec<String>,
}

impl AgentFlowGraph {
    /// Create a new empty graph with the given direction.
    pub fn new(direction: &str) -> Self {
        Self {
            diagram_type: "agentflow".to_string(),
            direction: direction.to_string(),
            schemas: Vec::new(),
            templates: Vec::new(),
            directives: Vec::new(),
            agents: Vec::new(),
            bundles: Vec::new(),
            lessons: Vec::new(),
            tests: Vec::new(),
            edges: Vec::new(),
            flows: Vec::new(),
            type_aliases: Vec::new(),
            types: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_edge_type_is_flow() {
        assert_eq!(EdgeType::default(), EdgeType::Flow);
    }

    #[test]
    fn new_graph_is_empty() {
        let g = AgentFlowGraph::new("LR");
        assert_eq!(g.diagram_type, "agentflow");
        assert_eq!(g.direction, "LR");
        assert!(g.agents.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn tool_metadata_serde_roundtrip() {
        let meta = ToolMetadata {
            description: "Search the web".to_string(),
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
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: ToolMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.description, "Search the web");
        assert_eq!(back.requires, vec!["^net.read"]);
        assert_eq!(back.source.as_deref(), Some("^search.duckduckgo(query)"));
    }

    #[test]
    fn edge_type_serde() {
        let edge = AgentFlowEdge {
            from: "a".to_string(),
            to: "b".to_string(),
            label: None,
            edge_type: EdgeType::Reference,
            stroke: EdgeStroke::Normal,
        };
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"type\":\"reference\""));
    }

    #[test]
    fn new_edge_types_serde() {
        for (et, expected) in [
            (EdgeType::OutputBinding, "outputbinding"),
            (EdgeType::Error, "error"),
            (EdgeType::Delegation, "delegation"),
            (EdgeType::Association, "association"),
            (EdgeType::Bidirectional, "bidirectional"),
            (EdgeType::Pipeline, "pipeline"),
        ] {
            let edge = AgentFlowEdge {
                from: "a".into(),
                to: "b".into(),
                label: None,
                edge_type: et,
                stroke: EdgeStroke::Normal,
            };
            let json = serde_json::to_string(&edge).unwrap();
            assert!(json.contains(expected), "expected {} in {}", expected, json);
        }
    }

    #[test]
    fn type_decl_serde_roundtrip() {
        let decl = AgentFlowTypeDecl {
            name: "Report".into(),
            kind: TypeDeclKind::Record {
                fields: BTreeMap::from([
                    ("title".into(), "String".into()),
                    ("body".into(), "String".into()),
                ]),
            },
        };
        let json = serde_json::to_string(&decl).unwrap();
        let back: AgentFlowTypeDecl = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Report");
    }
}
