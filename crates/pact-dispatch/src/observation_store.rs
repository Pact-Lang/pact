// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-25

//! Observation capture for agent memory.
//!
//! Records tool calls, results, and agent responses into a SQLite database
//! for later summarization and semantic search. Observations are scoped
//! per session and per agent.

use rusqlite::{params, Connection};
use std::path::PathBuf;

/// The kind of observation recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationKind {
    /// An outbound tool call (agent → tool).
    ToolCall,
    /// A tool execution result (tool → agent).
    ToolResult,
    /// The final agent response at end of dispatch.
    AgentResponse,
    /// An error during tool execution or dispatch.
    Error,
}

impl ObservationKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ToolCall => "tool_call",
            Self::ToolResult => "tool_result",
            Self::AgentResponse => "agent_response",
            Self::Error => "error",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "tool_call" => Self::ToolCall,
            "tool_result" => Self::ToolResult,
            "agent_response" => Self::AgentResponse,
            "error" => Self::Error,
            _ => Self::Error,
        }
    }
}

/// A single recorded observation.
#[derive(Debug, Clone)]
pub struct Observation {
    pub id: String,
    pub session_id: String,
    pub agent: String,
    pub tool: Option<String>,
    pub input: Option<String>,
    pub output: String,
    pub tokens_used: Option<u64>,
    pub timestamp: String,
    pub kind: ObservationKind,
}

/// SQLite-backed observation store.
pub struct ObservationStore {
    conn: Connection,
}

impl ObservationStore {
    /// Open or create the observation database.
    pub fn open() -> Result<Self, rusqlite::Error> {
        let path = Self::db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path)?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS observations (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                agent TEXT NOT NULL,
                tool TEXT,
                input TEXT,
                output TEXT NOT NULL,
                tokens_used INTEGER,
                timestamp TEXT NOT NULL,
                kind TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                agent TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                summary TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_obs_session ON observations(session_id);
            CREATE INDEX IF NOT EXISTS idx_obs_agent ON observations(agent);
            CREATE INDEX IF NOT EXISTS idx_obs_timestamp ON observations(timestamp);",
        )?;
        Ok(())
    }

    /// Record a new observation.
    pub fn record(&self, obs: &Observation) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO observations (id, session_id, agent, tool, input, output, tokens_used, timestamp, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                obs.id,
                obs.session_id,
                obs.agent,
                obs.tool,
                obs.input,
                obs.output,
                obs.tokens_used.map(|t| t as i64),
                obs.timestamp,
                obs.kind.as_str(),
            ],
        )?;
        Ok(())
    }

    /// Start a new session.
    pub fn start_session(&self, session_id: &str, agent: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO sessions (id, agent, started_at) VALUES (?1, ?2, ?3)",
            params![session_id, agent, now()],
        )?;
        Ok(())
    }

    /// End a session with an optional summary.
    pub fn end_session(
        &self,
        session_id: &str,
        summary: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, summary = ?2 WHERE id = ?3",
            params![now(), summary, session_id],
        )?;
        Ok(())
    }

    /// Get all observations for a session, ordered by timestamp.
    pub fn get_session_observations(
        &self,
        session_id: &str,
    ) -> Result<Vec<Observation>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, agent, tool, input, output, tokens_used, timestamp, kind
             FROM observations WHERE session_id = ?1 ORDER BY timestamp ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(Observation {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent: row.get(2)?,
                tool: row.get(3)?,
                input: row.get(4)?,
                output: row.get(5)?,
                tokens_used: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                timestamp: row.get(7)?,
                kind: ObservationKind::from_str(&row.get::<_, String>(8)?),
            })
        })?;
        rows.collect()
    }

    /// Get recent observations for an agent (across sessions), most recent first.
    pub fn get_agent_observations(
        &self,
        agent: &str,
        limit: usize,
    ) -> Result<Vec<Observation>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, agent, tool, input, output, tokens_used, timestamp, kind
             FROM observations WHERE agent = ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![agent, limit as i64], |row| {
            Ok(Observation {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent: row.get(2)?,
                tool: row.get(3)?,
                input: row.get(4)?,
                output: row.get(5)?,
                tokens_used: row.get::<_, Option<i64>>(6)?.map(|t| t as u64),
                timestamp: row.get(7)?,
                kind: ObservationKind::from_str(&row.get::<_, String>(8)?),
            })
        })?;
        rows.collect()
    }

    /// Count observations for a session.
    pub fn count_session_observations(
        &self,
        session_id: &str,
    ) -> Result<usize, rusqlite::Error> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM observations WHERE session_id = ?1",
            params![session_id],
            |row| row.get::<_, i64>(0).map(|c| c as usize),
        )
    }

    /// Get the session summary, if one exists.
    pub fn get_session_summary(
        &self,
        session_id: &str,
    ) -> Result<Option<String>, rusqlite::Error> {
        self.conn.query_row(
            "SELECT summary FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
    }

    fn db_path() -> PathBuf {
        let dir =
            std::env::var("PACT_OBSERVATION_DIR").unwrap_or_else(|_| ".pact/observations".into());
        PathBuf::from(dir).join("observations.db")
    }
}

