// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-08

//! Built-in capability provider registry.
//!
//! Maps capability paths (like `search.duckduckgo`) to concrete implementations.
//! Used when tools declare `source: !capability.provider(args)` instead of raw handlers.

use std::collections::HashMap;
use crate::DispatchError;

/// A registered provider with its execution logic.
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    /// Human-readable name.
    pub name: &'static str,
    /// Description of what this provider does.
    pub description: &'static str,
    /// Required permission path (e.g. "net.read").
    pub required_permission: &'static str,
}

/// The provider registry — maps capability paths to provider info and execution.
pub struct ProviderRegistry {
    providers: HashMap<&'static str, ProviderInfo>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    /// Create the default registry with all built-in providers.
    pub fn new() -> Self {
        let mut providers = HashMap::new();

        // Search providers
        providers.insert("search.duckduckgo", ProviderInfo {
            name: "DuckDuckGo Search",
            description: "Web search via DuckDuckGo Instant Answer API",
            required_permission: "net.read",
        });
        providers.insert("search.google", ProviderInfo {
            name: "Google Search",
            description: "Web search via Google Custom Search API (requires GOOGLE_API_KEY and GOOGLE_CX env vars)",
            required_permission: "net.read",
        });
        providers.insert("search.brave", ProviderInfo {
            name: "Brave Search",
            description: "Web search via Brave Search API (requires BRAVE_API_KEY env var)",
            required_permission: "net.read",
        });

        // HTTP providers
        providers.insert("http.get", ProviderInfo {
            name: "HTTP GET",
            description: "Make an HTTP GET request to a URL",
            required_permission: "net.read",
        });
        providers.insert("http.post", ProviderInfo {
            name: "HTTP POST",
            description: "Make an HTTP POST request with JSON body",
            required_permission: "net.write",
        });

        // Filesystem providers
        providers.insert("fs.read", ProviderInfo {
            name: "Read File",
            description: "Read contents of a file",
            required_permission: "fs.read",
        });
        providers.insert("fs.write", ProviderInfo {
            name: "Write File",
            description: "Write contents to a file",
            required_permission: "fs.write",
        });
        providers.insert("fs.glob", ProviderInfo {
            name: "Glob Files",
            description: "Find files matching a glob pattern",
            required_permission: "fs.read",
        });

        // Time providers
        providers.insert("time.now", ProviderInfo {
            name: "Current Time",
            description: "Get the current date and time",
            required_permission: "time.read",
        });

        // JSON providers
        providers.insert("json.parse", ProviderInfo {
            name: "Parse JSON",
            description: "Parse a JSON string into structured data",
            required_permission: "json.parse",
        });

        Self { providers }
    }

    /// Look up a provider by capability path.
    pub fn get(&self, capability: &str) -> Option<&ProviderInfo> {
        self.providers.get(capability)
    }

    /// Check if a capability path is registered.
    pub fn exists(&self, capability: &str) -> bool {
        self.providers.contains_key(capability)
    }

    /// List all registered capability paths.
    pub fn list(&self) -> Vec<&'static str> {
        let mut caps: Vec<_> = self.providers.keys().copied().collect();
        caps.sort();
        caps
    }

    /// List providers under a namespace (e.g. "search" returns ["search.duckduckgo", "search.google", "search.brave"]).
    pub fn list_namespace(&self, namespace: &str) -> Vec<&'static str> {
        let prefix = format!("{}.", namespace);
        let mut caps: Vec<_> = self.providers.keys()
            .filter(|k| k.starts_with(&prefix) || **k == namespace)
            .copied()
            .collect();
        caps.sort();
        caps
    }
}

/// Execute a provider capability with the given parameters.
pub async fn execute_provider(
    capability: &str,
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    match capability {
        "search.duckduckgo" => execute_search_duckduckgo(params).await,
        "search.google" => execute_search_google(params).await,
        "search.brave" => execute_search_brave(params).await,
        "http.get" => execute_http_get(params).await,
        "http.post" => execute_http_post(params).await,
        "fs.read" => execute_fs_read(params),
        "fs.write" => execute_fs_write(params),
        "fs.glob" => execute_fs_glob(params),
        "time.now" => execute_time_now(params),
        "json.parse" => execute_json_parse(params),
        _ => Err(DispatchError::ExecutionError(format!(
            "unknown provider capability: '{}'. Use ProviderRegistry::list() to see available providers.",
            capability
        ))),
    }
}

