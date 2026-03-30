// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-09-05

//! # pact-build
//!
//! Multi-target compiler backend for the PACT language. Generates deployment
//! artifacts from a checked AST for five platforms:
//!
//! - **Claude** — Anthropic `tool_use` JSON (`input_schema`)
//! - **OpenAI** — Function calling JSON (`parameters`, `strict: true`)
//! - **CrewAI** — YAML agent/task configuration
//! - **Cursor** — `.cursorrules` + `.cursor/mcp.json`
//! - **Gemini** — Google function declarations (uppercase types)
//!
//! All targets also produce:
//! - **TOML configs** — agent, tool, flow, skill, and permission definitions
//! - **Markdown prompts** — system prompts for each agent
//! - **Agent cards** — A2A discovery JSON (opt-in via `--agent-cards`)
//!
//! ## Architecture
//!
//! ```text
//! AST (from pact-core) → pact-build → Output Directory
//!                                      ├── pact.toml
//!                                      ├── agents/
//!                                      │   ├── <name>.toml
//!                                      │   ├── <name>.prompt.md
//!                                      │   └── <name>.agent_card.json  (--agent-cards)
//!                                      ├── tools/
//!                                      │   ├── <name>.toml
//!                                      │   └── <target>_tools.json
//!                                      ├── flows/
//!                                      │   └── <name>.toml
//!                                      └── permissions.toml
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use pact_build::{build, config::{BuildConfig, Target}};
//! use pact_core::ast::stmt::Program;
//!
//! # fn example(program: Program) -> Result<(), pact_build::BuildError> {
//! // Build for any supported target
//! let config = BuildConfig::new("example.pact", "./pact-out", Target::Claude);
//! // Also: Target::OpenAI, Target::CrewAI, Target::Cursor, Target::Gemini
//! build(&program, &config)?;
//! # Ok(())
//! # }
//! ```

/// Built-in capability provider definitions.
pub mod builtins;
/// Build configuration and target selection.
pub mod config;
/// Agent card JSON emission for A2A discovery.
pub mod emit_agent_card;
/// Claude tool_use JSON emission.
pub mod emit_claude;
/// CLAUDE.md process memory generation.
pub mod emit_claude_md;
/// Shared utilities for JSON schema generation.
pub mod emit_common;
/// CrewAI YAML configuration emission.
pub mod emit_crewai;
/// Cursor rules and MCP configuration emission.
pub mod emit_cursor;
/// Google Gemini function declarations emission.
pub mod emit_gemini;
/// Markdown prompt generation.
pub mod emit_markdown;
/// OpenAI function-calling JSON emission.
pub mod emit_openai;
/// Claude Code skill file generation.
pub mod emit_skill;
/// TOML configuration emission.
pub mod emit_toml;
/// Runtime guardrail enforcement helpers.
pub mod guardrails;
/// MCP server recommendation engine.
pub mod mcp_recommend;
/// Output format inference from program AST.
pub mod output_format;

use config::BuildConfig;
use pact_core::ast::stmt::{DeclKind, Program};

use thiserror::Error;

/// Errors that can occur during the build process.
#[derive(Debug, Error)]
pub enum BuildError {
    /// An I/O error occurred while writing output files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The input program contains no declarations.
    #[error("no declarations found in program")]
    EmptyProgram,
}

