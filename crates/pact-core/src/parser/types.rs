// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-05-08

//! Type annotation parsing.
//!
//! Handles parsing of type expressions such as `String`, `List<String>`,
//! and `String?`.

use super::{ParseError, Parser};
use crate::ast::types::{TypeExpr, TypeExprKind};
use crate::lexer::token::TokenKind;

impl<'t> Parser<'t> {
    /// Parse a type expression.
    ///
    /// ```text
    /// type_expr = named_type ( "?" )?
    /// named_type = IDENT ( "<" type_expr ("," type_expr)* ">" )?
    /// ```
    pub(crate) fn parse_type_expr(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.current_span();
        let name = self.expect_ident("type name")?;

        // Check for generic args: Name<T, U>
        let kind = if self.check(&TokenKind::Lt) {
            self.advance(); // consume <
            let mut args = vec![self.parse_type_expr()?];
            while self.check(&TokenKind::Comma) {
                self.advance();
                args.push(self.parse_type_expr()?);
            }
            self.expect(&TokenKind::Gt)?;
            TypeExprKind::Generic { name, args }
        } else {
            TypeExprKind::Named(name)
        };

        let span = start.merge(self.previous_span());
        let ty = TypeExpr { kind, span };

        // Check for optional: Type?
        if self.check(&TokenKind::Question) {
            self.advance();
            let span = start.merge(self.previous_span());
            return Ok(TypeExpr {
                kind: TypeExprKind::Optional(Box::new(ty)),
                span,
            });
        }

        Ok(ty)
    }
}
