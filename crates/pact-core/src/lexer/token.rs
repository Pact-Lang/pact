// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-03-25

//! Token definitions for the PACT language.
//!
//! Every lexeme produced by the lexer carries a [`TokenKind`] discriminant
//! and a [`Span`](crate::span::Span) locating it in the source text.

use crate::span::Span;

/// A single lexical token with its kind and source location.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The classification of this token.
    pub kind: TokenKind,
    /// The source location of this token.
    pub span: Span,
}

/// All possible token kinds in the PACT language.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Sigils ──────────────────────────────────────────────
    /// `@` — agent reference prefix
    At,
    /// `#` — tool reference prefix
    Hash,
    /// `~` — memory reference prefix
    Tilde,
    /// `!` — logical not (reserved for future use)
    Bang,
    /// `^` — permission reference prefix
    Caret,
    /// `$` — skill reference prefix
    Dollar,
    /// `%` — template reference prefix
    Percent,
    /// `?>` — fallback operator
    Fallback,
    /// `|>` — pipeline operator
    Pipe,
    /// `::` — type annotation separator
    ColonColon,
    /// `->` — agent dispatch / return type arrow
    Arrow,
    /// `=>` — match arm arrow
    FatArrow,

    // ── Delimiters ─────────────────────────────────────────
    /// `(` — opening parenthesis
    LParen,
    /// `)` — closing parenthesis
    RParen,
    /// `{` — opening brace
    LBrace,
    /// `}` — closing brace
    RBrace,
    /// `[` — opening bracket
    LBracket,
    /// `]` — closing bracket
    RBracket,

    // ── Punctuation ────────────────────────────────────────
    /// `,` — comma separator
    Comma,
    /// `:` — colon separator
    Colon,
    /// `.` — dot accessor
    Dot,
    /// `=` — assignment operator
    Eq,
    /// `==` — equality comparison
    EqEq,
    /// `!=` — inequality comparison
    BangEq,
    /// `<` — less-than comparison
    Lt,
    /// `>` — greater-than comparison
    Gt,
    /// `<=` — less-than-or-equal comparison
    LtEq,
    /// `>=` — greater-than-or-equal comparison
    GtEq,
    /// `+` — addition operator
    Plus,
    /// `-` — subtraction operator
    Minus,
    /// `*` — multiplication operator
    Star,
    /// `/` — division operator
    Slash,
    /// `|` — union type separator
    Bar,

    // ── Keywords ───────────────────────────────────────────
    /// `agent` — agent declaration keyword
    Agent,
    /// `agent_bundle` — agent bundle declaration keyword
    AgentBundle,
    /// `flow` — flow declaration keyword
    Flow,
    /// `schema` — schema declaration keyword
    Schema,
    /// `type` — type alias keyword
    Type,
    /// `permit_tree` — permission tree declaration keyword
    PermitTree,
    /// `test` — test block keyword
    Test,
    /// `permits` — agent permission list keyword
    Permits,
    /// `tools` — agent tool list keyword
    Tools,
    /// `model` — agent model specification keyword
    Model,
    /// `prompt` — agent prompt keyword
    Prompt,
    /// `memory` — agent memory specification keyword
    Memory,
    /// `agents` — agent list keyword (in bundles/flows)
    Agents,
    /// `fallbacks` — fallback list keyword
    Fallbacks,
    /// `match` — pattern match keyword
    Match,
    /// `return` — return value keyword
    Return,
    /// `fail` — failure keyword
    Fail,
    /// `record` — memory record keyword
    Record,
    /// `assert` — assertion keyword (in tests)
    Assert,
    /// `true` — boolean true literal
    True,
    /// `false` — boolean false literal
    False,
    /// `parallel` — parallel execution keyword
    Parallel,
    /// `on` — event handler keyword
    On,
    /// `if` — conditional keyword
    If,
    /// `else` — else branch keyword
    Else,
    /// `tool` — tool declaration keyword
    Tool,
    /// `requires` — tool permission requirements
    Requires,
    /// `params` — tool parameter block
    Params,
    /// `returns` — tool return type
    Returns,
    /// `description` — tool description field
    Description,
    /// `skill` — skill declaration keyword
    Skill,
    /// `skills` — agent skill list keyword
    Skills,
    /// `strategy` — skill strategy prompt keyword
    Strategy,
    /// `handler` — tool handler specification keyword
    Handler,
    /// `source` — tool capability source keyword
    Source,
    /// `import` — file import keyword
    Import,
    /// `template` — template declaration keyword
    Template,
    /// `output` — tool output template reference keyword
    Output,
    /// `section` — template section keyword
    Section,
    /// `directive` — directive declaration keyword
    Directive,
    /// `directives` — tool directives list keyword
    Directives,
    /// `retry` — tool retry count keyword
    Retry,
    /// `on_error` — error handler keyword
    OnError,
    /// `run` — flow call keyword
    Run,
    /// `validate` — tool output validation keyword
    Validate,
    /// `cache` — tool cache duration keyword
    Cache,
    /// `connect` — MCP server connection block keyword
    Connect,

    // ── Literals ───────────────────────────────────────────
    /// Integer literal, e.g. `42`
    IntLit(i64),
    /// Floating-point literal, e.g. `3.14`
    FloatLit(f64),
    /// String literal (double-quoted), e.g. `"hello"`
    StringLit(String),
    /// Prompt literal (angle-bracket delimited), e.g. `<<You are a helpful assistant>>`
    PromptLit(String),

    // ── Identifiers ────────────────────────────────────────
    /// A plain identifier, e.g. `name`, `result`
    Ident(String),

    // ── Special ────────────────────────────────────────────
    /// End-of-file sentinel
    Eof,
}

