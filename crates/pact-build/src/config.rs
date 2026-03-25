// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-09-10

//! Build configuration and output path resolution.
//!
//! [`BuildConfig`] controls where and how `pact build` writes its output
//! artifacts. It resolves output paths and creates the directory structure.

use std::path::PathBuf;

/// Supported compilation targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Generate Claude/Anthropic-compatible tool definitions.
    Claude,
}

impl Target {
    /// Parse a target name from a string.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "anthropic" => Some(Self::Claude),
            _ => None,
        }
    }

    /// Return the target name as a string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
        }
    }
}

/// Configuration for the `pact build` command.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Path to the source `.pact` file.
    pub source_path: PathBuf,
    /// Output directory for generated artifacts.
    pub out_dir: PathBuf,
    /// Compilation target (determines tool format).
    pub target: Target,
    /// Whether to emit Claude Code skill files.
    pub emit_claude_skill: bool,
    /// Whether to emit a CLAUDE.md file.
    pub emit_claude_md: bool,
    /// Whether to emit MCP server recommendations.
    pub emit_mcp_recommendations: bool,
}

impl BuildConfig {
    /// Create a new build configuration.
    pub fn new(
        source_path: impl Into<PathBuf>,
        out_dir: impl Into<PathBuf>,
        target: Target,
    ) -> Self {
        Self {
            source_path: source_path.into(),
            out_dir: out_dir.into(),
            target,
            emit_claude_skill: false,
            emit_claude_md: false,
            emit_mcp_recommendations: false,
        }
    }

    /// Return the Claude Code skills output directory.
    pub fn claude_skills_dir(&self) -> PathBuf {
        self.out_dir.join(".claude").join("skills")
    }

    /// Return the path to the project manifest file.
    pub fn manifest_path(&self) -> PathBuf {
        self.out_dir.join("pact.toml")
    }

    /// Return the agents output directory.
    pub fn agents_dir(&self) -> PathBuf {
        self.out_dir.join("agents")
    }

    /// Return the tools output directory.
    pub fn tools_dir(&self) -> PathBuf {
        self.out_dir.join("tools")
    }

    /// Return the skills output directory.
    pub fn skills_dir(&self) -> PathBuf {
        self.out_dir.join("skills")
    }

    /// Return the flows output directory.
    pub fn flows_dir(&self) -> PathBuf {
        self.out_dir.join("flows")
    }

    /// Return the permissions output file path.
    pub fn permissions_path(&self) -> PathBuf {
        self.out_dir.join("permissions.toml")
    }

    /// Create the output directory structure.
    pub fn create_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.out_dir)?;
        std::fs::create_dir_all(self.agents_dir())?;
        std::fs::create_dir_all(self.tools_dir())?;
        std::fs::create_dir_all(self.skills_dir())?;
        std::fs::create_dir_all(self.flows_dir())?;
        Ok(())
    }

    /// Return the source file name (for display in the manifest).
    pub fn source_name(&self) -> &str {
        self.source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.pact")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_parsing() {
        assert_eq!(Target::parse("claude"), Some(Target::Claude));
        assert_eq!(Target::parse("Claude"), Some(Target::Claude));
        assert_eq!(Target::parse("anthropic"), Some(Target::Claude));
        assert_eq!(Target::parse("unknown"), None);
    }

    #[test]
    fn path_resolution() {
        use std::path::Path;
        let config = BuildConfig::new("test.pact", "./out", Target::Claude);
        assert_eq!(config.manifest_path(), Path::new("./out/pact.toml"));
        assert_eq!(config.agents_dir(), Path::new("./out/agents"));
        assert_eq!(config.tools_dir(), Path::new("./out/tools"));
        assert_eq!(config.flows_dir(), Path::new("./out/flows"));
        assert_eq!(
            config.permissions_path(),
            Path::new("./out/permissions.toml")
        );
    }

    #[test]
    fn source_name() {
        let config = BuildConfig::new("examples/hello.pact", "./out", Target::Claude);
        assert_eq!(config.source_name(), "hello.pact");
    }
}
