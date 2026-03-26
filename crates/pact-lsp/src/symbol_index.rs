// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-12

//! Symbol index for the PACT language server.
//!
//! Collects all declaration (definition) sites and reference sites from a
//! parsed [`Program`], enabling go-to-definition and find-references queries.
//!
//! The index is rebuilt on every document change and is cheap to construct
//! because it performs a single AST walk via the [`Visitor`] trait.

use pact_core::ast::expr::{Expr, ExprKind};
use pact_core::ast::stmt::{Decl, DeclKind, Program};
use pact_core::ast::types::{TypeExpr, TypeExprKind};
use pact_core::ast::visit::Visitor;
use pact_core::span::Span;

/// The category of a symbol in the PACT language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// An agent declared with `agent @name { ... }` or `agent_bundle @name { ... }`.
    Agent,
    /// A tool declared with `tool #name { ... }`.
    Tool,
    /// A skill declared with `skill $name { ... }`.
    Skill,
    /// A template declared with `template %name { ... }`.
    Template,
    /// A directive declared with `directive %name { ... }`.
    Directive,
    /// A flow declared with `flow name(...) { ... }`.
    Flow,
    /// A schema declared with `schema Name { ... }`.
    Schema,
    /// A type alias declared with `type Name = ...`.
    TypeAlias,
    /// A permission tree declared with `permit_tree { ... }`.
    PermitTree,
    /// A lesson declared with `lesson "name" { ... }`.
    Lesson,
    /// A compliance profile declared with `compliance "name" { ... }`.
    Compliance,
}

/// A recorded location in the source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Byte offset where the symbol's span begins (inclusive).
    pub start: usize,
    /// Byte offset where the symbol's span ends (exclusive).
    pub end: usize,
}

impl Location {
    /// Create a location from a [`Span`].
    pub fn from_span(span: &Span) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

/// A definition site for a named symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The symbol's bare name (without sigil).
    pub name: String,
    /// What kind of declaration this is.
    pub kind: SymbolKind,
    /// Byte range of the entire declaration.
    pub location: Location,
}

/// A reference site where a symbol is used (not defined).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The symbol's bare name (without sigil).
    pub name: String,
    /// What kind of symbol is being referenced.
    pub kind: SymbolKind,
    /// Byte range of the reference expression.
    pub location: Location,
}

/// An index of all definitions and references in a single PACT document.
///
/// Built by walking the parsed AST once. Supports efficient lookup of
/// definitions by name+kind and references by name+kind.
#[derive(Debug, Default)]
pub struct SymbolIndex {
    /// All definition sites found in the document.
    pub definitions: Vec<Definition>,
    /// All reference sites found in the document.
    pub references: Vec<Reference>,
}

impl SymbolIndex {
    /// Build a symbol index from a parsed program.
    ///
    /// Walks the entire AST once, collecting both definition sites and
    /// reference sites for all named symbols.
    pub fn build(program: &Program) -> Self {
        let mut collector = SymbolCollector::default();
        collector.visit_program(program);
        collector.index
    }

    /// Find the definition of a symbol at the given byte offset.
    ///
    /// Searches both definitions (to handle go-to-definition on the
    /// declaration itself) and references, then looks up the matching
    /// definition.
    pub fn definition_at(&self, offset: usize) -> Option<&Definition> {
        // First check if cursor is on a definition itself
        for def in &self.definitions {
            if offset >= def.location.start && offset < def.location.end {
                return Some(def);
            }
        }

        // Then check if cursor is on a reference and find its definition
        for reference in &self.references {
            if offset >= reference.location.start && offset < reference.location.end {
                return self.find_definition(&reference.name, reference.kind);
            }
        }

        None
    }

    /// Find the definition of a symbol by name and kind.
    ///
    /// For `Schema` references, also checks `TypeAlias` definitions as a
    /// fallback, since type annotations (e.g. `x :: Report`) are recorded
    /// as schema references but may resolve to a type alias.
    pub fn find_definition(&self, name: &str, kind: SymbolKind) -> Option<&Definition> {
        self.definitions
            .iter()
            .find(|d| d.name == name && d.kind == kind)
            .or_else(|| {
                // A type reference recorded as Schema might actually be a TypeAlias
                if kind == SymbolKind::Schema {
                    self.definitions
                        .iter()
                        .find(|d| d.name == name && d.kind == SymbolKind::TypeAlias)
                } else {
                    None
                }
            })
    }