/// Build all output artifacts from a checked PACT program.
///
/// Creates the output directory structure and writes all TOML configs,
/// Markdown prompts, and Claude tool JSON files.
pub fn build(program: &Program, config: &BuildConfig) -> Result<(), BuildError> {
    if program.decls.is_empty() {
        return Err(BuildError::EmptyProgram);
    }

    // Create directory structure
    config.create_dirs()?;

    // Write project manifest
    let manifest = emit_toml::generate_manifest(program, config);
    std::fs::write(config.manifest_path(), manifest)?;

    // Write agent configs and prompts
    for decl in &program.decls {
        if let DeclKind::Agent(agent) = &decl.kind {
            let agent_toml = emit_toml::generate_agent_toml(agent);
            std::fs::write(
                config.agents_dir().join(format!("{}.toml", agent.name)),
                agent_toml,
            )?;

            let prompt_md = emit_markdown::generate_agent_prompt(agent, program);
            std::fs::write(
                config
                    .agents_dir()
                    .join(format!("{}.prompt.md", agent.name)),
                prompt_md,
            )?;
        }
    }

    // Write tool configs
    for decl in &program.decls {
        if let DeclKind::Tool(tool) = &decl.kind {
            let tool_toml = emit_toml::generate_tool_toml(tool);
            std::fs::write(
                config.tools_dir().join(format!("{}.toml", tool.name)),
                tool_toml,
            )?;
        }
    }

    // Write target-specific tool definitions
    match config.target {
        config::Target::Claude => {
            let json = emit_claude::generate_claude_tools_json(program);
            std::fs::write(config.tools_dir().join("claude_tools.json"), json)?;
        }
        config::Target::OpenAI => {
            let json = emit_openai::generate_openai_tools_json(program);
            std::fs::write(config.tools_dir().join("openai_tools.json"), json)?;
        }
        config::Target::CrewAI => {
            let yaml = emit_crewai::generate_crewai_config(program);
            std::fs::write(config.out_dir.join("crew.yaml"), yaml)?;
        }
        config::Target::Cursor => {
            let rules = emit_cursor::generate_cursor_rules(program);
            std::fs::write(config.out_dir.join(".cursorrules"), rules)?;
            let mcp = emit_cursor::generate_cursor_mcp_json(program);
            let cursor_dir = config.out_dir.join(".cursor");
            std::fs::create_dir_all(&cursor_dir)?;
            std::fs::write(cursor_dir.join("mcp.json"), mcp)?;
        }
        config::Target::Gemini => {
            let json = emit_gemini::generate_gemini_tools_json(program);
            std::fs::write(config.tools_dir().join("gemini_tools.json"), json)?;
        }
    }

    // Write skill configs
    for decl in &program.decls {
        if let DeclKind::Skill(skill) = &decl.kind {
            let skill_toml = emit_toml::generate_skill_toml(skill);
            std::fs::write(
                config.skills_dir().join(format!("{}.toml", skill.name)),
                skill_toml,
            )?;
        }
    }

    // Write flow configs
    for decl in &program.decls {
        if let DeclKind::Flow(flow) = &decl.kind {
            let flow_toml = emit_toml::generate_flow_toml(flow);
            std::fs::write(
                config.flows_dir().join(format!("{}.toml", flow.name)),
                flow_toml,
            )?;
        }
    }

    // Write permissions config
    let permissions = emit_toml::generate_permissions_toml(program);
    std::fs::write(config.permissions_path(), permissions)?;

    // Write Claude Code skill files (--claude-skill)
    if config.emit_claude_skill {
        for (path, content) in emit_skill::generate_all_skills(program) {
            let full = config.out_dir.join(&path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(full, content)?;
        }
    }

    // Write CLAUDE.md (--claude-md)
    if config.emit_claude_md {
        let md = emit_claude_md::generate_claude_md(program, config);
        std::fs::write(config.out_dir.join("CLAUDE.md"), md)?;
    }

    // Write MCP recommendations (--recommend-mcp)
    if config.emit_mcp_recommendations {
        let md = mcp_recommend::generate_recommendations_md(program);
        std::fs::write(config.out_dir.join("mcp_recommendations.md"), md)?;
    }

    // Write agent card JSON files (--agent-cards)
    if config.emit_agent_cards {
        for (filename, content) in
            emit_agent_card::generate_all_agent_cards(program, config.source_name())
        {
            std::fs::write(config.agents_dir().join(filename), content)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Target;
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
    fn build_creates_output_files() {
        let src = r#"
            permit_tree {
                ^llm { ^llm.query }
            }
            tool #greet {
                description: <<Generate a greeting.>>
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
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#;
        let program = parse_program(src);

        let tmp = std::env::temp_dir().join("pact-build-test");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = BuildConfig::new("test.pact", &tmp, Target::Claude);
        build(&program, &config).unwrap();

        // Verify files exist
        assert!(tmp.join("pact.toml").exists());
        assert!(tmp.join("agents/greeter.toml").exists());
        assert!(tmp.join("agents/greeter.prompt.md").exists());
        assert!(tmp.join("tools/greet.toml").exists());
        assert!(tmp.join("tools/claude_tools.json").exists());
        assert!(tmp.join("flows/hello.toml").exists());
        assert!(tmp.join("permissions.toml").exists());

        // Verify content
        let manifest = std::fs::read_to_string(tmp.join("pact.toml")).unwrap();
        assert!(manifest.contains("version = \"0.2\""));
        assert!(manifest.contains("target = \"claude\""));

        let agent = std::fs::read_to_string(tmp.join("agents/greeter.toml")).unwrap();
        assert!(agent.contains("name = \"greeter\""));

        let prompt = std::fs::read_to_string(tmp.join("agents/greeter.prompt.md")).unwrap();
        assert!(prompt.contains("You are a friendly greeter."));

        let claude = std::fs::read_to_string(tmp.join("tools/claude_tools.json")).unwrap();
        assert!(claude.contains("\"name\": \"greet\""));

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_empty_program_fails() {
        let program = Program { decls: vec![] };
        let config = BuildConfig::new("empty.pact", "/tmp/empty", Target::Claude);
        let result = build(&program, &config);
        assert!(matches!(result, Err(BuildError::EmptyProgram)));
    }

    /// Shared source for multi-target build tests.
    fn multi_target_src() -> &'static str {
        r#"
            tool #greet {
                description: <<Generate a greeting.>>
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
            flow hello(name :: String) -> String {
                result = @greeter -> #greet(name)
                return result
            }
        "#
    }

    #[test]
    fn build_openai_target() {
        let program = parse_program(multi_target_src());
        let tmp = std::env::temp_dir().join("pact-build-test-openai");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = BuildConfig::new("test.pact", &tmp, Target::OpenAI);
        build(&program, &config).unwrap();

        let json_path = tmp.join("tools/openai_tools.json");
        assert!(json_path.exists(), "openai_tools.json should exist");

        let json = std::fs::read_to_string(&json_path).unwrap();
        assert!(
            json.contains("\"function\""),
            "OpenAI tools JSON should contain \"function\" wrapper"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_crewai_target() {
        let program = parse_program(multi_target_src());
        let tmp = std::env::temp_dir().join("pact-build-test-crewai");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = BuildConfig::new("test.pact", &tmp, Target::CrewAI);
        build(&program, &config).unwrap();

        let yaml_path = tmp.join("crew.yaml");
        assert!(yaml_path.exists(), "crew.yaml should exist");

        let yaml = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(
            yaml.contains("agent") || yaml.contains("task"),
            "crew.yaml should contain agent or task entries"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_cursor_target() {
        let program = parse_program(multi_target_src());
        let tmp = std::env::temp_dir().join("pact-build-test-cursor");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = BuildConfig::new("test.pact", &tmp, Target::Cursor);
        build(&program, &config).unwrap();

        assert!(
            tmp.join(".cursorrules").exists(),
            ".cursorrules should exist"
        );
        assert!(
            tmp.join(".cursor/mcp.json").exists(),
            ".cursor/mcp.json should exist"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_gemini_target() {
        let program = parse_program(multi_target_src());
        let tmp = std::env::temp_dir().join("pact-build-test-gemini");
        let _ = std::fs::remove_dir_all(&tmp);

        let config = BuildConfig::new("test.pact", &tmp, Target::Gemini);
        build(&program, &config).unwrap();

        let json_path = tmp.join("tools/gemini_tools.json");
        assert!(json_path.exists(), "gemini_tools.json should exist");

        let json = std::fs::read_to_string(&json_path).unwrap();
        assert!(
            json.contains("STRING") || json.contains("OBJECT") || json.contains("NUMBER"),
            "Gemini tools JSON should contain uppercase types"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_with_agent_cards() {
        let program = parse_program(multi_target_src());
        let tmp = std::env::temp_dir().join("pact-build-test-agent-cards");
        let _ = std::fs::remove_dir_all(&tmp);

        let mut config = BuildConfig::new("test.pact", &tmp, Target::Claude);
        config.emit_agent_cards = true;
        build(&program, &config).unwrap();

        let agents_dir = tmp.join("agents");
        let has_agent_card = std::fs::read_dir(&agents_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with(".agent_card.json")
            });
        assert!(
            has_agent_card,
            "agents/ should contain an .agent_card.json file"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
