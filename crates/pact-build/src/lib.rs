// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-09-05

//! # pact-build
//!
//! Compiler backend for the PACT language. Generates deployment artifacts
//! from a checked AST:
//!
//! - **TOML configs** — agent, tool, flow, and permission definitions
//! - **Markdown prompts** — system prompts for each agent
//! - **Claude tool_use JSON** — Anthropic-compatible tool definitions
//!
//! ## Architecture
//!
//! ```text
//! AST (from pact-core) → pact-build → Output Directory
//!                                      ├── pact.toml
//!                                      ├── agents/
//!                                      │   ├── <name>.toml
//!                                      │   └── <name>.prompt.md
//!                                      ├── tools/
//!                                      │   ├── <name>.toml
//!                                      │   └── claude_tools.json
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
//! let config = BuildConfig::new("example.pact", "./pact-out", Target::Claude);
//! build(&program, &config)?;
//! # Ok(())
//! # }
//! ```

pub mod builtins;
pub mod config;
pub mod emit_claude;
pub mod emit_markdown;
pub mod emit_toml;
pub mod guardrails;

use config::BuildConfig;
use pact_core::ast::stmt::{DeclKind, Program};

use thiserror::Error;

/// Errors that can occur during the build process.
#[derive(Debug, Error)]
pub enum BuildError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

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

    // Write Claude tools JSON (if target is Claude)
    if config.target == config::Target::Claude {
        let claude_json = emit_claude::generate_claude_tools_json(program);
        std::fs::write(config.tools_dir().join("claude_tools.json"), claude_json)?;
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
}
