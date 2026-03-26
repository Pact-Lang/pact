// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! GitHub API client for package fetching.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, info};

use crate::RegistryError;

/// GitHub REST API client for fetching package metadata and tarballs.
pub struct GitHubClient {
    http: reqwest::Client,
    token: Option<String>,
}

/// A tag from the GitHub API.
#[derive(Debug, Deserialize)]
pub struct TagInfo {
    /// Tag name (e.g., "v0.1.0").
    pub name: String,
    /// Commit info.
    pub commit: TagCommit,
}

/// Commit reference in a tag.
#[derive(Debug, Deserialize)]
pub struct TagCommit {
    /// Commit SHA.
    pub sha: String,
}

impl GitHubClient {
    /// Create a new GitHub client.
    ///
    /// Optionally reads `GITHUB_TOKEN` or `GITHUB_ACCESS_TOKEN` from environment
    /// for higher rate limits.
    pub fn new() -> Self {
        dotenvy::dotenv().ok();
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GITHUB_ACCESS_TOKEN"))
            .ok();

        let http = reqwest::Client::builder()
            .user_agent("pact-registry/0.1")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { http, token }
    }

    /// List tags for a GitHub repository.
    pub async fn list_tags(&self, org: &str, repo: &str) -> Result<Vec<TagInfo>, RegistryError> {
        let url = format!("https://api.github.com/repos/{org}/{repo}/tags?per_page=100");
        debug!(org, repo, "fetching tags");

        let mut req = self.http.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let response = req
            .send()
            .await
            .map_err(|e| RegistryError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(RegistryError::GitHubApi { status, body });
        }

        let tags: Vec<TagInfo> = response
            .json()
            .await
            .map_err(|e| RegistryError::Http(e.to_string()))?;

        info!(org, repo, count = tags.len(), "fetched tags");
        Ok(tags)
    }

    /// Download a tarball for a specific tag and extract to a temp directory.
    ///
    /// Returns the path to the extracted directory containing package contents.
    pub async fn download_and_extract(
        &self,
        org: &str,
        repo: &str,
        tag: &str,
        dest: &Path,
    ) -> Result<PathBuf, RegistryError> {
        let url = format!("https://api.github.com/repos/{org}/{repo}/tarball/{tag}");
        info!(org, repo, tag, "downloading tarball");

        let mut req = self.http.get(&url);
        if let Some(token) = &self.token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let response = req
            .send()
            .await
            .map_err(|e| RegistryError::Http(e.to_string()))?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(RegistryError::GitHubApi { status, body });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| RegistryError::Http(e.to_string()))?;

        // Extract the tarball
        std::fs::create_dir_all(dest)?;
        let gz = flate2::read::GzDecoder::new(&bytes[..]);
        let mut archive = tar::Archive::new(gz);
        archive.unpack(dest)?;

        // GitHub tarballs extract to a directory like `org-repo-sha/`.
        // Find the first directory in dest.
        let extracted_dir = std::fs::read_dir(dest)?
            .filter_map(|e| e.ok())
            .find(|e| e.path().is_dir())
            .map(|e| e.path())
            .unwrap_or_else(|| dest.to_path_buf());

        Ok(extracted_dir)
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a version string from a git tag name.
///
/// Strips optional `v` prefix: "v0.1.0" -> "0.1.0", "0.1.0" -> "0.1.0".
pub fn parse_tag_version(tag: &str) -> Option<semver::Version> {
    let version_str = tag.strip_prefix('v').unwrap_or(tag);
    semver::Version::parse(version_str).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_with_v_prefix() {
        let v = parse_tag_version("v0.1.0").unwrap();
        assert_eq!(v, semver::Version::new(0, 1, 0));
    }

    #[test]
    fn parse_tag_without_prefix() {
        let v = parse_tag_version("1.2.3").unwrap();
        assert_eq!(v, semver::Version::new(1, 2, 3));
    }

    #[test]
    fn parse_tag_invalid() {
        assert!(parse_tag_version("not-a-version").is_none());
    }

    #[test]
    fn parse_tag_prerelease() {
        let v = parse_tag_version("v2.0.0-beta.1").unwrap();
        assert_eq!(v.major, 2);
        assert!(!v.pre.is_empty());
    }

    #[test]
    fn client_creates_successfully() {
        // Just verify the client can be constructed without panicking.
        // Token presence depends on environment, so we only test construction.
        let _client = GitHubClient::new();
    }
}
