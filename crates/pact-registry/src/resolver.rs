// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Version resolution and dependency solving.

use tracing::info;

use crate::cache::Cache;
use crate::client::{parse_tag_version, GitHubClient};
use crate::lockfile::{LockedPackage, Lockfile};
use crate::manifest::Manifest;
use crate::RegistryError;

/// Dependency resolver that fetches packages from GitHub and caches them locally.
pub struct Resolver {
    client: GitHubClient,
    cache: Cache,
}

impl Resolver {
    /// Create a new resolver with the given cache.
    pub fn new(cache: Cache) -> Self {
        Self {
            client: GitHubClient::new(),
            cache,
        }
    }

    /// Resolve all dependencies in a manifest.
    ///
    /// For each dependency, fetches available tags from GitHub, picks the
    /// highest version matching the constraint, and returns a lock file.
    pub async fn resolve(&self, manifest: &Manifest) -> Result<Lockfile, RegistryError> {
        let mut lockfile = Lockfile::default();

        for (name, dep) in &manifest.dependencies {
            let constraint: semver::VersionReq =
                dep.version.parse().map_err(RegistryError::SemverParse)?;

            let parts: Vec<&str> = dep.github.split('/').collect();
            if parts.len() != 2 {
                return Err(RegistryError::Http(format!(
                    "invalid github repo format: '{}', expected 'org/repo'",
                    dep.github
                )));
            }
            let (org, repo) = (parts[0], parts[1]);

            info!(package = name, repo = dep.github.as_str(), "resolving");

            let tags = self.client.list_tags(org, repo).await?;

            // Find the highest version matching the constraint
            let mut best: Option<(semver::Version, String, String)> = None;
            for tag in &tags {
                if let Some(version) = parse_tag_version(&tag.name) {
                    if constraint.matches(&version)
                        && best.as_ref().is_none_or(|(v, _, _)| &version > v)
                    {
                        best = Some((version, tag.name.clone(), tag.commit.sha.clone()));
                    }
                }
            }

            let (version, _tag_name, rev) =
                best.ok_or_else(|| RegistryError::NoMatchingVersion {
                    package: name.clone(),
                    constraint: dep.version.clone(),
                })?;

            lockfile.upsert(LockedPackage {
                name: name.clone(),
                version: version.to_string(),
                source: format!("github:{}", dep.github),
                rev,
                checksum: None,
            });

            info!(
                package = name,
                version = version.to_string().as_str(),
                "resolved"
            );
        }

        Ok(lockfile)
    }

    /// Fetch all locked packages into the local cache.
    ///
    /// Skips packages that are already cached.
    pub async fn fetch(&self, lockfile: &Lockfile) -> Result<(), RegistryError> {
        for pkg in &lockfile.packages {
            let github_repo = pkg.source.strip_prefix("github:").unwrap_or(&pkg.source);

            if self.cache.is_cached(github_repo, &pkg.version) {
                info!(package = pkg.name.as_str(), "already cached");
                continue;
            }

            let parts: Vec<&str> = github_repo.split('/').collect();
            if parts.len() != 2 {
                continue;
            }
            let (org, repo) = (parts[0], parts[1]);

            let tag = format!("v{}", pkg.version);
            let temp_dir =
                std::env::temp_dir().join(format!("pact-fetch-{}-{}", pkg.name, pkg.version));
            let _ = std::fs::remove_dir_all(&temp_dir);

            let extracted = self
                .client
                .download_and_extract(org, repo, &tag, &temp_dir)
                .await?;

            self.cache.store(github_repo, &pkg.version, &extracted)?;

            let _ = std::fs::remove_dir_all(&temp_dir);
            info!(
                package = pkg.name.as_str(),
                version = pkg.version.as_str(),
                "cached"
            );
        }

        Ok(())
    }

    /// Get the local entry file path for a dependency.
    pub fn package_entry_path(
        &self,
        name: &str,
        lockfile: &Lockfile,
    ) -> Result<std::path::PathBuf, RegistryError> {
        let pkg = lockfile
            .resolve(name)
            .ok_or_else(|| RegistryError::NotInLockfile(name.to_string()))?;

        let github_repo = pkg.source.strip_prefix("github:").unwrap_or(&pkg.source);

        self.cache.entry_file(github_repo, &pkg.version)
    }

    /// Get a reference to the cache.
    pub fn cache(&self) -> &Cache {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::LockedPackage;

    #[test]
    fn package_entry_path_from_lockfile() {
        let dir = std::env::temp_dir().join("pact-resolver-test-entry");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = Cache::with_root(dir.clone()).unwrap();

        // Create a fake cached package
        let pkg_dir = cache.package_dir("org/test-pkg", "1.0.0");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("main.pact"), "").unwrap();

        let resolver = Resolver::new(cache);
        let lockfile = Lockfile {
            packages: vec![LockedPackage {
                name: "test-pkg".to_string(),
                version: "1.0.0".to_string(),
                source: "github:org/test-pkg".to_string(),
                rev: "abc".to_string(),
                checksum: None,
            }],
        };

        let entry = resolver.package_entry_path("test-pkg", &lockfile).unwrap();
        assert!(entry.ends_with("main.pact"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn package_entry_not_in_lockfile() {
        let dir = std::env::temp_dir().join("pact-resolver-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let cache = Cache::with_root(dir.clone()).unwrap();
        let resolver = Resolver::new(cache);
        let lockfile = Lockfile::default();

        let result = resolver.package_entry_path("nonexistent", &lockfile);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
