// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-12

//! Structured audit logging for tool calls and agent execution.
//!
//! Provides an in-memory, thread-safe audit trail that records what agents
//! did during execution. Each entry captures the event type, timing,
//! token usage, and outcome, enabling post-run analysis and compliance
//! reporting.
//!
//! # Example
//!
//! ```
//! use pact_dispatch::audit::{AuditLogger, AuditEntry, AuditEventType};
//!
//! let logger = AuditLogger::new();
//! logger.log(AuditEntry::new(AuditEventType::ToolCall)
//!     .with_agent("researcher")
//!     .with_tool("web_search")
//!     .with_success(true));
//!
//! assert_eq!(logger.entries().len(), 1);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::SystemTime;

// ── Data Types ──────────────────────────────────────────────────

/// The kind of event being recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    /// A tool was invoked by an agent.
    ToolCall,
    /// A flow began execution.
    FlowStart,
    /// A flow finished execution.
    FlowEnd,
    /// An agent was dispatched to handle a request.
    AgentDispatch,
    /// A mediation compliance check was performed.
    MediationCheck,
    /// A cached result was reused instead of executing a tool.
    CacheHit,
    /// No cached result was found; the tool was executed fresh.
    CacheMiss,
    /// A rate limit was encountered during execution.
    RateLimitHit,
}

/// Token usage statistics for a single event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of input (prompt) tokens consumed.
    pub input_tokens: u64,
    /// Number of output (completion) tokens generated.
    pub output_tokens: u64,
}

/// A single audit log entry capturing one discrete event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// ISO 8601 formatted timestamp of when the event occurred.
    pub timestamp: String,
    /// The kind of event that occurred.
    pub event_type: AuditEventType,
    /// Name of the agent involved, if applicable.
    pub agent_name: Option<String>,
    /// Name of the flow involved, if applicable.
    pub flow_name: Option<String>,
    /// Name of the tool involved, if applicable.
    pub tool_name: Option<String>,
    /// Duration of the event in milliseconds, if measured.
    pub duration_ms: Option<u64>,
    /// Token usage for this event, if applicable.
    pub token_usage: Option<TokenUsage>,
    /// Whether the event completed successfully.
    pub success: bool,
    /// Error message if the event failed.
    pub error_message: Option<String>,
    /// Arbitrary key-value metadata attached to the event.
    pub metadata: HashMap<String, String>,
}

impl AuditEntry {
    /// Create a new audit entry with the given event type.
    ///
    /// The timestamp is set to the current time, `success` defaults to `true`,
    /// and all optional fields are `None`.
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            timestamp: iso8601_now(),
            event_type,
            agent_name: None,
            flow_name: None,
            tool_name: None,
            duration_ms: None,
            token_usage: None,
            success: true,
            error_message: None,
            metadata: HashMap::new(),
        }
    }

    /// Set the agent name.
    pub fn with_agent(mut self, name: &str) -> Self {
        self.agent_name = Some(name.to_string());
        self
    }

    /// Set the flow name.
    pub fn with_flow(mut self, name: &str) -> Self {
        self.flow_name = Some(name.to_string());
        self
    }

    /// Set the tool name.
    pub fn with_tool(mut self, name: &str) -> Self {
        self.tool_name = Some(name.to_string());
        self
    }

    /// Set the duration in milliseconds.
    pub fn with_duration_ms(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    /// Set token usage statistics.
    pub fn with_token_usage(mut self, input: u64, output: u64) -> Self {
        self.token_usage = Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
        });
        self
    }

    /// Set the success flag.
    pub fn with_success(mut self, success: bool) -> Self {
        self.success = success;
        self
    }

    /// Set an error message (also sets `success` to `false`).
    pub fn with_error(mut self, message: &str) -> Self {
        self.success = false;
        self.error_message = Some(message.to_string());
        self
    }

    /// Add a metadata key-value pair.
    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }
}

/// Aggregate statistics derived from a collection of audit entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    /// Total number of audit entries recorded.
    pub total_entries: usize,
    /// Number of entries with event type [`AuditEventType::ToolCall`].
    pub total_tool_calls: usize,
    /// Sum of all input and output tokens across all entries.
    pub total_tokens: u64,
    /// Unique agent names that appear in the audit log.
    pub agents_used: Vec<String>,
    /// Number of entries where `success` is `false`.
    pub error_count: usize,
    /// Average duration in milliseconds across entries that have a duration.
    pub avg_duration_ms: f64,
}

// ── AuditLogger ─────────────────────────────────────────────────

/// Thread-safe, in-memory audit logger.
///
/// Collects [`AuditEntry`] records during execution and provides
/// filtering, summarisation, and JSON export.
pub struct AuditLogger {
    entries: Mutex<Vec<AuditEntry>>,
}

