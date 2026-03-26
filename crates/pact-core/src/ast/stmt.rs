// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-04-12

//! Declaration / statement AST nodes.
//!
//! These nodes represent the top-level constructs in a `.pact` file:
//! agents, flows, schemas, type aliases, permission trees, tools, skills,
//! templates, directives, tests, imports, connections, lessons, and
//! compliance declarations.

use super::expr::Expr;
use super::types::TypeExpr;
use crate::span::Span;

/// A top-level declaration in a `.pact` file.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// The specific declaration variant.
    pub kind: DeclKind,
    /// Source span covering the entire declaration.
    pub span: Span,
}

/// All top-level declaration variants.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclKind {
    /// An agent declaration.
    ///
    /// ```pact
    /// agent @name {
    ///     permits: [^perm1, ^perm2]
    ///     tools: [#tool1, #tool2]
    ///     model: "gpt-4"
    ///     prompt: <<...>>
    /// }
    /// ```
    Agent(AgentDecl),

    /// An agent bundle declaration.
    ///
    /// ```pact
    /// agent_bundle @name {
    ///     agents: [@a, @b]
    ///     fallbacks: [@a ?> @b]
    /// }
    /// ```
    AgentBundle(AgentBundleDecl),

    /// A flow declaration.
    ///
    /// ```pact
    /// flow name(params) -> ReturnType {
    ///     body...
    /// }
    /// ```
    Flow(FlowDecl),

    /// A schema declaration.
    ///
    /// ```pact
    /// schema Name {
    ///     field :: Type
    /// }
    /// ```
    Schema(SchemaDecl),

    /// A type alias (union type).
    ///
    /// ```pact
    /// type Name = A | B | C
    /// ```
    TypeAlias(TypeAliasDecl),

    /// A permission tree declaration.
    ///
    /// ```pact
    /// permit_tree {
    ///     ^net { ^net.read, ^net.write }
    ///     ^llm { ^llm.query }
    /// }
    /// ```
    PermitTree(PermitTreeDecl),

    /// A tool declaration.
    ///
    /// ```pact
    /// tool #name {
    ///     description: <<...>>
    ///     requires: [^perm]
    ///     params { name :: Type }
    ///     returns :: Type
    /// }
    /// ```
    Tool(ToolDecl),

    /// A skill declaration.
    ///
    /// ```pact
    /// skill $name {
    ///     description: <<...>>
    ///     tools: [#tool1, #tool2]
    ///     strategy: <<...>>
    ///     params { name :: Type }
    ///     returns :: Type
    /// }
    /// ```
    Skill(SkillDecl),

    /// A test declaration.
    ///
    /// ```pact
    /// test "description" {
    ///     ...
    /// }
    /// ```
    Test(TestDecl),

    /// A template declaration.
    ///
    /// ```pact
    /// template %website_copy {
    ///     HERO_TAGLINE :: String      <<one powerful headline>>
    ///     HERO_SUBTITLE :: String     <<one compelling subtitle>>
    ///     MENU_ITEM :: String * 6     <<Name | Price | Description>>
    ///     section ENGLISH             <<paste the original copy>>
    /// }
    /// ```
    Template(TemplateDecl),

    /// A directive declaration — reusable prompt block with optional parameters.
    ///
    /// ```pact
    /// directive %scandinavian_design {
    ///     <<Use Google Fonts...>>
    ///     params {
    ///         heading_font :: String = "Playfair Display"
    ///     }
    /// }
    /// ```
    Directive(DirectiveDecl),

    /// An import declaration.
    ///
    /// ```pact
    /// import "path/to/file.pact"
    /// ```
    Import(ImportDecl),

    /// An MCP server connection block.
    ///
    /// ```pact
    /// connect {
    ///     slack   "stdio slack-mcp-server"
    ///     github  "sse https://github.internal/mcp"
    /// }
    /// ```
    Connect(ConnectDecl),

    /// A lesson declaration — formalized operational knowledge.
    ///
    /// ```pact
    /// lesson "cache_invalidation" {
    ///     context: <<After deploy, cache was stale for 10 minutes>>
    ///     rule: <<Always invalidate CDN cache after deploy>>
    ///     severity: warning
    /// }
    /// ```
    Lesson(LessonDecl),

    /// A compliance declaration — regulatory and governance metadata.
    ///
    /// ```pact
    /// compliance "payment_processing" {
    ///     risk: high
    ///     frameworks: [pci_dss, gdpr, sox]
    ///     audit: full
    ///     retention: "7y"
    ///     review_interval: "90d"
    ///     roles {
    ///         approver: "finance_lead"
    ///         executor: "payment_agent"
    ///         auditor: "compliance_team"
    ///     }
    /// }
    /// ```
    Compliance(ComplianceDecl),
}

