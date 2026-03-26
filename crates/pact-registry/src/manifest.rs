// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Package manifest (`Pact.toml`) parsing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::RegistryError;

/// A PACT package manifest (`Pact.toml`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Package metadata.
    pub package: PackageInfo,
    /// Package dependencies.
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// Package metadata section of the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInfo {
    /// Package name.
    pub name: String,
    /// Package version (semver).
    pub version: String,
    /// Short description.
    #[serde(default)]
    pub description: Option<String>,
    /// Author list.
    #[serde(default)]
    pub authors: Vec<String>,
    /// License identifier.
    #[serde(default)]
    pub license: Option<String>,
    /// Entry file (default: "main.pact").
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "main.pact".to_string()
}

/// A dependency specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// GitHub repository in `org/repo` format.
    pub github: String,
    /// Semver version constraint (e.g., "^0.1.0", "=1.2.3").
    pub version: String,
}

impl Manifest {
    /// Load a manifest from a directory containing `Pact.toml`.
    pub fn load(dir: &Path) -> Result<Self, RegistryError> {
        let path = dir.join("Pact.toml");
        if !path.exists() {
            return Err(RegistryError::ManifestNotFound(dir.display().to_string()));
        }
        let content = std::fs::read_to_string(&path)?;
        let manifest: Manifest = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Walk up from `start` looking for a directory containing `Pact.toml`.
    /// Returns the directory path (not the file path).
    pub fn find_upward(start: &Path) -> Option<PathBuf> {
        let mut dir = if start.is_file() {
            start.parent()?.to_path_buf()
        } else {
            start.to_path_buf()
        };
        loop {
            if dir.join("Pact.toml").exists() {
                return Some(dir);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Save this manifest to `Pact.toml` in the given directory.
    pub fn save(&self, dir: &Path) -> Result<(), RegistryError> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(dir.join("Pact.toml"), content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_manifest() {
        let toml_str = r#"
[package]
name = "my-project"
version = "0.1.0"
description = "Test project"
authors = ["Test Author"]
license = "MIT"
entry = "main.pact"

[dependencies]
pact-std = { github = "pact-lang/pact-std", version = "^0.1.0" }
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.name, "my-project");
        assert_eq!(manifest.package.version, "0.1.0");
        assert_eq!(manifest.package.entry, "main.pact");
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(
            manifest.dependencies["pact-std"].github,
            "pact-lang/pact-std"
        );
    }

    #[test]
    fn parse_manifest_defaults() {
        let toml_str = r#"
[package]
name = "minimal"
version = "0.1.0"
"#;
        let manifest: Manifest = toml::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.entry, "main.pact");
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn find_upward_finds_manifest() {
        let dir = std::env::temp_dir().join("pact-manifest-test-find");
        let _ = fs::remove_dir_all(&dir);
        let sub = dir.join("src").join("nested");
        fs::create_dir_all(&sub).unwrap();
        fs::write(
            dir.join("Pact.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let found = Manifest::find_upward(&sub);
        assert_eq!(found, Some(dir.clone()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_upward_returns_none() {
        let result = Manifest::find_upward(Path::new("/tmp/definitely-no-pact-toml-here-abc123"));
        assert!(result.is_none());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("pact-manifest-test-roundtrip");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut deps = BTreeMap::new();
        deps.insert(
            "web-tools".to_string(),
            Dependency {
                github: "pact-community/web-tools".to_string(),
                version: "^1.0.0".to_string(),
            },
        );

        let manifest = Manifest {
            package: PackageInfo {
                name: "roundtrip-test".to_string(),
                version: "0.2.0".to_string(),
                description: Some("test".to_string()),
                authors: vec!["Test".to_string()],
                license: Some("MIT".to_string()),
                entry: "app.pact".to_string(),
            },
            dependencies: deps,
        };

        manifest.save(&dir).unwrap();
        let loaded = Manifest::load(&dir).unwrap();
        assert_eq!(loaded.package.name, "roundtrip-test");
        assert_eq!(loaded.package.version, "0.2.0");
        assert_eq!(loaded.dependencies.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }
}
