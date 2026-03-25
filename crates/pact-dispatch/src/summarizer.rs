// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-25

//! Session summarization for agent memory.
//!
//! Produces compact summaries from a session's observations.
//! Uses heuristic extraction (no LLM call required) to capture:
//! - Tools called and their sequence
//! - Key outputs and error states
//! - Token usage statistics

use crate::observation_store::{Observation, ObservationKind, ObservationStore};

/// A structured session summary.
#[derive(Debug, Clone)]
pub struct SessionSummary {
    /// The session ID this summary covers.
    pub session_id: String,
    /// The agent that ran the session.
    pub agent: String,
    /// Tools called in order.
    pub tools_called: Vec<String>,
    /// Number of tool calls made.
    pub tool_call_count: usize,
    /// Number of errors encountered.
    pub error_count: usize,
    /// Total tokens used across the session.
    pub total_tokens: u64,
    /// The final agent response (truncated).
    pub final_output: Option<String>,
    /// Compact text summary.
    pub text: String,
}

/// Maximum length for the final output snippet in a summary.
const MAX_OUTPUT_SNIPPET: usize = 500;

/// Summarize a session's observations into a compact representation.
pub fn summarize_session(
    store: &ObservationStore,
    session_id: &str,
) -> Result<SessionSummary, rusqlite::Error> {
    let observations = store.get_session_observations(session_id)?;
    Ok(build_summary(session_id, &observations))
}

/// Build a summary from a list of observations (pure logic, no DB access).
pub fn build_summary(session_id: &str, observations: &[Observation]) -> SessionSummary {
    let agent = observations
        .first()
        .map(|o| o.agent.clone())
        .unwrap_or_default();

    let mut tools_called = Vec::new();
    let mut error_count = 0;
    let mut total_tokens: u64 = 0;
    let mut final_output = None;

    for obs in observations {
        if let Some(tokens) = obs.tokens_used {
            total_tokens += tokens;
        }

        match obs.kind {
            ObservationKind::ToolCall => {
                if let Some(tool) = &obs.tool {
                    tools_called.push(tool.clone());
                }
            }
            ObservationKind::Error => {
                error_count += 1;
            }
            ObservationKind::AgentResponse => {
                final_output = Some(truncate(&obs.output, MAX_OUTPUT_SNIPPET));
            }
            ObservationKind::ToolResult => {}
        }
    }

    let tool_call_count = tools_called.len();

    // Build compact text
    let text = format_summary_text(
        &agent,
        &tools_called,
        error_count,
        total_tokens,
        &final_output,
    );

    SessionSummary {
        session_id: session_id.to_string(),
        agent,
        tools_called,
        tool_call_count,
        error_count,
        total_tokens,
        final_output,
        text,
    }
}

fn format_summary_text(
    agent: &str,
    tools_called: &[String],
    error_count: usize,
    total_tokens: u64,
    final_output: &Option<String>,
) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Agent '{}' session", agent));

    if !tools_called.is_empty() {
        // Deduplicate consecutive tool calls
        let unique_tools: Vec<&str> = dedup_consecutive(tools_called);
        parts.push(format!(
            "called {} tool(s): {}",
            tools_called.len(),
            unique_tools.join(" -> ")
        ));
    } else {
        parts.push("no tool calls".to_string());
    }

    if error_count > 0 {
        parts.push(format!("{} error(s)", error_count));
    }

    if total_tokens > 0 {
        parts.push(format!("{} tokens used", total_tokens));
    }

    let mut text = parts.join("; ");

    if let Some(output) = final_output {
        text.push_str(&format!(". Output: {}", output));
    }

    text
}

/// Deduplicate consecutive identical strings, preserving order.
fn dedup_consecutive(items: &[String]) -> Vec<&str> {
    let mut result = Vec::new();
    for item in items {
        if result.last() != Some(&item.as_str()) {
            result.push(item.as_str());
        }
    }
    result
}

