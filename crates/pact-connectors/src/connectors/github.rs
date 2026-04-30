// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-04-15

//! GitHub connector — interact with repositories, pull requests, and issues.
//!
//! ## Operations
//!
//! | Operation       | Description                          | Required Params                  |
//! |----------------|--------------------------------------|----------------------------------|
//! | `push_file`    | Create or update a file in a repo    | `owner`, `repo`, `path`, `content`, `message` |
//! | `create_pr`    | Open a pull request                  | `owner`, `repo`, `head`, `base`, `title` |
//! | `create_issue` | Create an issue                      | `owner`, `repo`, `title`         |
//! | `read_file`    | Read a file from a repo              | `owner`, `repo`, `path`          |
//! | `list_repos`   | List repositories for a user/org     | `owner`                          |
//! | `list_issues`  | List issues (optionally since a date)| `owner`, `repo`                  |
//! | `list_pulls`   | List pull requests                   | `owner`, `repo`                  |
//! | `get_issue_comments` | Get comments on an issue        | `owner`, `repo`, `issue_number`  |
//! | `get_pull_comments`  | Get comments on a pull request  | `owner`, `repo`, `pull_number`   |
//!
//! ## Authentication
//!
//! Requires a **Personal Access Token** (classic or fine-grained).
//!
//! - Classic: Settings → Developer settings → Personal access tokens → Tokens (classic)
//!   - Scopes needed: `repo` (full control of private repos)
//! - Fine-grained: Settings → Developer settings → Personal access tokens → Fine-grained tokens
//!   - Permissions: Contents (read/write), Pull requests (read/write), Issues (read/write)

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use tracing::info;

use super::{Connector, ConnectorConfig, ConnectorError, GitHubConfig};
use async_trait::async_trait;

const API_BASE: &str = "https://api.github.com";

pub struct GitHubConnector;

#[async_trait]
impl Connector for GitHubConnector {
    fn name(&self) -> &'static str { "github" }
    fn description(&self) -> &'static str { "GitHub — interact with repositories, pull requests, and issues" }

