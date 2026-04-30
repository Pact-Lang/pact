// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-09-18

//! Claude tool_use JSON generation.
//!
//! Converts PACT tool declarations into the Anthropic Claude `tool_use` format.
//! This is used by:
//! - `pact build` to generate Claude-compatible tool definitions
//! - `pact run --dispatch claude` to construct API requests
//!
//! # Claude Tool Format
//!
//! ```json
//! {
//!   "name": "tool_name",
//!   "description": "What this tool does",
//!   "input_schema": {
//!     "type": "object",
//!     "properties": { ... },
//!     "required": [...]
//!   }
//! }
//! ```

use crate::emit_common::type_to_json_schema;
use pact_core::ast::expr::ExprKind;
use pact_core::ast::stmt::{AgentDecl, DeclKind, DirectiveDecl, Program, TemplateDecl, ToolDecl};
use pact_core::template::render_template;
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

/// A Claude-compatible tool definition.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeTool {
    /// The tool name as declared in PACT.
    pub name: String,
    /// Human-readable description of what the tool does.
    pub description: String,
    /// JSON Schema describing the tool's input parameters.
    pub input_schema: JsonValue,
}

/// A complete Claude API request payload for an agent.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeRequest {
    /// Claude model identifier (e.g. `"claude-sonnet-4-20250514"`).
    pub model: String,
    /// Maximum number of tokens to generate.
    pub max_tokens: u32,
    /// Optional system prompt for the conversation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Tools available to the agent.
    pub tools: Vec<ClaudeTool>,
    /// Conversation messages (starts with a user message).
    pub messages: Vec<ClaudeMessage>,
}

/// A message in a Claude conversation.
#[derive(Debug, Clone, Serialize)]
pub struct ClaudeMessage {
    /// Message role (`"user"` or `"assistant"`).
    pub role: String,
    /// Message content (text or structured content blocks).
    pub content: JsonValue,
}

/// Convert a PACT tool declaration to a Claude tool definition.
///
/// If a `program` is provided and the tool has an `output` template reference,
/// the rendered template format instructions are appended to the description.
pub fn tool_to_claude(tool: &ToolDecl) -> ClaudeTool {
    tool_to_claude_with_program(tool, None)
}

/// Convert a PACT tool declaration to a Claude tool definition, with optional
/// program context for resolving output template references.
pub fn tool_to_claude_with_program(tool: &ToolDecl, program: Option<&Program>) -> ClaudeTool {
    let mut description = match &tool.description.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => s.trim().to_string(),
        _ => String::new(),
    };

    // Handle MCP-imported tools
    if tool.mcp_import.is_some() && tool.params.is_empty() {
        description.push_str("\n\n[MCP-imported tool — schema resolved at runtime]");
    }

    if let Some(source) = &tool.source {
        if source.args.is_empty() {
            description.push_str(&format!(
                "\n\n[Backed by built-in provider: ^{}]",
                source.capability
            ));
        } else {
            description.push_str(&format!(
                "\n\n[Backed by built-in provider: ^{}({})]",
                source.capability,
                source.args.join(", ")
            ));
        }
    }

    // Append template output format if specified
    if let Some(template_name) = &tool.output {
        if let Some(prog) = program {
            if let Some(template) = find_template(prog, template_name) {
                let format_instructions = render_template(template);
                description.push_str("\n\n");
                description.push_str(&format_instructions);
            }
        }
    }

    // Append directive prompt blocks if specified
    if let Some(program) = program {
        if !tool.directives.is_empty() {
            let directive_texts: Vec<String> = tool
                .directives
                .iter()
                .filter_map(|name| find_directive(program, name))
                .map(pact_core::template::render_directive)
                .collect();
            if !directive_texts.is_empty() {
                description.push_str("\n\n");
                description.push_str(&directive_texts.join("\n\n"));
            }
        }
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for param in &tool.params {
        let type_schema = param
            .ty
            .as_ref()
            .map(type_to_json_schema)
            .unwrap_or_else(|| json!({}));

        let mut prop = type_schema;
        // Add a description for the parameter
        if let Some(obj) = prop.as_object_mut() {
            obj.insert(
                "description".to_string(),
                json!(format!("{} parameter", param.name)),
            );
        }

        properties.insert(param.name.clone(), prop);
        // All params are required for now
        required.push(json!(param.name));
    }

    // If the tool has no explicit params but is backed by a connector,
    // inject the known parameter schema for that connector operation.
    if properties.is_empty() {
        if let Some(handler) = &tool.handler {
            if let Some(op) = handler.strip_prefix("connector ") {
                let (props, reqs) = connector_operation_schema(op);
                properties = props;
                required = reqs;
            }
        }
    }

    let input_schema = json!({
        "type": "object",
        "properties": properties,
        "required": required,
    });

    ClaudeTool {
        name: tool.name.clone(),
        description,
        input_schema,
    }
}