/// Truncate a string to max_len, appending "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Summarize and persist a session's summary into the observation store.
pub fn finalize_session(
    store: &ObservationStore,
    session_id: &str,
) -> Result<SessionSummary, rusqlite::Error> {
    let summary = summarize_session(store, session_id)?;
    store.end_session(session_id, Some(&summary.text))?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation_store::{new_observation, ObservationKind, ObservationStore};

    #[test]
    fn summarize_empty_session() {
        let summary = build_summary("sess-empty", &[]);
        assert_eq!(summary.tool_call_count, 0);
        assert_eq!(summary.error_count, 0);
        assert_eq!(summary.total_tokens, 0);
        assert!(summary.final_output.is_none());
    }

    #[test]
    fn summarize_tool_sequence() {
        let obs = vec![
            new_observation(
                "s1",
                "agent_a",
                Some("fetch"),
                Some("{}"),
                "tool_use:fetch",
                None,
                ObservationKind::ToolCall,
            ),
            new_observation(
                "s1",
                "agent_a",
                Some("fetch"),
                None,
                "data",
                Some(100),
                ObservationKind::ToolResult,
            ),
            new_observation(
                "s1",
                "agent_a",
                Some("classify"),
                Some("{}"),
                "tool_use:classify",
                None,
                ObservationKind::ToolCall,
            ),
            new_observation(
                "s1",
                "agent_a",
                Some("classify"),
                None,
                "classified",
                Some(200),
                ObservationKind::ToolResult,
            ),
            new_observation(
                "s1",
                "agent_a",
                Some("fetch"),
                None,
                "final report",
                Some(150),
                ObservationKind::AgentResponse,
            ),
        ];

        let summary = build_summary("s1", &obs);
        assert_eq!(summary.agent, "agent_a");
        assert_eq!(summary.tool_call_count, 2);
        assert_eq!(summary.tools_called, vec!["fetch", "classify"]);
        assert_eq!(summary.total_tokens, 450);
        assert_eq!(summary.final_output, Some("final report".to_string()));
        assert!(summary.text.contains("fetch -> classify"));
    }

    #[test]
    fn summarize_with_errors() {
        let obs = vec![
            new_observation(
                "s2",
                "agent_b",
                Some("search"),
                Some("{}"),
                "tool_use:search",
                None,
                ObservationKind::ToolCall,
            ),
            new_observation(
                "s2",
                "agent_b",
                Some("search"),
                None,
                "failed",
                None,
                ObservationKind::Error,
            ),
        ];

        let summary = build_summary("s2", &obs);
        assert_eq!(summary.error_count, 1);
        assert!(summary.text.contains("1 error(s)"));
    }

    #[test]
    fn finalize_persists_summary() {
        let store = ObservationStore::in_memory().unwrap();
        let sid = "sess-fin";
        store.start_session(sid, "agent_c").unwrap();

        let obs = new_observation(
            sid,
            "agent_c",
            Some("tool_x"),
            None,
            "tool_use:tool_x",
            None,
            ObservationKind::ToolCall,
        );
        store.record(&obs).unwrap();

        let obs = new_observation(
            sid,
            "agent_c",
            Some("tool_x"),
            None,
            "done",
            Some(50),
            ObservationKind::AgentResponse,
        );
        store.record(&obs).unwrap();

        let summary = finalize_session(&store, sid).unwrap();
        assert_eq!(summary.tool_call_count, 1);

        // Verify persisted
        let stored = store.get_session_summary(sid).unwrap();
        assert!(stored.is_some());
        assert!(stored.unwrap().contains("agent_c"));
    }

    #[test]
    fn dedup_consecutive_tools() {
        let tools = vec!["a".into(), "a".into(), "b".into(), "a".into()];
        let result = dedup_consecutive(&tools);
        assert_eq!(result, vec!["a", "b", "a"]);
    }

    #[test]
    fn truncate_long_output() {
        let long = "x".repeat(600);
        let result = truncate(&long, 500);
        assert_eq!(result.len(), 503); // 500 + "..."
        assert!(result.ends_with("..."));
    }
}