    /// Find all references to the symbol at the given byte offset.
    ///
    /// If the cursor is on a definition, finds all references to that symbol.
    /// If the cursor is on a reference, finds all references to the same symbol.
    /// When `include_definition` is true, the definition site itself is included.
    pub fn references_at(&self, offset: usize, include_definition: bool) -> Vec<Location> {
        // Determine what symbol we're on
        let (name, kind) = if let Some(def) = self
            .definitions
            .iter()
            .find(|d| offset >= d.location.start && offset < d.location.end)
        {
            (def.name.as_str(), def.kind)
        } else if let Some(reference) = self
            .references
            .iter()
            .find(|r| offset >= r.location.start && offset < r.location.end)
        {
            (reference.name.as_str(), reference.kind)
        } else {
            return Vec::new();
        };

        let mut locations: Vec<Location> = self
            .references
            .iter()
            .filter(|r| r.name == name && r.kind == kind)
            .map(|r| r.location.clone())
            .collect();

        if include_definition {
            if let Some(def) = self.find_definition(name, kind) {
                locations.insert(0, def.location.clone());
            }
        }

        locations
    }
}

/// Internal visitor that walks the AST and collects symbols.
#[derive(Default)]
struct SymbolCollector {
    /// The index being built.
    index: SymbolIndex,
}

impl SymbolCollector {
    /// Record a definition.
    fn add_definition(&mut self, name: String, kind: SymbolKind, span: &Span) {
        self.index.definitions.push(Definition {
            name,
            kind,
            location: Location::from_span(span),
        });
    }

    /// Record a reference.
    fn add_reference(&mut self, name: String, kind: SymbolKind, span: &Span) {
        self.index.references.push(Reference {
            name,
            kind,
            location: Location::from_span(span),
        });
    }

    /// Walk a type expression looking for references to schemas or type aliases.
    fn visit_type_expr(&mut self, ty: &TypeExpr) {
        match &ty.kind {
            TypeExprKind::Named(name) => {
                // Built-in types are not references to user-defined symbols
                let builtins = ["String", "Int", "Float", "Bool", "List", "Map"];
                if !builtins.contains(&name.as_str()) {
                    // Could be a schema or type alias — record as Schema first,
                    // the consumer will check both.
                    self.add_reference(name.clone(), SymbolKind::Schema, &ty.span);
                }
            }
            TypeExprKind::Generic { name, args } => {
                let builtins = ["String", "Int", "Float", "Bool", "List", "Map"];
                if !builtins.contains(&name.as_str()) {
                    self.add_reference(name.clone(), SymbolKind::Schema, &ty.span);
                }
                for arg in args {
                    self.visit_type_expr(arg);
                }
            }
            TypeExprKind::Optional(inner) => {
                self.visit_type_expr(inner);
            }
        }
    }
}

