// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Local package cache (`~/.pact/cache/`).

use std::path::{Path, PathBuf};

use crate::RegistryError;

/// Local package cache manager.
///
/// Packages are stored in `~/.pact/cache/github/{org}/{repo}/{version}/`.
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// Create a new cache instance, creating the directory if needed.
    pub fn new() -> Result<Self, RegistryError> {
        let home = dirs::home_dir().ok_or(RegistryError::NoCacheDir)?;
        let root = home.join(".pact").join("cache");
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Create a cache at a custom path (for testing).
    pub fn with_root(root: PathBuf) -> Result<Self, RegistryError> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Get the root cache directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the directory path for a specific package version.
    ///
    /// Format: `{root}/github/{org}/{repo}/{version}/`
    pub fn package_dir(&self, github_repo: &str, version: &str) -> PathBuf {
        self.root.join("github").join(github_repo).join(version)
    }

    /// Check if a package version is already cached.
    pub fn is_cached(&self, github_repo: &str, version: &str) -> bool {
        let dir = self.package_dir(github_repo, version);
        dir.exists() && dir.is_dir()
    }

    /// Get the entry file path for a cached package.
    ///
    /// Looks for `Pact.toml` in the cached directory to determine the entry file.
    /// Falls back to `main.pact` if no manifest is found.
    pub fn entry_file(&self, github_repo: &str, version: &str) -> Result<PathBuf, RegistryError> {
        let dir = self.package_dir(github_repo, version);
        if !dir.exists() {
            return Err(RegistryError::NotCached {
                name: github_repo.to_string(),
                version: version.to_string(),
            });
        }

        // Try to read the package's own Pact.toml for the entry file
        let manifest_path = dir.join("Pact.toml");
        if manifest_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&manifest_path) {
                if let Ok(manifest) = toml::from_str::<crate::manifest::Manifest>(&content) {
                    return Ok(dir.join(&manifest.package.entry));
                }
            }
        }

        // Default to main.pact
        Ok(dir.join("main.pact"))
    }

    /// Store extracted package contents in the cache.
    pub fn store(
        &self,
        github_repo: &str,
        version: &str,
        source_dir: &Path,
    ) -> Result<PathBuf, RegistryError> {
        let target = self.package_dir(github_repo, version);
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::create_dir_all(&target)?;
        copy_dir_recursive(source_dir, &target)?;
        Ok(target)
    }

    /// Remove a cached package version.
    pub fn remove(&self, github_repo: &str, version: &str) -> Result<(), RegistryError> {
        let dir = self.package_dir(github_repo, version);
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Recursively copy a directory's contents.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), RegistryError> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_cache(name: &str) -> Cache {
        let dir = std::env::temp_dir().join(format!("pact-cache-test-{}", name));
        let _ = fs::remove_dir_all(&dir);
        Cache::with_root(dir).unwrap()
    }

    #[test]
    fn package_dir_format() {
        let cache = test_cache("dir-format");
        let dir = cache.package_dir("pact-lang/pact-std", "0.1.0");
        assert!(dir.ends_with("github/pact-lang/pact-std/0.1.0"));
    }

    #[test]
    fn not_cached_initially() {
        let cache = test_cache("not-cached");
        assert!(!cache.is_cached("org/pkg", "1.0.0"));
    }

    #[test]
    fn store_and_check_cached() {
        let cache = test_cache("store");
        let src = std::env::temp_dir().join("pact-cache-test-store-src");
        let _ = fs::remove_dir_all(&src);
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.pact"), "agent @a { permits: [] tools: [] }").unwrap();

        cache.store("org/test-pkg", "0.1.0", &src).unwrap();
        assert!(cache.is_cached("org/test-pkg", "0.1.0"));

        let entry = cache.entry_file("org/test-pkg", "0.1.0").unwrap();
        assert!(entry.ends_with("main.pact"));

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(cache.root());
    }

    #[test]
    fn entry_file_not_cached_errors() {
        let cache = test_cache("entry-not-cached");
        let result = cache.entry_file("org/missing", "1.0.0");
        assert!(result.is_err());
    }

    #[test]
    fn remove_cached_package() {
        let cache = test_cache("remove");
        let src = std::env::temp_dir().join("pact-cache-test-remove-src");
        let _ = fs::remove_dir_all(&src);
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.pact"), "").unwrap();

        cache.store("org/removable", "1.0.0", &src).unwrap();
        assert!(cache.is_cached("org/removable", "1.0.0"));

        cache.remove("org/removable", "1.0.0").unwrap();
        assert!(!cache.is_cached("org/removable", "1.0.0"));

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(cache.root());
    }
}
