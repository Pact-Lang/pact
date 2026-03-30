// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-06-10

//! Semantic analysis for the PACT language.
//!
//! The checker performs two passes over the AST:
//!
//! 1. **Name collection** — registers all top-level declarations into a
//!    [`SymbolTable`], detecting duplicates.
//! 2. **Validation** — verifies type references, agent-tool-permission
//!    consistency, and other semantic rules.
//!
//! # Tool Permission Resolution
//!
//! The checker uses a two-tier approach for tool permissions:
//! - **Declarative** — if `tool #name { ... }` declarations exist, their
//!   `requires` lists are used.
//! - **Fallback** — if a tool is referenced but not declared, the hardcoded
//!   [`tool_permission_registry`] is consulted.
//!
//! # Usage
//!
//! ```
//! use pact_core::checker::Checker;
//! use pact_core::ast::stmt::Program;
//! # use pact_core::lexer::Lexer;
//! # use pact_core::parser::Parser;
//! # use pact_core::span::SourceMap;
//! # let mut sm = SourceMap::new();
//! # let id = sm.add("test.pact", "agent @g { permits: [^llm.query] tools: [#greet] }");
//! # let tokens = Lexer::new(sm.text(id), id).lex().unwrap();
//! # let program = Parser::new(&tokens).parse().unwrap();
//! let errors = Checker::new().check(&program);
//! if errors.is_empty() {
//!     println!("OK");
//! }
//! ```

pub mod permissions;
pub mod scope;
pub mod types;

use crate::ast::expr::ExprKind;
use crate::ast::stmt::{DeclKind, Program};
use crate::ast::types::TypeExprKind;
use permissions::{permission_satisfies, tool_permission_registry};
use scope::{SymbolKind, SymbolTable};
use types::is_builtin_type;

use miette::Diagnostic;
use thiserror::Error;

/// A diagnostic error produced during semantic analysis.
#[derive(Debug, Error, Diagnostic, Clone)]
pub enum CheckError {
    /// A symbol was defined more than once at the top level.
    #[error("duplicate definition of '{name}'")]
    DuplicateDefinition {
        /// The duplicated symbol name.
        name: String,
        /// Location of the redefinition.
        #[label("redefined here")]
        span: miette::SourceSpan,
    },

    /// A type reference could not be resolved to any builtin or user-defined type.
    #[error("unknown type '{name}'")]
    UnknownType {
        /// The unresolved type name.
        name: String,
        /// Location where the type was referenced.
        #[label("used here")]
        span: miette::SourceSpan,
    },

    /// An agent uses a tool without holding a required permission.
    #[error("agent '@{agent}' uses tool '#{tool}' which requires permission '{permission}', but the agent does not have it")]
    #[diagnostic(help("add '^{permission}' to the agent's permits list"))]
    MissingPermission {
        /// Name of the agent lacking the permission.
        agent: String,
        /// Name of the tool that requires the permission.
        tool: String,
        /// The missing permission path.
        permission: String,
        /// Location of the tool reference in the agent declaration.
        #[label("tool used here")]
        span: miette::SourceSpan,
    },

