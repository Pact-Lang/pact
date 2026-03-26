// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-05-02

//! Recursive descent parser for the PACT language.
//!
//! The parser consumes a token stream from the [lexer](crate::lexer) and
//! produces an [AST](crate::ast). It uses recursive descent with precedence
//! climbing for expression parsing.
//!
//! # Usage
//!
//! ```
//! use pact_core::lexer::Lexer;
//! use pact_core::parser::Parser;
//! use pact_core::span::{SourceId, SourceMap};
//!
//! let mut sm = SourceMap::new();
//! let id = sm.add("example.pact", "agent @greeter { permits: [^llm.query] tools: [#greet] }");
//! let tokens = Lexer::new(sm.text(id), id).lex().unwrap();
//! let program = Parser::new(&tokens).parse().unwrap();
//! ```

/// Expression parsing (dispatch, pipelines, match, literals, etc.).
pub mod expr;
/// Statement and declaration parsing (agent, flow, schema, etc.).
pub mod stmt;
/// Type annotation parsing.
pub mod types;

use crate::ast::stmt::Program;
use crate::lexer::token::{Token, TokenKind};
use crate::span::Span;

use miette::Diagnostic;
use thiserror::Error;

/// Error produced during parsing.
#[derive(Debug, Error, Diagnostic, Clone)]
pub enum ParseError {
    /// A token was found where a different token was expected.
    #[error("expected {expected}, found {found}")]
    UnexpectedToken {
        /// Description of the expected token kind.
        expected: String,
        /// Description of the token that was actually found.
        found: String,
        /// Location of the unexpected token.
        #[label("here")]
        span: miette::SourceSpan,
    },
}

/// The PACT parser. Converts a token stream into an AST.
pub struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
}

impl<'t> Parser<'t> {
    /// Create a new parser for the given token slice.
    pub fn new(tokens: &'t [Token]) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the entire token stream into a [`Program`].
    pub fn parse(&mut self) -> Result<Program, ParseError> {
        let mut decls = Vec::new();
        while !self.check(&TokenKind::Eof) {
            decls.push(self.parse_decl()?);
        }
        Ok(Program { decls })
    }

    /// Parse, collecting all errors via panic-mode recovery.
    /// Returns partial AST and all errors found.
    pub fn parse_collecting_errors(&mut self) -> (Program, Vec<ParseError>) {
        let mut decls = Vec::new();
        let mut errors = Vec::new();

        while !self.check(&TokenKind::Eof) {
            match self.parse_decl() {
                Ok(decl) => decls.push(decl),
                Err(e) => {
                    errors.push(e);
                    self.synchronize();
                }
            }
        }

        (Program { decls }, errors)
    }

    /// Advance tokens until we find one that can start a new declaration.
    /// This is panic-mode error recovery.
    fn synchronize(&mut self) {
        loop {
            match self.peek_kind() {
                TokenKind::Agent
                | TokenKind::AgentBundle
                | TokenKind::Flow
                | TokenKind::Schema
                | TokenKind::Type
                | TokenKind::PermitTree
                | TokenKind::Test
                | TokenKind::Tool
                | TokenKind::Skill
                | TokenKind::Template
                | TokenKind::Directive
                | TokenKind::Import
                | TokenKind::Run
                | TokenKind::Compliance
                | TokenKind::Eof => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ── Helper methods ─────────────────────────────────────

    /// Return the kind of the current token without consuming it.
    pub(crate) fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    /// Return the kind of the next token (one ahead of current).
    pub(crate) fn peek_next_kind(&self) -> &TokenKind {
        if self.pos + 1 < self.tokens.len() {
            &self.tokens[self.pos + 1].kind
        } else {
            &TokenKind::Eof
        }
    }

    /// Return the span of the current token.
    pub(crate) fn current_span(&self) -> Span {
        self.tokens[self.pos].span
    }

    /// Return the span of the previously consumed token.
    pub(crate) fn previous_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.tokens[0].span
        }
    }

    /// Check if the current token matches the given kind.
    pub(crate) fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    /// Consume the current token and advance to the next one.
    pub(crate) fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// Consume a token if it matches, otherwise return an error.
    pub(crate) fn expect(&mut self, kind: &TokenKind) -> Result<&Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let span = self.current_span();
            Err(ParseError::UnexpectedToken {
                expected: kind.describe().to_string(),
                found: self.peek_kind().describe().to_string(),
                span: (span.start..span.end).into(),
            })
        }
    }