/// Return the JSON Schema properties and required list for a connector operation.
///
/// This provides the LLM with the parameter schema it needs to call
/// connector-backed tools correctly. Parameters that are always resolved
/// from connector credentials (like `owner`/`repo` for GitHub) are omitted
/// so the LLM only sees what it actually needs to provide.
fn connector_operation_schema(
    operation: &str,
) -> (serde_json::Map<String, JsonValue>, Vec<JsonValue>) {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    match operation {
        // ── GitHub ──────────────────────────────────────────────
        "github.push_file" => {
            properties.insert("path".into(), json!({"type": "string", "description": "File path in the repository (e.g. \"web/index.html\")"}));
            properties.insert("content".into(), json!({"type": "string", "description": "File content to write"}));
            properties.insert("message".into(), json!({"type": "string", "description": "Commit message (defaults to \"Update via PACT flow\")"}));
            properties.insert("branch".into(), json!({"type": "string", "description": "Target branch (defaults to \"main\")"}));
            required.push(json!("path"));
            required.push(json!("content"));
        }
        "github.create_pr" => {
            properties.insert("title".into(), json!({"type": "string", "description": "Pull request title"}));
            properties.insert("body".into(), json!({"type": "string", "description": "Pull request description (optional)"}));
            properties.insert("base".into(), json!({"type": "string", "description": "Base branch to merge into (defaults to \"main\")"}));
            properties.insert("head".into(), json!({"type": "string", "description": "Branch name (auto-generated if files is provided)"}));
            properties.insert("files".into(), json!({"type": "string", "description": "JSON object mapping file paths to content. When provided, automatically creates branch, pushes files, and opens PR. Example: {\"web/index.html\": \"<html>...</html>\", \"web/style.css\": \"body{}\"}"}));
            required.push(json!("title"));
            required.push(json!("files"));
        }
        "github.create_issue" => {
            properties.insert("title".into(), json!({"type": "string", "description": "Issue title"}));
            properties.insert("body".into(), json!({"type": "string", "description": "Issue body text (optional)"}));
            properties.insert("labels".into(), json!({"type": "string", "description": "Comma-separated label names (optional)"}));
            required.push(json!("title"));
        }
        "github.read_file" => {
            properties.insert("path".into(), json!({"type": "string", "description": "File path in the repository"}));
            properties.insert("branch".into(), json!({"type": "string", "description": "Branch name (defaults to \"main\")"}));
            required.push(json!("path"));
        }
        "github.list_repos" => {
            // owner comes from credentials
        }
        "github.list_issues" => {
            properties.insert("state".into(), json!({"type": "string", "description": "Filter by state: open, closed, or all (defaults to \"open\")"}));
            properties.insert("labels".into(), json!({"type": "string", "description": "Comma-separated label names to filter by"}));
            properties.insert("since".into(), json!({"type": "string", "description": "ISO 8601 timestamp to filter issues updated after"}));
        }
        "github.list_pulls" => {
            properties.insert("state".into(), json!({"type": "string", "description": "Filter by state: open, closed, or all (defaults to \"open\")"}));
        }
        "github.get_issue_comments" => {
            properties.insert("issue_number".into(), json!({"type": "string", "description": "Issue number"}));
            required.push(json!("issue_number"));
        }
        "github.get_pull_comments" => {
            properties.insert("pull_number".into(), json!({"type": "string", "description": "Pull request number"}));
            required.push(json!("pull_number"));
        }
        // ── Mermaid Chart ──────────────────────────────────────
        "mermaid.list_projects" => {
            // No params needed
        }
        "mermaid.list_documents" => {
            properties.insert("project_id".into(), json!({"type": "string", "description": "Project ID (auto-detected if omitted)"}));
        }
        "mermaid.get_document" => {
            properties.insert("document_id".into(), json!({"type": "string", "description": "Document/diagram ID"}));
            required.push(json!("document_id"));
        }
        "mermaid.create_document" => {
            properties.insert("code".into(), json!({"type": "string", "description": "Mermaid diagram code (e.g. \"graph TD\\n    A --> B\")"}));
            properties.insert("title".into(), json!({"type": "string", "description": "Document title (defaults to \"Untitled\")"}));
            properties.insert("project_id".into(), json!({"type": "string", "description": "Project ID (auto-detected if omitted)"}));
            required.push(json!("code"));
        }
        "mermaid.update_document" => {
            properties.insert("document_id".into(), json!({"type": "string", "description": "Document ID to update"}));
            properties.insert("code".into(), json!({"type": "string", "description": "Updated Mermaid diagram code"}));
            properties.insert("title".into(), json!({"type": "string", "description": "Updated title (optional)"}));
            properties.insert("project_id".into(), json!({"type": "string", "description": "Project ID (auto-detected if omitted)"}));
            required.push(json!("document_id"));
            required.push(json!("code"));
        }
        // ── Slack ──────────────────────────────────────────────
        "slack.post_message" => {
            properties.insert("channel".into(), json!({"type": "string", "description": "Channel ID or name to post to"}));
            properties.insert("text".into(), json!({"type": "string", "description": "Message text"}));
            required.push(json!("text"));
        }
        "slack.upload_file" => {
            properties.insert("channel".into(), json!({"type": "string", "description": "Channel ID or name"}));
            properties.insert("content".into(), json!({"type": "string", "description": "File content"}));
            properties.insert("filename".into(), json!({"type": "string", "description": "File name"}));
            required.push(json!("content"));
            required.push(json!("filename"));
        }
        "slack.add_reaction" => {
            properties.insert("channel".into(), json!({"type": "string", "description": "Channel ID"}));
            properties.insert("timestamp".into(), json!({"type": "string", "description": "Message timestamp"}));
            properties.insert("name".into(), json!({"type": "string", "description": "Emoji name (without colons)"}));
            required.push(json!("channel"));
            required.push(json!("timestamp"));
            required.push(json!("name"));
        }
        // ── Figma ──────────────────────────────────────────────
        "figma.get_file" => {
            properties.insert("file_key".into(), json!({"type": "string", "description": "Figma file key"}));
            required.push(json!("file_key"));
        }
        "figma.export_node" => {
            properties.insert("file_key".into(), json!({"type": "string", "description": "Figma file key"}));
            properties.insert("node_id".into(), json!({"type": "string", "description": "Node ID to export"}));
            properties.insert("format".into(), json!({"type": "string", "description": "Export format: png, svg, jpg, or pdf (defaults to \"png\")"}));
            required.push(json!("file_key"));
            required.push(json!("node_id"));
        }
        "figma.get_components" => {
            properties.insert("file_key".into(), json!({"type": "string", "description": "Figma file key"}));
            required.push(json!("file_key"));
        }
        "figma.get_styles" => {
            properties.insert("file_key".into(), json!({"type": "string", "description": "Figma file key"}));
            required.push(json!("file_key"));
        }
        // ── Resend ─────────────────────────────────────────────
        "resend.send_email" => {
            properties.insert("to".into(), json!({"type": "string", "description": "Recipient email address"}));
            properties.insert("subject".into(), json!({"type": "string", "description": "Email subject line"}));
            properties.insert("html".into(), json!({"type": "string", "description": "Email body as HTML"}));
            required.push(json!("to"));
            required.push(json!("subject"));
            required.push(json!("html"));
        }
        "resend.send_email_with_attachment" => {
            properties.insert("to".into(), json!({"type": "string", "description": "Recipient email address"}));
            properties.insert("subject".into(), json!({"type": "string", "description": "Email subject line"}));
            properties.insert("html".into(), json!({"type": "string", "description": "Email body as HTML"}));
            properties.insert("filename".into(), json!({"type": "string", "description": "Attachment filename"}));
            properties.insert("content".into(), json!({"type": "string", "description": "Base64-encoded attachment content"}));
            required.push(json!("to"));
            required.push(json!("subject"));
            required.push(json!("html"));
            required.push(json!("filename"));
            required.push(json!("content"));
        }
        // ── Google Drive ───────────────────────────────────────
        "gdrive.upload" => {
            properties.insert("name".into(), json!({"type": "string", "description": "File name"}));
            properties.insert("content".into(), json!({"type": "string", "description": "File content"}));
            properties.insert("mime_type".into(), json!({"type": "string", "description": "MIME type (e.g. \"text/html\")"}));
            properties.insert("folder_id".into(), json!({"type": "string", "description": "Parent folder ID (uses default if omitted)"}));
            required.push(json!("name"));
            required.push(json!("content"));
        }
        "gdrive.create_folder" => {
            properties.insert("name".into(), json!({"type": "string", "description": "Folder name"}));
            properties.insert("parent_id".into(), json!({"type": "string", "description": "Parent folder ID (optional)"}));
            required.push(json!("name"));
        }
        "gdrive.list" => {
            properties.insert("folder_id".into(), json!({"type": "string", "description": "Folder ID to list (uses default if omitted)"}));
            properties.insert("query".into(), json!({"type": "string", "description": "Search query (optional)"}));
        }
        "gdrive.share" => {
            properties.insert("file_id".into(), json!({"type": "string", "description": "File or folder ID to share"}));
            properties.insert("email".into(), json!({"type": "string", "description": "Email address to share with"}));
            properties.insert("role".into(), json!({"type": "string", "description": "Permission role: reader, writer, or commenter (defaults to \"reader\")"}));
            required.push(json!("file_id"));
            required.push(json!("email"));
        }
        "gdrive.get" => {
            properties.insert("file_id".into(), json!({"type": "string", "description": "File ID to retrieve"}));
            required.push(json!("file_id"));
        }
        _ => {
            // Unknown connector operation — leave schema empty
        }
    }

    (properties, required)
}