    fn credential_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "github",
            "description": self.description(),
            "credentials": {
                "token": { "type": "string", "required": true, "description": "Personal Access Token (ghp_...)", "ui": "password" },
                "owner": { "type": "string", "required": true, "description": "Repository owner (user or org)", "ui": "text" },
                "repo": { "type": "string", "required": true, "description": "Repository name", "ui": "text" },
            }
        })
    }

    fn spec(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "github",
            "description": self.description(),
            "auth": {
                "type": "bearer",
                "description": "Personal Access Token (classic or fine-grained)",
                "help": "Settings → Developer settings → Personal access tokens"
            },
            "credentials": self.credential_schema()["credentials"],
            "operations": {
                "push_file": {
                    "description": "Create or update a file in a repository",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "path": { "type": "string", "required": true, "description": "File path in the repository" },
                        "content": { "type": "string", "required": true, "description": "File content" },
                        "message": { "type": "string", "required": false, "default": "Update via PACT flow", "description": "Commit message" },
                        "branch": { "type": "string", "required": false, "default": "main" }
                    }
                },
                "create_pr": {
                    "description": "Open a pull request. If 'files' is provided (JSON object mapping path → content), automatically creates a branch, pushes the files, and opens the PR — one-step deploy.",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "title": { "type": "string", "required": true },
                        "files": { "type": "string", "required": false, "description": "JSON object mapping file paths to content (e.g. {\"web/index.html\": \"<html>...\", \"web/style.css\": \"...\"}). If provided, creates a branch, pushes files, then opens the PR." },
                        "head": { "type": "string", "required": false, "description": "Branch name. Auto-generated as 'pact/YYYYMMDD-HHMMSS' if files are provided and head is omitted." },
                        "base": { "type": "string", "required": false, "default": "main", "description": "Branch to merge into" },
                        "body": { "type": "string", "required": false, "description": "PR description" }
                    }
                },
                "create_issue": {
                    "description": "Create an issue",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "title": { "type": "string", "required": true },
                        "body": { "type": "string", "required": false },
                        "labels": { "type": "string", "required": false, "description": "Comma-separated label names" }
                    }
                },
                "read_file": {
                    "description": "Read a file from a repository",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "path": { "type": "string", "required": true },
                        "branch": { "type": "string", "required": false, "default": "main" }
                    }
                },
                "list_repos": {
                    "description": "List repositories for a user or organization",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" }
                    }
                },
                "list_issues": {
                    "description": "List issues for a repository (excludes PRs)",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "state": { "type": "string", "required": false, "default": "open", "enum": ["open", "closed", "all"] },
                        "since": { "type": "string", "required": false, "description": "ISO 8601 timestamp — only issues updated after this" },
                        "labels": { "type": "string", "required": false, "description": "Comma-separated label names" },
                        "per_page": { "type": "string", "required": false, "default": "30" }
                    }
                },
                "list_pulls": {
                    "description": "List pull requests for a repository",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "state": { "type": "string", "required": false, "default": "open", "enum": ["open", "closed", "all"] },
                        "per_page": { "type": "string", "required": false, "default": "30" }
                    }
                },
                "get_issue_comments": {
                    "description": "Get comments on an issue",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "issue_number": { "type": "string", "required": true },
                        "since": { "type": "string", "required": false, "description": "ISO 8601 timestamp" },
                        "per_page": { "type": "string", "required": false, "default": "30" }
                    }
                },
                "get_pull_comments": {
                    "description": "Get all comments on a pull request (conversation + inline review)",
                    "params": {
                        "owner": { "type": "string", "required": true, "default_from": "credentials.owner" },
                        "repo": { "type": "string", "required": true, "default_from": "credentials.repo" },
                        "pull_number": { "type": "string", "required": true },
                        "since": { "type": "string", "required": false, "description": "ISO 8601 timestamp" },
                        "per_page": { "type": "string", "required": false, "default": "30" }
                    }
                }
            }
        })
    }

    fn operations(&self) -> Vec<&'static str> {
        vec!["push_file", "create_pr", "create_issue", "read_file", "list_repos", "list_issues", "list_pulls", "get_issue_comments", "get_pull_comments"]
    }

    fn is_configured(&self, config: &ConnectorConfig) -> bool {
        config.github.is_some()
    }

    async fn execute(
        &self,
        action: &str,
        params: &HashMap<String, String>,
        config: &ConnectorConfig,
    ) -> Result<String, ConnectorError> {
        let cfg = config.github.as_ref()
            .ok_or(ConnectorError::NotConfigured("github".into()))?;
        execute_action(action, params, cfg).await
    }
}

/// Execute a GitHub operation.
pub async fn execute_action(
    action: &str,
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    match action {
        "push_file" => push_file(params, config).await,
        "create_pr" => create_pr(params, config).await,
        "create_issue" => create_issue(params, config).await,
        "read_file" => read_file(params, config).await,
        "list_repos" => list_repos(params, config).await,
        "list_issues" => list_issues(params, config).await,
        "list_pulls" => list_pulls(params, config).await,
        "get_issue_comments" => get_issue_comments(params, config).await,
        "get_pull_comments" => get_pull_comments(params, config).await,
        _ => Err(ConnectorError::InvalidOperation(format!(
            "github.{action}"
        ))),
    }
}

fn get_param<'a>(
    params: &'a HashMap<String, String>,
    key: &str,
    config: &'a GitHubConfig,
) -> Result<&'a str, ConnectorError> {
    // Try params first, then fall back to config defaults.
    if let Some(v) = params.get(key) {
        return Ok(v.as_str());
    }
    match key {
        "owner" => Ok(config.owner.as_str()),
        "repo" => Ok(config.repo.as_str()),
        _ => Err(ConnectorError::MissingParam(key.into())),
    }
}

fn client(_config: &GitHubConfig) -> reqwest::Client {
    reqwest::Client::new()
}

