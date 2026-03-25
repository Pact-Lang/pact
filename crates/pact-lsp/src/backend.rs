// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-01-25

//! LSP backend implementation for the PACT language.
//!
//! Provides diagnostics (lex + parse + check errors), keyword/sigil completions,
//! and hover information for declaration names.

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use pact_core::ast::stmt::{DeclKind, Program};
use pact_core::checker::{CheckError, Checker};
use pact_core::lexer::{LexError, Lexer};
use pact_core::parser::{ParseError, Parser};
use pact_core::span::SourceId;

use crate::symbol_index::SymbolIndex;

/// The PACT language server backend.
pub struct PactBackend {
    client: Client,
    /// Open document contents, keyed by URI.
    documents: DashMap<Url, String>,
}

impl PactBackend {
    /// Create a new backend connected to the given LSP client.
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: DashMap::new(),
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for PactBackend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".into(),
                        "#".into(),
                        "^".into(),
                        "$".into(),
                        "~".into(),
                        "%".into(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "PACT language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();
        self.documents.insert(uri.clone(), text.clone());
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // We use full sync, so the last content change is the full document.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.insert(uri.clone(), change.text.clone());
            self.publish_diagnostics(uri, &change.text).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let trigger = params
            .context
            .as_ref()
            .and_then(|ctx| ctx.trigger_character.as_deref());

        let mut items = Vec::new();

        match trigger {
            Some("@") => {
                // Suggest agent names from the current document
                let names = collect_declaration_names(&text, DeclFilter::Agent);
                for name in names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::CLASS),
                        detail: Some("Agent".into()),
                        insert_text: Some(name),
                        ..Default::default()
                    });
                }
            }
            Some("#") => {
                let names = collect_declaration_names(&text, DeclFilter::Tool);
                for name in names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("Tool".into()),
                        insert_text: Some(name),
                        ..Default::default()
                    });
                }
            }
            Some("^") => {
                // Suggest common permission paths
                let perms = vec![
                    "net",
                    "net.read",
                    "net.write",
                    "llm",
                    "llm.query",
                    "fs",
                    "fs.read",
                    "fs.write",
                ];
                for p in perms {
                    items.push(CompletionItem {
                        label: p.into(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some("Permission".into()),
                        insert_text: Some(p.into()),
                        ..Default::default()
                    });
                }
            }
            Some("$") => {
                let names = collect_declaration_names(&text, DeclFilter::Skill);
                for name in names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::METHOD),
                        detail: Some("Skill".into()),
                        insert_text: Some(name),
                        ..Default::default()
                    });
                }
            }
            Some("~") => {
                items.push(CompletionItem {
                    label: "context".into(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Memory reference".into()),
                    ..Default::default()
                });
                items.push(CompletionItem {
                    label: "history".into(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Memory reference".into()),
                    ..Default::default()
                });
            }
            Some("%") => {
                let names = collect_declaration_names(&text, DeclFilter::Template);
                for name in names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::STRUCT),
                        detail: Some("Template".into()),
                        insert_text: Some(name),
                        ..Default::default()
                    });
                }
                let directive_names = collect_declaration_names(&text, DeclFilter::Directive);
                for name in directive_names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::SNIPPET),
                        detail: Some("Directive".into()),
                        insert_text: Some(name),
                        ..Default::default()
                    });
                }
            }
            _ => {
                // Keyword completions based on partial typing
                let line_text = get_line_text(&text, position.line);
                let prefix = get_word_prefix(&line_text, position.character as usize);

                let keywords = [
                    ("agent", "Agent declaration", CompletionItemKind::KEYWORD),
                    ("tool", "Tool declaration", CompletionItemKind::KEYWORD),
                    ("flow", "Flow declaration", CompletionItemKind::KEYWORD),
                    ("schema", "Schema declaration", CompletionItemKind::KEYWORD),
                    (
                        "permit_tree",
                        "Permission tree declaration",
                        CompletionItemKind::KEYWORD,
                    ),
                    ("test", "Test declaration", CompletionItemKind::KEYWORD),
                    ("skill", "Skill declaration", CompletionItemKind::KEYWORD),
                    (
                        "agent_bundle",
                        "Agent bundle declaration",
                        CompletionItemKind::KEYWORD,
                    ),
                    (
                        "template",
                        "Template declaration",
                        CompletionItemKind::KEYWORD,
                    ),
                    (
                        "directive",
                        "Directive declaration",
                        CompletionItemKind::KEYWORD,
                    ),
                    (
                        "type",
                        "Type alias declaration",
                        CompletionItemKind::KEYWORD,
                    ),
                    ("import", "Import statement", CompletionItemKind::KEYWORD),
                    ("return", "Return statement", CompletionItemKind::KEYWORD),
                    ("fail", "Fail statement", CompletionItemKind::KEYWORD),
                    ("match", "Match expression", CompletionItemKind::KEYWORD),
                    ("parallel", "Parallel block", CompletionItemKind::KEYWORD),
                    ("retry", "Retry count for tool", CompletionItemKind::KEYWORD),
                    (
                        "cache",
                        "Cache duration for tool",
                        CompletionItemKind::KEYWORD,
                    ),
                    (
                        "validate",
                        "Validation mode for tool",
                        CompletionItemKind::KEYWORD,
                    ),
                    (
                        "on_error",
                        "Error handling expression",
                        CompletionItemKind::KEYWORD,
                    ),
                    ("run", "Run a flow", CompletionItemKind::KEYWORD),
                    (
                        "env",
                        "Environment variable lookup",
                        CompletionItemKind::FUNCTION,
                    ),
                ];

                for (kw, detail, kind) in &keywords {
                    if prefix.is_empty() || kw.starts_with(&prefix) {
                        items.push(CompletionItem {
                            label: (*kw).into(),
                            kind: Some(*kind),
                            detail: Some((*detail).into()),
                            ..Default::default()
                        });
                    }
                }

                // Type name completions
                let type_names = ["String", "Int", "Float", "Bool", "List", "Map"];
                for ty in &type_names {
                    if prefix.is_empty() || ty.starts_with(&prefix) {
                        items.push(CompletionItem {
                            label: (*ty).into(),
                            kind: Some(CompletionItemKind::TYPE_PARAMETER),
                            detail: Some("Built-in type".into()),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let offset = position_to_offset(&text, position);
        let hover_info = find_hover_info(&text, offset);

        Ok(hover_info.map(|info| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: info,
            }),
            range: None,
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let offset = position_to_offset(&text, position);
        let index = build_symbol_index(&text);

        let def = match index.definition_at(offset) {
            Some(d) => d,
            None => return Ok(None),
        };

        let range = Range::new(
            offset_to_position(&text, def.location.start),
            offset_to_position(&text, def.location.end),
        );

        Ok(Some(GotoDefinitionResponse::Scalar(
            tower_lsp::lsp_types::Location {
                uri: uri.clone(),
                range,
            },
        )))
    }

    async fn references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<tower_lsp::lsp_types::Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let offset = position_to_offset(&text, position);
        let index = build_symbol_index(&text);
        let include_definition = params.context.include_declaration;
        let locations = index.references_at(offset, include_definition);

        if locations.is_empty() {
            return Ok(None);
        }

        let lsp_locations: Vec<tower_lsp::lsp_types::Location> = locations
            .iter()
            .map(|loc| tower_lsp::lsp_types::Location {
                uri: uri.clone(),
                range: Range::new(
                    offset_to_position(&text, loc.start),
                    offset_to_position(&text, loc.end),
                ),
            })
            .collect();

        Ok(Some(lsp_locations))
    }
}

impl PactBackend {
    /// Run full diagnostics pipeline on a document and publish the results.
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let diagnostics = diagnose(text);
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
}

// ── Diagnostic conversion ─────────────────────────────────────────────

/// Convert a byte offset in `text` to an LSP `Position` (0-based line and character).
pub(crate) fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}

/// Convert an LSP `Position` to a byte offset in `text`.
pub(crate) fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 0u32;
    for (i, ch) in text.char_indices() {
        if current_line == pos.line && current_col == pos.character {
            return i;
        }
        if ch == '\n' {
            if current_line == pos.line {
                // Cursor is past end of this line
                return i;
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }
    text.len()
}

/// Convert a `miette::SourceSpan` to an LSP `Range` given the source text.
fn source_span_to_range(text: &str, span: miette::SourceSpan) -> Range {
    let start_offset = span.offset();
    let end_offset = span.offset() + span.len();
    Range::new(
        offset_to_position(text, start_offset),
        offset_to_position(text, end_offset),
    )
}

/// Extract the `miette::SourceSpan` from a `LexError`.
fn lex_error_span(err: &LexError) -> miette::SourceSpan {
    match err {
        LexError::UnexpectedChar { span, .. } => *span,
        LexError::UnterminatedString { span } => *span,
        LexError::UnterminatedPrompt { span } => *span,
        LexError::InvalidNumber { span } => *span,
    }
}

/// Extract the `miette::SourceSpan` from a `ParseError`.
fn parse_error_span(err: &ParseError) -> miette::SourceSpan {
    match err {
        ParseError::UnexpectedToken { span, .. } => *span,
    }
}

/// Extract the `miette::SourceSpan` from a `CheckError`.
fn check_error_span(err: &CheckError) -> miette::SourceSpan {
    match err {
        CheckError::DuplicateDefinition { span, .. } => *span,
        CheckError::UnknownType { span, .. } => *span,
        CheckError::MissingPermission { span, .. } => *span,
        CheckError::UnknownAgent { span, .. } => *span,
        CheckError::UnknownFlow { span, .. } => *span,
        CheckError::TypeInferenceWarning { .. } => miette::SourceSpan::new(0.into(), 0),
        CheckError::SourceArgNotAParam { span, .. } => *span,
        CheckError::UnknownTemplate { span, .. } => *span,
        CheckError::UnknownDirective { span, .. } => *span,
        CheckError::UnknownMcpServer { span, .. } => *span,
        CheckError::InvalidMcpTransport { span, .. } => *span,
        CheckError::InvalidLessonSeverity { span, .. } => *span,
    }
}

/// Run the full diagnostic pipeline (lex -> parse -> check) on source text
/// and return LSP diagnostics.
pub(crate) fn diagnose(text: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let source_id = SourceId(0);

    // Phase 1: Lex
    let tokens = match Lexer::new(text, source_id).lex() {
        Ok(tokens) => tokens,
        Err(err) => {
            diagnostics.push(Diagnostic {
                range: source_span_to_range(text, lex_error_span(&err)),
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("pact".into()),
                message: err.to_string(),
                ..Default::default()
            });
            return diagnostics;
        }
    };

    // Phase 2: Parse (collecting errors for better recovery)
    let mut parser = Parser::new(&tokens);
    let (program, parse_errors) = parser.parse_collecting_errors();

    for err in &parse_errors {
        diagnostics.push(Diagnostic {
            range: source_span_to_range(text, parse_error_span(err)),
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("pact".into()),
            message: err.to_string(),
            ..Default::default()
        });
    }

    // Phase 3: Check (even with parse errors, we can check the partial AST)
    let check_errors = Checker::new().check(&program);
    for err in &check_errors {
        diagnostics.push(Diagnostic {
            range: source_span_to_range(text, check_error_span(err)),
            severity: Some(DiagnosticSeverity::WARNING),
            source: Some("pact".into()),
            message: err.to_string(),
            ..Default::default()
        });
    }

    diagnostics
}

// ── Symbol index helper ───────────────────────────────────────────────

/// Parse a document and build a [`SymbolIndex`] from the resulting AST.
///
/// Returns a default (empty) index if lexing fails.
fn build_symbol_index(text: &str) -> SymbolIndex {
    let source_id = SourceId(0);
    let tokens = match Lexer::new(text, source_id).lex() {
        Ok(t) => t,
        Err(_) => return SymbolIndex::default(),
    };
    let mut parser = Parser::new(&tokens);
    let (program, _) = parser.parse_collecting_errors();
    SymbolIndex::build(&program)
}

// ── Completion helpers ────────────────────────────────────────────────

/// What kind of declarations to filter for.
enum DeclFilter {
    Agent,
    Tool,
    Skill,
    Template,
    Directive,
}

/// Parse the document and collect names for a given declaration kind.
fn collect_declaration_names(text: &str, filter: DeclFilter) -> Vec<String> {
    let source_id = SourceId(0);
    let tokens = match Lexer::new(text, source_id).lex() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let mut parser = Parser::new(&tokens);
    let (program, _) = parser.parse_collecting_errors();
    extract_names(&program, filter)
}

/// Extract declaration names from a parsed program.
fn extract_names(program: &Program, filter: DeclFilter) -> Vec<String> {
    let mut names = Vec::new();
    for decl in &program.decls {
        match (&filter, &decl.kind) {
            (DeclFilter::Agent, DeclKind::Agent(a)) => names.push(a.name.clone()),
            (DeclFilter::Agent, DeclKind::AgentBundle(ab)) => names.push(ab.name.clone()),
            (DeclFilter::Tool, DeclKind::Tool(t)) => names.push(t.name.clone()),
            (DeclFilter::Skill, DeclKind::Skill(s)) => names.push(s.name.clone()),
            (DeclFilter::Template, DeclKind::Template(t)) => names.push(t.name.clone()),
            (DeclFilter::Directive, DeclKind::Directive(d)) => names.push(d.name.clone()),
            _ => {}
        }
    }
    names
}

/// Get the text of a specific line (0-indexed).
fn get_line_text(text: &str, line: u32) -> String {
    text.lines().nth(line as usize).unwrap_or("").to_string()
}

/// Extract the word prefix ending at the cursor column.
fn get_word_prefix(line: &str, col: usize) -> String {
    let bytes = line.as_bytes();
    let end = col.min(bytes.len());
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    line[start..end].to_string()
}

// ── Hover helpers ─────────────────────────────────────────────────────

/// Find hover information for the token at the given byte offset.
fn find_hover_info(text: &str, offset: usize) -> Option<String> {
    let source_id = SourceId(0);
    let tokens = Lexer::new(text, source_id).lex().ok()?;
    let mut parser = Parser::new(&tokens);
    let (program, _) = parser.parse_collecting_errors();

    for decl in &program.decls {
        // Check if the offset falls within this declaration's span
        if offset < decl.span.start || offset >= decl.span.end {
            continue;
        }

        match &decl.kind {
            DeclKind::Agent(a) => {
                let tools: Vec<String> = a
                    .tools
                    .iter()
                    .filter_map(|e| {
                        if let pact_core::ast::expr::ExprKind::ToolRef(n) = &e.kind {
                            Some(format!("#{}", n))
                        } else {
                            None
                        }
                    })
                    .collect();
                let permits: Vec<String> = a
                    .permits
                    .iter()
                    .filter_map(|e| {
                        if let pact_core::ast::expr::ExprKind::PermissionRef(segs) = &e.kind {
                            Some(format!("^{}", segs.join(".")))
                        } else {
                            None
                        }
                    })
                    .collect();
                let model = a.model.as_ref().and_then(|e| {
                    if let pact_core::ast::expr::ExprKind::StringLit(s) = &e.kind {
                        Some(s.clone())
                    } else {
                        None
                    }
                });
                let mut info = format!("**agent** `@{}`\n", a.name);
                if let Some(m) = model {
                    info.push_str(&format!("\n- **model**: `{}`\n", m));
                }
                if !permits.is_empty() {
                    info.push_str(&format!("\n- **permits**: {}\n", permits.join(", ")));
                }
                if !tools.is_empty() {
                    info.push_str(&format!("\n- **tools**: {}\n", tools.join(", ")));
                }
                return Some(info);
            }
            DeclKind::Flow(f) => {
                let params: Vec<String> = f
                    .params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{} :: {}", p.name, format_type_expr(ty)),
                        None => p.name.clone(),
                    })
                    .collect();
                let ret = f
                    .return_type
                    .as_ref()
                    .map(|ty| format!(" -> {}", format_type_expr(ty)))
                    .unwrap_or_default();
                return Some(format!(
                    "**flow** `{}`\n\n```pact\nflow {}({}){}\n```",
                    f.name,
                    f.name,
                    params.join(", "),
                    ret
                ));
            }
            DeclKind::Schema(s) => {
                let fields: Vec<String> = s
                    .fields
                    .iter()
                    .map(|f| format!("  {} :: {}", f.name, format_type_expr(&f.ty)))
                    .collect();
                return Some(format!(
                    "**schema** `{}`\n\n```pact\nschema {} {{\n{}\n}}\n```",
                    s.name,
                    s.name,
                    fields.join("\n")
                ));
            }
            DeclKind::Tool(t) => {
                let params: Vec<String> = t
                    .params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{} :: {}", p.name, format_type_expr(ty)),
                        None => p.name.clone(),
                    })
                    .collect();
                let ret = t
                    .return_type
                    .as_ref()
                    .map(|ty| format!("\n- **returns**: `{}`", format_type_expr(ty)))
                    .unwrap_or_default();
                let mut info = format!("**tool** `#{}`\n", t.name);
                if !params.is_empty() {
                    info.push_str(&format!("\n- **params**: {}", params.join(", ")));
                }
                info.push_str(&ret);
                if let Some(source) = &t.source {
                    if source.args.is_empty() {
                        info.push_str(&format!("\n- **source**: `^{}`", source.capability));
                    } else {
                        info.push_str(&format!(
                            "\n- **source**: `^{}({})`",
                            source.capability,
                            source.args.join(", ")
                        ));
                    }
                }
                if let Some(output) = &t.output {
                    info.push_str(&format!("\n- **output**: `%{}`", output));
                }
                if let Some(retry) = t.retry {
                    info.push_str(&format!("\n- **retry**: {}", retry));
                }
                if let Some(cache) = &t.cache {
                    info.push_str(&format!("\n- **cache**: `{}`", cache));
                }
                if let Some(validate) = &t.validate {
                    info.push_str(&format!("\n- **validate**: `{}`", validate));
                }
                return Some(info);
            }
            DeclKind::Skill(s) => {
                let params: Vec<String> = s
                    .params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(ty) => format!("{} :: {}", p.name, format_type_expr(ty)),
                        None => p.name.clone(),
                    })
                    .collect();
                let ret = s
                    .return_type
                    .as_ref()
                    .map(|ty| format!("\n- **returns**: `{}`", format_type_expr(ty)))
                    .unwrap_or_default();
                let mut info = format!("**skill** `${}`\n", s.name);
                if !params.is_empty() {
                    info.push_str(&format!("\n- **params**: {}", params.join(", ")));
                }
                info.push_str(&ret);
                return Some(info);
            }
            DeclKind::TypeAlias(t) => {
                return Some(format!(
                    "**type** `{}`\n\n```pact\ntype {} = {}\n```",
                    t.name,
                    t.name,
                    t.variants.join(" | ")
                ));
            }
            DeclKind::PermitTree(_) => {
                return Some("**permit_tree**\n\nPermission hierarchy declaration.".into());
            }
            DeclKind::AgentBundle(ab) => {
                let agents: Vec<String> = ab
                    .agents
                    .iter()
                    .filter_map(|e| {
                        if let pact_core::ast::expr::ExprKind::AgentRef(n) = &e.kind {
                            Some(format!("@{}", n))
                        } else {
                            None
                        }
                    })
                    .collect();
                let mut info = format!("**agent_bundle** `@{}`\n", ab.name);
                if !agents.is_empty() {
                    info.push_str(&format!("\n- **agents**: {}\n", agents.join(", ")));
                }
                return Some(info);
            }
            DeclKind::Template(t) => {
                let entries: Vec<String> = t
                    .entries
                    .iter()
                    .map(|e| match e {
                        pact_core::ast::stmt::TemplateEntry::Field { name, ty, .. } => {
                            format!("  {} :: {}", name, format_type_expr(ty))
                        }
                        pact_core::ast::stmt::TemplateEntry::Repeat {
                            name, ty, count, ..
                        } => {
                            format!("  {} :: {} * {}", name, format_type_expr(ty), count)
                        }
                        pact_core::ast::stmt::TemplateEntry::Section { name, .. } => {
                            format!("  section {}", name)
                        }
                    })
                    .collect();
                return Some(format!(
                    "**template** `%{}`\n\n```pact\ntemplate %{} {{\n{}\n}}\n```",
                    t.name,
                    t.name,
                    entries.join("\n")
                ));
            }
            DeclKind::Directive(d) => {
                let params: Vec<String> = d
                    .params
                    .iter()
                    .map(|p| format!("  {} :: {} = ...", p.name, format_type_expr(&p.ty)))
                    .collect();
                let mut info = format!("**directive** `%{}`\n", d.name);
                if !params.is_empty() {
                    info.push_str(&format!(
                        "\n```pact\ndirective %{} {{\n  <<...>>\n  params {{\n{}\n  }}\n}}\n```",
                        d.name,
                        params.join("\n")
                    ));
                }
                return Some(info);
            }
            DeclKind::Test(t) => {
                return Some(format!("**test** `\"{}\"`", t.description));
            }
            DeclKind::Import(i) => {
                return Some(format!("**import** `\"{}\"`", i.path));
            }
            DeclKind::Connect(c) => {
                let names: Vec<_> = c.servers.iter().map(|s| s.name.as_str()).collect();
                return Some(format!("**connect** — MCP servers: {}", names.join(", ")));
            }
            DeclKind::Lesson(l) => {
                let mut info = format!("**lesson** `\"{}\"`\n", l.name);
                if let Some(sev) = &l.severity {
                    info.push_str(&format!("\n- **severity**: {}", sev));
                }
                if let Some(rule) = &l.rule {
                    info.push_str(&format!("\n- **rule**: {}", rule));
                }
                return Some(info);
            }
        }
    }

    None
}