/// Find a template declaration by name in the program.
fn find_template<'a>(program: &'a Program, name: &str) -> Option<&'a TemplateDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Template(t) if t.name == name => Some(t),
        _ => None,
    })
}

/// Find a directive declaration by name in the program.
fn find_directive<'a>(program: &'a Program, name: &str) -> Option<&'a DirectiveDecl> {
    program.decls.iter().find_map(|d| match &d.kind {
        DeclKind::Directive(dir) if dir.name == name => Some(dir),
        _ => None,
    })
}

/// Build a complete Claude API request for an agent.
///
/// This constructs the request payload that would be sent to the
/// Anthropic Messages API, including the agent's model, system prompt,
/// tools, and an initial user message.
pub fn build_agent_request(
    agent: &AgentDecl,
    program: &Program,
    user_message: &str,
) -> ClaudeRequest {
    // Extract model
    let model = agent
        .model
        .as_ref()
        .and_then(|e| match &e.kind {
            ExprKind::StringLit(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

    // Extract system prompt
    let system = agent.prompt.as_ref().and_then(|e| match &e.kind {
        ExprKind::PromptLit(s) | ExprKind::StringLit(s) => Some(s.trim().to_string()),
        _ => None,
    });

    // Collect tools for this agent
    let tool_names: Vec<&str> = agent
        .tools
        .iter()
        .filter_map(|e| match &e.kind {
            ExprKind::ToolRef(name) => Some(name.as_str()),
            _ => None,
        })
        .collect();

    let tools: Vec<ClaudeTool> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) if tool_names.contains(&t.name.as_str()) => {
                Some(tool_to_claude_with_program(t, Some(program)))
            }
            _ => None,
        })
        .collect();

    ClaudeRequest {
        model,
        max_tokens: 16384,
        system,
        tools,
        messages: vec![ClaudeMessage {
            role: "user".to_string(),
            content: json!(user_message),
        }],
    }
}

