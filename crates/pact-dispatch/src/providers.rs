// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-08

//! Built-in capability provider registry.
//!
//! Maps capability paths (like `search.duckduckgo`) to concrete implementations.
//! Used when tools declare `source: !capability.provider(args)` instead of raw handlers.

use crate::DispatchError;
use std::collections::HashMap;
use tracing::debug;

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
        providers.insert(
            "search.duckduckgo",
            ProviderInfo {
                name: "DuckDuckGo Search",
                description: "Web search via DuckDuckGo Instant Answer API",
                required_permission: "net.read",
            },
        );
        providers.insert("search.google", ProviderInfo {
            name: "Google Search",
            description: "Web search via Google Custom Search API (requires GOOGLE_API_KEY and GOOGLE_CX env vars)",
            required_permission: "net.read",
        });
        providers.insert(
            "search.brave",
            ProviderInfo {
                name: "Brave Search",
                description: "Web search via Brave Search API (requires BRAVE_API_KEY env var)",
                required_permission: "net.read",
            },
        );

        // HTTP providers
        providers.insert(
            "http.get",
            ProviderInfo {
                name: "HTTP GET",
                description: "Make an HTTP GET request to a URL",
                required_permission: "net.read",
            },
        );
        providers.insert(
            "http.post",
            ProviderInfo {
                name: "HTTP POST",
                description: "Make an HTTP POST request with JSON body",
                required_permission: "net.write",
            },
        );

        // Filesystem providers
        providers.insert(
            "fs.read",
            ProviderInfo {
                name: "Read File",
                description: "Read contents of a file",
                required_permission: "fs.read",
            },
        );
        providers.insert(
            "fs.write",
            ProviderInfo {
                name: "Write File",
                description: "Write contents to a file",
                required_permission: "fs.write",
            },
        );
        providers.insert(
            "fs.glob",
            ProviderInfo {
                name: "Glob Files",
                description: "Find files matching a glob pattern",
                required_permission: "fs.read",
            },
        );

        // Time providers
        providers.insert(
            "time.now",
            ProviderInfo {
                name: "Current Time",
                description: "Get the current date and time",
                required_permission: "time.read",
            },
        );

        // JSON providers
        providers.insert(
            "json.parse",
            ProviderInfo {
                name: "Parse JSON",
                description: "Parse a JSON string into structured data",
                required_permission: "json.parse",
            },
        );

        // Security scanning providers
        providers.insert(
            "scan.headers",
            ProviderInfo {
                name: "HTTP Security Headers",
                description: "Analyze HTTP security headers (HSTS, CSP, X-Frame-Options, etc.) of a target URL",
                required_permission: "scan.passive",
            },
        );
        providers.insert(
            "scan.ssl",
            ProviderInfo {
                name: "SSL/TLS Analysis",
                description: "Analyze SSL/TLS certificate and configuration of a target domain",
                required_permission: "scan.passive",
            },
        );
        providers.insert(
            "scan.dns",
            ProviderInfo {
                name: "DNS Enumeration",
                description: "Enumerate DNS records (A, AAAA, MX, TXT, CNAME, NS) for a target domain",
                required_permission: "scan.passive",
            },
        );
        providers.insert(
            "scan.ports",
            ProviderInfo {
                name: "Port Scanner",
                description: "Scan common TCP ports on a target host to identify open services",
                required_permission: "scan.active",
            },
        );
        providers.insert(
            "scan.http",
            ProviderInfo {
                name: "HTTP Probe",
                description: "Probe HTTP endpoints for common misconfigurations, default pages, and server info disclosure",
                required_permission: "scan.active",
            },
        );
        providers.insert(
            "scan.technologies",
            ProviderInfo {
                name: "Technology Fingerprint",
                description: "Detect web technologies, frameworks, and server software from HTTP responses",
                required_permission: "scan.passive",
            },
        );

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
        let mut caps: Vec<_> = self
            .providers
            .keys()
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
        "scan.headers" => execute_scan_headers(params).await,
        "scan.ssl" => execute_scan_ssl(params).await,
        "scan.dns" => execute_scan_dns(params).await,
        "scan.ports" => execute_scan_ports(params).await,
        "scan.http" => execute_scan_http(params).await,
        "scan.technologies" => execute_scan_technologies(params).await,
        _ => Err(DispatchError::ExecutionError(format!(
            "unknown provider capability: '{}'. Use ProviderRegistry::list() to see available providers.",
            capability
        ))),
    }
}