/// Produce a user-friendly error message from a GitHub API failure.
fn friendly_error(
    status: reqwest::StatusCode,
    api_message: &str,
    operation: &str,
    owner: &str,
    repo: &str,
) -> ConnectorError {
    let code = status.as_u16();
    let detail = match code {
        401 => format!(
            "GitHub authentication failed for '{owner}/{repo}'. \
             Your Personal Access Token may be invalid, expired, or revoked. \
             Generate a new token at GitHub → Settings → Developer settings → Personal access tokens."
        ),
        403 => format!(
            "GitHub denied access to '{owner}/{repo}'. \
             Your token does not have the required permissions. \
             Ensure the token has the 'repo' scope (classic) or 'Contents: read/write' permission (fine-grained)."
        ),
        404 => format!(
            "GitHub repository '{owner}/{repo}' was not found. \
             Either the repository does not exist, or your token does not have access to it. \
             Check that 'owner' and 'repo' are correct and that your token has the 'repo' scope."
        ),
        422 => format!(
            "GitHub rejected the request for '{owner}/{repo}' ({operation}): {api_message}. \
             This usually means invalid input — check file paths, branch names, and content."
        ),
        _ => format!(
            "GitHub API error ({code}) for '{owner}/{repo}' ({operation}): {api_message}"
        ),
    };
    ConnectorError::ApiError {
        status: code,
        message: detail,
    }
}

fn auth_headers(
    config: &GitHubConfig,
) -> Vec<(reqwest::header::HeaderName, reqwest::header::HeaderValue)> {
    vec![
        (
            AUTHORIZATION,
            format!("Bearer {}", config.token).parse().unwrap(),
        ),
        (
            ACCEPT,
            "application/vnd.github+json".parse().unwrap(),
        ),
        (
            USER_AGENT,
            "PACT-Server/1.0".parse().unwrap(),
        ),
    ]
}

/// Create or update a file in a repository.
async fn push_file(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let path = params
        .get("path")
        .ok_or_else(|| ConnectorError::MissingParam("path".into()))?;
    let content = params
        .get("content")
        .ok_or_else(|| ConnectorError::MissingParam("content".into()))?;
    let message = params
        .get("message")
        .map(|s| s.as_str())
        .unwrap_or("Update via PACT flow");
    let branch = params.get("branch").map(|s| s.as_str()).unwrap_or("main");

    // Check if file exists (to get SHA for update).
    let url = format!("{API_BASE}/repos/{owner}/{repo}/contents/{path}");
    let http = client(config);
    let mut req = http.get(&url);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }
    req = req.query(&[("ref", branch)]);
    let existing_sha = match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
            body.get("sha").and_then(|s| s.as_str()).map(String::from)
        }
        _ => None,
    };

    let encoded = BASE64.encode(content.as_bytes());
    let mut body = serde_json::json!({
        "message": message,
        "content": encoded,
        "branch": branch,
    });
    if let Some(sha) = existing_sha {
        body["sha"] = serde_json::json!(sha);
    }

    let mut req = http.put(&url).json(&body);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let html_url = resp_body["content"]["html_url"]
            .as_str()
            .unwrap_or("(unknown)");
        info!(owner, repo, path, "file pushed to GitHub");
        Ok(serde_json::json!({
            "status": "success",
            "url": html_url,
            "path": path,
            "branch": branch,
        })
        .to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "push_file",
            owner,
            repo,
        ))
    }
}

