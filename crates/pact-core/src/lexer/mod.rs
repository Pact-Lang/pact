// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-03-29

//! Lexer for the PACT language.
//!
//! The lexer (also called tokenizer or scanner) converts raw source text into
//! a flat sequence of [`Token`]s. Each token carries a [`TokenKind`] and a
//! [`Span`] indicating its position in the source.
//!
//! # Usage
//!
//! ```
//! use pact_core::lexer::Lexer;
//! use pact_core::span::{SourceId, SourceMap};
//!
//! let mut sm = SourceMap::new();
//! let id = sm.add("example.pact", "agent @greeter {}");
//! let tokens = Lexer::new(sm.text(id), id).lex().unwrap();
//! ```

/// Cursor utilities for character-by-character source traversal.
pub mod cursor;
/// Token types and token-kind enumeration.
pub mod token;

use crate::span::{SourceId, Span};
use cursor::Cursor;
use token::{Token, TokenKind};

use miette::Diagnostic;
use thiserror::Error;

/// Error produced during lexing.
#[derive(Debug, Error, Diagnostic, Clone)]
pub enum LexError {
    /// An unexpected character was encountered.
    #[error("unexpected character '{ch}'")]
    UnexpectedChar {
        /// The character that was not recognized.
        ch: char,
        /// Location of the unexpected character.
        #[label("here")]
        span: miette::SourceSpan,
    },

    /// A string literal was opened but never closed.
    #[error("unterminated string literal")]
    UnterminatedString {
        /// Location where the string begins.
        #[label("string starts here")]
        span: miette::SourceSpan,
    },

    /// A prompt literal `<<...>>` was opened but never closed.
    #[error("unterminated prompt literal `<<...>>`")]
    UnterminatedPrompt {
        /// Location where the prompt begins.
        #[label("prompt starts here")]
        span: miette::SourceSpan,
    },

    /// A numeric literal could not be parsed.
    #[error("invalid number literal")]
    InvalidNumber {
        /// Location of the invalid number.
        #[label("here")]
        span: miette::SourceSpan,
    },
}

