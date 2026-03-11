// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-11

//! Tool result caching.
//!
//! Provides a simple in-memory cache for tool execution results with
//! time-based expiration. Tools can opt in via `cache: "24h"` in their
//! declaration.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Global tool cache instance.
pub fn global_cache() -> &'static ToolCache {
    static CACHE: OnceLock<ToolCache> = OnceLock::new();
    CACHE.get_or_init(ToolCache::new)
}

/// Thread-safe in-memory cache for tool results.
pub struct ToolCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    value: String,
    expires_at: Instant,
}

impl Default for ToolCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Get a cached value if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(key) {
            if Instant::now() < entry.expires_at {
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Store a value in the cache with a time-to-live duration.
    pub fn set(&self, key: String, value: String, ttl: Duration) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            key,
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
    }

    /// Clear all entries from the cache.
    pub fn clear(&self) {
        let mut entries = self.entries.lock().unwrap();
        entries.clear();
    }
}

/// Parse a duration string like "24h", "30m", "7d", "60s" into a [`Duration`].
///
/// Supported units:
/// - `s` — seconds
/// - `m` — minutes
/// - `h` — hours
/// - `d` — days
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;

    match unit {
        "s" => Some(Duration::from_secs(num)),
        "m" => Some(Duration::from_secs(num * 60)),
        "h" => Some(Duration::from_secs(num * 3600)),
        "d" => Some(Duration::from_secs(num * 86400)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("60s"), Some(Duration::from_secs(60)));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("30m"), Some(Duration::from_secs(1800)));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("24h"), Some(Duration::from_secs(86400)));
    }

    #[test]
    fn parse_duration_days() {
        assert_eq!(parse_duration("7d"), Some(Duration::from_secs(604800)));
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("10x"), None);
    }

    #[test]
    fn cache_get_set() {
        let cache = ToolCache::new();
        cache.set("key".into(), "value".into(), Duration::from_secs(60));
        assert_eq!(cache.get("key"), Some("value".to_string()));
    }

    #[test]
    fn cache_miss() {
        let cache = ToolCache::new();
        assert_eq!(cache.get("nonexistent"), None);
    }

    #[test]
    fn cache_expiry() {
        let cache = ToolCache::new();
        cache.set(
            "ephemeral".into(),
            "gone_soon".into(),
            Duration::from_millis(50),
        );
        assert_eq!(cache.get("ephemeral"), Some("gone_soon".to_string()));
        thread::sleep(Duration::from_millis(100));
        assert_eq!(cache.get("ephemeral"), None);
    }

    #[test]
    fn cache_clear() {
        let cache = ToolCache::new();
        cache.set("a".into(), "1".into(), Duration::from_secs(60));
        cache.set("b".into(), "2".into(), Duration::from_secs(60));
        cache.clear();
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
    }
}