/// Open a pull request.
///
/// If a `files` parameter is provided (JSON object mapping path → content),
/// the operation will:
///   1. Get the latest commit SHA on the base branch
///   2. Create a new branch (`head`, auto-generated if not provided)
///   3. Push each file to that branch
///   4. Open a PR from the branch to `base`
///
/// This makes `create_pr` a one-step deploy: the LLM agent passes the files
/// and gets back a PR URL.
async fn create_pr(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let title = params
        .get("title")
        .ok_or_else(|| ConnectorError::MissingParam("title".into()))?;
    let base = params
        .get("base")
        .map(|s| s.as_str())
        .unwrap_or("main");
    let body_text = params.get("body").map(|s| s.as_str()).unwrap_or("");
    let http = client(config);

    // If `files` is provided, push them to a new branch first.
    let files_json = params.get("files");
    let mut resolved_base = base.to_string();
    let head: String = if let Some(files_str) = files_json {
        let files: serde_json::Value = serde_json::from_str(files_str)
            .map_err(|e| ConnectorError::HttpError(format!("invalid files JSON: {e}")))?;
        let files_map = files.as_object()
            .ok_or_else(|| ConnectorError::HttpError("files must be a JSON object mapping path → content".into()))?;

        if files_map.is_empty() {
            return Err(ConnectorError::HttpError("files object is empty".into()));
        }

        // Generate branch name if not provided.
        let branch = params.get("head").map(|s| s.to_string()).unwrap_or_else(|| {
            let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            format!("pact/{ts}")
        });

        // Step 1: Get the base branch's latest commit SHA.
        // Try the requested base branch first, then fall back to common defaults.
        let branches_to_try = if base == "main" {
            vec!["main", "master"]
        } else if base == "master" {
            vec!["master", "main"]
        } else {
            vec![base]
        };

        let mut base_sha = None;
        for try_branch in &branches_to_try {
            let base_ref_url = format!("{API_BASE}/repos/{owner}/{repo}/git/ref/heads/{try_branch}");
            let mut req = http.get(&base_ref_url);
            for (k, v) in auth_headers(config) {
                req = req.header(k, v);
            }
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    let resp_body: serde_json::Value = resp.json().await
                        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
                    if let Some(sha) = resp_body["object"]["sha"].as_str() {
                        info!(owner, repo, branch = try_branch, "resolved base branch");
                        base_sha = Some(sha.to_string());
                        resolved_base = try_branch.to_string();
                        break;
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    info!(
                        owner, repo, branch = try_branch,
                        status = status.as_u16(),
                        api_message = body["message"].as_str().unwrap_or(""),
                        "branch not found, trying next"
                    );
                }
                Err(e) => {
                    return Err(ConnectorError::HttpError(format!(
                        "failed to reach GitHub API for {owner}/{repo}: {e}"
                    )));
                }
            }
        }

        // If no base branch found, the repo is likely empty.
        // Bootstrap it by pushing an initial README to the default branch.
        let base_sha = if let Some(sha) = base_sha {
            sha
        } else {
            info!(owner, repo, "repo appears empty — bootstrapping with initial commit");
            let init_url = format!("{API_BASE}/repos/{owner}/{repo}/contents/README.md");
            let init_content = BASE64.encode(format!("# {repo}\n\nInitialized by PACT flow.\n").as_bytes());
            let mut req = http.put(&init_url).json(&serde_json::json!({
                "message": "Initial commit via PACT flow",
                "content": init_content,
            }));
            for (k, v) in auth_headers(config) {
                req = req.header(k, v);
            }
            let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
            let init_status = resp.status();
            let init_body: serde_json::Value = resp.json().await.unwrap_or_default();
            if !init_status.is_success() {
                return Err(ConnectorError::ApiError {
                    status: init_status.as_u16(),
                    message: format!(
                        "failed to initialize empty repo {owner}/{repo}: {}",
                        init_body["message"].as_str().unwrap_or("unknown error")
                    ),
                });
            }
            // Now get the SHA of the default branch.
            let base_ref_url = format!("{API_BASE}/repos/{owner}/{repo}/git/ref/heads/{base}");
            let mut req = http.get(&base_ref_url);
            for (k, v) in auth_headers(config) {
                req = req.header(k, v);
            }
            let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
            let resp_body: serde_json::Value = resp.json().await
                .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
            resp_body["object"]["sha"].as_str()
                .ok_or_else(|| ConnectorError::HttpError(format!(
                    "bootstrapped repo {owner}/{repo} but could not read branch SHA"
                )))?
                .to_string()
        };

        // Step 2: Create the new branch.
        let create_ref_url = format!("{API_BASE}/repos/{owner}/{repo}/git/refs");
        let mut req = http.post(&create_ref_url).json(&serde_json::json!({
            "ref": format!("refs/heads/{branch}"),
            "sha": base_sha,
        }));
        for (k, v) in auth_headers(config) {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
        let ref_status = resp.status();
        if !ref_status.is_success() {
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            // 422 "Reference already exists" is OK — reuse the branch.
            let msg = err["message"].as_str().unwrap_or("");
            if !msg.contains("Reference already exists") {
                return Err(ConnectorError::ApiError {
                    status: ref_status.as_u16(),
                    message: format!("failed to create branch '{branch}': {msg}"),
                });
            }
        }
        info!(owner, repo, branch = %branch, files = files_map.len(), "created branch for PR");

        // Step 3: Push each file to the branch.
        for (path, content_val) in files_map {
            let content = content_val.as_str().unwrap_or("");
            let encoded = BASE64.encode(content.as_bytes());
            let file_url = format!("{API_BASE}/repos/{owner}/{repo}/contents/{path}");

            // Check if file exists on the branch (need SHA for update).
            let mut req = http.get(&file_url).query(&[("ref", branch.as_str())]);
            for (k, v) in auth_headers(config) {
                req = req.header(k, v);
            }
            let existing_sha = match req.send().await {
                Ok(r) if r.status().is_success() => {
                    let body: serde_json::Value = r.json().await.unwrap_or_default();
                    body.get("sha").and_then(|s| s.as_str()).map(String::from)
                }
                _ => None,
            };

            let mut body = serde_json::json!({
                "message": format!("Add {path} via PACT flow"),
                "content": encoded,
                "branch": branch,
            });
            if let Some(sha) = existing_sha {
                body["sha"] = serde_json::json!(sha);
            }

            let mut req = http.put(&file_url).json(&body);
            for (k, v) in auth_headers(config) {
                req = req.header(k, v);
            }
            let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
            let push_status = resp.status();
            if !push_status.is_success() {
                let err: serde_json::Value = resp.json().await.unwrap_or_default();
                return Err(ConnectorError::ApiError {
                    status: push_status.as_u16(),
                    message: format!("failed to push {path}: {}", err["message"].as_str().unwrap_or("unknown")),
                });
            }
            info!(owner, repo, path, branch = %branch, "pushed file");
        }

        branch
    } else {
        // No files — caller must provide an existing branch.
        params
            .get("head")
            .ok_or_else(|| ConnectorError::MissingParam("head (or files)".into()))?
            .clone()
    };

    // Step 4: Create the pull request.
    let pr_url = format!("{API_BASE}/repos/{owner}/{repo}/pulls");
    let mut req = http
        .post(&pr_url)
        .json(&serde_json::json!({
            "title": title,
            "head": head,
            "base": resolved_base,
            "body": body_text,
        }));
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(owner, repo, title, head = %head, "PR created");
        Ok(serde_json::json!({
            "status": "success",
            "pr_number": resp_body["number"],
            "url": resp_body["html_url"],
            "branch": head,
        })
        .to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "create_pr",
            owner,
            repo,
        ))
    }
}