/// The PACT lexer. Converts source text into a sequence of tokens.
pub struct Lexer<'src> {
    cursor: Cursor<'src>,
    source_id: SourceId,
    tokens: Vec<Token>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source text.
    pub fn new(src: &'src str, source_id: SourceId) -> Self {
        Self {
            cursor: Cursor::new(src),
            source_id,
            tokens: Vec::new(),
        }
    }

    /// Lex the entire source into a token vector (ending with [`TokenKind::Eof`]).
    pub fn lex(mut self) -> Result<Vec<Token>, LexError> {
        while !self.cursor.is_at_end() {
            self.skip_whitespace_and_comments();
            if self.cursor.is_at_end() {
                break;
            }
            let token = self.next_token()?;
            self.tokens.push(token);
        }
        // Append EOF
        let eof_offset = self.cursor.offset();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.source_id, eof_offset, eof_offset),
        });
        Ok(self.tokens)
    }

    /// Skip whitespace and `--` line comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            self.cursor.eat_while(|c| c.is_ascii_whitespace());

            // Check for `--` line comment
            if self.cursor.peek() == Some('-') && self.cursor.peek_next() == Some('-') {
                self.cursor.advance(); // -
                self.cursor.advance(); // -
                self.cursor.eat_while(|c| c != '\n');
                continue;
            }

            break;
        }
    }

    /// Produce the next token starting at the current cursor position.
    fn next_token(&mut self) -> Result<Token, LexError> {
        let start = self.cursor.offset();
        let ch = self.cursor.advance().unwrap();

        let kind = match ch {
            '@' => TokenKind::At,
            '#' => TokenKind::Hash,
            '$' => TokenKind::Dollar,
            '%' => TokenKind::Percent,
            '~' => TokenKind::Tilde,
            '^' => TokenKind::Caret,
            '+' => TokenKind::Plus,
            '*' => TokenKind::Star,
            ',' => TokenKind::Comma,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,

            '.' => TokenKind::Dot,

            ':' => {
                if self.cursor.peek() == Some(':') {
                    self.cursor.advance();
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }

            '=' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::EqEq
                } else if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::FatArrow
                } else {
                    TokenKind::Eq
                }
            }

            '!' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::BangEq
                } else {
                    TokenKind::Bang
                }
            }

            '-' => {
                if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }

            '?' => {
                if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::Fallback
                } else {
                    return Err(LexError::UnexpectedChar {
                        ch: '?',
                        span: (start..start + 1).into(),
                    });
                }
            }

            '|' => {
                if self.cursor.peek() == Some('>') {
                    self.cursor.advance();
                    TokenKind::Pipe
                } else {
                    TokenKind::Bar
                }
            }

            '<' => {
                if self.cursor.peek() == Some('<') {
                    // Prompt literal <<...>>
                    self.cursor.advance(); // consume second <
                    return self.lex_prompt_literal(start);
                } else if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }

            '>' => {
                if self.cursor.peek() == Some('=') {
                    self.cursor.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }

            '/' => TokenKind::Slash,

            '"' => return self.lex_string(start),

            c if c.is_ascii_digit() => return self.lex_number(start),

            c if is_ident_start(c) => return self.lex_ident_or_keyword(start),

            other => {
                return Err(LexError::UnexpectedChar {
                    ch: other,
                    span: (start..self.cursor.offset()).into(),
                });
            }
        };

        Ok(Token {
            kind,
            span: Span::new(self.source_id, start, self.cursor.offset()),
        })
    }

    /// Lex a `"..."` string literal. The opening quote has already been consumed.
    fn lex_string(&mut self, start: usize) -> Result<Token, LexError> {
        let mut value = String::new();
        loop {
            match self.cursor.advance() {
                Some('"') => break,
                Some('\\') => match self.cursor.advance() {
                    Some('n') => value.push('\n'),
                    Some('t') => value.push('\t'),
                    Some('\\') => value.push('\\'),
                    Some('"') => value.push('"'),
                    Some(c) => {
                        value.push('\\');
                        value.push(c);
                    }
                    None => {
                        return Err(LexError::UnterminatedString {
                            span: (start..self.cursor.offset()).into(),
                        });
                    }
                },
                Some(c) => value.push(c),
                None => {
                    return Err(LexError::UnterminatedString {
                        span: (start..self.cursor.offset()).into(),
                    });
                }
            }
        }
        Ok(Token {
            kind: TokenKind::StringLit(value),
            span: Span::new(self.source_id, start, self.cursor.offset()),
        })
    }

    /// Lex a `<<...>>` prompt literal. Both opening `<` chars have been consumed.
    fn lex_prompt_literal(&mut self, start: usize) -> Result<Token, LexError> {
        let mut value = String::new();
        loop {
            match self.cursor.advance() {
                Some('>') if self.cursor.peek() == Some('>') => {
                    self.cursor.advance(); // consume second >
                    break;
                }
                Some(c) => value.push(c),
                None => {
                    return Err(LexError::UnterminatedPrompt {
                        span: (start..self.cursor.offset()).into(),
                    });
                }
            }
        }
        Ok(Token {
            kind: TokenKind::PromptLit(value),
            span: Span::new(self.source_id, start, self.cursor.offset()),
        })
    }

    /// Lex an integer or float literal. The first digit has already been consumed.
    fn lex_number(&mut self, start: usize) -> Result<Token, LexError> {
        self.cursor.eat_while(|c| c.is_ascii_digit());

        // Check for decimal point (but not `..` range or method call on int)
        if self.cursor.peek() == Some('.')
            && self.cursor.peek_next().is_some_and(|c| c.is_ascii_digit())
        {
            self.cursor.advance(); // consume '.'
            self.cursor.eat_while(|c| c.is_ascii_digit());
            let text = self.cursor.slice(start, self.cursor.offset());
            let value: f64 = text.parse().map_err(|_| LexError::InvalidNumber {
                span: (start..self.cursor.offset()).into(),
            })?;
            return Ok(Token {
                kind: TokenKind::FloatLit(value),
                span: Span::new(self.source_id, start, self.cursor.offset()),
            });
        }

        let text = self.cursor.slice(start, self.cursor.offset());
        let value: i64 = text.parse().map_err(|_| LexError::InvalidNumber {
            span: (start..self.cursor.offset()).into(),
        })?;
        Ok(Token {
            kind: TokenKind::IntLit(value),
            span: Span::new(self.source_id, start, self.cursor.offset()),
        })
    }

    /// Lex an identifier or keyword. The first character has already been consumed.
    fn lex_ident_or_keyword(&mut self, start: usize) -> Result<Token, LexError> {
        self.cursor.eat_while(is_ident_continue);
        let text = self.cursor.slice(start, self.cursor.offset());

        let kind = match text {
            "agent_bundle" => TokenKind::AgentBundle,
            "agent" => TokenKind::Agent,
            "flow" => TokenKind::Flow,
            "schema" => TokenKind::Schema,
            "type" => TokenKind::Type,
            "permit_tree" => TokenKind::PermitTree,
            "test" => TokenKind::Test,
            "permits" => TokenKind::Permits,
            "tool" => TokenKind::Tool,
            "tools" => TokenKind::Tools,
            "model" => TokenKind::Model,
            "prompt" => TokenKind::Prompt,
            "memory" => TokenKind::Memory,
            "agents" => TokenKind::Agents,
            "fallbacks" => TokenKind::Fallbacks,
            "match" => TokenKind::Match,
            "return" => TokenKind::Return,
            "fail" => TokenKind::Fail,
            "record" => TokenKind::Record,
            "assert" => TokenKind::Assert,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "parallel" => TokenKind::Parallel,
            "on" => TokenKind::On,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "requires" => TokenKind::Requires,
            "params" => TokenKind::Params,
            "returns" => TokenKind::Returns,
            "description" => TokenKind::Description,
            "skill" => TokenKind::Skill,
            "skills" => TokenKind::Skills,
            "strategy" => TokenKind::Strategy,
            "handler" => TokenKind::Handler,
            "source" => TokenKind::Source,
            "import" => TokenKind::Import,
            "template" => TokenKind::Template,
            "output" => TokenKind::Output,
            "section" => TokenKind::Section,
            "directives" => TokenKind::Directives,
            "directive" => TokenKind::Directive,
            "retry" => TokenKind::Retry,
            "on_error" => TokenKind::OnError,
            "run" => TokenKind::Run,
            "validate" => TokenKind::Validate,
            "cache" => TokenKind::Cache,
            "connect" => TokenKind::Connect,
            "lesson" => TokenKind::Lesson,
            "compliance" => TokenKind::Compliance,
            _ => TokenKind::Ident(text.to_string()),
        };

        Ok(Token {
            kind,
            span: Span::new(self.source_id, start, self.cursor.offset()),
        })
    }
}