// ── Search Providers ─────────────────────────────────────────────

async fn execute_search_duckduckgo(
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    let query = params.get("query").ok_or_else(|| {
        DispatchError::ExecutionError("search.duckduckgo requires a 'query' parameter".into())
    })?;

    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1",
        urlencoding::encode(query)
    );

    debug!(query = query.as_str(), "search.duckduckgo");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("DuckDuckGo search failed: {e}")))?;

    let body = response.text().await.map_err(|e| {
        DispatchError::ExecutionError(format!("failed to read DuckDuckGo response: {e}"))
    })?;

    debug!(bytes = body.len(), "search.duckduckgo response");
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
        DispatchError::ExecutionError(
            "search.google requires GOOGLE_CX env var (Custom Search Engine ID)".into(),
        )
    })?;

    let url = format!(
        "https://www.googleapis.com/customsearch/v1?key={}&cx={}&q={}",
        api_key,
        cx,
        urlencoding::encode(query)
    );

    debug!(query = query.as_str(), "search.google");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("Google search failed: {e}")))?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        DispatchError::ExecutionError(format!("failed to read Google response: {e}"))
    })?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "Google API returned {status}: {body}"
        )));
    }

    debug!(bytes = body.len(), "search.google response");
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

    debug!(query = query.as_str(), "search.brave");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let response = client
        .get(&url)
        .header("X-Subscription-Token", &api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("Brave search failed: {e}")))?;

    let status = response.status();
    let body = response.text().await.map_err(|e| {
        DispatchError::ExecutionError(format!("failed to read Brave response: {e}"))
    })?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "Brave API returned {status}: {body}"
        )));
    }

    debug!(bytes = body.len(), "search.brave response");
    Ok(body)
}

// ── HTTP Providers ───────────────────────────────────────────────

async fn execute_http_get(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").ok_or_else(|| {
        DispatchError::ExecutionError("http.get requires a 'url' parameter".into())
    })?;

    debug!(url = url.as_str(), "http.get");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP GET failed: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "HTTP GET {url} returned {status}"
        )));
    }

    debug!(status = %status, bytes = body.len(), "http.get response");
    Ok(body)
}

async fn execute_http_post(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").ok_or_else(|| {
        DispatchError::ExecutionError("http.post requires a 'url' parameter".into())
    })?;
    let body_content = params
        .get("body")
        .cloned()
        .unwrap_or_else(|| "{}".to_string());

    debug!(url = url.as_str(), "http.post");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;
    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body_content)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP POST failed: {e}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("failed to read response: {e}")))?;

    if !status.is_success() {
        return Err(DispatchError::ExecutionError(format!(
            "HTTP POST {url} returned {status}"
        )));
    }

    debug!(status = %status, bytes = body.len(), "http.post response");
    Ok(body)
}

// ── Filesystem Providers ─────────────────────────────────────────

fn execute_fs_read(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let path = params.get("path").ok_or_else(|| {
        DispatchError::ExecutionError("fs.read requires a 'path' parameter".into())
    })?;

    debug!(path = path.as_str(), "fs.read");

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

    debug!(path = path.as_str(), "fs.write");

    std::fs::write(path, content).map_err(|e| {
        DispatchError::ExecutionError(format!("failed to write file '{path}': {e}"))
    })?;

    Ok(format!("wrote {} bytes to {path}", content.len()))
}

fn execute_fs_glob(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let pattern = params.get("pattern").ok_or_else(|| {
        DispatchError::ExecutionError("fs.glob requires a 'pattern' parameter".into())
    })?;

    debug!(pattern = pattern.as_str(), "fs.glob");

    let paths: Vec<String> = glob::glob(pattern)
        .map_err(|e| DispatchError::ExecutionError(format!("invalid glob pattern: {e}")))?
        .filter_map(|entry| entry.ok())
        .map(|p| p.display().to_string())
        .collect();

    serde_json::to_string_pretty(&paths).map_err(|e| {
        DispatchError::ExecutionError(format!("failed to serialize glob results: {e}"))
    })
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
    let input = params
        .get("input")
        .or_else(|| params.get("text"))
        .ok_or_else(|| {
            DispatchError::ExecutionError("json.parse requires an 'input' parameter".into())
        })?;

    // Validate it's valid JSON, then pretty-print it
    let parsed: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| DispatchError::ExecutionError(format!("invalid JSON: {e}")))?;

    serde_json::to_string_pretty(&parsed)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON serialization failed: {e}")))
}