/// Create an issue.
async fn create_issue(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let title = params
        .get("title")
        .ok_or_else(|| ConnectorError::MissingParam("title".into()))?;
    let body_text = params.get("body").map(|s| s.as_str()).unwrap_or("");
    let labels: Vec<&str> = params
        .get("labels")
        .map(|s| s.split(',').map(|l| l.trim()).collect())
        .unwrap_or_default();

    let url = format!("{API_BASE}/repos/{owner}/{repo}/issues");
    let http = client(config);
    let mut req = http
        .post(&url)
        .json(&serde_json::json!({
            "title": title,
            "body": body_text,
            "labels": labels,
        }));
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        info!(owner, repo, title, "issue created");
        Ok(serde_json::json!({
            "status": "success",
            "issue_number": resp_body["number"],
            "url": resp_body["html_url"],
        })
        .to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "create_issue",
            owner,
            repo,
        ))
    }
}

/// Read a file from a repository.
async fn read_file(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let path = params
        .get("path")
        .ok_or_else(|| ConnectorError::MissingParam("path".into()))?;
    let branch = params.get("branch").map(|s| s.as_str()).unwrap_or("main");

    let url = format!("{API_BASE}/repos/{owner}/{repo}/contents/{path}");
    let http = client(config);
    let mut req = http.get(&url).query(&[("ref", branch)]);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let encoded = resp_body["content"]
            .as_str()
            .unwrap_or("")
            .replace('\n', "");
        let decoded = BASE64
            .decode(&encoded)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_else(|_| encoded);
        Ok(decoded)
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "read_file",
            owner,
            repo,
        ))
    }
}

