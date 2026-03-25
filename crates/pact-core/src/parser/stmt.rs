// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-05-22

//! Top-level declaration parsing.
//!
//! Handles parsing of `agent`, `agent_bundle`, `flow`, `schema`, `type`,
//! `permit_tree`, and `test` declarations.

use super::{ParseError, Parser};
use crate::ast::stmt::*;
use crate::lexer::token::TokenKind;

impl<'t> Parser<'t> {
    /// Parse a top-level declaration based on the leading keyword.
    pub(crate) fn parse_decl(&mut self) -> Result<Decl, ParseError> {
        let span = self.current_span();
        match self.peek_kind().clone() {
            TokenKind::Agent => {
                self.advance();
                // Check if this is `agent_bundle` — no, agent_bundle is its own keyword
                let decl = self.parse_agent_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Agent(decl),
                })
            }
            TokenKind::AgentBundle => {
                self.advance();
                let decl = self.parse_agent_bundle_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::AgentBundle(decl),
                })
            }
            TokenKind::Flow => {
                self.advance();
                let decl = self.parse_flow_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Flow(decl),
                })
            }
            TokenKind::Schema => {
                self.advance();
                let decl = self.parse_schema_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Schema(decl),
                })
            }
            TokenKind::Type => {
                self.advance();
                let decl = self.parse_type_alias_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::TypeAlias(decl),
                })
            }
            TokenKind::PermitTree => {
                self.advance();
                let decl = self.parse_permit_tree_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::PermitTree(decl),
                })
            }
            TokenKind::Tool => {
                self.advance();
                let decl = self.parse_tool_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Tool(decl),
                })
            }
            TokenKind::Skill => {
                self.advance();
                let decl = self.parse_skill_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Skill(decl),
                })
            }
            TokenKind::Test => {
                self.advance();
                let decl = self.parse_test_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Test(decl),
                })
            }
            TokenKind::Template => {
                self.advance();
                let decl = self.parse_template_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Template(decl),
                })
            }
            TokenKind::Directive => {
                self.advance();
                let decl = self.parse_directive_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Directive(decl),
                })
            }
            TokenKind::Import => {
                self.advance();
                let decl = self.parse_import_decl(span)?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Import(decl),
                })
            }
            TokenKind::Connect => {
                self.advance();
                let decl = self.parse_connect_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Connect(decl),
                })
            }
            TokenKind::Lesson => {
                self.advance();
                let decl = self.parse_lesson_decl()?;
                Ok(Decl {
                    span: span.merge(self.previous_span()),
                    kind: DeclKind::Lesson(decl),
                })
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "declaration (agent, skill, tool, flow, schema, type, permit_tree, template, directive, test, import, connect, lesson)"
                    .to_string(),
                found: self.peek_kind().describe().to_string(),
                span: (span.start..span.end).into(),
            }),
        }
    }

    /// Parse `agent @name { ... }`.
    fn parse_agent_decl(&mut self) -> Result<AgentDecl, ParseError> {
        self.expect(&TokenKind::At)?;
        let name = self.expect_ident("agent name")?;
        self.expect(&TokenKind::LBrace)?;

        let mut permits = Vec::new();
        let mut tools = Vec::new();
        let mut skills = Vec::new();
        let mut model = None;
        let mut prompt = None;
        let mut memory = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Permits => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    permits =
                        self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Tools => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    tools = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Skills => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    skills =
                        self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Model => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    model = Some(self.parse_expr()?);
                }
                TokenKind::Prompt => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    prompt = Some(self.parse_expr()?);
                }
                TokenKind::Memory => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    memory =
                        self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "agent field (permits, tools, skills, model, prompt, memory)"
                            .to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        Ok(AgentDecl {
            name,
            permits,
            tools,
            skills,
            model,
            prompt,
            memory,
        })
    }

    /// Parse `agent_bundle @name { ... }`.
    fn parse_agent_bundle_decl(&mut self) -> Result<AgentBundleDecl, ParseError> {
        self.expect(&TokenKind::At)?;
        let name = self.expect_ident("bundle name")?;
        self.expect(&TokenKind::LBrace)?;

        let mut agents = Vec::new();
        let mut fallbacks = None;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Agents => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    agents =
                        self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Fallbacks => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    fallbacks = Some(self.parse_expr()?);
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "bundle field (agents, fallbacks)".to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        Ok(AgentBundleDecl {
            name,
            agents,
            fallbacks,
        })
    }

    /// Parse `flow name(params) -> ReturnType { body }`.
    fn parse_flow_decl(&mut self) -> Result<FlowDecl, ParseError> {
        let name = self.expect_ident("flow name")?;

        // Parameters
        self.expect(&TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(&TokenKind::RParen)?;

        // Optional return type
        let return_type = if self.check(&TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type_expr()?)
        } else {
            None
        };

        // Body
        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_stmt_expr()?);
        }
        self.expect(&TokenKind::RBrace)?;

        Ok(FlowDecl {
            name,
            params,
            return_type,
            body,
        })
    }

    /// Parse a parameter list: `name :: Type, name :: Type`.
    fn parse_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
            let span = self.current_span();
            let name = self.expect_ident("parameter name")?;
            let ty = if self.check(&TokenKind::ColonColon) {
                self.advance();
                Some(self.parse_type_expr()?)
            } else {
                None
            };
            let param_span = span.merge(self.previous_span());
            params.push(Param {
                name,
                ty,
                span: param_span,
            });
            if !self.check(&TokenKind::RParen) {
                self.expect(&TokenKind::Comma)?;
            }
        }
        Ok(params)
    }

    /// Parse `tool #name { ... }` or `tool #name = mcp server/tool`.
    fn parse_tool_decl(&mut self) -> Result<ToolDecl, ParseError> {
        self.expect(&TokenKind::Hash)?;
        let name = self.expect_ident("tool name")?;

        // Check for shorthand: tool #name = mcp server/tool
        if self.check(&TokenKind::Eq) {
            self.advance();
            let kw = self.expect_ident("'mcp' keyword")?;
            if kw != "mcp" {
                let span = self.previous_span();
                return Err(ParseError::UnexpectedToken {
                    expected: "'mcp' keyword after '='".to_string(),
                    found: kw,
                    span: (span.start..span.end).into(),
                });
            }
            let server = self.expect_ident("server name")?;
            self.expect(&TokenKind::Slash)?;
            let tool = self.expect_ident("tool name")?;
            return Ok(ToolDecl {
                name,
                description: crate::ast::expr::Expr {
                    kind: crate::ast::expr::ExprKind::PromptLit(format!(
                        "MCP tool {}/{} (schema resolved at runtime)",
                        server, tool
                    )),
                    span: self.previous_span(),
                },
                requires: vec![],
                handler: Some(format!("mcp {}/{}", server, tool)),
                source: None,
                output: None,
                directives: vec![],
                params: vec![],
                return_type: None,
                retry: None,
                validate: None,
                cache: None,
                mcp_import: Some((server, tool)),
            });
        }

        self.expect(&TokenKind::LBrace)?;

        let mut description = None;
        let mut requires = Vec::new();
        let mut handler = None;
        let mut source = None;
        let mut output = None;
        let mut directives = Vec::new();
        let mut params = Vec::new();
        let mut return_type = None;
        let mut retry = None;
        let mut validate = None;
        let mut cache = None;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Description => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    description = Some(self.parse_expr()?);
                }
                TokenKind::Requires => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    requires =
                        self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Handler => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    match self.peek_kind().clone() {
                        TokenKind::StringLit(s) => {
                            handler = Some(s.clone());
                            self.advance();
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "handler string".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                }
                TokenKind::Source => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    // Parse ^capability.path(arg1, arg2)
                    self.expect(&TokenKind::Caret)?;
                    let mut path = self.expect_ident("capability name")?;
                    while self.check(&TokenKind::Dot) {
                        self.advance();
                        let segment = self.expect_ident("capability segment")?;
                        path = format!("{}.{}", path, segment);
                    }
                    // Parse optional (args)
                    let mut args = Vec::new();
                    if self.check(&TokenKind::LParen) {
                        self.advance();
                        args = self.parse_comma_separated(
                            |p| p.expect_ident("parameter name"),
                            &TokenKind::RParen,
                        )?;
                        self.expect(&TokenKind::RParen)?;
                    }
                    source = Some(SourceSpec {
                        capability: path,
                        args,
                    });
                }
                TokenKind::Output => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::Percent)?;
                    let template_name = self.expect_ident("template name")?;
                    output = Some(template_name);
                }
                TokenKind::Directives => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
                        self.expect(&TokenKind::Percent)?;
                        let dname = self.expect_ident("directive name")?;
                        directives.push(dname);
                        if self.check(&TokenKind::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Params => {
                    self.advance();
                    self.expect(&TokenKind::LBrace)?;
                    params = self.parse_tool_params()?;
                    self.expect(&TokenKind::RBrace)?;
                }
                TokenKind::Returns => {
                    self.advance();
                    self.expect(&TokenKind::ColonColon)?;
                    return_type = Some(self.parse_type_expr()?);
                }
                TokenKind::Retry => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    match self.peek_kind().clone() {
                        TokenKind::IntLit(n) => {
                            self.advance();
                            retry = Some(n as u32);
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "retry count (integer)".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                }
                TokenKind::Validate => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    let mode = self.expect_ident("validation mode (strict or lenient)")?;
                    validate = Some(mode);
                }
                TokenKind::Cache => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    match self.peek_kind().clone() {
                        TokenKind::StringLit(s) => {
                            self.advance();
                            cache = Some(s);
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "cache duration string (e.g. \"24h\")".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "tool field (description, requires, handler, source, output, directives, params, returns, retry, validate, cache)"
                            .to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        // Description is required
        let description = description.ok_or_else(|| {
            let span = self.previous_span();
            ParseError::UnexpectedToken {
                expected: "tool description".to_string(),
                found: "end of tool block".to_string(),
                span: (span.start..span.end).into(),
            }
        })?;

        Ok(ToolDecl {
            name,
            description,
            requires,
            handler,
            source,
            output,
            directives,
            params,
            return_type,
            retry,
            validate,
            cache,
            mcp_import: None,
        })
    }

    /// Parse tool parameter list: `name :: Type` entries separated by commas or newlines.
    fn parse_tool_params(&mut self) -> Result<Vec<Param>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let span = self.current_span();
            let name = self.expect_ident("parameter name")?;
            self.expect(&TokenKind::ColonColon)?;
            let ty = self.parse_type_expr()?;
            let param_span = span.merge(self.previous_span());
            params.push(Param {
                name,
                ty: Some(ty),
                span: param_span,
            });
            // Optional comma
            self.eat(&TokenKind::Comma);
        }
        Ok(params)
    }

    /// Parse `schema Name { field :: Type, ... }`.
    fn parse_schema_decl(&mut self) -> Result<SchemaDecl, ParseError> {
        let name = self.expect_ident("schema name")?;
        self.expect(&TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let span = self.current_span();
            let field_name = self.expect_ident("field name")?;
            self.expect(&TokenKind::ColonColon)?;
            let ty = self.parse_type_expr()?;
            let field_span = span.merge(self.previous_span());
            fields.push(SchemaField {
                name: field_name,
                ty,
                span: field_span,
            });
            // Optional comma
            self.eat(&TokenKind::Comma);
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(SchemaDecl { name, fields })
    }

    /// Parse `type Name = A | B | C`.
    fn parse_type_alias_decl(&mut self) -> Result<TypeAliasDecl, ParseError> {
        let name = self.expect_ident("type name")?;
        self.expect(&TokenKind::Eq)?;

        let mut variants = vec![self.expect_ident("variant name")?];
        while self.check(&TokenKind::Bar) {
            self.advance();
            variants.push(self.expect_ident("variant name")?);
        }

        Ok(TypeAliasDecl { name, variants })
    }

    /// Parse `permit_tree { ... }`.
    fn parse_permit_tree_decl(&mut self) -> Result<PermitTreeDecl, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut nodes = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            nodes.push(self.parse_permit_node()?);
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(PermitTreeDecl { nodes })
    }

    /// Parse a permission node: `^name { children... }` or `^name.sub`.
    fn parse_permit_node(&mut self) -> Result<PermitNode, ParseError> {
        let span = self.current_span();
        self.expect(&TokenKind::Caret)?;
        let mut path = vec![self.expect_ident("permission name")?];
        while self.check(&TokenKind::Dot) {
            self.advance();
            path.push(self.expect_ident("permission segment")?);
        }

        let children = if self.check(&TokenKind::LBrace) {
            self.advance();
            let mut kids = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                kids.push(self.parse_permit_node()?);
                self.eat(&TokenKind::Comma);
            }
            self.expect(&TokenKind::RBrace)?;
            kids
        } else {
            Vec::new()
        };

        let node_span = span.merge(self.previous_span());
        Ok(PermitNode {
            path,
            children,
            span: node_span,
        })
    }

    /// Parse `skill $name { description: <<...>> tools: [...] strategy: <<...>> params { ... } returns :: Type }`.
    fn parse_skill_decl(&mut self) -> Result<SkillDecl, ParseError> {
        self.expect(&TokenKind::Dollar)?;
        let name = self.expect_ident("skill name")?;
        self.expect(&TokenKind::LBrace)?;

        let mut description = None;
        let mut tools = Vec::new();
        let mut strategy = None;
        let mut params = Vec::new();
        let mut return_type = None;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Description => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    description = Some(self.parse_expr()?);
                }
                TokenKind::Tools => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    self.expect(&TokenKind::LBracket)?;
                    tools = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                    self.expect(&TokenKind::RBracket)?;
                }
                TokenKind::Strategy => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    strategy = Some(self.parse_expr()?);
                }
                TokenKind::Params => {
                    self.advance();
                    self.expect(&TokenKind::LBrace)?;
                    params = self.parse_tool_params()?;
                    self.expect(&TokenKind::RBrace)?;
                }
                TokenKind::Returns => {
                    self.advance();
                    self.expect(&TokenKind::ColonColon)?;
                    return_type = Some(self.parse_type_expr()?);
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "skill field (description, tools, strategy, params, returns)"
                            .to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        let description = description.ok_or_else(|| {
            let span = self.previous_span();
            ParseError::UnexpectedToken {
                expected: "skill description".to_string(),
                found: "end of skill block".to_string(),
                span: (span.start..span.end).into(),
            }
        })?;

        Ok(SkillDecl {
            name,
            description,
            tools,
            strategy,
            params,
            return_type,
        })
    }

    /// Parse `import "path/to/file.pact"`.
    fn parse_import_decl(&mut self, span: crate::span::Span) -> Result<ImportDecl, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                let path = s.clone();
                self.advance();
                let decl_span = span.merge(self.previous_span());
                Ok(ImportDecl {
                    path,
                    span: decl_span,
                })
            }
            _ => {
                let cur_span = self.current_span();
                Err(ParseError::UnexpectedToken {
                    expected: "import path string".to_string(),
                    found: self.peek_kind().describe().to_string(),
                    span: (cur_span.start..cur_span.end).into(),
                })
            }
        }
    }

    /// Parse `connect { name "transport", ... }`.
    fn parse_connect_decl(&mut self) -> Result<ConnectDecl, ParseError> {
        self.expect(&TokenKind::LBrace)?;
        let mut servers = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let entry_span = self.current_span();
            let name = self.expect_ident("server name")?;
            let transport = match self.peek_kind().clone() {
                TokenKind::StringLit(s) => {
                    self.advance();
                    s
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "transport string (e.g. \"stdio cmd\" or \"sse url\")".to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            };
            let span = entry_span.merge(self.previous_span());
            servers.push(ConnectEntry {
                name,
                transport,
                span,
            });
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(ConnectDecl { servers })
    }

    /// Parse `test "description" { body }`.
    fn parse_test_decl(&mut self) -> Result<TestDecl, ParseError> {
        let description = match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let span = self.current_span();
                return Err(ParseError::UnexpectedToken {
                    expected: "test description string".to_string(),
                    found: self.peek_kind().describe().to_string(),
                    span: (span.start..span.end).into(),
                });
            }
        };

        self.expect(&TokenKind::LBrace)?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            body.push(self.parse_stmt_expr()?);
        }
        self.expect(&TokenKind::RBrace)?;

        Ok(TestDecl { description, body })
    }

    /// Parse `directive %name { <<prompt text>> [params { ... }] }`.
    fn parse_directive_decl(&mut self) -> Result<DirectiveDecl, ParseError> {
        self.expect(&TokenKind::Percent)?;
        let name = self.expect_ident("directive name")?;
        self.expect(&TokenKind::LBrace)?;

        // Parse the prompt text (required)
        let text = match self.peek_kind().clone() {
            TokenKind::PromptLit(s) => {
                self.advance();
                s
            }
            _ => {
                let span = self.current_span();
                return Err(ParseError::UnexpectedToken {
                    expected: "prompt literal (<<...>>)".to_string(),
                    found: self.peek_kind().describe().to_string(),
                    span: (span.start..span.end).into(),
                });
            }
        };

        // Parse optional params block
        let mut params = Vec::new();
        if self.check(&TokenKind::Params) {
            self.advance();
            self.expect(&TokenKind::LBrace)?;
            params = self.parse_directive_params()?;
            self.expect(&TokenKind::RBrace)?;
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(DirectiveDecl { name, text, params })
    }

    /// Parse directive parameter list: `name :: Type = default_value` entries.
    fn parse_directive_params(&mut self) -> Result<Vec<DirectiveParam>, ParseError> {
        let mut params = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let name = self.expect_ident("parameter name")?;
            self.expect(&TokenKind::ColonColon)?;
            let ty = self.parse_type_expr()?;
            self.expect(&TokenKind::Eq)?;
            let default = self.parse_expr()?;
            params.push(DirectiveParam { name, ty, default });
        }
        Ok(params)
    }

    /// Parse `lesson "name" { context: <<...>> rule: <<...>> severity: ident }`.
    fn parse_lesson_decl(&mut self) -> Result<LessonDecl, ParseError> {
        let name = match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                let span = self.current_span();
                return Err(ParseError::UnexpectedToken {
                    expected: "lesson name string".to_string(),
                    found: self.peek_kind().describe().to_string(),
                    span: (span.start..span.end).into(),
                });
            }
        };

        self.expect(&TokenKind::LBrace)?;

        let mut context = None;
        let mut rule = None;
        let mut severity = None;

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            // Lesson fields use contextual keywords (plain idents, not reserved keywords)
            match self.peek_kind().clone() {
                TokenKind::Ident(ref s) if s == "context" => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    match self.peek_kind().clone() {
                        TokenKind::PromptLit(s) | TokenKind::StringLit(s) => {
                            context = Some(s.clone());
                            self.advance();
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "context string or prompt literal".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                }
                TokenKind::Ident(ref s) if s == "rule" => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    match self.peek_kind().clone() {
                        TokenKind::PromptLit(s) | TokenKind::StringLit(s) => {
                            rule = Some(s.clone());
                            self.advance();
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "rule string or prompt literal".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                }
                TokenKind::Ident(ref s) if s == "severity" => {
                    self.advance();
                    self.expect(&TokenKind::Colon)?;
                    let sev = self.expect_ident("severity level (info, warning, error)")?;
                    severity = Some(sev);
                }
                _ => {
                    let span = self.current_span();
                    return Err(ParseError::UnexpectedToken {
                        expected: "lesson field (context, rule, severity)".to_string(),
                        found: self.peek_kind().describe().to_string(),
                        span: (span.start..span.end).into(),
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;

        Ok(LessonDecl {
            name,
            context,
            rule,
            severity,
        })
    }

    /// Parse `template %name { ... }`.
    fn parse_template_decl(&mut self) -> Result<TemplateDecl, ParseError> {
        self.expect(&TokenKind::Percent)?;
        let name = self.expect_ident("template name")?;
        self.expect(&TokenKind::LBrace)?;

        let mut entries = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            if self.check(&TokenKind::Section) {
                // section NAME <<description>>
                self.advance();
                let section_name = self.expect_ident("section name")?;
                let desc = match self.peek_kind() {
                    TokenKind::PromptLit(_) => {
                        if let TokenKind::PromptLit(s) = self.peek_kind().clone() {
                            self.advance();
                            Some(s)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                entries.push(TemplateEntry::Section {
                    name: section_name,
                    description: desc,
                });
            } else {
                // FIELD_NAME :: Type [* count] [<<description>>]
                let field_name = self.expect_ident("field name")?;
                self.expect(&TokenKind::ColonColon)?;
                let ty = self.parse_type_expr()?;

                // Check for repeat: * count
                let repeat_count = if self.check(&TokenKind::Star) {
                    self.advance();
                    match self.peek_kind().clone() {
                        TokenKind::IntLit(n) => {
                            self.advance();
                            Some(n as usize)
                        }
                        _ => {
                            let span = self.current_span();
                            return Err(ParseError::UnexpectedToken {
                                expected: "repeat count".to_string(),
                                found: self.peek_kind().describe().to_string(),
                                span: (span.start..span.end).into(),
                            });
                        }
                    }
                } else {
                    None
                };

                // Check for description: <<...>>
                let desc = match self.peek_kind() {
                    TokenKind::PromptLit(_) => {
                        if let TokenKind::PromptLit(s) = self.peek_kind().clone() {
                            self.advance();
                            Some(s)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                if let Some(count) = repeat_count {
                    entries.push(TemplateEntry::Repeat {
                        name: field_name,
                        ty,
                        count,
                        description: desc,
                    });
                } else {
                    entries.push(TemplateEntry::Field {
                        name: field_name,
                        ty,
                        description: desc,
                    });
                }
            }
        }

        self.expect(&TokenKind::RBrace)?;
        Ok(TemplateDecl { name, entries })
    }
}