/// Directive declaration — reusable prompt block with optional parameters.
/// Referenced by tools via `directives: [%name, ...]`.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveDecl {
    /// Directive name (without the `%` prefix).
    pub name: String,
    /// The prompt text content.
    pub text: String,
    /// Optional parameters with default values.
    pub params: Vec<DirectiveParam>,
}

/// A directive parameter with a type and default value.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveParam {
    /// Parameter name.
    pub name: String,
    /// Type annotation.
    pub ty: TypeExpr,
    /// Default value (required for directive params).
    pub default: Expr,
}

/// The kind of import: local file or registry package.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportKind {
    /// A relative file import: `import "path/to/file.pact"`.
    File,
    /// A registry package import: `import "pkg:name"` or `import "pkg:name@^0.1"`.
    Package {
        /// Package name (e.g., `"pact-std"` from `"pkg:pact-std"`).
        name: String,
        /// Optional version constraint (e.g., `"^0.1"` from `"pkg:pact-std@^0.1"`).
        version: Option<String>,
    },
}

/// Import declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// The raw path string from the source.
    pub path: String,
    /// Whether this is a file or package import.
    pub kind: ImportKind,
    /// Source span of the import declaration.
    pub span: Span,
}

/// A single MCP server connection entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectEntry {
    /// Logical name for this server (e.g. "slack", "github").
    pub name: String,
    /// Transport string: "stdio command..." or "sse url...".
    pub transport: String,
    /// Source span of this entry.
    pub span: Span,
}

/// MCP server connection block declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectDecl {
    /// The MCP server entries declared in this block.
    pub servers: Vec<ConnectEntry>,
}

/// Lesson declaration — formalized operational knowledge.
/// Captures context, rules, and severity for process memory.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonDecl {
    /// Lesson name (a string identifier).
    pub name: String,
    /// When/why this lesson was learned.
    pub context: Option<String>,
    /// The guideline or rule.
    pub rule: Option<String>,
    /// Severity level: "info", "warning", or "error".
    pub severity: Option<String>,
}

/// Compliance declaration — regulatory and governance metadata.
/// Defines risk tiers, regulatory frameworks, audit requirements, and
/// separation-of-duty roles for regulated agent deployments.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplianceDecl {
    /// Compliance profile name.
    pub name: String,
    /// Risk tier: "low", "medium", "high", "critical".
    pub risk: Option<String>,
    /// Regulatory frameworks (e.g. "gdpr", "hipaa", "pci_dss", "sox", "ccpa").
    pub frameworks: Vec<String>,
    /// Audit level: "none", "summary", "full".
    pub audit: Option<String>,
    /// Data retention period (e.g. "7y", "90d", "indefinite").
    pub retention: Option<String>,
    /// Review interval (e.g. "90d", "1y").
    pub review_interval: Option<String>,
    /// Separation-of-duty role assignments.
    pub roles: Vec<ComplianceRole>,
}

/// A role in a separation-of-duties declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplianceRole {
    /// Role type (e.g. "approver", "executor", "auditor", "reviewer").
    pub role: String,
    /// Assigned entity name (agent name or team string).
    pub assignee: String,
}

/// Template declaration — reusable output format specification.
/// Referenced by tools via `output: %template_name`.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateDecl {
    /// Template name (without the `%` prefix).
    pub name: String,
    /// Template entries (fields, repeats, sections).
    pub entries: Vec<TemplateEntry>,
}

/// A single entry in a template declaration.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateEntry {
    /// A named field: `FIELD_NAME :: Type <<description>>`
    Field {
        /// Field identifier.
        name: String,
        /// Type annotation for the field.
        ty: TypeExpr,
        /// Optional human-readable description in `<<...>>`.
        description: Option<String>,
    },
    /// A repeated field: `FIELD_NAME :: Type * count <<description>>`
    Repeat {
        /// Field identifier.
        name: String,
        /// Type annotation for each repeated element.
        ty: TypeExpr,
        /// Number of repetitions requested.
        count: usize,
        /// Optional human-readable description in `<<...>>`.
        description: Option<String>,
    },
    /// A labeled section: `section NAME <<description>>`
    Section {
        /// Section label.
        name: String,
        /// Optional human-readable description in `<<...>>`.
        description: Option<String>,
    },
}