/// List issues for a repository.
///
/// Optional params: `state` (open|closed|all), `since` (ISO 8601 timestamp),
/// `labels` (comma-separated), `per_page` (default 30).
async fn list_issues(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let state = params.get("state").map(|s| s.as_str()).unwrap_or("open");
    let per_page = params.get("per_page").map(|s| s.as_str()).unwrap_or("30");

    let url = format!("{API_BASE}/repos/{owner}/{repo}/issues");
    let http = client(config);
    let mut req = http
        .get(&url)
        .query(&[("state", state), ("per_page", per_page), ("sort", "updated"), ("direction", "desc")]);

    if let Some(since) = params.get("since") {
        req = req.query(&[("since", since.as_str())]);
    }
    if let Some(labels) = params.get("labels") {
        req = req.query(&[("labels", labels.as_str())]);
    }
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let issues: Vec<serde_json::Value> = resp_body
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter(|i| i.get("pull_request").is_none()) // GitHub API includes PRs in issues endpoint
            .map(|i| serde_json::json!({
                "number": i["number"],
                "title": i["title"],
                "state": i["state"],
                "user": i["user"]["login"],
                "labels": i["labels"].as_array().map(|l| l.iter().map(|x| x["name"].as_str().unwrap_or("")).collect::<Vec<_>>()).unwrap_or_default(),
                "created_at": i["created_at"],
                "updated_at": i["updated_at"],
                "comments": i["comments"],
                "url": i["html_url"],
            }))
            .collect();
        info!(owner, repo, count = issues.len(), "listed issues");
        Ok(serde_json::json!(issues).to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "list_issues",
            owner,
            repo,
        ))
    }
}

/// List pull requests for a repository.
///
/// Optional params: `state` (open|closed|all), `per_page` (default 30).
async fn list_pulls(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let state = params.get("state").map(|s| s.as_str()).unwrap_or("open");
    let per_page = params.get("per_page").map(|s| s.as_str()).unwrap_or("30");

    let url = format!("{API_BASE}/repos/{owner}/{repo}/pulls");
    let http = client(config);
    let mut req = http
        .get(&url)
        .query(&[("state", state), ("per_page", per_page), ("sort", "updated"), ("direction", "desc")]);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let pulls: Vec<serde_json::Value> = resp_body
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|p| serde_json::json!({
                "number": p["number"],
                "title": p["title"],
                "state": p["state"],
                "user": p["user"]["login"],
                "head": p["head"]["ref"],
                "base": p["base"]["ref"],
                "draft": p["draft"],
                "mergeable_state": p["mergeable_state"],
                "created_at": p["created_at"],
                "updated_at": p["updated_at"],
                "review_comments": p["review_comments"],
                "url": p["html_url"],
            }))
            .collect();
        info!(owner, repo, count = pulls.len(), "listed pull requests");
        Ok(serde_json::json!(pulls).to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "list_pulls",
            owner,
            repo,
        ))
    }
}

/// Get comments on an issue.
///
/// Optional params: `since` (ISO 8601 timestamp), `per_page` (default 30).
async fn get_issue_comments(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let issue_number = params
        .get("issue_number")
        .ok_or_else(|| ConnectorError::MissingParam("issue_number".into()))?;
    let per_page = params.get("per_page").map(|s| s.as_str()).unwrap_or("30");

    let url = format!("{API_BASE}/repos/{owner}/{repo}/issues/{issue_number}/comments");
    let http = client(config);
    let mut req = http.get(&url).query(&[("per_page", per_page)]);
    if let Some(since) = params.get("since") {
        req = req.query(&[("since", since.as_str())]);
    }
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req.send().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp.json().await.map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let comments: Vec<serde_json::Value> = resp_body
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|c| serde_json::json!({
                "id": c["id"],
                "user": c["user"]["login"],
                "body": c["body"],
                "created_at": c["created_at"],
                "updated_at": c["updated_at"],
            }))
            .collect();
        info!(owner, repo, issue_number, count = comments.len(), "fetched issue comments");
        Ok(serde_json::json!(comments).to_string())
    } else {
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "get_issue_comments",
            owner,
            repo,
        ))
    }
}