// ── Security Scanning Providers ──────────────────────────────────

async fn execute_scan_headers(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.headers requires a 'url' parameter".into())
    })?;

    debug!(url = url.as_str(), "scan.headers");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("scan.headers failed: {e}")))?;

    let security_headers = [
        "strict-transport-security",
        "content-security-policy",
        "x-content-type-options",
        "x-frame-options",
        "x-xss-protection",
        "referrer-policy",
        "permissions-policy",
        "cross-origin-opener-policy",
        "cross-origin-resource-policy",
        "cross-origin-embedder-policy",
    ];

    let mut result = serde_json::Map::new();
    result.insert("url".into(), serde_json::Value::String(url.clone()));
    result.insert(
        "status".into(),
        serde_json::Value::Number(response.status().as_u16().into()),
    );

    let mut headers_found = serde_json::Map::new();
    let mut headers_missing = Vec::new();

    for &header in &security_headers {
        if let Some(val) = response.headers().get(header) {
            headers_found.insert(
                header.into(),
                serde_json::Value::String(val.to_str().unwrap_or("(non-UTF8)").to_string()),
            );
        } else {
            headers_missing.push(serde_json::Value::String(header.into()));
        }
    }

    result.insert("present".into(), serde_json::Value::Object(headers_found));
    result.insert("missing".into(), serde_json::Value::Array(headers_missing));

    // Include server header if present (info disclosure check)
    if let Some(server) = response.headers().get("server") {
        result.insert(
            "server".into(),
            serde_json::Value::String(
                server.to_str().unwrap_or("(non-UTF8)").to_string(),
            ),
        );
    }

    serde_json::to_string_pretty(&result)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
}

async fn execute_scan_ssl(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let domain = params.get("domain").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.ssl requires a 'domain' parameter".into())
    })?;

    debug!(domain = domain.as_str(), "scan.ssl");

    // Connect via HTTPS and inspect the TLS state
    let url = if domain.starts_with("https://") {
        domain.clone()
    } else {
        format!("https://{domain}")
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    let response = client.get(&url).send().await;

    let mut result = serde_json::Map::new();
    result.insert("domain".into(), serde_json::Value::String(domain.clone()));

    match response {
        Ok(resp) => {
            result.insert("tls_connected".into(), serde_json::Value::Bool(true));
            result.insert(
                "status".into(),
                serde_json::Value::Number(resp.status().as_u16().into()),
            );
            // Check for HSTS
            let has_hsts = resp.headers().get("strict-transport-security").is_some();
            result.insert("hsts".into(), serde_json::Value::Bool(has_hsts));
        }
        Err(e) => {
            result.insert("tls_connected".into(), serde_json::Value::Bool(false));
            result.insert(
                "error".into(),
                serde_json::Value::String(format!("{e}")),
            );
        }
    }

    // Test if HTTP redirects to HTTPS
    let http_url = if domain.starts_with("http://") {
        domain.clone()
    } else {
        format!("http://{domain}")
    };

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    if let Ok(http_resp) = http_client.get(&http_url).send().await {
        let redirects_to_https = http_resp
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|loc| loc.starts_with("https://"));
        result.insert(
            "http_redirects_to_https".into(),
            serde_json::Value::Bool(redirects_to_https),
        );
    }

    serde_json::to_string_pretty(&result)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
}

async fn execute_scan_dns(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let domain = params.get("domain").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.dns requires a 'domain' parameter".into())
    })?;

    debug!(domain = domain.as_str(), "scan.dns");

    // Use DNS-over-HTTPS (DoH) via Cloudflare for portable resolution
    let record_types = ["A", "AAAA", "MX", "TXT", "CNAME", "NS"];
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    let mut records = serde_json::Map::new();
    records.insert("domain".into(), serde_json::Value::String(domain.clone()));

    for rtype in &record_types {
        let url = format!(
            "https://cloudflare-dns.com/dns-query?name={}&type={}",
            urlencoding::encode(domain),
            rtype
        );

        if let Ok(resp) = client
            .get(&url)
            .header("Accept", "application/dns-json")
            .send()
            .await
        {
            if let Ok(body) = resp.text().await {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                    if let Some(answers) = json.get("Answer") {
                        let data: Vec<serde_json::Value> = answers
                            .as_array()
                            .unwrap_or(&vec![])
                            .iter()
                            .filter_map(|a| a.get("data").cloned())
                            .collect();
                        if !data.is_empty() {
                            records.insert(
                                (*rtype).to_string(),
                                serde_json::Value::Array(data),
                            );
                        }
                    }
                }
            }
        }
    }

    serde_json::to_string_pretty(&records)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
}