/// Create a new observation with a generated ID and current timestamp.
pub fn new_observation(
    session_id: &str,
    agent: &str,
    tool: Option<&str>,
    input: Option<&str>,
    output: &str,
    tokens_used: Option<u64>,
    kind: ObservationKind,
) -> Observation {
    Observation {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        agent: agent.to_string(),
        tool: tool.map(String::from),
        input: input.map(String::from),
        output: output.to_string(),
        tokens_used,
        timestamp: now(),
        kind,
    }
}

/// Generate a new session ID.
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_retrieve_observation() {
        let store = ObservationStore::in_memory().unwrap();
        let sid = "sess-1";
        store.start_session(sid, "test_agent").unwrap();

        let obs = new_observation(
            sid,
            "test_agent",
            Some("search"),
            Some("{\"q\": \"rust\"}"),
            "found 42 results",
            Some(150),
            ObservationKind::ToolResult,
        );
        store.record(&obs).unwrap();

        let results = store.get_session_observations(sid).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent, "test_agent");
        assert_eq!(results[0].tool.as_deref(), Some("search"));
        assert_eq!(results[0].output, "found 42 results");
        assert_eq!(results[0].kind, ObservationKind::ToolResult);
    }

    #[test]
    fn session_lifecycle() {
        let store = ObservationStore::in_memory().unwrap();
        let sid = "sess-2";
        store.start_session(sid, "agent_a").unwrap();
        store
            .end_session(sid, Some("Agent processed 3 tool calls"))
            .unwrap();

        let summary = store.get_session_summary(sid).unwrap();
        assert_eq!(summary, Some("Agent processed 3 tool calls".to_string()));
    }

    #[test]
    fn agent_observations_across_sessions() {
        let store = ObservationStore::in_memory().unwrap();

        for i in 0..5 {
            let sid = format!("sess-{}", i);
            store.start_session(&sid, "agent_x").unwrap();
            let obs = new_observation(
                &sid,
                "agent_x",
                Some("tool_a"),
                None,
                &format!("result {}", i),
                None,
                ObservationKind::ToolResult,
            );
            store.record(&obs).unwrap();
        }

        let recent = store.get_agent_observations("agent_x", 3).unwrap();
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].output, "result 4");
    }

    #[test]
    fn count_session_observations() {
        let store = ObservationStore::in_memory().unwrap();
        let sid = "sess-count";
        store.start_session(sid, "counter").unwrap();

        for i in 0..4 {
            let obs = new_observation(
                sid,
                "counter",
                Some("t"),
                None,
                &format!("r{}", i),
                None,
                ObservationKind::ToolCall,
            );
            store.record(&obs).unwrap();
        }

        assert_eq!(store.count_session_observations(sid).unwrap(), 4);
    }

    #[test]
    fn observation_kind_roundtrip() {
        for kind in [
            ObservationKind::ToolCall,
            ObservationKind::ToolResult,
            ObservationKind::AgentResponse,
            ObservationKind::Error,
        ] {
            assert_eq!(ObservationKind::from_str(kind.as_str()), kind);
        }
    }
}