impl AuditLogger {
    /// Create an empty audit logger.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Record an audit entry.
    pub fn log(&self, entry: AuditEntry) {
        self.entries.lock().expect("audit lock poisoned").push(entry);
    }

    /// Return a snapshot of all recorded entries.
    pub fn entries(&self) -> Vec<AuditEntry> {
        self.entries.lock().expect("audit lock poisoned").clone()
    }

    /// Return entries for a specific agent.
    pub fn entries_for_agent(&self, agent_name: &str) -> Vec<AuditEntry> {
        self.entries
            .lock()
            .expect("audit lock poisoned")
            .iter()
            .filter(|e| e.agent_name.as_deref() == Some(agent_name))
            .cloned()
            .collect()
    }

    /// Return entries for a specific flow.
    pub fn entries_for_flow(&self, flow_name: &str) -> Vec<AuditEntry> {
        self.entries
            .lock()
            .expect("audit lock poisoned")
            .iter()
            .filter(|e| e.flow_name.as_deref() == Some(flow_name))
            .cloned()
            .collect()
    }

    /// Compute aggregate statistics over all recorded entries.
    pub fn summary(&self) -> AuditSummary {
        let entries = self.entries.lock().expect("audit lock poisoned");

        let total_entries = entries.len();

        let total_tool_calls = entries
            .iter()
            .filter(|e| e.event_type == AuditEventType::ToolCall)
            .count();

        let total_tokens: u64 = entries
            .iter()
            .filter_map(|e| e.token_usage.as_ref())
            .map(|u| u.input_tokens + u.output_tokens)
            .sum();

        let mut agents: Vec<String> = entries
            .iter()
            .filter_map(|e| e.agent_name.clone())
            .collect();
        agents.sort();
        agents.dedup();

        let error_count = entries.iter().filter(|e| !e.success).count();

        let durations: Vec<u64> = entries.iter().filter_map(|e| e.duration_ms).collect();
        let avg_duration_ms = if durations.is_empty() {
            0.0
        } else {
            durations.iter().sum::<u64>() as f64 / durations.len() as f64
        };

        AuditSummary {
            total_entries,
            total_tool_calls,
            total_tokens,
            agents_used: agents,
            error_count,
            avg_duration_ms,
        }
    }

    /// Serialize all entries to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let entries = self.entries.lock().expect("audit lock poisoned");
        serde_json::to_string_pretty(&*entries)
    }

    /// Remove all recorded entries.
    pub fn clear(&self) {
        self.entries.lock().expect("audit lock poisoned").clear();
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ─────────────────────────────────────────────────────

/// Format the current system time as an ISO 8601 string (UTC).
fn iso8601_now() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();

    // Decompose seconds into date and time components (UTC)
    let days = total_secs / 86400;
    let time_secs = total_secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Convert days since epoch to year-month-day using a civil calendar algorithm
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hours, minutes, seconds, millis
    )
}