async fn execute_scan_ports(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let host = params.get("host").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.ports requires a 'host' parameter".into())
    })?;

    debug!(host = host.as_str(), "scan.ports");

    // Scan common ports via TCP connect
    let common_ports: &[(u16, &str)] = &[
        (21, "ftp"),
        (22, "ssh"),
        (23, "telnet"),
        (25, "smtp"),
        (53, "dns"),
        (80, "http"),
        (110, "pop3"),
        (143, "imap"),
        (443, "https"),
        (445, "smb"),
        (993, "imaps"),
        (995, "pop3s"),
        (3306, "mysql"),
        (3389, "rdp"),
        (5432, "postgresql"),
        (6379, "redis"),
        (8080, "http-alt"),
        (8443, "https-alt"),
        (27017, "mongodb"),
    ];

    let mut open_ports = Vec::new();
    let timeout = std::time::Duration::from_millis(
        params
            .get("timeout_ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1500),
    );

    for &(port, service) in common_ports {
        let addr = format!("{}:{}", host, port);
        let connected = if let Ok(addr) = addr.parse::<std::net::SocketAddr>() {
            tokio::time::timeout(timeout, tokio::net::TcpStream::connect(addr))
                .await
                .is_ok_and(|r| r.is_ok())
        } else if let Ok(addrs) = tokio::net::lookup_host(&addr).await {
            let mut found = false;
            for resolved in addrs.take(1) {
                if tokio::time::timeout(timeout, tokio::net::TcpStream::connect(resolved))
                    .await
                    .is_ok_and(|r| r.is_ok())
                {
                    found = true;
                }
            }
            found
        } else {
            false
        };

        if connected {
            let mut entry = serde_json::Map::new();
            entry.insert("port".into(), serde_json::Value::Number(port.into()));
            entry.insert("service".into(), serde_json::Value::String(service.into()));
            entry.insert("state".into(), serde_json::Value::String("open".into()));
            open_ports.push(serde_json::Value::Object(entry));
        }
    }

    let mut result = serde_json::Map::new();
    result.insert("host".into(), serde_json::Value::String(host.clone()));
    result.insert(
        "ports_scanned".into(),
        serde_json::Value::Number(common_ports.len().into()),
    );
    result.insert(
        "open".into(),
        serde_json::Value::Array(open_ports),
    );

    serde_json::to_string_pretty(&result)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
}