// ── Search Providers ─────────────────────────────────────────────

async fn execute_search_duckduckgo(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let query = params.get("query").ok_or_else(|| {
        DispatchError::ExecutionError("search.duckduckgo requires a 'query' parameter".into())
    })?;

    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    println!("[PROVIDER] search.duckduckgo: {query}");

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await
        .map_err(|e| DispatchError::ExecutionError(format!("DuckDuckGo search failed: {e}")))?;

    let body = response.text().await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read DuckDuckGo response: {e}")))?;

    println!("[PROVIDER] search.duckduckgo => {} bytes", body.len());
    Ok(body)
}

async fn execute_search_google(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let query = params.get("query").ok_or_else(|| {
        DispatchError::ExecutionError("search.google requires a 'query' parameter".into())
    })?;

    let api_key = std::env::var("GOOGLE_API_KEY").map_err(|_| {
        DispatchError::ExecutionError("search.google requires GOOGLE_API_KEY env var".into())
    })?;
    let cx = std::env::var("GOOGLE_CX").map_err(|_| {
        DispatchError::ExecutionError("search.google requires GOOGLE_CX env var (Custom Search Engine ID)".into())
    })?;

    let url = format!(
        "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}",
        api_key, cx, urlencoding::encode(query)
    );

    println!("[PROVIDER] search.google: {query}");

    let client = reqwest::Client::new();
    let response = client.get(&url).send().await
        .map_err(|e| DispatchError::ExecutionError(format!("Google search failed: {e}")))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read Google response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!("Google API returned {status}: {body}")));
    }

    println!("[PROVIDER] search.google => {} bytes", body.len());
    Ok(body)
}

async fn execute_search_brave(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let query = params.get("query").ok_or_else(|| {
        DispatchError::ExecutionError("search.brave requires a 'query' parameter".into())
    })?;

    let api_key = std::env::var("BRAVE_API_KEY").map_err(|_| {
        DispatchError::ExecutionError("search.brave requires BRAVE_API_KEY env var".into())
    })?;

    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}",
        urlencoding::encode(query)
    );

    println!("[PROVIDER] search.brave: {query}");

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .header("X-Subscription-Token", &api_key)
        .header("Accept", "application/json")
        .send().await
        .map_err(|e| DispatchError::ExecutionError(format!("Brave search failed: {e}")))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read Brave response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!("Brave API returned {status}: {body}")));
    }

    println!("[PROVIDER] search.brave => {} bytes", body.len());
    Ok(body)
}

// ── HTTP Providers ───────────────────────────────────────────────

async fn execute_http_get(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").ok_or_else(|| {
        DispatchError::ExecutionError("http.get requires a 'url' parameter".into())
    })?;

    println!("[PROVIDER] http.get: {url}");

    let client = reqwest::Client::new();
    let response = client.get(url).send().await
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP GET failed: {e}")))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!("HTTP GET {url} returned {status}")));
    }

    println!("[PROVIDER] http.get => {status} ({} bytes)", body.len());
    Ok(body)
}

async fn execute_http_post(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").ok_or_else(|| {
        DispatchError::ExecutionError("http.post requires a 'url' parameter".into())
    })?;
    let body_content = params.get("body").cloned().unwrap_or_else(|| "{}".to_string());

    println!("[PROVIDER] http.post: {url}");

    let client = reqwest::Client::new();
    let response = client.post(url)
        .header("Content-Type", "application/json")
        .body(body_content)
        .send().await
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP POST failed: {e}")))?;

    let status = response.status();
    let body = response.text().await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!("HTTP POST {url} returned {status}")));
    }

    println!("[PROVIDER] http.post => {status} ({} bytes)", body.len());
    Ok(body)
}

// ── Filesystem Providers ─────────────────────────────────────────

fn execute_fs_read(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let path = params.get("path").ok_or_else(|| {
        DispatchError::ExecutionError("fs.read requires a 'path' parameter".into())
    })?;

    println!("[PROVIDER] fs.read: {path}");

    std::fs::read_to_string(path)
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read file '{path}': {e}")))
}