    /// A reference to an agent that was never declared.
    #[error("unknown agent '@{name}'")]
    UnknownAgent {
        /// The unresolved agent name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A reference to a flow that was never declared.
    #[error("unknown flow '{name}'")]
    UnknownFlow {
        /// The unresolved flow name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A variable was reassigned with a value of a different inferred type.
    #[error("type inference warning: variable '{variable}' was inferred as {expected} but is being assigned {found}")]
    TypeInferenceWarning {
        /// The variable being reassigned.
        variable: String,
        /// The previously inferred type.
        expected: String,
        /// The type of the new value.
        found: String,
    },

    /// A tool's `source` block references an argument not declared in `params`.
    #[error("tool '#{tool}' source arg '{arg}' does not match any declared parameter")]
    #[diagnostic(help(
        "source args should reference parameters declared in the tool's params block"
    ))]
    SourceArgNotAParam {
        /// Name of the tool containing the invalid source arg.
        tool: String,
        /// The source argument that does not match any parameter.
        arg: String,
        /// Location of the tool declaration.
        #[label("tool declared here")]
        span: miette::SourceSpan,
    },

    /// A reference to a template that was never declared.
    #[error("unknown template '%{name}'")]
    #[diagnostic(help("define a template with `template %{name} {{ ... }}`"))]
    UnknownTemplate {
        /// The unresolved template name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A reference to a directive that was never declared.
    #[error("unknown directive '%{name}'")]
    #[diagnostic(help("define a directive with `directive %{name} {{ ... }}`"))]
    UnknownDirective {
        /// The unresolved directive name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A tool's MCP handler references an undeclared MCP server.
    #[error("unknown MCP server '{name}'")]
    #[diagnostic(help("declare the server in a `connect {{ {name} \"stdio ...\" }}` block"))]
    UnknownMcpServer {
        /// The unresolved MCP server name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A lesson has an invalid severity value.
    #[error("invalid lesson severity '{value}' for lesson '{name}'")]
    #[diagnostic(help("severity must be one of: info, warning, error"))]
    InvalidLessonSeverity {
        /// The lesson name.
        name: String,
        /// The invalid severity value.
        value: String,
        /// Location of the severity value.
        #[label("invalid severity here")]
        span: miette::SourceSpan,
    },

    /// An MCP connect entry has an invalid transport prefix.
    #[error("invalid MCP transport '{transport}' for server '{name}'")]
    #[diagnostic(help("transport must start with 'stdio ' or 'sse '"))]
    InvalidMcpTransport {
        /// The server name with the invalid transport.
        name: String,
        /// The invalid transport string.
        transport: String,
        /// Location of the entry.
        #[label("declared here")]
        span: miette::SourceSpan,
    },

    /// A compliance profile has an invalid risk tier.
    #[error("invalid compliance risk tier '{value}' for profile '{name}'")]
    #[diagnostic(help("risk must be one of: low, medium, high, critical"))]
    InvalidComplianceRisk {
        /// The compliance profile name.
        name: String,
        /// The invalid risk value.
        value: String,
        /// Location of the risk value.
        #[label("invalid risk here")]
        span: miette::SourceSpan,
    },

    /// A compliance profile has an invalid audit level.
    #[error("invalid compliance audit level '{value}' for profile '{name}'")]
    #[diagnostic(help("audit must be one of: none, summary, full"))]
    InvalidComplianceAudit {
        /// The compliance profile name.
        name: String,
        /// The invalid audit value.
        value: String,
        /// Location of the audit value.
        #[label("invalid audit here")]
        span: miette::SourceSpan,
    },

    /// An agent references a compliance profile that was never declared.
    #[error("unknown compliance profile '{name}'")]
    #[diagnostic(help("define a compliance profile with `compliance \"{name}\" {{ ... }}`"))]
    UnknownCompliance {
        /// The unresolved compliance profile name.
        name: String,
        /// Location of the reference.
        #[label("referenced here")]
        span: miette::SourceSpan,
    },

    /// A compliance profile has conflicting separation-of-duty roles.
    #[error("compliance profile '{name}': agent '{agent}' holds conflicting roles ({role_a} and {role_b})")]
    #[diagnostic(help(
        "separation of duties requires that no agent holds both approver and executor roles"
    ))]
    ComplianceSodConflict {
        /// The compliance profile name.
        name: String,
        /// The agent holding conflicting roles.
        agent: String,
        /// One of the conflicting roles.
        role_a: String,
        /// The other conflicting role.
        role_b: String,
        /// Location of the compliance declaration.
        #[label("declared here")]
        span: miette::SourceSpan,
    },

    /// A federation registry URL is not a valid HTTP(S) URL.
    #[error("invalid federation registry URL '{url}'")]
    #[diagnostic(help("federation registry URLs must start with 'https://' or 'http://'"))]
    InvalidFederationUrl {
        /// The invalid URL.
        url: String,
        /// Location of the registry entry.
        #[label("declared here")]
        span: miette::SourceSpan,
    },

    /// A federation registry entry has no trust permissions.
    #[error("federation registry '{url}' has empty trust permissions")]
    #[diagnostic(help("add at least one permission to the trust list, e.g. trust: [^llm.query]"))]
    EmptyFederationTrust {
        /// The registry URL.
        url: String,
        /// Location of the registry entry.
        #[label("declared here")]
        span: miette::SourceSpan,
    },

    /// A remote agent endpoint URL is not a valid HTTP(S) URL.
    #[error("agent '@{agent}' has invalid endpoint URL '{url}'")]
    #[diagnostic(help("endpoint URLs must start with 'https://' or 'http://'"))]
    InvalidAgentEndpoint {
        /// The agent name.
        agent: String,
        /// The invalid URL.
        url: String,
        /// Location of the agent declaration.
        #[label("declared here")]
        span: miette::SourceSpan,
    },
}

/// The semantic checker for PACT programs.
pub struct Checker {
    /// Collected top-level declarations.
    symbols: SymbolTable,
    /// Accumulated semantic errors.
    errors: Vec<CheckError>,
    /// Whether the program contains any `tool` declarations.
    /// When true, we use declarative tool info; when false, we fall back
    /// to the hardcoded registry for backward compatibility.
    has_tool_decls: bool,
}