/// Format a type expression for display.
fn format_type_expr(ty: &pact_core::ast::types::TypeExpr) -> String {
    use pact_core::ast::types::TypeExprKind;
    match &ty.kind {
        TypeExprKind::Named(n) => n.clone(),
        TypeExprKind::Generic { name, args } => {
            let arg_strs: Vec<String> = args.iter().map(format_type_expr).collect();
            format!("{}<{}>", name, arg_strs.join(", "))
        }
        TypeExprKind::Optional(inner) => {
            format!("{}?", format_type_expr(inner))
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_to_position_first_line() {
        let text = "agent @greeter {}";
        let pos = offset_to_position(text, 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        let pos = offset_to_position(text, 6);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 6);
    }

    #[test]
    fn offset_to_position_multiline() {
        let text = "line one\nline two\nline three";
        // "line two" starts at offset 9
        let pos = offset_to_position(text, 9);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // "two" at offset 14
        let pos = offset_to_position(text, 14);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 5);

        // "three" at offset 23
        let pos = offset_to_position(text, 23);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn offset_to_position_end_of_text() {
        let text = "abc\ndef";
        let pos = offset_to_position(text, 7);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn position_to_offset_basic() {
        let text = "line one\nline two\nline three";
        let off = position_to_offset(text, Position::new(0, 0));
        assert_eq!(off, 0);

        let off = position_to_offset(text, Position::new(1, 0));
        assert_eq!(off, 9);

        let off = position_to_offset(text, Position::new(1, 5));
        assert_eq!(off, 14);
    }

    #[test]
    fn diagnose_valid_program_no_errors() {
        let src = "agent @greeter { permits: [^llm.query] tools: [#greet] }";
        let diags = diagnose(src);
        assert!(
            diags.is_empty(),
            "expected no diagnostics, got: {:?}",
            diags
        );
    }

    #[test]
    fn diagnose_lex_error() {
        let src = "agent @greeter { ` }";
        let diags = diagnose(src);
        assert!(!diags.is_empty(), "expected at least one diagnostic");
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert!(diags[0].message.contains("unexpected character"));
    }

    #[test]
    fn diagnose_parse_error() {
        // Missing braces / malformed declaration
        let src = "agent";
        let diags = diagnose(src);
        assert!(!diags.is_empty(), "expected at least one diagnostic");
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn diagnose_check_error_unknown_type() {
        let src = "flow f(x :: UnknownType) { return x }";
        let diags = diagnose(src);
        assert!(!diags.is_empty(), "expected at least one diagnostic");
        // Check errors are reported as warnings
        let type_diag = diags.iter().find(|d| d.message.contains("unknown type"));
        assert!(type_diag.is_some(), "expected unknown type diagnostic");
        assert_eq!(
            type_diag.unwrap().severity,
            Some(DiagnosticSeverity::WARNING)
        );
    }

    #[test]
    fn diagnose_check_error_missing_permission() {
        let src = "agent @bad { permits: [] tools: [#web_search] }";
        let diags = diagnose(src);
        let perm_diag = diags.iter().find(|d| d.message.contains("permission"));
        assert!(
            perm_diag.is_some(),
            "expected missing permission diagnostic"
        );
    }

    #[test]
    fn diagnose_check_error_duplicate_def() {
        let src = "agent @a { permits: [] tools: [] } agent @a { permits: [] tools: [] }";
        let diags = diagnose(src);
        let dup_diag = diags.iter().find(|d| d.message.contains("duplicate"));
        assert!(
            dup_diag.is_some(),
            "expected duplicate definition diagnostic"
        );
    }

    #[test]
    fn source_span_to_range_single_line() {
        let text = "agent @greeter {}";
        let span = miette::SourceSpan::new(6.into(), 8); // "@greeter"
        let range = source_span_to_range(text, span);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 14);
    }

    #[test]
    fn source_span_to_range_multiline() {
        let text = "first\nsecond\nthird";
        // Span covering "second"
        let span = miette::SourceSpan::new(6.into(), 6);
        let range = source_span_to_range(text, span);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 6);
    }

    #[test]
    fn get_word_prefix_basic() {
        assert_eq!(get_word_prefix("  agent", 7), "agent");
        assert_eq!(get_word_prefix("  ag", 4), "ag");
        assert_eq!(get_word_prefix("  ", 2), "");
    }

    #[test]
    fn collect_agent_names() {
        let src = "agent @foo { permits: [] tools: [] } agent @bar { permits: [] tools: [] }";
        let names = collect_declaration_names(src, DeclFilter::Agent);
        assert!(names.contains(&"foo".to_string()));
        assert!(names.contains(&"bar".to_string()));
    }
}
