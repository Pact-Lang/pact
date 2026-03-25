// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-11

//! Agent memory persistence.
//!
//! Provides key-value storage that persists across agent runs.
//! Memory is scoped per agent and stored as JSON files.

use std::collections::HashMap;
use std::path::PathBuf;

/// In-memory store backed by a JSON file on disk.
pub struct MemoryStore {
    agent_name: String,
    session_id: Option<String>,
    data: HashMap<String, String>,
    path: PathBuf,
}

impl MemoryStore {
    /// Create or load a memory store for an agent.
    pub fn load(agent_name: &str) -> Self {
        let path = Self::memory_path(agent_name);
        let data = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self {
            agent_name: agent_name.to_string(),
            session_id: None,
            data,
            path,
        }
    }

    /// Create or load a memory store for an agent within a specific session.
    /// The session ID is used for observation correlation in the dispatch layer.
    pub fn with_session(agent_name: &str, session_id: &str) -> Self {
        let mut store = Self::load(agent_name);
        store.session_id = Some(session_id.to_string());
        store
    }

    /// Get the agent name this store belongs to.
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    /// Get the session ID, if set.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Get the number of entries in the store.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get a value from memory.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }

    /// Set a value in memory and persist to disk.
    pub fn set(&mut self, key: String, value: String) {
        self.data.insert(key, value);
        self.save();
    }

    /// Remove a value from memory.
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let val = self.data.remove(key);
        if val.is_some() {
            self.save();
        }
        val
    }

    /// List all keys in memory.
    pub fn keys(&self) -> Vec<&str> {
        self.data.keys().map(|k| k.as_str()).collect()
    }

    /// Clear all memory for this agent.
    pub fn clear(&mut self) {
        self.data.clear();
        self.save();
    }

    fn save(&self) {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(&self.data) {
            std::fs::write(&self.path, json).ok();
        }
    }

    fn memory_path(agent_name: &str) -> PathBuf {
        let dir = std::env::var("PACT_MEMORY_DIR").unwrap_or_else(|_| ".pact/memory".to_string());
        PathBuf::from(dir).join(format!("{}.json", agent_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_get_set() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory");
        let mut store = MemoryStore::load("test_agent");
        store.set("key1".to_string(), "value1".to_string());
        assert_eq!(store.get("key1"), Some("value1"));
        store.clear();
    }

    #[test]
    fn memory_store_remove() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory");
        let mut store = MemoryStore::load("test_remove");
        store.set("key".to_string(), "val".to_string());
        assert_eq!(store.remove("key"), Some("val".to_string()));
        assert_eq!(store.get("key"), None);
        store.clear();
    }

    #[test]
    fn memory_store_keys() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory");
        let mut store = MemoryStore::load("test_keys");
        store.set("a".into(), "1".into());
        store.set("b".into(), "2".into());
        let mut keys = store.keys();
        keys.sort();
        assert_eq!(keys, vec!["a", "b"]);
        store.clear();
    }

    #[test]
    fn memory_persistence() {
        std::env::set_var("PACT_MEMORY_DIR", "/tmp/pact_test_memory");
        {
            let mut store = MemoryStore::load("test_persist");
            store.set("persistent".to_string(), "data".to_string());
        }
        // Load again -- should still have the data
        let store = MemoryStore::load("test_persist");
        assert_eq!(store.get("persistent"), Some("data"));
        // Cleanup
        let mut store = MemoryStore::load("test_persist");
        store.clear();
    }
}