/// Convert days since the Unix epoch (1970-01-01) to (year, month, day).
///
/// Uses the algorithm from Howard Hinnant's `chrono`-compatible date library.
fn days_to_ymd(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // day of era [0, 146096]
    let yoe =
        (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn log_single_entry() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_agent("researcher"));
        assert_eq!(logger.entries().len(), 1);
        assert_eq!(
            logger.entries()[0].agent_name.as_deref(),
            Some("researcher")
        );
    }

    #[test]
    fn log_multiple_entries() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::ToolCall));
        logger.log(AuditEntry::new(AuditEventType::FlowStart));
        logger.log(AuditEntry::new(AuditEventType::FlowEnd));
        assert_eq!(logger.entries().len(), 3);
    }

    #[test]
    fn filter_by_agent() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_agent("alpha"));
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_agent("beta"));
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_agent("alpha"));

        let alpha = logger.entries_for_agent("alpha");
        assert_eq!(alpha.len(), 2);
        assert!(alpha.iter().all(|e| e.agent_name.as_deref() == Some("alpha")));
    }

    #[test]
    fn filter_by_flow() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::FlowStart).with_flow("onboard"));
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_flow("review"));
        logger.log(AuditEntry::new(AuditEventType::FlowEnd).with_flow("onboard"));

        let onboard = logger.entries_for_flow("onboard");
        assert_eq!(onboard.len(), 2);
    }

    #[test]
    fn filter_returns_empty_for_unknown() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::ToolCall).with_agent("alpha"));
        assert!(logger.entries_for_agent("unknown").is_empty());
        assert!(logger.entries_for_flow("unknown").is_empty());
    }

    #[test]
    fn summary_basic() {
        let logger = AuditLogger::new();
        logger.log(
            AuditEntry::new(AuditEventType::ToolCall)
                .with_agent("a")
                .with_token_usage(100, 50)
                .with_duration_ms(200),
        );
        logger.log(
            AuditEntry::new(AuditEventType::ToolCall)
                .with_agent("b")
                .with_token_usage(80, 40)
                .with_duration_ms(300),
        );
        logger.log(
            AuditEntry::new(AuditEventType::FlowStart)
                .with_agent("a")
                .with_error("something broke"),
        );

        let summary = logger.summary();
        assert_eq!(summary.total_entries, 3);
        assert_eq!(summary.total_tool_calls, 2);
        assert_eq!(summary.total_tokens, 270); // (100+50) + (80+40)
        assert_eq!(summary.agents_used, vec!["a", "b"]);
        assert_eq!(summary.error_count, 1);
        assert!((summary.avg_duration_ms - 250.0).abs() < f64::EPSILON);
    }

    #[test]
    fn summary_empty_logger() {
        let logger = AuditLogger::new();
        let summary = logger.summary();
        assert_eq!(summary.total_entries, 0);
        assert_eq!(summary.total_tool_calls, 0);
        assert_eq!(summary.total_tokens, 0);
        assert!(summary.agents_used.is_empty());
        assert_eq!(summary.error_count, 0);
        assert!((summary.avg_duration_ms - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn json_serialization() {
        let logger = AuditLogger::new();
        logger.log(
            AuditEntry::new(AuditEventType::CacheHit)
                .with_tool("web_search")
                .with_metadata("cache_key", "search:rust"),
        );

        let json = logger.to_json().unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["event_type"], "CacheHit");
        assert_eq!(parsed[0]["tool_name"], "web_search");
        assert_eq!(parsed[0]["metadata"]["cache_key"], "search:rust");
    }

    #[test]
    fn json_roundtrip() {
        let entry = AuditEntry::new(AuditEventType::AgentDispatch)
            .with_agent("writer")
            .with_flow("compose")
            .with_tool("draft")
            .with_duration_ms(150)
            .with_token_usage(200, 100)
            .with_success(true);

        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.event_type, AuditEventType::AgentDispatch);
        assert_eq!(deserialized.agent_name.as_deref(), Some("writer"));
        assert_eq!(deserialized.flow_name.as_deref(), Some("compose"));
        assert_eq!(deserialized.tool_name.as_deref(), Some("draft"));
        assert_eq!(deserialized.duration_ms, Some(150));
        assert_eq!(
            deserialized.token_usage,
            Some(TokenUsage {
                input_tokens: 200,
                output_tokens: 100,
            })
        );
        assert!(deserialized.success);
    }

    #[test]
    fn clear_removes_all_entries() {
        let logger = AuditLogger::new();
        logger.log(AuditEntry::new(AuditEventType::ToolCall));
        logger.log(AuditEntry::new(AuditEventType::ToolCall));
        assert_eq!(logger.entries().len(), 2);

        logger.clear();
        assert!(logger.entries().is_empty());
    }

    #[test]
    fn concurrent_logging() {
        let logger = Arc::new(AuditLogger::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let logger = Arc::clone(&logger);
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    logger.log(
                        AuditEntry::new(AuditEventType::ToolCall)
                            .with_agent(&format!("agent_{}", i))
                            .with_metadata("iteration", &j.to_string()),
                    );
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(logger.entries().len(), 100);
    }

    #[test]
    fn all_event_types_serialize() {
        let types = [
            AuditEventType::ToolCall,
            AuditEventType::FlowStart,
            AuditEventType::FlowEnd,
            AuditEventType::AgentDispatch,
            AuditEventType::MediationCheck,
            AuditEventType::CacheHit,
            AuditEventType::CacheMiss,
            AuditEventType::RateLimitHit,
        ];
        for event_type in &types {
            let entry = AuditEntry::new(event_type.clone());
            let json = serde_json::to_string(&entry).unwrap();
            let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed.event_type, event_type);
        }
    }

    #[test]
    fn error_entry_sets_success_false() {
        let entry = AuditEntry::new(AuditEventType::ToolCall).with_error("timeout");
        assert!(!entry.success);
        assert_eq!(entry.error_message.as_deref(), Some("timeout"));
    }

    #[test]
    fn timestamp_is_iso8601() {
        let entry = AuditEntry::new(AuditEventType::ToolCall);
        // Basic structural check: YYYY-MM-DDTHH:MM:SS.mmmZ
        assert!(entry.timestamp.ends_with('Z'));
        assert_eq!(entry.timestamp.len(), 24);
        assert_eq!(&entry.timestamp[4..5], "-");
        assert_eq!(&entry.timestamp[7..8], "-");
        assert_eq!(&entry.timestamp[10..11], "T");
        assert_eq!(&entry.timestamp[13..14], ":");
        assert_eq!(&entry.timestamp[16..17], ":");
        assert_eq!(&entry.timestamp[19..20], ".");
    }

    #[test]
    fn default_trait() {
        let logger = AuditLogger::default();
        assert!(logger.entries().is_empty());
    }
}