impl Visitor for SymbolCollector {
    fn visit_decl(&mut self, decl: &Decl) {
        match &decl.kind {
            DeclKind::Agent(a) => {
                self.add_definition(a.name.clone(), SymbolKind::Agent, &decl.span);
            }
            DeclKind::AgentBundle(ab) => {
                self.add_definition(ab.name.clone(), SymbolKind::Agent, &decl.span);
            }
            DeclKind::Tool(t) => {
                self.add_definition(t.name.clone(), SymbolKind::Tool, &decl.span);
                // Visit type expressions in params and return type
                for p in &t.params {
                    if let Some(ty) = &p.ty {
                        self.visit_type_expr(ty);
                    }
                }
                if let Some(ty) = &t.return_type {
                    self.visit_type_expr(ty);
                }
                // Record template reference from output field
                if let Some(output) = &t.output {
                    // We don't have a span for the output field itself, use decl span
                    self.index.references.push(Reference {
                        name: output.clone(),
                        kind: SymbolKind::Template,
                        location: Location::from_span(&decl.span),
                    });
                }
                // Record directive references
                for d in &t.directives {
                    self.index.references.push(Reference {
                        name: d.clone(),
                        kind: SymbolKind::Directive,
                        location: Location::from_span(&decl.span),
                    });
                }
            }
            DeclKind::Skill(s) => {
                self.add_definition(s.name.clone(), SymbolKind::Skill, &decl.span);
                for p in &s.params {
                    if let Some(ty) = &p.ty {
                        self.visit_type_expr(ty);
                    }
                }
                if let Some(ty) = &s.return_type {
                    self.visit_type_expr(ty);
                }
            }
            DeclKind::Template(t) => {
                self.add_definition(t.name.clone(), SymbolKind::Template, &decl.span);
            }
            DeclKind::Directive(d) => {
                self.add_definition(d.name.clone(), SymbolKind::Directive, &decl.span);
            }
            DeclKind::Flow(f) => {
                self.add_definition(f.name.clone(), SymbolKind::Flow, &decl.span);
                for p in &f.params {
                    if let Some(ty) = &p.ty {
                        self.visit_type_expr(ty);
                    }
                }
                if let Some(ty) = &f.return_type {
                    self.visit_type_expr(ty);
                }
            }
            DeclKind::Schema(s) => {
                self.add_definition(s.name.clone(), SymbolKind::Schema, &decl.span);
                for field in &s.fields {
                    self.visit_type_expr(&field.ty);
                }
            }
            DeclKind::TypeAlias(t) => {
                self.add_definition(t.name.clone(), SymbolKind::TypeAlias, &decl.span);
            }
            DeclKind::PermitTree(_) => {
                self.add_definition("permit_tree".into(), SymbolKind::PermitTree, &decl.span);
            }
            DeclKind::Lesson(l) => {
                self.add_definition(l.name.clone(), SymbolKind::Lesson, &decl.span);
            }
            DeclKind::Compliance(c) => {
                self.add_definition(c.name.clone(), SymbolKind::Compliance, &decl.span);
            }
            DeclKind::Federation(_) => {}
            DeclKind::Test(_) | DeclKind::Import(_) | DeclKind::Connect(_) => {}
        }

        // Recurse into child expressions via the default Visitor implementation
        // but we need to call it ourselves since we overrode visit_decl
        match &decl.kind {
            DeclKind::Agent(a) => {
                for p in &a.permits {
                    self.visit_expr(p);
                }
                for t in &a.tools {
                    self.visit_expr(t);
                }
                for s in &a.skills {
                    self.visit_expr(s);
                }
                if let Some(m) = &a.model {
                    self.visit_expr(m);
                }
                if let Some(p) = &a.prompt {
                    self.visit_expr(p);
                }
                for m in &a.memory {
                    self.visit_expr(m);
                }
            }
            DeclKind::AgentBundle(ab) => {
                for a in &ab.agents {
                    self.visit_expr(a);
                }
                if let Some(f) = &ab.fallbacks {
                    self.visit_expr(f);
                }
            }
            DeclKind::Flow(f) => {
                for expr in &f.body {
                    self.visit_expr(expr);
                }
            }
            DeclKind::Tool(t) => {
                self.visit_expr(&t.description);
                for r in &t.requires {
                    self.visit_expr(r);
                }
            }
            DeclKind::Skill(s) => {
                self.visit_expr(&s.description);
                for t in &s.tools {
                    self.visit_expr(t);
                }
                if let Some(st) = &s.strategy {
                    self.visit_expr(st);
                }
            }
            DeclKind::Test(t) => {
                for expr in &t.body {
                    self.visit_expr(expr);
                }
            }
            _ => {}
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::AgentRef(name) => {
                self.add_reference(name.clone(), SymbolKind::Agent, &expr.span);
            }
            ExprKind::ToolRef(name) => {
                self.add_reference(name.clone(), SymbolKind::Tool, &expr.span);
            }
            ExprKind::SkillRef(name) => {
                self.add_reference(name.clone(), SymbolKind::Skill, &expr.span);
            }
            ExprKind::TemplateRef(name) => {
                self.add_reference(name.clone(), SymbolKind::Template, &expr.span);
            }
            ExprKind::RunFlow { flow_name, args } => {
                self.add_reference(flow_name.clone(), SymbolKind::Flow, &expr.span);
                for arg in args {
                    self.visit_expr(arg);
                }
                return; // Already recursed into args
            }
            _ => {}
        }

        // Default recursion for all other expression kinds
        match &expr.kind {
            ExprKind::AgentDispatch { agent, tool, args } => {
                self.visit_expr(agent);
                self.visit_expr(tool);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::Pipeline { left, right } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::FallbackChain { primary, fallback } => {
                self.visit_expr(primary);
                self.visit_expr(fallback);
            }
            ExprKind::Parallel(exprs) => {
                for e in exprs {
                    self.visit_expr(e);
                }
            }
            ExprKind::Match { subject, arms } => {
                self.visit_expr(subject);
                for arm in arms {
                    self.visit_match_arm(arm);
                }
            }
            ExprKind::FieldAccess { object, .. } => {
                self.visit_expr(object);
            }
            ExprKind::FuncCall { callee, args } => {
                self.visit_expr(callee);
                for arg in args {
                    self.visit_expr(arg);
                }
            }
            ExprKind::BinOp { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::Return(e) | ExprKind::Fail(e) | ExprKind::Assert(e) => {
                self.visit_expr(e);
            }
            ExprKind::Assign { value, .. } => {
                self.visit_expr(value);
            }
            ExprKind::Record(exprs) => {
                for e in exprs {
                    self.visit_expr(e);
                }
            }
            ExprKind::Typed { expr, ty } => {
                self.visit_expr(expr);
                self.visit_type_expr(ty);
            }
            ExprKind::ListLit(items) => {
                for item in items {
                    self.visit_expr(item);
                }
            }
            ExprKind::RecordFields(fields) => {
                for (_, expr) in fields {
                    self.visit_expr(expr);
                }
            }
            ExprKind::OnError { body, fallback } => {
                self.visit_expr(body);
                self.visit_expr(fallback);
            }
            // Leaf nodes and RunFlow (already handled above)
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceId;

    /// Helper: parse source text and build a symbol index.
    fn index_from_source(src: &str) -> SymbolIndex {
        let source_id = SourceId(0);
        let tokens = Lexer::new(src, source_id).lex().expect("lex failed");
        let mut parser = Parser::new(&tokens);
        let (program, _) = parser.parse_collecting_errors();
        SymbolIndex::build(&program)
    }

    #[test]
    fn agent_definition_is_collected() {
        let idx = index_from_source("agent @greeter { permits: [] tools: [] }");
        assert_eq!(idx.definitions.len(), 1);
        assert_eq!(idx.definitions[0].name, "greeter");
        assert_eq!(idx.definitions[0].kind, SymbolKind::Agent);
    }

    #[test]
    fn tool_definition_is_collected() {
        let idx = index_from_source(
            r#"tool #web_search { description: <<"search">> requires: [^net.read] params { query :: String } returns :: String }"#,
        );
        let tool_def = idx.definitions.iter().find(|d| d.kind == SymbolKind::Tool);
        assert!(tool_def.is_some());
        assert_eq!(tool_def.unwrap().name, "web_search");
    }

    #[test]
    fn skill_definition_is_collected() {
        let idx = index_from_source(
            r#"skill $research { description: <<"research things">> tools: [#web_search] params { topic :: String } returns :: String }"#,
        );
        let skill_def = idx.definitions.iter().find(|d| d.kind == SymbolKind::Skill);
        assert!(skill_def.is_some());
        assert_eq!(skill_def.unwrap().name, "research");
    }

    #[test]
    fn flow_definition_is_collected() {
        let idx = index_from_source("flow greet(name :: String) -> String { return name }");
        let flow_def = idx.definitions.iter().find(|d| d.kind == SymbolKind::Flow);
        assert!(flow_def.is_some());
        assert_eq!(flow_def.unwrap().name, "greet");
    }

    #[test]
    fn schema_definition_is_collected() {
        let idx = index_from_source("schema Report { title :: String }");
        let schema_def = idx
            .definitions
            .iter()
            .find(|d| d.kind == SymbolKind::Schema);
        assert!(schema_def.is_some());
        assert_eq!(schema_def.unwrap().name, "Report");
    }

    #[test]
    fn template_and_directive_definitions_are_collected() {
        let idx = index_from_source(
            r#"template %report_card { GRADE :: String <<"A-F grade">> }
directive %style { <<"Use clean style">> }"#,
        );
        let tmpl = idx
            .definitions
            .iter()
            .find(|d| d.kind == SymbolKind::Template);
        assert!(tmpl.is_some());
        assert_eq!(tmpl.unwrap().name, "report_card");

        let dir = idx
            .definitions
            .iter()
            .find(|d| d.kind == SymbolKind::Directive);
        assert!(dir.is_some());
        assert_eq!(dir.unwrap().name, "style");
    }

    #[test]
    fn agent_ref_in_dispatch_is_a_reference() {
        let src = r#"agent @helper { permits: [^llm.query] tools: [#greet] }
tool #greet { description: <<"greet">>, requires: [^llm.query], params { name :: String }, returns :: String }
flow main() { @helper -> #greet("world") }"#;
        let idx = index_from_source(src);

        // The flow body should contain an @helper reference
        let agent_refs: Vec<_> = idx
            .references
            .iter()
            .filter(|r| r.kind == SymbolKind::Agent && r.name == "helper")
            .collect();
        assert!(
            !agent_refs.is_empty(),
            "expected at least one @helper reference"
        );
    }

    #[test]
    fn tool_ref_in_agent_is_a_reference() {
        let src = r#"tool #greet { description: <<"greet">>, requires: [^llm.query], params { name :: String }, returns :: String }
agent @helper { permits: [^llm.query] tools: [#greet] }"#;
        let idx = index_from_source(src);

        let tool_refs: Vec<_> = idx
            .references
            .iter()
            .filter(|r| r.kind == SymbolKind::Tool && r.name == "greet")
            .collect();
        assert!(
            !tool_refs.is_empty(),
            "expected at least one #greet reference"
        );
    }

    #[test]
    fn run_flow_records_flow_reference() {
        let src = "flow helper() { return 1 }\nflow main() { run helper() }";
        let idx = index_from_source(src);

        let flow_refs: Vec<_> = idx
            .references
            .iter()
            .filter(|r| r.kind == SymbolKind::Flow && r.name == "helper")
            .collect();
        assert!(
            !flow_refs.is_empty(),
            "expected a flow reference for run helper()"
        );
    }

    #[test]
    fn definition_at_returns_correct_definition() {
        let src = "agent @greeter { permits: [] tools: [] }";
        let idx = index_from_source(src);

        // Offset 0 is inside the agent declaration
        let def = idx.definition_at(0);
        assert!(def.is_some());
        assert_eq!(def.unwrap().name, "greeter");
    }

    #[test]
    fn references_at_includes_all_usages() {
        let src = r#"tool #greet { description: <<"hi">> requires: [^llm.query] params { name :: String } returns :: String }
agent @a { permits: [^llm.query] tools: [#greet] }
agent @b { permits: [^llm.query] tools: [#greet] }"#;
        let idx = index_from_source(src);

        // Find the tool definition offset
        let tool_def = idx
            .definitions
            .iter()
            .find(|d| d.kind == SymbolKind::Tool && d.name == "greet")
            .unwrap();
        let locs = idx.references_at(tool_def.location.start, false);
        // At least 2 references from the two agent declarations
        assert!(
            locs.len() >= 2,
            "expected at least 2 references to #greet, got {}",
            locs.len()
        );
    }

    #[test]
    fn type_alias_definition_is_collected() {
        let idx = index_from_source("type Sentiment = Positive | Negative | Neutral");
        let alias = idx
            .definitions
            .iter()
            .find(|d| d.kind == SymbolKind::TypeAlias);
        assert!(alias.is_some());
        assert_eq!(alias.unwrap().name, "Sentiment");
    }

    #[test]
    fn schema_type_ref_in_flow_param_is_reference() {
        let src = "schema Report { title :: String }\nflow analyze(r :: Report) { return r }";
        let idx = index_from_source(src);

        let schema_refs: Vec<_> = idx
            .references
            .iter()
            .filter(|r| r.kind == SymbolKind::Schema && r.name == "Report")
            .collect();
        assert!(
            !schema_refs.is_empty(),
            "expected a schema reference for Report in flow params"
        );
    }
}