impl Checker {
    /// Create a new checker.
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
            errors: Vec::new(),
            has_tool_decls: false,
        }
    }

    /// Run all semantic checks on a program. Returns the list of errors found.
    pub fn check(mut self, program: &Program) -> Vec<CheckError> {
        self.collect_names(program);
        self.validate(program);
        self.run_type_inference(program);
        self.errors
    }

    /// Pass 1: Collect all top-level names into the symbol table.
    fn collect_names(&mut self, program: &Program) {
        for decl in &program.decls {
            match &decl.kind {
                DeclKind::Agent(a) => {
                    let permits: Vec<Vec<String>> = a
                        .permits
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::PermissionRef(segs) => Some(segs.clone()),
                            _ => None,
                        })
                        .collect();
                    let tools: Vec<String> = a
                        .tools
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::ToolRef(name) => Some(name.clone()),
                            _ => None,
                        })
                        .collect();
                    if !self
                        .symbols
                        .define(a.name.clone(), SymbolKind::Agent { permits, tools })
                    {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: a.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
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
                    if !self
                        .symbols
                        .define(ab.name.clone(), SymbolKind::AgentBundle { agents })
                    {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: ab.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Tool(t) => {
                    self.has_tool_decls = true;
                    let requires: Vec<Vec<String>> = t
                        .requires
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::PermissionRef(segs) => Some(segs.clone()),
                            _ => None,
                        })
                        .collect();
                    let params: Vec<(String, String, bool)> = t
                        .params
                        .iter()
                        .map(|p| {
                            let type_name =
                                p.ty.as_ref()
                                    .map(Self::type_expr_to_string)
                                    .unwrap_or_else(|| "Any".to_string());
                            // All params are required for now (Optional<T> support later)
                            (p.name.clone(), type_name, true)
                        })
                        .collect();
                    let return_type = t.return_type.as_ref().map(Self::type_expr_to_string);
                    if !self.symbols.define(
                        t.name.clone(),
                        SymbolKind::Tool {
                            requires,
                            params,
                            return_type,
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: t.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Flow(f) => {
                    if !self.symbols.define(
                        f.name.clone(),
                        SymbolKind::Flow {
                            param_count: f.params.len(),
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: f.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Schema(s) => {
                    let fields: Vec<(String, String)> = s
                        .fields
                        .iter()
                        .map(|f| {
                            let type_name = Self::type_expr_to_string(&f.ty);
                            (f.name.clone(), type_name)
                        })
                        .collect();
                    if !self
                        .symbols
                        .define(s.name.clone(), SymbolKind::Schema { fields })
                    {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: s.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::TypeAlias(t) => {
                    if !self.symbols.define(
                        t.name.clone(),
                        SymbolKind::TypeAlias {
                            variants: t.variants.clone(),
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: t.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::PermitTree(pt) => {
                    self.collect_permit_nodes(&pt.nodes);
                }
                DeclKind::Skill(s) => {
                    let tools: Vec<String> = s
                        .tools
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::ToolRef(name) => Some(name.clone()),
                            _ => None,
                        })
                        .collect();
                    let params: Vec<(String, String, bool)> = s
                        .params
                        .iter()
                        .map(|p| {
                            let type_name =
                                p.ty.as_ref()
                                    .map(Self::type_expr_to_string)
                                    .unwrap_or_else(|| "Any".to_string());
                            (p.name.clone(), type_name, true)
                        })
                        .collect();
                    let return_type = s.return_type.as_ref().map(Self::type_expr_to_string);
                    if !self.symbols.define(
                        s.name.clone(),
                        SymbolKind::Skill {
                            tools,
                            params,
                            return_type,
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: s.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Template(t) => {
                    let entries: Vec<String> = t
                        .entries
                        .iter()
                        .map(|e| match e {
                            crate::ast::stmt::TemplateEntry::Field { name, .. } => name.clone(),
                            crate::ast::stmt::TemplateEntry::Repeat { name, .. } => name.clone(),
                            crate::ast::stmt::TemplateEntry::Section { name, .. } => name.clone(),
                        })
                        .collect();
                    if !self
                        .symbols
                        .define(t.name.clone(), SymbolKind::Template { entries })
                    {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: t.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Directive(d) => {
                    let params: Vec<String> = d.params.iter().map(|p| p.name.clone()).collect();
                    if !self
                        .symbols
                        .define(d.name.clone(), SymbolKind::Directive { params })
                    {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: d.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Connect(c) => {
                    for entry in &c.servers {
                        // Validate transport prefix
                        if !entry.transport.starts_with("stdio ")
                            && !entry.transport.starts_with("sse ")
                        {
                            self.errors.push(CheckError::InvalidMcpTransport {
                                name: entry.name.clone(),
                                transport: entry.transport.clone(),
                                span: (entry.span.start..entry.span.end).into(),
                            });
                        }
                        self.symbols.define(
                            format!("__mcp__{}", entry.name),
                            SymbolKind::McpServer {
                                name: entry.name.clone(),
                            },
                        );
                        // Auto-define mcp.{name} permission
                        self.symbols
                            .define_permission(format!("mcp.{}", entry.name));
                    }
                }
                DeclKind::Lesson(l) => {
                    // Validate severity if present
                    if let Some(ref sev) = l.severity {
                        if !matches!(sev.as_str(), "info" | "warning" | "error") {
                            self.errors.push(CheckError::InvalidLessonSeverity {
                                name: l.name.clone(),
                                value: sev.clone(),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }
                    if !self.symbols.define(
                        l.name.clone(),
                        SymbolKind::Lesson {
                            severity: l.severity.clone(),
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: l.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Compliance(c) => {
                    // Validate risk tier
                    if let Some(ref risk) = c.risk {
                        if !matches!(risk.as_str(), "low" | "medium" | "high" | "critical") {
                            self.errors.push(CheckError::InvalidComplianceRisk {
                                name: c.name.clone(),
                                value: risk.clone(),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }
                    // Validate audit level
                    if let Some(ref audit) = c.audit {
                        if !matches!(audit.as_str(), "none" | "summary" | "full") {
                            self.errors.push(CheckError::InvalidComplianceAudit {
                                name: c.name.clone(),
                                value: audit.clone(),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }
                    // SOD: no agent should hold both approver and executor
                    let mut role_map: std::collections::HashMap<&str, Vec<&str>> =
                        std::collections::HashMap::new();
                    for role in &c.roles {
                        role_map.entry(&role.assignee).or_default().push(&role.role);
                    }
                    for (assignee, roles) in &role_map {
                        let has_approver = roles.contains(&"approver");
                        let has_executor = roles.contains(&"executor");
                        if has_approver && has_executor {
                            self.errors.push(CheckError::ComplianceSodConflict {
                                name: c.name.clone(),
                                agent: assignee.to_string(),
                                role_a: "approver".to_string(),
                                role_b: "executor".to_string(),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }
                    if !self.symbols.define(
                        c.name.clone(),
                        SymbolKind::Compliance {
                            risk: c.risk.clone(),
                            audit: c.audit.clone(),
                        },
                    ) {
                        self.errors.push(CheckError::DuplicateDefinition {
                            name: c.name.clone(),
                            span: (decl.span.start..decl.span.end).into(),
                        });
                    }
                }
                DeclKind::Test(_) => {
                    // Tests don't define symbols
                }
                DeclKind::Import(_) => {
                    // Imports are resolved by the loader before checking
                }
                DeclKind::Federation(f) => {
                    // Validate federation registry URLs
                    for entry in &f.registries {
                        if !entry.url.starts_with("https://") && !entry.url.starts_with("http://") {
                            self.errors.push(CheckError::InvalidFederationUrl {
                                url: entry.url.clone(),
                                span: (entry.span.start..entry.span.end).into(),
                            });
                        }
                        if entry.trust.is_empty() {
                            self.errors.push(CheckError::EmptyFederationTrust {
                                url: entry.url.clone(),
                                span: (entry.span.start..entry.span.end).into(),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Recursively collect permission paths from a permit tree.
    fn collect_permit_nodes(&mut self, nodes: &[crate::ast::stmt::PermitNode]) {
        for node in nodes {
            let path = node.path.join(".");
            self.symbols.define_permission(path);
            self.collect_permit_nodes(&node.children);
        }
    }

    /// Pass 2: Validate semantic rules.
    fn validate(&mut self, program: &Program) {
        // Build the fallback registry only if no tool declarations exist.
        let fallback_registry = if self.has_tool_decls {
            None
        } else {
            Some(tool_permission_registry())
        };

        for decl in &program.decls {
            match &decl.kind {
                DeclKind::Agent(a) => {
                    let permits: Vec<Vec<String>> = a
                        .permits
                        .iter()
                        .filter_map(|e| match &e.kind {
                            ExprKind::PermissionRef(segs) => Some(segs.clone()),
                            _ => None,
                        })
                        .collect();

                    for tool_expr in &a.tools {
                        if let ExprKind::ToolRef(tool_name) = &tool_expr.kind {
                            self.check_tool_permissions(
                                &a.name,
                                tool_name,
                                &permits,
                                tool_expr.span.start,
                                tool_expr.span.end,
                                &fallback_registry,
                            );
                        }
                    }

                    // Validate compliance profile reference
                    if let Some(ref compliance_name) = a.compliance {
                        match self.symbols.lookup(compliance_name) {
                            Some(SymbolKind::Compliance { .. }) => {} // OK
                            _ => {
                                self.errors.push(CheckError::UnknownCompliance {
                                    name: compliance_name.clone(),
                                    span: (decl.span.start..decl.span.end).into(),
                                });
                            }
                        }
                    }

                    // Validate remote agent endpoint URL
                    if let Some(ref url) = a.endpoint {
                        if !url.starts_with("https://") && !url.starts_with("http://") {
                            self.errors.push(CheckError::InvalidAgentEndpoint {
                                agent: a.name.clone(),
                                url: url.clone(),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }
                }
                DeclKind::Tool(t) => {
                    // Validate tool parameter types
                    for param in &t.params {
                        if let Some(ty) = &param.ty {
                            self.check_type_exists(ty);
                        }
                    }
                    if let Some(rt) = &t.return_type {
                        self.check_type_exists(rt);
                    }

                    // Validate source capability reference
                    if let Some(source) = &t.source {
                        let param_names: Vec<&str> =
                            t.params.iter().map(|p| p.name.as_str()).collect();
                        for arg in &source.args {
                            if !param_names.contains(&arg.as_str()) {
                                self.errors.push(CheckError::SourceArgNotAParam {
                                    tool: t.name.clone(),
                                    arg: arg.clone(),
                                    span: (decl.span.start..decl.span.end).into(),
                                });
                            }
                        }
                    }

                    // Validate output template reference
                    if let Some(output) = &t.output {
                        match self.symbols.lookup(output) {
                            Some(SymbolKind::Template { .. }) => {} // OK
                            _ => {
                                self.errors.push(CheckError::UnknownTemplate {
                                    name: output.clone(),
                                    span: (decl.span.start..decl.span.end).into(),
                                });
                            }
                        }
                    }

                    // Validate directive references
                    for dir_name in &t.directives {
                        match self.symbols.lookup(dir_name) {
                            Some(SymbolKind::Directive { .. }) => {} // OK
                            _ => {
                                self.errors.push(CheckError::UnknownDirective {
                                    name: dir_name.clone(),
                                    span: (decl.span.start..decl.span.end).into(),
                                });
                            }
                        }
                    }

                    // Validate validate: field values
                    if let Some(validate) = &t.validate {
                        if validate != "strict" && validate != "lenient" {
                            self.errors.push(CheckError::UnknownType {
                                name: format!(
                                    "invalid validation mode '{}' (expected 'strict' or 'lenient')",
                                    validate
                                ),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }

                    // Validate cache: duration format (number followed by s/m/h/d)
                    if let Some(cache) = &t.cache {
                        let valid = cache.len() >= 2
                            && cache[..cache.len() - 1].parse::<u64>().is_ok()
                            && matches!(cache.chars().last(), Some('s' | 'm' | 'h' | 'd'));
                        if !valid {
                            self.errors.push(CheckError::UnknownType {
                                name: format!("invalid cache duration '{}' (expected format like '24h', '30m', '7d')", cache),
                                span: (decl.span.start..decl.span.end).into(),
                            });
                        }
                    }

                    // Validate MCP handler references
                    if let Some(handler_str) = &t.handler {
                        if let Some(rest) = handler_str.strip_prefix("mcp ") {
                            if let Some((server, _tool)) = rest.split_once('/') {
                                let key = format!("__mcp__{}", server);
                                if self.symbols.lookup(&key).is_none() {
                                    self.errors.push(CheckError::UnknownMcpServer {
                                        name: server.to_string(),
                                        span: (decl.span.start..decl.span.end).into(),
                                    });
                                }
                            }
                        }
                    }

                    // Warn if retry count is unreasonably high
                    if let Some(retry) = t.retry {
                        if retry > 10 {
                            self.errors.push(CheckError::TypeInferenceWarning {
                                variable: format!("tool #{}", t.name),
                                expected: "retry count <= 10".to_string(),
                                found: format!("retry: {}", retry),
                            });
                        }
                    }
                }
                DeclKind::Template(t) => {
                    // Validate template entry types
                    for entry in &t.entries {
                        match entry {
                            crate::ast::stmt::TemplateEntry::Field { ty, .. } => {
                                self.check_type_exists(ty);
                            }
                            crate::ast::stmt::TemplateEntry::Repeat { ty, .. } => {
                                self.check_type_exists(ty);
                            }
                            crate::ast::stmt::TemplateEntry::Section { .. } => {}
                        }
                    }
                }
                DeclKind::Flow(f) => {
                    for param in &f.params {
                        if let Some(ty) = &param.ty {
                            self.check_type_exists(ty);
                        }
                    }
                    if let Some(rt) = &f.return_type {
                        self.check_type_exists(rt);
                    }
                    // Validate expressions in flow body
                    for expr in &f.body {
                        self.check_expr(expr);
                    }
                }
                DeclKind::Schema(s) => {
                    for field in &s.fields {
                        self.check_type_exists(&field.ty);
                    }
                }
                DeclKind::Directive(d) => {
                    // Validate directive parameter types
                    for param in &d.params {
                        self.check_type_exists(&param.ty);
                    }
                }
                _ => {}
            }
        }
    }

    /// Recursively check expressions for semantic errors (e.g. RunFlow references).
    fn check_expr(&mut self, expr: &crate::ast::expr::Expr) {
        match &expr.kind {
            ExprKind::RunFlow { flow_name, args } => {
                // Verify the referenced flow exists
                match self.symbols.lookup(flow_name) {
                    Some(SymbolKind::Flow { .. }) => {} // OK
                    _ => {
                        self.errors.push(CheckError::UnknownFlow {
                            name: flow_name.clone(),
                            span: (expr.span.start..expr.span.end).into(),
                        });
                    }
                }
                for arg in args {
                    self.check_expr(arg);
                }
            }
            ExprKind::OnError { body, fallback } => {
                self.check_expr(body);
                self.check_expr(fallback);
            }
            ExprKind::Env(_) => {
                // No static validation needed — runtime check
            }
            ExprKind::Assign { value, .. } => {
                self.check_expr(value);
            }
            ExprKind::Pipeline { left, right } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            ExprKind::FallbackChain { primary, fallback } => {
                self.check_expr(primary);
                self.check_expr(fallback);
            }
            ExprKind::AgentDispatch { agent, tool, args } => {
                self.check_expr(agent);
                self.check_expr(tool);
                for arg in args {
                    self.check_expr(arg);
                }
            }
            ExprKind::FuncCall { callee, args } => {
                self.check_expr(callee);
                for arg in args {
                    self.check_expr(arg);
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                self.check_expr(left);
                self.check_expr(right);
            }
            ExprKind::Return(inner) | ExprKind::Fail(inner) | ExprKind::Assert(inner) => {
                self.check_expr(inner);
            }
            ExprKind::Parallel(exprs) | ExprKind::ListLit(exprs) | ExprKind::Record(exprs) => {
                for e in exprs {
                    self.check_expr(e);
                }
            }
            ExprKind::Match { subject, arms } => {
                self.check_expr(subject);
                for arm in arms {
                    self.check_expr(&arm.body);
                }
            }
            ExprKind::FieldAccess { object, .. } => {
                self.check_expr(object);
            }
            ExprKind::Typed { expr, .. } => {
                self.check_expr(expr);
            }
            ExprKind::RecordFields(fields) => {
                for (_, val) in fields {
                    self.check_expr(val);
                }
            }
            // Leaf nodes — no recursion needed
            _ => {}
        }
    }

    /// Check that an agent has the permissions required by a tool.
    ///
    /// First looks up the tool in the symbol table (declarative). If not found,
    /// falls back to the hardcoded registry (if provided).
    fn check_tool_permissions(
        &mut self,
        agent_name: &str,
        tool_name: &str,
        agent_permits: &[Vec<String>],
        span_start: usize,
        span_end: usize,
        fallback: &Option<std::collections::HashMap<&str, Vec<&str>>>,
    ) {
        // Try declarative tool lookup first
        if let Some(SymbolKind::Tool { requires, .. }) = self.symbols.lookup(tool_name) {
            let requires = requires.clone();
            for perm_path in &requires {
                let perm_str = perm_path.join(".");
                if !permission_satisfies(agent_permits, &perm_str) {
                    self.errors.push(CheckError::MissingPermission {
                        agent: agent_name.to_string(),
                        tool: tool_name.to_string(),
                        permission: perm_str,
                        span: (span_start..span_end).into(),
                    });
                }
            }
            return;
        }

        // Fall back to hardcoded registry
        if let Some(registry) = fallback {
            if let Some(required) = registry.get(tool_name) {
                for perm in required {
                    if !permission_satisfies(agent_permits, perm) {
                        self.errors.push(CheckError::MissingPermission {
                            agent: agent_name.to_string(),
                            tool: tool_name.to_string(),
                            permission: perm.to_string(),
                            span: (span_start..span_end).into(),
                        });
                    }
                }
            }
        }
    }

    /// Check that a type reference refers to a known type.
    fn check_type_exists(&mut self, ty: &crate::ast::types::TypeExpr) {
        match &ty.kind {
            TypeExprKind::Named(name) => {
                if !is_builtin_type(name) && self.symbols.lookup(name).is_none() {
                    self.errors.push(CheckError::UnknownType {
                        name: name.clone(),
                        span: (ty.span.start..ty.span.end).into(),
                    });
                }
            }
            TypeExprKind::Generic { name, args } => {
                if !is_builtin_type(name) && self.symbols.lookup(name).is_none() {
                    self.errors.push(CheckError::UnknownType {
                        name: name.clone(),
                        span: (ty.span.start..ty.span.end).into(),
                    });
                }
                for arg in args {
                    self.check_type_exists(arg);
                }
            }
            TypeExprKind::Optional(inner) => {
                self.check_type_exists(inner);
            }
        }
    }

    /// Run basic type inference over all flows in the program.
    fn run_type_inference(&mut self, program: &Program) {
        let mut inference = types::TypeInference::new();
        inference.infer_program(program, &self.symbols);
        for warning in &inference.warnings {
            self.errors.push(CheckError::TypeInferenceWarning {
                variable: warning.variable.clone(),
                expected: warning.expected.clone(),
                found: warning.found.clone(),
            });
        }
    }

    /// Convert a type expression to a string representation.
    pub fn type_expr_to_string(ty: &crate::ast::types::TypeExpr) -> String {
        match &ty.kind {
            TypeExprKind::Named(n) => n.clone(),
            TypeExprKind::Generic { name, args } => {
                let arg_strs: Vec<String> = args.iter().map(Self::type_expr_to_string).collect();
                format!("{}<{}>", name, arg_strs.join(", "))
            }
            TypeExprKind::Optional(inner) => {
                format!("{}?", Self::type_expr_to_string(inner))
            }
        }
    }
}

impl Default for Checker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::span::SourceMap;

    fn check_src(src: &str) -> Vec<CheckError> {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let program = Parser::new(&tokens).parse().unwrap();
        Checker::new().check(&program)
    }

    // ── Backward-compatible tests (no tool decls → hardcoded fallback) ──

    #[test]
    fn valid_agent_passes() {
        let errors = check_src("agent @greeter { permits: [^llm.query] tools: [#greet] }");
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn missing_permission_detected() {
        let errors = check_src("agent @bad { permits: [] tools: [#web_search] }");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::MissingPermission { .. }));
    }

    #[test]
    fn parent_permission_satisfies() {
        let errors = check_src("agent @ok { permits: [^net] tools: [#web_search] }");
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn unknown_type_detected() {
        let errors = check_src("flow f(x :: UnknownType) { return x }");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::UnknownType { .. }));
    }

    #[test]
    fn builtin_type_passes() {
        let errors = check_src("flow f(x :: String) -> String { return x }");
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn duplicate_definition_detected() {
        let errors =
            check_src("agent @a { permits: [] tools: [] } agent @a { permits: [] tools: [] }");
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::DuplicateDefinition { .. }));
    }

    #[test]
    fn schema_field_types_checked() {
        let errors = check_src("schema Report { title :: String, score :: Float }");
        assert!(errors.is_empty());
    }

    #[test]
    fn permit_tree_collects_permissions() {
        let src = r#"
            permit_tree {
                ^net {
                    ^net.read
                    ^net.write
                }
            }
            agent @fetcher { permits: [^net.read] tools: [#web_search] }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // ── Declarative tool tests ─────────────────────────────────────

    #[test]
    fn declarative_tool_permissions() {
        let src = r#"
            tool #custom_search {
                description: <<Search things>>
                requires: [^net.read]
                params {
                    query :: String
                }
                returns :: String
            }
            agent @searcher { permits: [^net.read] tools: [#custom_search] }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn declarative_tool_missing_permission() {
        let src = r#"
            tool #custom_search {
                description: <<Search things>>
                requires: [^net.read]
                params {
                    query :: String
                }
            }
            agent @bad { permits: [] tools: [#custom_search] }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::MissingPermission { .. }));
    }

    #[test]
    fn declarative_tool_type_checking() {
        let src = r#"
            tool #analyze {
                description: <<Analyze data>>
                requires: [^llm.query]
                params {
                    data :: String
                }
                returns :: UnknownType
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::UnknownType { .. }));
    }

    #[test]
    fn declarative_tool_duplicate() {
        let src = r#"
            tool #x {
                description: <<First>>
                requires: []
                params {}
            }
            tool #x {
                description: <<Second>>
                requires: []
                params {}
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::DuplicateDefinition { .. }));
    }

    #[test]
    fn declarative_tool_parent_permission() {
        let src = r#"
            tool #fetch {
                description: <<Fetch data>>
                requires: [^net.read]
                params {
                    url :: String
                }
            }
            agent @ok { permits: [^net] tools: [#fetch] }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn declarative_tool_multiple_permissions() {
        let src = r#"
            tool #upload {
                description: <<Upload file>>
                requires: [^net.write, ^fs.read]
                params {
                    path :: String
                    url :: String
                }
            }
            agent @uploader { permits: [^net.write] tools: [#upload] }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        match &errors[0] {
            CheckError::MissingPermission { permission, .. } => {
                assert_eq!(permission, "fs.read");
            }
            _ => panic!("expected MissingPermission"),
        }
    }

    // ── Type inference tests ─────────────────────────────────────

    #[test]
    fn type_inference_no_warnings_for_consistent_types() {
        let src = r#"
            flow f(x :: String) -> String {
                y = "hello"
                return y
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn type_inference_warns_on_incompatible_reassignment() {
        let src = r#"
            flow f() {
                x = "hello"
                x = 42
                return x
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(
            matches!(&errors[0], CheckError::TypeInferenceWarning { variable, expected, found }
                if variable == "x" && expected == "String" && found == "Int"
            ),
            "expected TypeInferenceWarning, got: {:?}",
            errors[0]
        );
    }

    // ── MCP / connect tests ────────────────────────────────────

    #[test]
    fn connect_block_valid() {
        let src = r#"
            connect {
                slack "stdio slack-mcp-server"
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn connect_block_invalid_transport() {
        let src = r#"
            connect {
                bad "ftp://example.com"
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::InvalidMcpTransport { .. }));
    }

    #[test]
    fn mcp_handler_valid() {
        let src = r#"
            connect {
                slack "stdio slack-mcp-server"
            }
            tool #post {
                description: <<Post.>>
                requires: [^mcp.slack]
                handler: "mcp slack/send_message"
                params { text :: String }
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn mcp_handler_unknown_server() {
        let src = r#"
            tool #post {
                description: <<Post.>>
                requires: []
                handler: "mcp slack/send_message"
                params { text :: String }
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], CheckError::UnknownMcpServer { .. }));
    }

    #[test]
    fn mcp_tool_shorthand_with_connect() {
        let src = r#"
            connect {
                github "stdio github-mcp-server"
            }
            tool #create_issue = mcp github/create_issue
            agent @bot {
                permits: [^mcp.github]
                tools: [#create_issue]
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn type_inference_dispatch_return_type() {
        let src = r#"
            tool #search {
                description: <<Search>>
                requires: []
                params {
                    query :: String
                }
                returns :: String
            }
            agent @bot { permits: [] tools: [#search] }
            flow f() {
                result = @bot -> #search("test")
                result = 42
                return result
            }
        "#;
        let errors = check_src(src);
        // The dispatch returns String, then result is reassigned to Int -> warning
        let inference_warnings: Vec<_> = errors
            .iter()
            .filter(|e| matches!(e, CheckError::TypeInferenceWarning { .. }))
            .collect();
        assert_eq!(
            inference_warnings.len(),
            1,
            "expected 1 type inference warning, got: {:?}",
            inference_warnings
        );
    }

    #[test]
    fn checker_duplicate_lesson() {
        let src = r#"
            lesson "cache" {
                rule: <<Always invalidate cache>>
            }
            lesson "cache" {
                rule: <<Never cache>>
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(
                |e| matches!(e, CheckError::DuplicateDefinition { name, .. } if name == "cache")
            ),
            "expected duplicate definition error for lesson 'cache'"
        );
    }

    #[test]
    fn checker_invalid_severity() {
        let src = r#"
            lesson "test_lesson" {
                rule: <<Do the thing>>
                severity: critical
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::InvalidLessonSeverity { name, value, .. }
                if name == "test_lesson" && value == "critical"
            )),
            "expected invalid severity error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_valid_lesson_passes() {
        let src = r#"
            lesson "good_lesson" {
                context: <<Found a bug>>
                rule: <<Fix the bug>>
                severity: warning
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // ── Compliance tests ──────────────────────────────────────────

    #[test]
    fn checker_valid_compliance() {
        let src = r#"
            compliance "payment_processing" {
                risk: high
                frameworks: [pci_dss, gdpr]
                audit: full
                retention: "7y"
                review_interval: "90d"
                roles {
                    approver: "finance_lead"
                    executor: "payment_agent"
                    auditor: "compliance_team"
                }
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn checker_invalid_compliance_risk() {
        let src = r#"
            compliance "bad_risk" {
                risk: extreme
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(
            matches!(&errors[0], CheckError::InvalidComplianceRisk { name, value, .. }
                if name == "bad_risk" && value == "extreme"
            ),
            "expected InvalidComplianceRisk, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn checker_invalid_compliance_audit() {
        let src = r#"
            compliance "bad_audit" {
                audit: verbose
            }
        "#;
        let errors = check_src(src);
        assert_eq!(errors.len(), 1, "expected 1 error, got: {:?}", errors);
        assert!(
            matches!(&errors[0], CheckError::InvalidComplianceAudit { name, value, .. }
                if name == "bad_audit" && value == "verbose"
            ),
            "expected InvalidComplianceAudit, got: {:?}",
            errors[0]
        );
    }

    #[test]
    fn checker_unknown_compliance_ref() {
        let src = r#"
            agent @processor {
                permits: [^llm.query]
                tools: [#greet]
                compliance: "nonexistent_profile"
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::UnknownCompliance { name, .. }
                if name == "nonexistent_profile"
            )),
            "expected UnknownCompliance error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_compliance_sod_conflict() {
        let src = r#"
            compliance "conflicting_roles" {
                risk: high
                audit: full
                roles {
                    approver: "same_agent"
                    executor: "same_agent"
                }
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(|e| matches!(
                e,
                CheckError::ComplianceSodConflict { name, agent, .. }
                if name == "conflicting_roles" && agent == "same_agent"
            )),
            "expected ComplianceSodConflict error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_federation_valid() {
        let src = r#"
            federation {
                "https://agents.example.com" trust: [^llm.query]
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn checker_federation_invalid_url() {
        let src = r#"
            federation {
                "ftp://invalid.com" trust: [^llm.query]
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(|e| matches!(e, CheckError::InvalidFederationUrl { url, .. } if url == "ftp://invalid.com")),
            "expected InvalidFederationUrl error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_federation_empty_trust() {
        let src = r#"
            federation {
                "https://agents.example.com" trust: []
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, CheckError::EmptyFederationTrust { .. })),
            "expected EmptyFederationTrust error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_agent_invalid_endpoint() {
        let src = r#"
            agent @bot {
                permits: []
                tools: []
                endpoint: "not-a-url"
            }
        "#;
        let errors = check_src(src);
        assert!(
            errors.iter().any(|e| matches!(e, CheckError::InvalidAgentEndpoint { agent, url, .. } if agent == "bot" && url == "not-a-url")),
            "expected InvalidAgentEndpoint error, got: {:?}",
            errors
        );
    }

    #[test]
    fn checker_agent_valid_endpoint() {
        let src = r#"
            agent @bot {
                permits: []
                tools: []
                endpoint: "https://remote.example.com/agent"
            }
        "#;
        let errors = check_src(src);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }
}
