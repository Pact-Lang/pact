// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Output format inference for PACT programs.
//!
//! Analyzes a compiled PACT program to determine what file format
//! a flow's final output should be saved as. Uses signals from:
//! - Flow return types (`-> HTML`, `-> PDF`, etc.)
//! - Tool names and descriptions (e.g. `#render_html`, `#generate_pdf`)
//! - Agent prompt text (mentions of "website", "PDF report", etc.)

use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{DeclKind, Program};

use crate::emit_common::extract_prompt_text;

/// Supported output file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    // Text formats
    Html,
    Pdf,
    Markdown,
    Json,
    Csv,
    Yaml,
    Svg,
    Xml,
    Sql,
    PlainText,
    // Media formats
    Image,
    Audio,
    Video,
    // Archive formats
    Code,
    // Office formats
    Slides,
    Excel,
}

impl OutputFormat {
    /// File extension (without dot).
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Yaml => "yaml",
            Self::Svg => "svg",
            Self::Xml => "xml",
            Self::Sql => "sql",
            Self::PlainText => "txt",
            Self::Image => "png",
            Self::Audio => "mp3",
            Self::Video => "mp4",
            Self::Code => "zip",
            Self::Slides => "pptx",
            Self::Excel => "xlsx",
        }
    }

    /// MIME type for the format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Html => "text/html",
            Self::Pdf => "application/pdf",
            Self::Markdown => "text/markdown",
            Self::Json => "application/json",
            Self::Csv => "text/csv",
            Self::Yaml => "application/yaml",
            Self::Svg => "image/svg+xml",
            Self::Xml => "application/xml",
            Self::Sql => "application/sql",
            Self::PlainText => "text/plain",
            Self::Image => "image/png",
            Self::Audio => "audio/mpeg",
            Self::Video => "video/mp4",
            Self::Code => "application/zip",
            Self::Slides => {
                "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            }
            Self::Excel => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    /// Whether this format produces binary content (not UTF-8 text).
    pub fn is_binary(&self) -> bool {
        matches!(
            self,
            Self::Pdf
                | Self::Image
                | Self::Audio
                | Self::Video
                | Self::Code
                | Self::Slides
                | Self::Excel
        )
    }
}

/// Infer the output format of a flow from the program AST.
///
/// Returns `None` if no strong signal is found.
pub fn infer_output_format(program: &Program, flow_name: &str) -> Option<OutputFormat> {
    let flow = program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Flow(f) if f.name == flow_name => Some(f),
        _ => None,
    })?;

    let mut scores: Vec<(OutputFormat, u32)> = Vec::new();
    let mut add = |fmt: OutputFormat, weight: u32| {
        if let Some(entry) = scores.iter_mut().find(|(f, _)| *f == fmt) {
            entry.1 += weight;
        } else {
            scores.push((fmt, weight));
        }
    };

    // Signal 1: return type name (weight 100)
    if let Some(ref ty) = flow.return_type {
        if let Some(fmt) = type_name_to_format(&ty.kind) {
            add(fmt, 100);
        }
    }

    // Collect tool names referenced in the flow body.
    let tool_names = collect_tool_refs(&flow.body);

    // Look up tool declarations.
    let tools: Vec<_> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) if tool_names.contains(&t.name.as_str()) => Some(t),
            _ => None,
        })
        .collect();

    for tool in &tools {
        // Signal 2: tool name (weight 50)
        if let Some(fmt) = text_to_format(&tool.name) {
            add(fmt, 50);
        }

        // Signal 3: tool description (weight 30)
        let desc = extract_prompt_text(&tool.description);
        if let Some(fmt) = text_to_format(&desc) {
            add(fmt, 30);
        }

        // Signal 4: tool return type (weight 40)
        if let Some(ref ty) = tool.return_type {
            if let Some(fmt) = type_name_to_format(&ty.kind) {
                add(fmt, 40);
            }
        }

        // Signal 5: tool output template name (weight 30)
        if let Some(output) = &tool.output {
            if let Some(fmt) = text_to_format(output) {
                add(fmt, 30);
            }
        }
    }

    // Signal 6: agent prompts of agents dispatched in the flow (weight 10)
    let agent_names = collect_agent_refs(&flow.body);
    for decl in &program.decls {
        if let DeclKind::Agent(agent) = &decl.kind {
            if agent_names.contains(&agent.name.as_str()) {
                if let Some(ref prompt_expr) = agent.prompt {
                    let prompt = extract_prompt_text(prompt_expr);
                    if let Some(fmt) = text_to_format(&prompt) {
                        add(fmt, 10);
                    }
                }
            }
        }
    }

    // Pick highest score.
    scores.sort_by(|a, b| b.1.cmp(&a.1));
    scores.first().map(|(fmt, _)| *fmt)
}