/// Generate the Claude tools JSON file content for all tools in a program.
pub fn generate_claude_tools_json(program: &Program) -> String {
    let tools: Vec<ClaudeTool> = program
        .decls
        .iter()
        .filter_map(|d| match &d.kind {
            DeclKind::Tool(t) => Some(tool_to_claude_with_program(t, Some(program))),
            _ => None,
        })
        .collect();

    serde_json::to_string_pretty(&tools).expect("JSON serialization should not fail")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pact_core::lexer::Lexer;
    use pact_core::parser::Parser;
    use pact_core::span::SourceMap;

    fn parse_program(src: &str) -> Program {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", src);
        let tokens = Lexer::new(src, id).lex().unwrap();
        Parser::new(&tokens).parse().unwrap()
    }

    #[test]
    fn tool_to_claude_basic() {
        let src = r#"tool #greet {
            description: <<Generate a greeting message.>>
            requires: [^llm.query]
            params {
                name :: String
            }
            returns :: String
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let claude_tool = tool_to_claude(tool);
            assert_eq!(claude_tool.name, "greet");
            assert_eq!(claude_tool.description, "Generate a greeting message.");

            let schema = &claude_tool.input_schema;
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["properties"]["name"]["type"], "string");
            assert_eq!(schema["required"][0], "name");
        }
    }

    #[test]
    fn tool_with_multiple_params() {
        let src = r#"tool #search {
            description: <<Search for things.>>
            requires: [^net.read]
            params {
                query :: String
                limit :: Int
            }
            returns :: List<String>
        }"#;
        let program = parse_program(src);
        if let DeclKind::Tool(tool) = &program.decls[0].kind {
            let claude_tool = tool_to_claude(tool);
            let schema = &claude_tool.input_schema;
            assert_eq!(schema["properties"]["query"]["type"], "string");
            assert_eq!(schema["properties"]["limit"]["type"], "integer");
            assert_eq!(schema["required"].as_array().unwrap().len(), 2);
        }
    }

    #[test]
    fn type_mapping() {
        use pact_core::ast::types::{TypeExpr, TypeExprKind};
        use pact_core::span::{SourceId, Span};

        let span = Span::new(SourceId(0), 0, 0);

        let string_ty = TypeExpr {
            kind: TypeExprKind::Named("String".into()),
            span,
        };
        assert_eq!(type_to_json_schema(&string_ty), json!({"type": "string"}));

        let int_ty = TypeExpr {
            kind: TypeExprKind::Named("Int".into()),
            span,
        };
        assert_eq!(type_to_json_schema(&int_ty), json!({"type": "integer"}));

        let bool_ty = TypeExpr {
            kind: TypeExprKind::Named("Bool".into()),
            span,
        };
        assert_eq!(type_to_json_schema(&bool_ty), json!({"type": "boolean"}));

        let list_ty = TypeExpr {
            kind: TypeExprKind::Generic {
                name: "List".into(),
                args: vec![TypeExpr {
                    kind: TypeExprKind::Named("String".into()),
                    span,
                }],
            },
            span,
        };
        assert_eq!(
            type_to_json_schema(&list_ty),
            json!({"type": "array", "items": {"type": "string"}})
        );
    }

    #[test]
    fn build_agent_request_full() {
        let src = r#"
            tool #greet {
                description: <<Say hello.>>
                requires: [^llm.query]
                params { name :: String }
                returns :: String
            }
            agent @greeter {
                permits: [^llm.query]
                tools: [#greet]
                model: "claude-sonnet-4-20250514"
                prompt: <<You are a friendly greeter.>>
            }
        "#;
        let program = parse_program(src);
        if let DeclKind::Agent(agent) = &program.decls[1].kind {
            let request = build_agent_request(agent, &program, "Greet the world");
            assert_eq!(request.model, "claude-sonnet-4-20250514");
            assert_eq!(
                request.system,
                Some("You are a friendly greeter.".to_string())
            );
            assert_eq!(request.tools.len(), 1);
            assert_eq!(request.tools[0].name, "greet");
            assert_eq!(request.messages[0].role, "user");
        }
    }

    #[test]
    fn generate_claude_tools_json_output() {
        let src = r#"
            tool #a { description: <<Tool A>> requires: [] params { x :: String } }
            tool #b { description: <<Tool B>> requires: [] params { y :: Int } }
        "#;
        let program = parse_program(src);
        let json_str = generate_claude_tools_json(&program);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["name"], "a");
        assert_eq!(parsed[1]["name"], "b");
    }
}
