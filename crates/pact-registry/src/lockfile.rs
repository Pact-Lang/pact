// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Lock file (`pact.lock`) management.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::RegistryError;

/// A lock file pinning exact dependency versions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Lockfile {
    /// Locked package entries.
    #[serde(default, rename = "package")]
    pub packages: Vec<LockedPackage>,
}

/// A single locked package entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Package name.
    pub name: String,
    /// Exact resolved version.
    pub version: String,
    /// Source identifier (e.g., "github:org/repo").
    pub source: String,
    /// Git commit hash for this version.
    pub rev: String,
    /// SHA-256 checksum of the downloaded tarball.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

impl Lockfile {
    /// Load a lock file from disk.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let content = std::fs::read_to_string(path)?;
        let lockfile: Lockfile = toml::from_str(&content)?;
        Ok(lockfile)
    }

    /// Save the lock file to disk.
    pub fn save(&self, path: &Path) -> Result<(), RegistryError> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Look up a package by name.
    pub fn resolve(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Check if a package is already locked.
    pub fn contains(&self, name: &str) -> bool {
        self.packages.iter().any(|p| p.name == name)
    }

    /// Add or update a locked package entry.
    pub fn upsert(&mut self, pkg: LockedPackage) {
        if let Some(existing) = self.packages.iter_mut().find(|p| p.name == pkg.name) {
            *existing = pkg;
        } else {
            self.packages.push(pkg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_lockfile() {
        let toml_str = r#"
[[package]]
name = "pact-std"
version = "0.1.2"
source = "github:pact-lang/pact-std"
rev = "abc123def456"
checksum = "sha256:deadbeef"
"#;
        let lockfile: Lockfile = toml::from_str(toml_str).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].name, "pact-std");
        assert_eq!(lockfile.packages[0].version, "0.1.2");
        assert_eq!(lockfile.packages[0].rev, "abc123def456");
    }

    #[test]
    fn resolve_finds_package() {
        let lockfile = Lockfile {
            packages: vec![LockedPackage {
                name: "web-tools".to_string(),
                version: "1.0.0".to_string(),
                source: "github:org/web-tools".to_string(),
                rev: "aaa".to_string(),
                checksum: None,
            }],
        };
        assert!(lockfile.resolve("web-tools").is_some());
        assert!(lockfile.resolve("nonexistent").is_none());
    }

    #[test]
    fn upsert_adds_new() {
        let mut lockfile = Lockfile::default();
        lockfile.upsert(LockedPackage {
            name: "new-pkg".to_string(),
            version: "0.1.0".to_string(),
            source: "github:org/new-pkg".to_string(),
            rev: "bbb".to_string(),
            checksum: None,
        });
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn upsert_updates_existing() {
        let mut lockfile = Lockfile {
            packages: vec![LockedPackage {
                name: "pkg".to_string(),
                version: "0.1.0".to_string(),
                source: "github:org/pkg".to_string(),
                rev: "old".to_string(),
                checksum: None,
            }],
        };
        lockfile.upsert(LockedPackage {
            name: "pkg".to_string(),
            version: "0.2.0".to_string(),
            source: "github:org/pkg".to_string(),
            rev: "new".to_string(),
            checksum: None,
        });
        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].version, "0.2.0");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("pact-lockfile-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let lockfile = Lockfile {
            packages: vec![LockedPackage {
                name: "test-pkg".to_string(),
                version: "1.0.0".to_string(),
                source: "github:test/test-pkg".to_string(),
                rev: "abc123".to_string(),
                checksum: Some("sha256:123456".to_string()),
            }],
        };

        let path = dir.join("pact.lock");
        lockfile.save(&path).unwrap();
        let loaded = Lockfile::load(&path).unwrap();
        assert_eq!(loaded.packages.len(), 1);
        assert_eq!(loaded.packages[0].name, "test-pkg");

        let _ = fs::remove_dir_all(&dir);
    }
}