/// Try to infer format from raw output content (fallback heuristic).
///
/// Works best for text-based formats. Binary formats (Image, Audio, Video,
/// Code, Slides, Excel) cannot be reliably detected from text content —
/// use `infer_output_format` for AST-level inference instead.
pub fn infer_from_content(content: &str) -> OutputFormat {
    let trimmed = content.trim();

    if trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
    {
        return OutputFormat::Html;
    }
    if trimmed.starts_with('<') && trimmed.ends_with('>') && trimmed.contains("<svg") {
        return OutputFormat::Svg;
    }
    if trimmed.starts_with("<?xml")
        || (trimmed.starts_with('<') && trimmed.ends_with('>') && !trimmed.contains("<html"))
    {
        return OutputFormat::Xml;
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return OutputFormat::Json;
    }
    if trimmed.starts_with("---\n") || trimmed.contains("\n---\n") {
        return OutputFormat::Yaml;
    }
    // SQL detection: common keywords at start of lines.
    if trimmed.lines().take(5).any(|l| {
        let up = l.trim().to_uppercase();
        up.starts_with("SELECT ")
            || up.starts_with("CREATE ")
            || up.starts_with("INSERT ")
            || up.starts_with("ALTER ")
            || up.starts_with("DROP ")
            || up.starts_with("BEGIN")
    }) {
        return OutputFormat::Sql;
    }
    if trimmed.contains("\n# ") || trimmed.starts_with("# ") || trimmed.contains("\n## ") {
        return OutputFormat::Markdown;
    }
    if trimmed
        .lines()
        .all(|line| line.contains(',') || line.is_empty())
        && trimmed.lines().count() > 1
    {
        return OutputFormat::Csv;
    }

    OutputFormat::PlainText
}

/// Map a `TypeExprKind` to an output format.
fn type_name_to_format(kind: &pact_core::ast::types::TypeExprKind) -> Option<OutputFormat> {
    use pact_core::ast::types::TypeExprKind;
    match kind {
        TypeExprKind::Named(name) => match name.to_lowercase().as_str() {
            "html" | "webpage" | "website" => Some(OutputFormat::Html),
            "pdf" => Some(OutputFormat::Pdf),
            "markdown" | "md" => Some(OutputFormat::Markdown),
            "json" => Some(OutputFormat::Json),
            "csv" => Some(OutputFormat::Csv),
            "yaml" | "yml" => Some(OutputFormat::Yaml),
            "svg" => Some(OutputFormat::Svg),
            "xml" => Some(OutputFormat::Xml),
            "sql" => Some(OutputFormat::Sql),
            "image" | "png" | "jpg" | "jpeg" | "photo" => Some(OutputFormat::Image),
            "audio" | "mp3" | "wav" | "sound" | "voice" => Some(OutputFormat::Audio),
            "video" | "mp4" | "clip" | "animation" => Some(OutputFormat::Video),
            "code" | "project" | "archive" | "zip" => Some(OutputFormat::Code),
            "slides" | "presentation" | "pptx" | "deck" => Some(OutputFormat::Slides),
            "excel" | "xlsx" | "spreadsheet" | "workbook" => Some(OutputFormat::Excel),
            _ => None,
        },
        TypeExprKind::Optional(inner) => type_name_to_format(&inner.kind),
        TypeExprKind::Generic { .. } => None,
    }
}