fn execute_fs_write(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let path = params.get("path").ok_or_else(|| {
        DispatchError::ExecutionError("fs.write requires a 'path' parameter".into())
    })?;
    let content = params.get("content").ok_or_else(|| {
        DispatchError::ExecutionError("fs.write requires a 'content' parameter".into())
    })?;

    println!("[PROVIDER] fs.write: {path}");

    std::fs::write(path, content)
        .map_err(|e| DispatchError::ExecutionError(format!("failed to write file '{path}': {e}")))?;

    Ok(format!("wrote {} bytes to {path}", content.len()))
}

fn execute_fs_glob(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let pattern = params.get("pattern").ok_or_else(|| {
        DispatchError::ExecutionError("fs.glob requires a 'pattern' parameter".into())
    })?;

    println!("[PROVIDER] fs.glob: {pattern}");

    let paths: Vec<String> = glob::glob(pattern)
        .map_err(|e| DispatchError::ExecutionError(format!("invalid glob pattern: {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|p| p.display().to_string())
        .collect();

    serde_json::to_string_pretty(&paths)
        .map_err(|e| DispatchError::ExecutionError(format!("failed to serialize glob results: {e}")))
}

// ── Utility Providers ────────────────────────────────────────────

fn execute_time_now(_params: &HashMap<String, String>) -> Result<String, DispatchError> {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| DispatchError::ExecutionError(format!("time error: {e}")))?;
    Ok(format!("{}", now.as_secs()))
}

fn execute_json_parse(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let input = params.get("input").or_else(|| params.get("text")).ok_or_else(|| {
        DispatchError::ExecutionError("json.parse requires an 'input' parameter".into())
    })?;

    // Validate it's valid JSON, then pretty-print it
    let parsed: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| DispatchError::ExecutionError(format!("invalid JSON: {e}")))?;

    serde_json::to_string_pretty(&parsed)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON serialization failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_search_providers() {
        let reg = ProviderRegistry::new();
        assert!(reg.exists("search.duckduckgo"));
        assert!(reg.exists("search.google"));
        assert!(reg.exists("search.brave"));
    }

    #[test]
    fn registry_has_http_providers() {
        let reg = ProviderRegistry::new();
        assert!(reg.exists("http.get"));
        assert!(reg.exists("http.post"));
    }

    #[test]
    fn registry_has_fs_providers() {
        let reg = ProviderRegistry::new();
        assert!(reg.exists("fs.read"));
        assert!(reg.exists("fs.write"));
        assert!(reg.exists("fs.glob"));
    }

    #[test]
    fn registry_unknown_returns_none() {
        let reg = ProviderRegistry::new();
        assert!(!reg.exists("search.bing"));
        assert!(reg.get("search.bing").is_none());
    }

    #[test]
    fn registry_list_namespace() {
        let reg = ProviderRegistry::new();
        let search = reg.list_namespace("search");
        assert_eq!(search.len(), 3);
        assert!(search.contains(&"search.duckduckgo"));
        assert!(search.contains(&"search.google"));
        assert!(search.contains(&"search.brave"));
    }

    #[test]
    fn registry_list_all() {
        let reg = ProviderRegistry::new();
        let all = reg.list();
        assert!(all.len() >= 10);
    }

    #[test]
    fn provider_info_has_permission() {
        let reg = ProviderRegistry::new();
        let ddg = reg.get("search.duckduckgo").unwrap();
        assert_eq!(ddg.required_permission, "net.read");
    }

    #[test]
    fn time_now_works() {
        let params = HashMap::new();
        let result = execute_time_now(&params).unwrap();
        let secs: u64 = result.parse().unwrap();
        assert!(secs > 1_700_000_000); // After 2023
    }

    #[test]
    fn json_parse_valid() {
        let mut params = HashMap::new();
        params.insert("input".to_string(), r#"{"key": "value"}"#.to_string());
        let result = execute_json_parse(&params).unwrap();
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn json_parse_invalid() {
        let mut params = HashMap::new();
        params.insert("input".to_string(), "not json".to_string());
        assert!(execute_json_parse(&params).is_err());
    }

    #[tokio::test]
    async fn unknown_provider_errors() {
        let params = HashMap::new();
        let result = execute_provider("search.bing", &params).await;
        assert!(result.is_err());
    }
}