impl TokenKind {
    /// Return a human-readable description of this token kind for diagnostics.
    pub fn describe(&self) -> &'static str {
        match self {
            Self::At => "'@'",
            Self::Hash => "'#'",
            Self::Tilde => "'~'",
            Self::Bang => "'!'",
            Self::Caret => "'^'",
            Self::Dollar => "'$'",
            Self::Percent => "'%'",
            Self::Fallback => "'?>'",
            Self::Pipe => "'|>'",
            Self::ColonColon => "'::'",
            Self::Arrow => "'->'",
            Self::FatArrow => "'=>'",
            Self::LParen => "'('",
            Self::RParen => "')'",
            Self::LBrace => "'{'",
            Self::RBrace => "'}'",
            Self::LBracket => "'['",
            Self::RBracket => "']'",
            Self::Comma => "','",
            Self::Colon => "':'",
            Self::Dot => "'.'",
            Self::Eq => "'='",
            Self::EqEq => "'=='",
            Self::BangEq => "'!='",
            Self::Lt => "'<'",
            Self::Gt => "'>'",
            Self::LtEq => "'<='",
            Self::GtEq => "'>='",
            Self::Plus => "'+'",
            Self::Minus => "'-'",
            Self::Star => "'*'",
            Self::Slash => "'/'",
            Self::Bar => "'|'",
            Self::Agent => "'agent'",
            Self::AgentBundle => "'agent_bundle'",
            Self::Flow => "'flow'",
            Self::Schema => "'schema'",
            Self::Type => "'type'",
            Self::PermitTree => "'permit_tree'",
            Self::Test => "'test'",
            Self::Permits => "'permits'",
            Self::Tools => "'tools'",
            Self::Model => "'model'",
            Self::Prompt => "'prompt'",
            Self::Memory => "'memory'",
            Self::Agents => "'agents'",
            Self::Fallbacks => "'fallbacks'",
            Self::Match => "'match'",
            Self::Return => "'return'",
            Self::Fail => "'fail'",
            Self::Record => "'record'",
            Self::Assert => "'assert'",
            Self::True => "'true'",
            Self::False => "'false'",
            Self::Parallel => "'parallel'",
            Self::On => "'on'",
            Self::If => "'if'",
            Self::Else => "'else'",
            Self::Tool => "'tool'",
            Self::Requires => "'requires'",
            Self::Params => "'params'",
            Self::Returns => "'returns'",
            Self::Description => "'description'",
            Self::Skill => "'skill'",
            Self::Skills => "'skills'",
            Self::Strategy => "'strategy'",
            Self::Handler => "'handler'",
            Self::Source => "'source'",
            Self::Import => "'import'",
            Self::Template => "'template'",
            Self::Output => "'output'",
            Self::Section => "'section'",
            Self::Directive => "'directive'",
            Self::Directives => "'directives'",
            Self::Retry => "'retry'",
            Self::OnError => "'on_error'",
            Self::Run => "'run'",
            Self::Validate => "'validate'",
            Self::Cache => "'cache'",
            Self::Connect => "'connect'",
            Self::IntLit(_) => "integer",
            Self::FloatLit(_) => "float",
            Self::StringLit(_) => "string",
            Self::PromptLit(_) => "prompt literal",
            Self::Ident(_) => "identifier",
            Self::Eof => "end of file",
        }
    }
}
