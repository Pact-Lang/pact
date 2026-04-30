// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-05-15

//! Expression parsing with precedence climbing.
//!
//! Precedence levels (lowest to highest):
//! 1. Fallback (`?>`)
//! 2. Pipeline (`|>`)
//! 3. Comparison (`==`, `!=`, `<`, `>`, `<=`, `>=`)
//! 4. Addition (`+`, `-`)
//! 5. Multiplication (`*`, `/`)
//! 6. Agent dispatch (`->`)
//! 7. Field access (`.`)
//! 8. Call (`(...)`)
//! 9. Primary (literals, refs, grouping)

use super::{ParseError, Parser};
use crate::ast::expr::*;
use crate::ast::Expr;
use crate::lexer::token::TokenKind;

impl<'t> Parser<'t> {
    /// Parse a full expression (entry point — lowest precedence).
    pub(crate) fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_fallback()?;

        // on_error is the lowest precedence postfix operator
        if self.check(&TokenKind::OnError) {
            self.advance();
            let fallback = self.parse_fallback()?;
            let span = expr.span.merge(fallback.span);
            expr = Expr {
                kind: ExprKind::OnError {
                    body: Box::new(expr),
                    fallback: Box::new(fallback),
                },
                span,
            };
        }

        Ok(expr)
    }

    /// Parse a statement-level expression (assignment or expression).
    pub(crate) fn parse_stmt_expr(&mut self) -> Result<Expr, ParseError> {
        // Check for `return expr`
        if self.check(&TokenKind::Return) {
            let start = self.current_span();
            self.advance();
            let value = self.parse_expr()?;
            let span = start.merge(value.span);
            return Ok(Expr {
                kind: ExprKind::Return(Box::new(value)),
                span,
            });
        }

        // Check for `fail expr`
        if self.check(&TokenKind::Fail) {
            let start = self.current_span();
            self.advance();
            let value = self.parse_expr()?;
            let span = start.merge(value.span);
            return Ok(Expr {
                kind: ExprKind::Fail(Box::new(value)),
                span,
            });
        }

        // Check for `assert expr`
        if self.check(&TokenKind::Assert) {
            let start = self.current_span();
            self.advance();
            let value = self.parse_expr()?;
            let span = start.merge(value.span);
            return Ok(Expr {
                kind: ExprKind::Assert(Box::new(value)),
                span,
            });
        }

        // Try assignment: `ident = expr` (including contextual keywords as variable names)
        let maybe_name = match self.peek_kind() {
            TokenKind::Ident(name) => Some(name.clone()),
            tok => tok.as_contextual_ident().map(String::from),
        };
        if let Some(name) = maybe_name {
            if self.peek_next_kind() == &TokenKind::Eq {
                let start = self.current_span();
                self.advance(); // ident or contextual keyword
                self.advance(); // =
                let value = self.parse_expr()?;
                let span = start.merge(value.span);
                return Ok(Expr {
                    kind: ExprKind::Assign {
                        name,
                        value: Box::new(value),
                    },
                    span,
                });
            }
        }

        self.parse_expr()
    }

    // ── Precedence levels ─────────────────────────────────────

    /// Level 1: Fallback `?>`.
    fn parse_fallback(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pipeline()?;
        while self.check(&TokenKind::Fallback) {
            self.advance();
            let right = self.parse_pipeline()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::FallbackChain {
                    primary: Box::new(left),
                    fallback: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    /// Level 2: Pipeline `|>`.
    fn parse_pipeline(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comparison()?;
        while self.check(&TokenKind::Pipe) {
            self.advance();
            let right = self.parse_comparison()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::Pipeline {
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    /// Level 3: Comparison operators.
    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::EqEq => BinOpKind::Eq,
                TokenKind::BangEq => BinOpKind::Neq,
                TokenKind::Lt => BinOpKind::Lt,
                TokenKind::Gt => BinOpKind::Gt,
                TokenKind::LtEq => BinOpKind::LtEq,
                TokenKind::GtEq => BinOpKind::GtEq,
                _ => break,
            };
            self.advance();
            let right = self.parse_addition()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    /// Level 4: Addition and subtraction.
    fn parse_addition(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus => BinOpKind::Add,
                TokenKind::Minus => BinOpKind::Sub,
                _ => break,
            };
            self.advance();
            let right = self.parse_multiplication()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    /// Level 5: Multiplication and division.
    fn parse_multiplication(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_dispatch()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star => BinOpKind::Mul,
                TokenKind::Slash => BinOpKind::Div,
                _ => break,
            };
            self.advance();
            let right = self.parse_dispatch()?;
            let span = left.span.merge(right.span);
            left = Expr {
                kind: ExprKind::BinOp {
                    left: Box::new(left),
                    op,
                    right: Box::new(right),
                },
                span,
            };
        }
        Ok(left)
    }

    /// Level 6: Agent dispatch `->`.
    fn parse_dispatch(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_postfix()?;
        if self.check(&TokenKind::Arrow) {
            self.advance();
            // Expect a tool call: #tool(args)
            let tool_expr = self.parse_postfix()?;
            // The tool_expr should be a FuncCall with a ToolRef callee,
            // but we handle it generically.
            match tool_expr.kind {
                ExprKind::FuncCall { callee, args } => {
                    let span = left.span.merge(tool_expr.span);
                    Ok(Expr {
                        kind: ExprKind::AgentDispatch {
                            agent: Box::new(left),
                            tool: callee,
                            args,
                        },
                        span,
                    })
                }
                _ => {
                    // Dispatch with no args: @agent -> #tool
                    let span = left.span.merge(tool_expr.span);
                    Ok(Expr {
                        kind: ExprKind::AgentDispatch {
                            agent: Box::new(left),
                            tool: Box::new(tool_expr),
                            args: vec![],
                        },
                        span,
                    })
                }
            }
        } else {
            Ok(left)
        }
    }

    /// Level 7-8: Postfix — field access and function calls.
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(&TokenKind::Dot) {
                self.advance();
                let field = self.expect_any_ident("field name")?;
                let span = expr.span.merge(self.previous_span());
                expr = Expr {
                    kind: ExprKind::FieldAccess {
                        object: Box::new(expr),
                        field,
                    },
                    span,
                };
            } else if self.check(&TokenKind::LParen) {
                self.advance();
                let args = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RParen)?;
                self.expect(&TokenKind::RParen)?;
                let span = expr.span.merge(self.previous_span());
                expr = Expr {
                    kind: ExprKind::FuncCall {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    /// Level 9: Primary expressions.
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.current_span();

        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::IntLit(n),
                    span,
                })
            }
            TokenKind::FloatLit(n) => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::FloatLit(n),
                    span,
                })
            }
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::StringLit(s),
                    span,
                })
            }
            TokenKind::PromptLit(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::PromptLit(s),
                    span,
                })
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::BoolLit(true),
                    span,
                })
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr {
                    kind: ExprKind::BoolLit(false),
                    span,
                })
            }

            // Agent ref: @name
            TokenKind::At => {
                self.advance();
                let name = self.expect_ident("agent name")?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::AgentRef(name),
                    span,
                })
            }

            // Tool ref: #name
            TokenKind::Hash => {
                self.advance();
                let name = self.expect_name("tool name")?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::ToolRef(name),
                    span,
                })
            }

            // Memory ref: ~name
            TokenKind::Tilde => {
                self.advance();
                let name = self.expect_ident("memory name")?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::MemoryRef(name),
                    span,
                })
            }

            // Permission ref: ^name.name.name
            TokenKind::Caret => {
                self.advance();
                let mut segments = vec![self.expect_any_ident("permission name")?];
                while self.check(&TokenKind::Dot) {
                    self.advance();
                    segments.push(self.expect_any_ident("permission segment")?);
                }
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::PermissionRef(segments),
                    span,
                })
            }

            // Skill ref: $name
            TokenKind::Dollar => {
                self.advance();
                let name = self.expect_ident("skill name")?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::SkillRef(name),
                    span,
                })
            }

            // Template ref: %name
            TokenKind::Percent => {
                self.advance();
                let name = self.expect_ident("template name")?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::TemplateRef(name),
                    span,
                })
            }

            // Identifier (check for `env(...)` builtin first)
            TokenKind::Ident(name)
                if name == "env" && self.peek_next_kind() == &TokenKind::LParen =>
            {
                self.advance(); // consume `env`
                self.advance(); // consume `(`
                let key = match self.peek_kind().clone() {
                    TokenKind::StringLit(s) => {
                        self.advance();
                        s
                    }
                    _ => {
                        let err_span = self.current_span();
                        return Err(ParseError::UnexpectedToken {
                            expected: "string literal for env key".to_string(),
                            found: self.peek_kind().describe().to_string(),
                            span: (err_span.start..err_span.end).into(),
                        });
                    }
                };
                self.expect(&TokenKind::RParen)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::Env(key),
                    span,
                })
            }

            // Identifier
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Ident(name),
                    span,
                })
            }

            // Contextual keyword used as identifier (e.g. `endpoint` as a variable)
            ref tok if tok.as_contextual_ident().is_some() => {
                let name = tok.as_contextual_ident().unwrap().to_string();
                self.advance();
                Ok(Expr {
                    kind: ExprKind::Ident(name),
                    span,
                })
            }

            // Grouped expression: (expr)
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(inner)
            }

            // Record literal: { key: expr, ... }
            TokenKind::LBrace => {
                self.advance();
                // Check if this is a record literal: { ident: expr, ... }
                // We look ahead for the pattern: Ident Colon
                let mut fields = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    let field_name = self.expect_any_ident("record field name")?;
                    self.expect(&TokenKind::Colon)?;
                    let value = self.parse_expr()?;
                    fields.push((field_name, value));
                    if !self.check(&TokenKind::RBrace) && !self.eat(&TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(&TokenKind::RBrace)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::RecordFields(fields),
                    span,
                })
            }

            // List literal: [a, b, c]
            TokenKind::LBracket => {
                self.advance();
                let items = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBracket)?;
                self.expect(&TokenKind::RBracket)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::ListLit(items),
                    span,
                })
            }

            // Run flow: run flow_name(args)
            TokenKind::Run => {
                self.advance();
                let flow_name = self.expect_name("flow name")?;
                self.expect(&TokenKind::LParen)?;
                let args = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RParen)?;
                self.expect(&TokenKind::RParen)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::RunFlow { flow_name, args },
                    span,
                })
            }

            // Parallel block: parallel { expr, expr, ... }
            TokenKind::Parallel => {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let items = self.parse_comma_separated(|p| p.parse_expr(), &TokenKind::RBrace)?;
                self.expect(&TokenKind::RBrace)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::Parallel(items),
                    span,
                })
            }

            // Match expression
            TokenKind::Match => {
                self.advance();
                let subject = self.parse_expr()?;
                self.expect(&TokenKind::LBrace)?;
                let mut arms = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    let arm = self.parse_match_arm()?;
                    arms.push(arm);
                    // Optional comma between arms
                    self.eat(&TokenKind::Comma);
                }
                self.expect(&TokenKind::RBrace)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::Match {
                        subject: Box::new(subject),
                        arms,
                    },
                    span,
                })
            }

            // Record block (for tests)
            TokenKind::Record => {
                self.advance();
                self.expect(&TokenKind::LBrace)?;
                let mut body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
                    body.push(self.parse_stmt_expr()?);
                }
                self.expect(&TokenKind::RBrace)?;
                let span = span.merge(self.previous_span());
                Ok(Expr {
                    kind: ExprKind::Record(body),
                    span,
                })
            }

            _ => Err(ParseError::UnexpectedToken {
                expected: "expression".to_string(),
                found: self.peek_kind().describe().to_string(),
                span: (span.start..span.end).into(),
            }),
        }
    }

    /// Parse a match arm: `pattern => body`.
    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.current_span();
        let pattern = match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                MatchPattern::StringLit(s)
            }
            TokenKind::IntLit(n) => {
                self.advance();
                MatchPattern::IntLit(n)
            }
            TokenKind::True => {
                self.advance();
                MatchPattern::BoolLit(true)
            }
            TokenKind::False => {
                self.advance();
                MatchPattern::BoolLit(false)
            }
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                MatchPattern::Wildcard
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                MatchPattern::Ident(name)
            }
            _ => {
                let span = self.current_span();
                return Err(ParseError::UnexpectedToken {
                    expected: "match pattern".to_string(),
                    found: self.peek_kind().describe().to_string(),
                    span: (span.start..span.end).into(),
                });
            }
        };

        self.expect(&TokenKind::FatArrow)?;

        // Body can be a block `{ ... }` or a single expression
        let body = if self.check(&TokenKind::LBrace) {
            // Parse a block as a sequence — for now just parse one expr
            self.parse_expr()?
        } else {
            self.parse_expr()?
        };

        let span = start.merge(body.span);
        Ok(MatchArm {
            pattern,
            body,
            span,
        })
    }
}