/// Get review comments on a pull request.
///
/// Optional params: `since` (ISO 8601 timestamp), `per_page` (default 30).
async fn get_pull_comments(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;
    let repo = get_param(params, "repo", config)?;
    let pull_number = params
        .get("pull_number")
        .ok_or_else(|| ConnectorError::MissingParam("pull_number".into()))?;
    let per_page = params.get("per_page").map(|s| s.as_str()).unwrap_or("30");

    // Fetch both issue-style comments and review comments
    let issue_url = format!("{API_BASE}/repos/{owner}/{repo}/issues/{pull_number}/comments");
    let review_url = format!("{API_BASE}/repos/{owner}/{repo}/pulls/{pull_number}/comments");
    let http = client(config);

    let mut issue_req = http.get(&issue_url).query(&[("per_page", per_page)]);
    if let Some(since) = params.get("since") {
        issue_req = issue_req.query(&[("since", since.as_str())]);
    }
    for (k, v) in auth_headers(config) {
        issue_req = issue_req.header(k, v);
    }

    let mut review_req = client(config).get(&review_url).query(&[("per_page", per_page)]);
    if let Some(since) = params.get("since") {
        review_req = review_req.query(&[("since", since.as_str())]);
    }
    for (k, v) in auth_headers(config) {
        review_req = review_req.header(k, v);
    }

    let (issue_resp, review_resp) = tokio::join!(
        issue_req.send(),
        review_req.send(),
    );

    let mut all_comments: Vec<serde_json::Value> = Vec::new();

    if let Ok(resp) = issue_resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                for c in body.as_array().unwrap_or(&vec![]) {
                    all_comments.push(serde_json::json!({
                        "type": "comment",
                        "id": c["id"],
                        "user": c["user"]["login"],
                        "body": c["body"],
                        "created_at": c["created_at"],
                        "updated_at": c["updated_at"],
                    }));
                }
            }
        }
    }

    if let Ok(resp) = review_resp {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                for c in body.as_array().unwrap_or(&vec![]) {
                    all_comments.push(serde_json::json!({
                        "type": "review",
                        "id": c["id"],
                        "user": c["user"]["login"],
                        "body": c["body"],
                        "path": c["path"],
                        "line": c["line"],
                        "created_at": c["created_at"],
                        "updated_at": c["updated_at"],
                    }));
                }
            }
        }
    }

    // Sort by created_at descending
    all_comments.sort_by(|a, b| {
        let a_time = a["created_at"].as_str().unwrap_or("");
        let b_time = b["created_at"].as_str().unwrap_or("");
        b_time.cmp(a_time)
    });

    info!(owner, repo, pull_number, count = all_comments.len(), "fetched pull request comments");
    Ok(serde_json::json!(all_comments).to_string())
}

/// List repositories for a user or organization.
async fn list_repos(
    params: &HashMap<String, String>,
    config: &GitHubConfig,
) -> Result<String, ConnectorError> {
    let owner = get_param(params, "owner", config)?;

    let url = format!("{API_BASE}/users/{owner}/repos");
    let http = client(config);
    let mut req = http
        .get(&url)
        .query(&[("sort", "updated"), ("per_page", "10")]);
    for (k, v) in auth_headers(config) {
        req = req.header(k, v);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;
    let status = resp.status();
    let resp_body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ConnectorError::HttpError(e.to_string()))?;

    if status.is_success() {
        let repos: Vec<serde_json::Value> = resp_body
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r["name"],
                    "full_name": r["full_name"],
                    "description": r["description"],
                    "url": r["html_url"],
                    "language": r["language"],
                    "updated_at": r["updated_at"],
                })
            })
            .collect();
        Ok(serde_json::json!(repos).to_string())
    } else {
        let repo = config.repo.as_str();
        Err(friendly_error(
            status,
            resp_body["message"].as_str().unwrap_or("unknown error"),
            "list_repos",
            owner,
            repo,
        ))
    }
}