/// Agent declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentDecl {
    /// Agent name (without the `@` prefix).
    pub name: String,
    /// Required permissions.
    pub permits: Vec<Expr>,
    /// Available tools.
    pub tools: Vec<Expr>,
    /// Available skills.
    pub skills: Vec<Expr>,
    /// Optional model specifier.
    pub model: Option<Expr>,
    /// Optional prompt literal.
    pub prompt: Option<Expr>,
    /// Optional memory references.
    pub memory: Vec<Expr>,
    /// Optional compliance profile reference.
    pub compliance: Option<String>,
}

/// Agent bundle declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentBundleDecl {
    /// Bundle name (without the `@` prefix).
    pub name: String,
    /// Member agents.
    pub agents: Vec<Expr>,
    /// Fallback chain expression.
    pub fallbacks: Option<Expr>,
}

/// Flow declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowDecl {
    /// Flow name.
    pub name: String,
    /// Parameters with type annotations.
    pub params: Vec<Param>,
    /// Return type annotation.
    pub return_type: Option<TypeExpr>,
    /// Body expressions (statements).
    pub body: Vec<Expr>,
}

/// A parameter with an optional type annotation.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Optional type annotation (`:: Type`).
    pub ty: Option<TypeExpr>,
    /// Source span of the parameter.
    pub span: Span,
}

/// Schema declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDecl {
    /// Schema name.
    pub name: String,
    /// Fields defined in the schema body.
    pub fields: Vec<SchemaField>,
}

/// A single field in a schema.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaField {
    /// Field name.
    pub name: String,
    /// Type annotation (`:: Type`).
    pub ty: TypeExpr,
    /// Source span of the field definition.
    pub span: Span,
}

/// Type alias declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    /// Alias name.
    pub name: String,
    /// Union variant names (`A | B | C`).
    pub variants: Vec<String>,
}

/// Permission tree declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct PermitTreeDecl {
    /// Top-level permission nodes in the tree.
    pub nodes: Vec<PermitNode>,
}

/// A node in a permission tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PermitNode {
    /// The permission path segments (e.g. `["net"]` for `!net`).
    pub path: Vec<String>,
    /// Child permissions.
    pub children: Vec<PermitNode>,
    /// Source span of this permission node.
    pub span: Span,
}

/// Built-in capability source specification.
/// Represents `source: !capability.provider(param1, param2)`.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpec {
    /// The capability path, e.g. "search.duckduckgo".
    pub capability: String,
    /// Parameter names to pass from the tool's params.
    pub args: Vec<String>,
}

/// Tool declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolDecl {
    /// Tool name (without the `#` prefix).
    pub name: String,
    /// Description prompt literal for LLM consumption.
    pub description: Expr,
    /// Required permissions (as permission ref expressions).
    pub requires: Vec<Expr>,
    /// Optional handler specification for real execution.
    /// Format: `"http METHOD url"`, `"sh command"`, `"builtin:name"`.
    pub handler: Option<String>,
    /// Optional built-in capability source for execution.
    /// Alternative to `handler:` — uses the provider registry.
    pub source: Option<SourceSpec>,
    /// Optional output template reference (template name without `%` prefix).
    pub output: Option<String>,
    /// Directive names referenced by this tool (without `%` prefix).
    pub directives: Vec<String>,
    /// Tool parameters with type annotations.
    pub params: Vec<Param>,
    /// Return type annotation.
    pub return_type: Option<TypeExpr>,
    /// Retry count on failure (e.g. `retry: 3`).
    pub retry: Option<u32>,
    /// Output validation mode: "strict" or "lenient".
    pub validate: Option<String>,
    /// Cache duration string (e.g. "24h", "30m", "7d").
    pub cache: Option<String>,
    /// MCP import shorthand: (server_name, tool_name) from `tool #name = mcp server/tool`.
    pub mcp_import: Option<(String, String)>,
}

/// Skill declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillDecl {
    /// Skill name (without the `$` prefix).
    pub name: String,
    /// Description prompt literal for LLM consumption.
    pub description: Expr,
    /// Tools this skill uses.
    pub tools: Vec<Expr>,
    /// Strategy prompt — instructions for how to use the tools.
    pub strategy: Option<Expr>,
    /// Skill parameters with type annotations.
    pub params: Vec<Param>,
    /// Return type annotation.
    pub return_type: Option<TypeExpr>,
}

/// Test declaration fields.
#[derive(Debug, Clone, PartialEq)]
pub struct TestDecl {
    /// Human-readable test description string.
    pub description: String,
    /// Body expressions forming the test logic.
    pub body: Vec<Expr>,
}

/// A complete PACT program (one source file).
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// All top-level declarations in the source file.
    pub decls: Vec<Decl>,
}