    /// Consume the current token if it's an identifier, returning the name.
    pub(crate) fn expect_ident(&mut self, context: &str) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(name)
            }
            _ => {
                let span = self.current_span();
                Err(ParseError::UnexpectedToken {
                    expected: format!("{context} (identifier)"),
                    found: self.peek_kind().describe().to_string(),
                    span: (span.start..span.end).into(),
                })
            }
        }
    }

    /// Consume the current token if it matches, otherwise do nothing.
    pub(crate) fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Parse a comma-separated list of items until the closing token.
    pub(crate) fn parse_comma_separated<T>(
        &mut self,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, ParseError>,
        closing: &TokenKind,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();
        while !self.check(closing) && !self.check(&TokenKind::Eof) {
            items.push(parse_item(self)?);
            if !self.check(closing) {
                // Allow trailing comma
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::expr::ExprKind;
    use crate::ast::stmt::DeclKind;
    use crate::lexer::Lexer;
    use crate::span::SourceMap;

    /// Helper: parse source into a Program.
    fn parse(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn parse_minimal_agent() {
        let prog = parse("agent @greeter { permits: [^llm.query] tools: [#greet] }");
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Agent(a) => {
                assert_eq!(a.name, "greeter");
                assert_eq!(a.permits.len(), 1);
                assert_eq!(a.tools.len(), 1);
            }
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn parse_agent_with_model_and_prompt() {
        let src = r#"agent @writer {
            permits: [^llm.query]
            tools: [#write]
            model: "gpt-4"
            prompt: <<You are a helpful writer>>
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Agent(a) => {
                assert_eq!(a.name, "writer");
                assert!(a.model.is_some());
                assert!(a.prompt.is_some());
            }
            _ => panic!("expected Agent"),
        }
    }

    #[test]
    fn parse_flow() {
        let src = r#"flow hello(name :: String) -> String {
            result = @greeter -> #greet(name)
            return result
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => {
                assert_eq!(f.name, "hello");
                assert_eq!(f.params.len(), 1);
                assert_eq!(f.params[0].name, "name");
                assert!(f.return_type.is_some());
                assert_eq!(f.body.len(), 2);
            }
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_schema() {
        let src = "schema Report { title :: String, body :: String }";
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Schema(s) => {
                assert_eq!(s.name, "Report");
                assert_eq!(s.fields.len(), 2);
            }
            _ => panic!("expected Schema"),
        }
    }

    #[test]
    fn parse_type_alias() {
        let src = "type Status = Success | Failure | Pending";
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::TypeAlias(t) => {
                assert_eq!(t.name, "Status");
                assert_eq!(t.variants, vec!["Success", "Failure", "Pending"]);
            }
            _ => panic!("expected TypeAlias"),
        }
    }

    #[test]
    fn parse_permit_tree() {
        let src = r#"permit_tree {
            ^net {
                ^net.read
                ^net.write
            }
            ^llm {
                ^llm.query
            }
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::PermitTree(pt) => {
                assert_eq!(pt.nodes.len(), 2);
                assert_eq!(pt.nodes[0].path, vec!["net"]);
                assert_eq!(pt.nodes[0].children.len(), 2);
                assert_eq!(pt.nodes[1].path, vec!["llm"]);
                assert_eq!(pt.nodes[1].children.len(), 1);
            }
            _ => panic!("expected PermitTree"),
        }
    }

    #[test]
    fn parse_test_decl() {
        let src = r#"test "hello works" {
            result = @greeter -> #greet("world")
            assert result == "greet_result"
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Test(t) => {
                assert_eq!(t.description, "hello works");
                assert_eq!(t.body.len(), 2);
            }
            _ => panic!("expected Test"),
        }
    }

    #[test]
    fn parse_pipeline() {
        let src = "flow pipe() { result = a |> b |> c return result }";
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => {
                // The first statement should be an assignment with a pipeline
                match &f.body[0].kind {
                    ExprKind::Assign { value, .. } => {
                        assert!(matches!(value.kind, ExprKind::Pipeline { .. }));
                    }
                    _ => panic!("expected assignment"),
                }
            }
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_fallback_chain() {
        let src = "flow fb() { result = @a -> #t() ?> @b -> #t() return result }";
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => {
                    assert!(matches!(value.kind, ExprKind::FallbackChain { .. }));
                }
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_match_expr() {
        let src = r#"flow m(x :: String) -> String {
            result = match x {
                "a" => "alpha",
                "b" => "beta",
                _ => "unknown"
            }
            return result
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => match &value.kind {
                    ExprKind::Match { arms, .. } => {
                        assert_eq!(arms.len(), 3);
                    }
                    _ => panic!("expected Match"),
                },
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_agent_bundle() {
        let src = r#"agent_bundle @research_team {
            agents: [@researcher, @writer]
            fallbacks: @researcher ?> @writer
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::AgentBundle(ab) => {
                assert_eq!(ab.name, "research_team");
                assert_eq!(ab.agents.len(), 2);
                assert!(ab.fallbacks.is_some());
            }
            _ => panic!("expected AgentBundle"),
        }
    }

    #[test]
    fn parse_error_location() {
        let src = "agent { }"; // missing @name
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let result = Parser::new(&tokens).parse();
        assert!(result.is_err());
    }

    #[test]
    fn parse_recovers_multiple_errors() {
        // "agent { }" is missing @name → error
        // "flow hello() { return 1 }" is valid → parsed successfully
        let src = r#"agent { } flow hello() { return 1 }"#;
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let (program, errors) = Parser::new(&tokens).parse_collecting_errors();
        assert_eq!(errors.len(), 1, "expected 1 parse error");
        assert_eq!(
            program.decls.len(),
            1,
            "expected 1 successfully parsed decl"
        );
        match &program.decls[0].kind {
            DeclKind::Flow(f) => assert_eq!(f.name, "hello"),
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_backward_compat() {
        let src = "agent { }"; // missing @name
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        let result = Parser::new(&tokens).parse();
        assert!(
            result.is_err(),
            "parse() should still return Err on first error"
        );
    }

    #[test]
    fn parse_complete_program() {
        let src = r#"
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
            }

            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }

            test "hello works" {
                result = @greeter -> #greet("world")
                assert result == "greet_result"
            }
        "#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 3);
    }

    #[test]
    fn parse_record_literal() {
        let src = r#"flow make() {
            result = { title: "Hello", count: 42 }
            return result
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => match &value.kind {
                    ExprKind::RecordFields(fields) => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].0, "title");
                        assert_eq!(fields[1].0, "count");
                    }
                    _ => panic!("expected RecordFields, got {:?}", value.kind),
                },
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_record_literal_with_variable() {
        let src = r#"flow make(summary :: String) {
            result = { title: "Hello", body: summary, count: 42 }
            return result
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => match &value.kind {
                    ExprKind::RecordFields(fields) => {
                        assert_eq!(fields.len(), 3);
                        assert_eq!(fields[0].0, "title");
                        assert_eq!(fields[1].0, "body");
                        assert!(
                            matches!(fields[1].1.kind, ExprKind::Ident(ref n) if n == "summary")
                        );
                        assert_eq!(fields[2].0, "count");
                    }
                    _ => panic!("expected RecordFields"),
                },
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_match_with_wildcard() {
        use crate::ast::expr::MatchPattern;
        let src = r#"flow m(x :: String) -> String {
            result = match x {
                "a" => "alpha",
                _ => "default"
            }
            return result
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => match &value.kind {
                    ExprKind::Match { arms, .. } => {
                        assert_eq!(arms.len(), 2);
                        assert!(matches!(arms[1].pattern, MatchPattern::Wildcard));
                    }
                    _ => panic!("expected Match"),
                },
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_connect_block() {
        let src = r#"connect {
            slack "stdio slack-mcp-server"
            github "stdio github-mcp-server --token abc"
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Connect(c) => {
                assert_eq!(c.servers.len(), 2);
                assert_eq!(c.servers[0].name, "slack");
                assert_eq!(c.servers[0].transport, "stdio slack-mcp-server");
                assert_eq!(c.servers[1].name, "github");
                assert_eq!(
                    c.servers[1].transport,
                    "stdio github-mcp-server --token abc"
                );
            }
            _ => panic!("expected Connect"),
        }
    }

    #[test]
    fn parse_connect_with_sse() {
        let src = r#"connect {
            sentry "sse https://sentry.internal/mcp"
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Connect(c) => {
                assert_eq!(c.servers.len(), 1);
                assert_eq!(c.servers[0].name, "sentry");
                assert_eq!(c.servers[0].transport, "sse https://sentry.internal/mcp");
            }
            _ => panic!("expected Connect"),
        }
    }

    #[test]
    fn parse_tool_mcp_shorthand() {
        let src = "tool #create_issue = mcp github/create_issue";
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Tool(t) => {
                assert_eq!(t.name, "create_issue");
                assert_eq!(
                    t.mcp_import,
                    Some(("github".to_string(), "create_issue".to_string()))
                );
                assert_eq!(t.handler, Some("mcp github/create_issue".to_string()));
                assert!(t.params.is_empty());
            }
            _ => panic!("expected Tool"),
        }
    }

    #[test]
    fn parse_tool_mcp_handler() {
        let src = r#"tool #post_alert {
            description: <<Post alert.>>
            requires: [^mcp.slack]
            handler: "mcp slack/send_message"
            params { channel :: String, text :: String }
            returns :: String
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Tool(t) => {
                assert_eq!(t.name, "post_alert");
                assert_eq!(t.handler, Some("mcp slack/send_message".to_string()));
                assert!(t.mcp_import.is_none());
                assert_eq!(t.params.len(), 2);
            }
            _ => panic!("expected Tool"),
        }
    }

    #[test]
    fn parse_match_with_type_pattern() {
        use crate::ast::expr::MatchPattern;
        let src = r#"flow classify(x :: Any) -> String {
            result = match x {
                "hello" => "specific",
                other => "catch-all"
            }
            return result
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Flow(f) => match &f.body[0].kind {
                ExprKind::Assign { value, .. } => match &value.kind {
                    ExprKind::Match { arms, .. } => {
                        assert_eq!(arms.len(), 2);
                        assert!(matches!(&arms[1].pattern, MatchPattern::Ident(n) if n == "other"));
                    }
                    _ => panic!("expected Match"),
                },
                _ => panic!("expected assignment"),
            },
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn parse_lesson_basic() {
        let src = r#"lesson "cache_invalidation" {
            context: <<After deploy, cache was stale for 10 minutes>>
            rule: <<Always invalidate CDN cache after deploy>>
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Lesson(l) => {
                assert_eq!(l.name, "cache_invalidation");
                assert_eq!(
                    l.context.as_deref(),
                    Some("After deploy, cache was stale for 10 minutes")
                );
                assert_eq!(
                    l.rule.as_deref(),
                    Some("Always invalidate CDN cache after deploy")
                );
                assert!(l.severity.is_none());
            }
            _ => panic!("expected Lesson"),
        }
    }

    #[test]
    fn parse_lesson_with_severity() {
        let src = r#"lesson "rate_limit" {
            context: <<API returned 429 during peak hours>>
            rule: <<Add exponential backoff to all API calls>>
            severity: warning
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Lesson(l) => {
                assert_eq!(l.name, "rate_limit");
                assert!(l.context.is_some());
                assert!(l.rule.is_some());
                assert_eq!(l.severity.as_deref(), Some("warning"));
            }
            _ => panic!("expected Lesson"),
        }
    }

    #[test]
    fn parse_compliance_basic() {
        let src = r#"compliance "payment_processing" {
            risk: high
            frameworks: [pci_dss, gdpr, sox]
            audit: full
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Compliance(c) => {
                assert_eq!(c.name, "payment_processing");
                assert_eq!(c.risk.as_deref(), Some("high"));
                assert_eq!(c.frameworks, vec!["pci_dss", "gdpr", "sox"]);
                assert_eq!(c.audit.as_deref(), Some("full"));
            }
            _ => panic!("expected Compliance"),
        }
    }

    #[test]
    fn parse_compliance_with_roles() {
        let src = r#"compliance "finance_ops" {
            risk: critical
            frameworks: [sox]
            audit: full
            retention: "7y"
            review_interval: "90d"
            roles {
                approver: "finance_lead"
                executor: "payment_agent"
                auditor: "compliance_team"
            }
        }"#;
        let prog = parse(src);
        match &prog.decls[0].kind {
            DeclKind::Compliance(c) => {
                assert_eq!(c.name, "finance_ops");
                assert_eq!(c.risk.as_deref(), Some("critical"));
                assert_eq!(c.retention.as_deref(), Some("7y"));
                assert_eq!(c.review_interval.as_deref(), Some("90d"));
                assert_eq!(c.roles.len(), 3);
                assert_eq!(c.roles[0].role, "approver");
                assert_eq!(c.roles[0].assignee, "finance_lead");
                assert_eq!(c.roles[1].role, "executor");
                assert_eq!(c.roles[1].assignee, "payment_agent");
                assert_eq!(c.roles[2].role, "auditor");
                assert_eq!(c.roles[2].assignee, "compliance_team");
            }
            _ => panic!("expected Compliance"),
        }
    }

    #[test]
    fn parse_compliance_minimal() {
        let src = r#"compliance "empty_profile" {
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 1);
        match &prog.decls[0].kind {
            DeclKind::Compliance(c) => {
                assert_eq!(c.name, "empty_profile");
                assert!(c.risk.is_none());
                assert!(c.frameworks.is_empty());
                assert!(c.audit.is_none());
                assert!(c.retention.is_none());
                assert!(c.review_interval.is_none());
                assert!(c.roles.is_empty());
            }
            _ => panic!("expected Compliance"),
        }
    }

    #[test]
    fn parse_agent_with_compliance() {
        let src = r#"compliance "pci" {
            risk: high
            audit: full
        }
        agent @processor {
            permits: [^llm.query]
            tools: [#greet]
            compliance: "pci"
        }"#;
        let prog = parse(src);
        assert_eq!(prog.decls.len(), 2);
        match &prog.decls[1].kind {
            DeclKind::Agent(a) => {
                assert_eq!(a.name, "processor");
                assert_eq!(a.compliance.as_deref(), Some("pci"));
            }
            _ => panic!("expected Agent"),
        }
    }
}