/// Scan text (tool name, description, prompt) for format keywords.
fn text_to_format(text: &str) -> Option<OutputFormat> {
    let lower = text.to_lowercase();

    // Order matters: check more specific patterns first.
    let patterns: &[(&[&str], OutputFormat)] = &[
        // Media — check first to avoid false positives (e.g. "image" in "imagine")
        (
            &[
                "generate_image",
                "create_image",
                "dall-e",
                "dalle",
                "stable diffusion",
                "midjourney",
                "image_gen",
                "text_to_image",
                "text-to-image",
                "render_image",
            ],
            OutputFormat::Image,
        ),
        (
            &[
                "text_to_speech",
                "text-to-speech",
                "tts",
                "generate_audio",
                "create_audio",
                "synthesize_voice",
                "podcast",
                "narrat",
            ],
            OutputFormat::Audio,
        ),
        (
            &[
                "generate_video",
                "create_video",
                "text_to_video",
                "text-to-video",
                "animate",
                "render_video",
            ],
            OutputFormat::Video,
        ),
        // Office
        (
            &["slide", "presentation", "pptx", "deck", "powerpoint"],
            OutputFormat::Slides,
        ),
        (&["excel", "xlsx", "workbook"], OutputFormat::Excel),
        // Archive
        (
            &[
                "scaffold",
                "boilerplate",
                "project_gen",
                "code_gen",
                "generate_project",
                "create_project",
                "zip_code",
                "source_code",
            ],
            OutputFormat::Code,
        ),
        // Text formats
        (
            &["html", "webpage", "website", "web page", "web_page"],
            OutputFormat::Html,
        ),
        (&["pdf", "portable document"], OutputFormat::Pdf),
        (&["svg", "vector graphic"], OutputFormat::Svg),
        (
            &["csv", "comma-separated", "comma_separated"],
            OutputFormat::Csv,
        ),
        (&["yaml", "yml"], OutputFormat::Yaml),
        (&["xml"], OutputFormat::Xml),
        (
            &["sql", "database script", "migration", "query"],
            OutputFormat::Sql,
        ),
        (&["json"], OutputFormat::Json),
        (&["markdown", ".md"], OutputFormat::Markdown),
    ];

    for (keywords, fmt) in patterns {
        for kw in *keywords {
            if lower.contains(kw) {
                return Some(*fmt);
            }
        }
    }

    None
}

/// Collect tool names referenced in a flow body (via `ToolRef` or `AgentDispatch`).
fn collect_tool_refs(exprs: &[pact_core::ast::expr::Expr]) -> Vec<&str> {
    let mut names = Vec::new();
    for expr in exprs {
        collect_tool_refs_expr(expr, &mut names);
    }
    names
}

fn collect_tool_refs_expr<'a>(expr: &'a pact_core::ast::expr::Expr, names: &mut Vec<&'a str>) {
    match &expr.kind {
        ExprKind::ToolRef(name) => names.push(name),
        ExprKind::AgentDispatch { tool, args, .. } => {
            collect_tool_refs_expr(tool, names);
            for arg in args {
                collect_tool_refs_expr(arg, names);
            }
        }
        ExprKind::Assign { value, .. } => {
            collect_tool_refs_expr(value, names);
        }
        ExprKind::Return(inner) => {
            collect_tool_refs_expr(inner, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_tool_refs_expr(subject, names);
            for arm in arms {
                collect_tool_refs_expr(&arm.body, names);
            }
        }
        ExprKind::Pipeline { left, right } => {
            collect_tool_refs_expr(left, names);
            collect_tool_refs_expr(right, names);
        }
        ExprKind::FallbackChain { primary, fallback } => {
            collect_tool_refs_expr(primary, names);
            collect_tool_refs_expr(fallback, names);
        }
        ExprKind::Parallel(branches) => {
            for branch in branches {
                collect_tool_refs_expr(branch, names);
            }
        }
        ExprKind::FuncCall { callee, args } => {
            collect_tool_refs_expr(callee, names);
            for arg in args {
                collect_tool_refs_expr(arg, names);
            }
        }
        _ => {}
    }
}

/// Collect agent names referenced in a flow body.
fn collect_agent_refs(exprs: &[pact_core::ast::expr::Expr]) -> Vec<&str> {
    let mut names = Vec::new();
    for expr in exprs {
        collect_agent_refs_expr(expr, &mut names);
    }
    names
}