/// Returns `true` if `c` can start an identifier.
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Returns `true` if `c` can continue an identifier.
fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::span::SourceMap;

    /// Helper: lex source and return token kinds (excluding EOF).
    fn lex_kinds(src: &str) -> Vec<TokenKind> {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        tokens
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn sigils() {
        let kinds = lex_kinds("@ # ~ ! ^ ?> |> :: -> =>");
        assert_eq!(
            kinds,
            vec![
                TokenKind::At,
                TokenKind::Hash,
                TokenKind::Tilde,
                TokenKind::Bang,
                TokenKind::Caret,
                TokenKind::Fallback,
                TokenKind::Pipe,
                TokenKind::ColonColon,
                TokenKind::Arrow,
                TokenKind::FatArrow,
            ]
        );
    }

    #[test]
    fn delimiters_and_punctuation() {
        let kinds = lex_kinds("( ) { } [ ] , : . = == != < > <= >=");
        assert_eq!(
            kinds,
            vec![
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Comma,
                TokenKind::Colon,
                TokenKind::Dot,
                TokenKind::Eq,
                TokenKind::EqEq,
                TokenKind::BangEq,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::LtEq,
                TokenKind::GtEq,
            ]
        );
    }

    #[test]
    fn keywords() {
        let kinds = lex_kinds("agent flow schema type permit_tree test return fail match");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Agent,
                TokenKind::Flow,
                TokenKind::Schema,
                TokenKind::Type,
                TokenKind::PermitTree,
                TokenKind::Test,
                TokenKind::Return,
                TokenKind::Fail,
                TokenKind::Match,
            ]
        );
    }

    #[test]
    fn identifiers() {
        let kinds = lex_kinds("foo bar_baz _hidden x123");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Ident("bar_baz".into()),
                TokenKind::Ident("_hidden".into()),
                TokenKind::Ident("x123".into()),
            ]
        );
    }

    #[test]
    fn number_literals() {
        let kinds = lex_kinds("42 2.72 0 100");
        assert_eq!(
            kinds,
            vec![
                TokenKind::IntLit(42),
                TokenKind::FloatLit(2.72),
                TokenKind::IntLit(0),
                TokenKind::IntLit(100),
            ]
        );
    }

    #[test]
    fn string_literal() {
        let kinds = lex_kinds(r#""hello world" "escaped \"quote\"""#);
        assert_eq!(
            kinds,
            vec![
                TokenKind::StringLit("hello world".into()),
                TokenKind::StringLit("escaped \"quote\"".into()),
            ]
        );
    }

    #[test]
    fn prompt_literal() {
        let kinds = lex_kinds("<<You are a helpful assistant>>");
        assert_eq!(
            kinds,
            vec![TokenKind::PromptLit("You are a helpful assistant".into())]
        );
    }

    #[test]
    fn line_comments() {
        let kinds = lex_kinds("agent -- this is a comment\nflow");
        assert_eq!(kinds, vec![TokenKind::Agent, TokenKind::Flow]);
    }

    #[test]
    fn agent_decl_tokens() {
        let src = r#"agent @greeter {
    permits: [^llm.query]
    tools: [#greet]
}"#;
        let kinds = lex_kinds(src);
        assert_eq!(
            kinds,
            vec![
                TokenKind::Agent,
                TokenKind::At,
                TokenKind::Ident("greeter".into()),
                TokenKind::LBrace,
                TokenKind::Permits,
                TokenKind::Colon,
                TokenKind::LBracket,
                TokenKind::Caret,
                TokenKind::Ident("llm".into()),
                TokenKind::Dot,
                TokenKind::Ident("query".into()),
                TokenKind::RBracket,
                TokenKind::Tools,
                TokenKind::Colon,
                TokenKind::LBracket,
                TokenKind::Hash,
                TokenKind::Ident("greet".into()),
                TokenKind::RBracket,
                TokenKind::RBrace,
            ]
        );
    }

    #[test]
    fn unterminated_string_error() {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", "\"hello");
        let result = Lexer::new("\"hello", id).lex();
        assert!(matches!(result, Err(LexError::UnterminatedString { .. })));
    }

    #[test]
    fn unterminated_prompt_error() {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", "<<hello");
        let result = Lexer::new("<<hello", id).lex();
        assert!(matches!(result, Err(LexError::UnterminatedPrompt { .. })));
    }

    #[test]
    fn unexpected_char_error() {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", "`");
        let result = Lexer::new("`", id).lex();
        assert!(matches!(result, Err(LexError::UnexpectedChar { .. })));
    }

    #[test]
    fn dollar_sigil() {
        let kinds = lex_kinds("$age_verification");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Dollar,
                TokenKind::Ident("age_verification".into())
            ]
        );
    }

    #[test]
    fn skill_keywords() {
        let kinds = lex_kinds("skill skills strategy");
        assert_eq!(
            kinds,
            vec![TokenKind::Skill, TokenKind::Skills, TokenKind::Strategy]
        );
    }

    #[test]
    fn lesson_keyword() {
        let kinds = lex_kinds("lesson");
        assert_eq!(kinds, vec![TokenKind::Lesson]);
    }

    #[test]
    fn context_rule_severity_are_idents() {
        // These are contextual — only special inside lesson blocks
        let kinds = lex_kinds("context rule severity");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Ident("context".into()),
                TokenKind::Ident("rule".into()),
                TokenKind::Ident("severity".into()),
            ]
        );
    }

    #[test]
    fn connect_keyword() {
        let kinds = lex_kinds(r#"connect { slack "stdio cmd" }"#);
        assert_eq!(
            kinds,
            vec![
                TokenKind::Connect,
                TokenKind::LBrace,
                TokenKind::Ident("slack".into()),
                TokenKind::StringLit("stdio cmd".into()),
                TokenKind::RBrace,
            ]
        );
    }
}