async fn execute_scan_http(params: &HashMap<String, String>) -> Result<String, DispatchError> {
    let url = params.get("url").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.http requires a 'url' parameter".into())
    })?;

    debug!(url = url.as_str(), "scan.http");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    let mut findings = Vec::new();

    // Check main page
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("scan.http failed: {e}")))?;

    let status = response.status().as_u16();
    let headers = response.headers().clone();

    // Check for server info disclosure
    if let Some(server) = headers.get("server") {
        let s = server.to_str().unwrap_or("");
        if s.contains('/') {
            findings.push(serde_json::json!({
                "type": "info_disclosure",
                "severity": "low",
                "detail": format!("Server header reveals version: {s}")
            }));
        }
    }

    // Check for X-Powered-By disclosure
    if let Some(powered) = headers.get("x-powered-by") {
        findings.push(serde_json::json!({
            "type": "info_disclosure",
            "severity": "low",
            "detail": format!("X-Powered-By header reveals technology: {}", powered.to_str().unwrap_or(""))
        }));
    }

    // Probe common sensitive paths
    let sensitive_paths = [
        "/.env", "/.git/config", "/wp-admin/", "/admin/",
        "/robots.txt", "/.well-known/security.txt", "/sitemap.xml",
    ];

    let base = url.trim_end_matches('/');
    for path in &sensitive_paths {
        if let Ok(resp) = client.get(format!("{base}{path}")).send().await {
            let path_status = resp.status().as_u16();
            if path_status == 200 {
                let severity = if *path == "/.env" || *path == "/.git/config" {
                    "high"
                } else if *path == "/robots.txt" || *path == "/.well-known/security.txt" || *path == "/sitemap.xml" {
                    "informational"
                } else {
                    "medium"
                };
                findings.push(serde_json::json!({
                    "type": "exposed_path",
                    "severity": severity,
                    "path": path,
                    "status": path_status
                }));
            }
        }
    }

    let result = serde_json::json!({
        "url": url,
        "status": status,
        "findings": findings,
        "findings_count": findings.len()
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
}

async fn execute_scan_technologies(
    params: &HashMap<String, String>,
) -> Result<String, DispatchError> {
    let url = params.get("url").or_else(|| params.get("target")).ok_or_else(|| {
        DispatchError::ExecutionError("scan.technologies requires a 'url' parameter".into())
    })?;

    debug!(url = url.as_str(), "scan.technologies");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| DispatchError::ExecutionError(format!("HTTP client error: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| DispatchError::ExecutionError(format!("scan.technologies failed: {e}")))?;

    let headers = response.headers().clone();
    let body = response.text().await.unwrap_or_default();

    let mut technologies = Vec::new();

    // Detect from headers
    if let Some(server) = headers.get("server") {
        technologies.push(serde_json::json!({
            "name": server.to_str().unwrap_or("unknown"),
            "source": "server-header"
        }));
    }
    if let Some(powered) = headers.get("x-powered-by") {
        technologies.push(serde_json::json!({
            "name": powered.to_str().unwrap_or("unknown"),
            "source": "x-powered-by"
        }));
    }
    if headers.get("x-drupal-cache").is_some() || headers.get("x-drupal-dynamic-cache").is_some() {
        technologies.push(serde_json::json!({"name": "Drupal", "source": "header"}));
    }

    // Detect from HTML body patterns
    let body_lower = body.to_lowercase();
    let patterns: &[(&str, &str)] = &[
        ("wp-content/", "WordPress"),
        ("next/static", "Next.js"),
        ("__nuxt", "Nuxt.js"),
        ("_next/data", "Next.js"),
        ("react", "React"),
        ("vue.js", "Vue.js"),
        ("angular", "Angular"),
        ("jquery", "jQuery"),
        ("bootstrap", "Bootstrap"),
        ("tailwindcss", "Tailwind CSS"),
        ("cloudflare", "Cloudflare"),
        ("shopify", "Shopify"),
        ("squarespace", "Squarespace"),
        ("wix.com", "Wix"),
    ];

    for &(pattern, name) in patterns {
        if body_lower.contains(pattern)
            && !technologies.iter().any(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .is_some_and(|n| n == name)
            })
        {
            technologies.push(serde_json::json!({
                "name": name,
                "source": "html-pattern"
            }));
        }
    }

    let result = serde_json::json!({
        "url": url,
        "technologies": technologies,
        "count": technologies.len()
    });

    serde_json::to_string_pretty(&result)
        .map_err(|e| DispatchError::ExecutionError(format!("JSON error: {e}")))
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
    fn registry_has_scan_providers() {
        let reg = ProviderRegistry::new();
        assert!(reg.exists("scan.headers"));
        assert!(reg.exists("scan.ssl"));
        assert!(reg.exists("scan.dns"));
        assert!(reg.exists("scan.ports"));
        assert!(reg.exists("scan.http"));
        assert!(reg.exists("scan.technologies"));
    }

    #[test]
    fn registry_scan_namespace() {
        let reg = ProviderRegistry::new();
        let scan = reg.list_namespace("scan");
        assert_eq!(scan.len(), 6);
        assert!(scan.contains(&"scan.headers"));
        assert!(scan.contains(&"scan.ports"));
    }

    #[test]
    fn scan_providers_have_correct_permissions() {
        let reg = ProviderRegistry::new();
        assert_eq!(
            reg.get("scan.headers").unwrap().required_permission,
            "scan.passive"
        );
        assert_eq!(
            reg.get("scan.ports").unwrap().required_permission,
            "scan.active"
        );
    }

    #[test]
    fn registry_list_all() {
        let reg = ProviderRegistry::new();
        let all = reg.list();
        assert!(all.len() >= 16);
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