fn collect_agent_refs_expr<'a>(expr: &'a pact_core::ast::expr::Expr, names: &mut Vec<&'a str>) {
    match &expr.kind {
        ExprKind::AgentRef(name) => names.push(name),
        ExprKind::AgentDispatch {
            agent, tool, args, ..
        } => {
            collect_agent_refs_expr(agent, names);
            collect_agent_refs_expr(tool, names);
            for arg in args {
                collect_agent_refs_expr(arg, names);
            }
        }
        ExprKind::Assign { value, .. } => {
            collect_agent_refs_expr(value, names);
        }
        ExprKind::Return(inner) => {
            collect_agent_refs_expr(inner, names);
        }
        ExprKind::Match { subject, arms } => {
            collect_agent_refs_expr(subject, names);
            for arm in arms {
                collect_agent_refs_expr(&arm.body, names);
            }
        }
        ExprKind::Pipeline { left, right } => {
            collect_agent_refs_expr(left, names);
            collect_agent_refs_expr(right, names);
        }
        ExprKind::FallbackChain { primary, fallback } => {
            collect_agent_refs_expr(primary, names);
            collect_agent_refs_expr(fallback, names);
        }
        ExprKind::Parallel(branches) => {
            for branch in branches {
                collect_agent_refs_expr(branch, names);
            }
        }
        ExprKind::FuncCall { callee, args } => {
            collect_agent_refs_expr(callee, names);
            for arg in args {
                collect_agent_refs_expr(arg, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn infer_from_return_type() {
        let src = r#"
            tool #render {
                description: <<Render content.>>
                params { content :: String }
                returns :: String
            }
            agent @builder {
                tools: [#render]
                model: "claude-sonnet-4-20250514"
                prompt: <<You build things.>>
            }
            flow build_site(topic :: String) -> HTML {
                result = @builder -> #render(topic)
                return result
            }
        "#;
        let program = parse(src);
        let fmt = infer_output_format(&program, "build_site");
        assert_eq!(fmt, Some(OutputFormat::Html));
    }

    #[test]
    fn infer_from_tool_name() {
        let src = r#"
            tool #generate_pdf {
                description: <<Generate a document.>>
                params { data :: String }
                returns :: String
            }
            agent @writer {
                tools: [#generate_pdf]
                model: "claude-sonnet-4-20250514"
                prompt: <<You write documents.>>
            }
            flow make_report(data :: String) -> String {
                result = @writer -> #generate_pdf(data)
                return result
            }
        "#;
        let program = parse(src);
        let fmt = infer_output_format(&program, "make_report");
        assert_eq!(fmt, Some(OutputFormat::Pdf));
    }

    #[test]
    fn infer_from_tool_description() {
        let src = r#"
            tool #export {
                description: <<Export data as CSV spreadsheet.>>
                params { rows :: String }
                returns :: String
            }
            agent @exporter {
                tools: [#export]
                model: "claude-sonnet-4-20250514"
                prompt: <<You export data.>>
            }
            flow export_data(rows :: String) -> String {
                result = @exporter -> #export(rows)
                return result
            }
        "#;
        let program = parse(src);
        let fmt = infer_output_format(&program, "export_data");
        assert_eq!(fmt, Some(OutputFormat::Csv));
    }

    #[test]
    fn infer_from_agent_prompt() {
        let src = r#"
            tool #create {
                description: <<Create content.>>
                params { topic :: String }
                returns :: String
            }
            agent @web_dev {
                tools: [#create]
                model: "claude-sonnet-4-20250514"
                prompt: <<You are a website builder. You create HTML pages.>>
            }
            flow build(topic :: String) -> String {
                result = @web_dev -> #create(topic)
                return result
            }
        "#;
        let program = parse(src);
        let fmt = infer_output_format(&program, "build");
        assert_eq!(fmt, Some(OutputFormat::Html));
    }

    #[test]
    fn no_signal_returns_none() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                tools: [#greet]
                model: "claude-sonnet-4-20250514"
                prompt: <<You greet people.>>
            }
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let program = parse(src);
        let fmt = infer_output_format(&program, "hello");
        assert_eq!(fmt, None);
    }

    #[test]
    fn infer_content_html() {
        assert_eq!(
            infer_from_content("<!DOCTYPE html><html><body>Hi</body></html>"),
            OutputFormat::Html,
        );
    }

    #[test]
    fn infer_content_json() {
        assert_eq!(
            infer_from_content(r#"{"key": "value"}"#),
            OutputFormat::Json,
        );
    }

    #[test]
    fn infer_content_markdown() {
        assert_eq!(
            infer_from_content("# Title\n\nSome text"),
            OutputFormat::Markdown,
        );
    }

    #[test]
    fn infer_content_plain() {
        assert_eq!(
            infer_from_content("Just some plain text here."),
            OutputFormat::PlainText,
        );
    }

    #[test]
    fn nonexistent_flow_returns_none() {
        let src = r#"
            flow hello() -> String {
                return "hi"
            }
        "#;
        let program = parse(src);
        assert_eq!(infer_output_format(&program, "missing"), None);
    }

    #[test]
    fn infer_image_from_return_type() {
        let src = r#"
            tool #generate {
                description: <<Create visual content.>>
                params { input :: String }
                returns :: String
            }
            agent @artist {
                tools: [#generate]
                model: "claude-sonnet-4-20250514"
                prompt: <<You create art.>>
            }
            flow create_art(input :: String) -> Image {
                result = @artist -> #generate(input)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "create_art"),
            Some(OutputFormat::Image)
        );
    }

    #[test]
    fn infer_image_from_tool_name() {
        let src = r#"
            tool #generate_image {
                description: <<Create a picture.>>
                params { input :: String }
                returns :: String
            }
            agent @artist {
                tools: [#generate_image]
                model: "claude-sonnet-4-20250514"
                prompt: <<You create art.>>
            }
            flow draw(input :: String) -> String {
                result = @artist -> #generate_image(input)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "draw"),
            Some(OutputFormat::Image)
        );
    }

    #[test]
    fn infer_audio_from_tool_description() {
        let src = r#"
            tool #speak {
                description: <<Convert text to speech using TTS engine.>>
                params { text :: String }
                returns :: String
            }
            agent @narrator {
                tools: [#speak]
                model: "claude-sonnet-4-20250514"
                prompt: <<You narrate content.>>
            }
            flow narrate(text :: String) -> String {
                result = @narrator -> #speak(text)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "narrate"),
            Some(OutputFormat::Audio)
        );
    }

    #[test]
    fn infer_video_from_return_type() {
        let src = r#"
            tool #render {
                description: <<Render content.>>
                params { script :: String }
                returns :: String
            }
            agent @director {
                tools: [#render]
                model: "claude-sonnet-4-20250514"
                prompt: <<You direct videos.>>
            }
            flow produce(script :: String) -> Video {
                result = @director -> #render(script)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "produce"),
            Some(OutputFormat::Video)
        );
    }

    #[test]
    fn infer_slides_from_tool_name() {
        let src = r#"
            tool #create_presentation {
                description: <<Build a slide deck.>>
                params { topic :: String }
                returns :: String
            }
            agent @presenter {
                tools: [#create_presentation]
                model: "claude-sonnet-4-20250514"
                prompt: <<You make presentations.>>
            }
            flow present(topic :: String) -> String {
                result = @presenter -> #create_presentation(topic)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "present"),
            Some(OutputFormat::Slides)
        );
    }

    #[test]
    fn infer_excel_from_return_type() {
        let src = r#"
            tool #analyze {
                description: <<Analyze data.>>
                params { data :: String }
                returns :: String
            }
            agent @analyst {
                tools: [#analyze]
                model: "claude-sonnet-4-20250514"
                prompt: <<You analyze data.>>
            }
            flow report(data :: String) -> Spreadsheet {
                result = @analyst -> #analyze(data)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "report"),
            Some(OutputFormat::Excel)
        );
    }

    #[test]
    fn infer_code_from_tool_description() {
        let src = r#"
            tool #build {
                description: <<Scaffold a new project from template.>>
                params { name :: String }
                returns :: String
            }
            agent @dev {
                tools: [#build]
                model: "claude-sonnet-4-20250514"
                prompt: <<You build software.>>
            }
            flow scaffold(name :: String) -> String {
                result = @dev -> #build(name)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "scaffold"),
            Some(OutputFormat::Code)
        );
    }

    #[test]
    fn infer_sql_from_return_type() {
        let src = r#"
            tool #gen {
                description: <<Generate queries.>>
                params { input :: String }
                returns :: String
            }
            agent @dba {
                tools: [#gen]
                model: "claude-sonnet-4-20250514"
                prompt: <<You manage databases.>>
            }
            flow migrate(input :: String) -> SQL {
                result = @dba -> #gen(input)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "migrate"),
            Some(OutputFormat::Sql)
        );
    }

    #[test]
    fn infer_content_sql() {
        assert_eq!(
            infer_from_content("CREATE TABLE users (\n  id SERIAL PRIMARY KEY\n);"),
            OutputFormat::Sql,
        );
    }

    #[test]
    fn infer_content_xml() {
        assert_eq!(
            infer_from_content("<?xml version=\"1.0\"?>\n<root><item/></root>"),
            OutputFormat::Xml,
        );
    }

    #[test]
    fn infer_markdown_from_return_type() {
        let src = r#"
            tool #write {
                description: <<Write content.>>
                params { topic :: String }
                returns :: String
            }
            agent @writer {
                tools: [#write]
                model: "claude-sonnet-4-20250514"
                prompt: <<You write docs.>>
            }
            flow write_docs(topic :: String) -> Markdown {
                result = @writer -> #write(topic)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "write_docs"),
            Some(OutputFormat::Markdown)
        );
    }

    #[test]
    fn infer_json_from_return_type() {
        let src = r#"
            tool #fetch {
                description: <<Fetch data.>>
                params { url :: String }
                returns :: String
            }
            agent @api {
                tools: [#fetch]
                model: "claude-sonnet-4-20250514"
                prompt: <<You fetch API data.>>
            }
            flow get_data(url :: String) -> JSON {
                result = @api -> #fetch(url)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "get_data"),
            Some(OutputFormat::Json)
        );
    }

    #[test]
    fn infer_yaml_from_return_type() {
        let src = r#"
            tool #generate {
                description: <<Generate config.>>
                params { name :: String }
                returns :: String
            }
            agent @devops {
                tools: [#generate]
                model: "claude-sonnet-4-20250514"
                prompt: <<You create configs.>>
            }
            flow gen_config(name :: String) -> YAML {
                result = @devops -> #generate(name)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "gen_config"),
            Some(OutputFormat::Yaml)
        );
    }

    #[test]
    fn infer_svg_from_return_type() {
        let src = r#"
            tool #draw {
                description: <<Draw a diagram.>>
                params { spec :: String }
                returns :: String
            }
            agent @designer {
                tools: [#draw]
                model: "claude-sonnet-4-20250514"
                prompt: <<You draw diagrams.>>
            }
            flow diagram(spec :: String) -> SVG {
                result = @designer -> #draw(spec)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "diagram"),
            Some(OutputFormat::Svg)
        );
    }

    #[test]
    fn infer_xml_from_return_type() {
        let src = r#"
            tool #export {
                description: <<Export data.>>
                params { data :: String }
                returns :: String
            }
            agent @exporter {
                tools: [#export]
                model: "claude-sonnet-4-20250514"
                prompt: <<You export data.>>
            }
            flow export(data :: String) -> XML {
                result = @exporter -> #export(data)
                return result
            }
        "#;
        let program = parse(src);
        assert_eq!(
            infer_output_format(&program, "export"),
            Some(OutputFormat::Xml)
        );
    }

    #[test]
    fn infer_plain_text_fallback() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                tools: [#greet]
                model: "claude-sonnet-4-20250514"
                prompt: <<You greet people.>>
            }
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let program = parse(src);
        // No format signal — should return None (caller uses infer_from_content as fallback).
        assert_eq!(infer_output_format(&program, "hello"), None);
        // Content fallback gives PlainText for generic strings.
        assert_eq!(infer_from_content("Hello, world!"), OutputFormat::PlainText);
    }

    #[test]
    fn infer_content_csv() {
        assert_eq!(
            infer_from_content("name,age,city\nAlice,30,Stockholm\nBob,25,Gothenburg"),
            OutputFormat::Csv,
        );
    }

    #[test]
    fn infer_content_yaml() {
        assert_eq!(
            infer_from_content("---\nname: test\nversion: 1.0\n"),
            OutputFormat::Yaml,
        );
    }

    #[test]
    fn infer_content_svg() {
        assert_eq!(
            infer_from_content(
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"50\"/></svg>"
            ),
            OutputFormat::Svg,
        );
    }

    #[test]
    fn binary_format_detection() {
        assert!(OutputFormat::Image.is_binary());
        assert!(OutputFormat::Audio.is_binary());
        assert!(OutputFormat::Video.is_binary());
        assert!(OutputFormat::Code.is_binary());
        assert!(OutputFormat::Slides.is_binary());
        assert!(OutputFormat::Excel.is_binary());
        assert!(OutputFormat::Pdf.is_binary());
        assert!(!OutputFormat::Html.is_binary());
        assert!(!OutputFormat::Json.is_binary());
        assert!(!OutputFormat::PlainText.is_binary());
    }
}
