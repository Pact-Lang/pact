// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-12

//! Per-agent and per-flow rate limiting.
//!
//! Prevents runaway agents from burning API credits by enforcing:
//!
//! - **Per-agent call budgets** — each agent gets a maximum number of API calls
//! - **Per-flow token limits** — each flow gets a maximum token budget
//! - **Global call cap** — hard ceiling on total calls across all agents
//!
//! The [`RateLimiter`] is thread-safe and uses interior mutability via
//! [`std::sync::Mutex`], matching the pattern used in [`crate::cache`].

use std::collections::HashMap;
use std::sync::Mutex;

// ── Configuration ───────────────────────────────────────────────

/// Configuration for rate limiting thresholds.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum API calls allowed per agent.
    pub max_calls_per_agent: u64,
    /// Maximum tokens allowed per flow.
    pub max_tokens_per_flow: u64,
    /// Maximum total API calls across all agents.
    pub max_global_calls: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_calls_per_agent: 100,
            max_tokens_per_flow: 100_000,
            max_global_calls: 1_000,
        }
    }
}

// ── Errors ──────────────────────────────────────────────────────

/// Errors returned when a rate limit is exceeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitError {
    /// The per-agent call budget has been exhausted.
    AgentCallLimitExceeded {
        /// Name of the agent that hit the limit.
        agent_name: String,
        /// Number of calls already made.
        current: u64,
        /// Maximum calls allowed.
        max: u64,
    },
    /// The per-flow token budget has been exhausted.
    FlowTokenLimitExceeded {
        /// Name of the flow that hit the limit.
        flow_name: String,
        /// Tokens already consumed.
        current: u64,
        /// Maximum tokens allowed.
        max: u64,
    },
    /// The global call budget has been exhausted.
    GlobalCallLimitExceeded {
        /// Total calls already made.
        current: u64,
        /// Maximum calls allowed.
        max: u64,
    },
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::AgentCallLimitExceeded {
                agent_name,
                current,
                max,
            } => write!(
                f,
                "RATE LIMIT: agent @{} exceeded call budget ({}/{})",
                agent_name, current, max,
            ),
            RateLimitError::FlowTokenLimitExceeded {
                flow_name,
                current,
                max,
            } => write!(
                f,
                "RATE LIMIT: flow '{}' exceeded token budget ({}/{})",
                flow_name, current, max,
            ),
            RateLimitError::GlobalCallLimitExceeded { current, max } => write!(
                f,
                "RATE LIMIT: global call budget exceeded ({}/{})",
                current, max,
            ),
        }
    }
}

// ── Summary ─────────────────────────────────────────────────────

/// Snapshot of current usage counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSummary {
    /// Per-agent call counts.
    pub agent_calls: HashMap<String, u64>,
    /// Per-flow token totals.
    pub flow_tokens: HashMap<String, u64>,
    /// Total calls across all agents.
    pub global_calls: u64,
}

// ── Rate Limiter ────────────────────────────────────────────────

/// Internal mutable state guarded by a [`Mutex`].
struct RateLimitState {
    agent_calls: HashMap<String, u64>,
    flow_tokens: HashMap<String, u64>,
    global_calls: u64,
}

/// Thread-safe rate limiter for API dispatch.
///
/// Tracks per-agent call counts, per-flow token usage, and a global call
/// counter. All mutations go through a [`Mutex`], so the limiter can be
/// shared across threads without external synchronisation.
///
/// # Example
///
/// ```
/// use pact_dispatch::rate_limit::{RateLimiter, RateLimitConfig};
///
/// let limiter = RateLimiter::new(RateLimitConfig {
///     max_calls_per_agent: 5,
///     max_tokens_per_flow: 10_000,
///     max_global_calls: 50,
/// });
///
/// limiter.check_agent_limit("worker").unwrap();
/// limiter.record_agent_call("worker");
/// ```
pub struct RateLimiter {
    config: RateLimitConfig,
    state: Mutex<RateLimitState>,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimitConfig::default())
    }
}

impl RateLimiter {
    /// Create a rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            state: Mutex::new(RateLimitState {
                agent_calls: HashMap::new(),
                flow_tokens: HashMap::new(),
                global_calls: 0,
            }),
        }
    }

    /// Check whether the named agent can make another API call.
    ///
    /// Returns `Ok(())` if the agent is within its budget, or
    /// [`RateLimitError::AgentCallLimitExceeded`] if not.
    pub fn check_agent_limit(&self, agent_name: &str) -> Result<(), RateLimitError> {
        let state = self.state.lock().unwrap();
        let current = state.agent_calls.get(agent_name).copied().unwrap_or(0);
        if current >= self.config.max_calls_per_agent {
            Err(RateLimitError::AgentCallLimitExceeded {
                agent_name: agent_name.to_string(),
                current,
                max: self.config.max_calls_per_agent,
            })
        } else {
            Ok(())
        }
    }

    /// Record one API call for the named agent.
    ///
    /// Also increments the global call counter.
    pub fn record_agent_call(&self, agent_name: &str) {
        let mut state = self.state.lock().unwrap();
        *state.agent_calls.entry(agent_name.to_string()).or_insert(0) += 1;
        state.global_calls += 1;
    }

    /// Check whether the named flow can consume additional tokens.
    ///
    /// `tokens` is the number of tokens about to be consumed. Returns
    /// `Ok(())` if the flow would remain within budget, or
    /// [`RateLimitError::FlowTokenLimitExceeded`] if the addition would
    /// exceed the limit.
    pub fn check_flow_tokens(&self, flow_name: &str, tokens: u64) -> Result<(), RateLimitError> {
        let state = self.state.lock().unwrap();
        let current = state.flow_tokens.get(flow_name).copied().unwrap_or(0);
        if current + tokens > self.config.max_tokens_per_flow {
            Err(RateLimitError::FlowTokenLimitExceeded {
                flow_name: flow_name.to_string(),
                current,
                max: self.config.max_tokens_per_flow,
            })
        } else {
            Ok(())
        }
    }

    /// Record token usage for the named flow.
    pub fn record_flow_tokens(&self, flow_name: &str, tokens: u64) {
        let mut state = self.state.lock().unwrap();
        *state.flow_tokens.entry(flow_name.to_string()).or_insert(0) += tokens;
    }

    /// Check whether the global call budget has been exhausted.
    ///
    /// Returns `Ok(())` if more calls are allowed, or
    /// [`RateLimitError::GlobalCallLimitExceeded`] if not.
    pub fn check_global_limit(&self) -> Result<(), RateLimitError> {
        let state = self.state.lock().unwrap();
        if state.global_calls >= self.config.max_global_calls {
            Err(RateLimitError::GlobalCallLimitExceeded {
                current: state.global_calls,
                max: self.config.max_global_calls,
            })
        } else {
            Ok(())
        }
    }

    /// Reset all counters to zero.
    ///
    /// Useful between test runs or when starting a new session.
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.agent_calls.clear();
        state.flow_tokens.clear();
        state.global_calls = 0;
    }

    /// Return a snapshot of current usage counters.
    pub fn usage_summary(&self) -> UsageSummary {
        let state = self.state.lock().unwrap();
        UsageSummary {
            agent_calls: state.agent_calls.clone(),
            flow_tokens: state.flow_tokens.clone(),
            global_calls: state.global_calls,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_calls_per_agent, 100);
        assert_eq!(config.max_tokens_per_flow, 100_000);
        assert_eq!(config.max_global_calls, 1_000);
    }

    #[test]
    fn agent_under_limit_passes() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_calls_per_agent: 5,
            ..Default::default()
        });
        limiter.record_agent_call("worker");
        limiter.record_agent_call("worker");
        assert!(limiter.check_agent_limit("worker").is_ok());
    }

    #[test]
    fn agent_at_limit_rejected() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_calls_per_agent: 3,
            ..Default::default()
        });
        for _ in 0..3 {
            limiter.record_agent_call("worker");
        }
        let err = limiter.check_agent_limit("worker").unwrap_err();
        assert_eq!(
            err,
            RateLimitError::AgentCallLimitExceeded {
                agent_name: "worker".to_string(),
                current: 3,
                max: 3,
            }
        );
    }

    #[test]
    fn different_agents_tracked_independently() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_calls_per_agent: 2,
            ..Default::default()
        });
        limiter.record_agent_call("alpha");
        limiter.record_agent_call("alpha");
        limiter.record_agent_call("beta");

        assert!(limiter.check_agent_limit("alpha").is_err());
        assert!(limiter.check_agent_limit("beta").is_ok());
    }

    #[test]
    fn flow_under_token_limit_passes() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_tokens_per_flow: 10_000,
            ..Default::default()
        });
        limiter.record_flow_tokens("main", 5_000);
        assert!(limiter.check_flow_tokens("main", 4_000).is_ok());
    }

    #[test]
    fn flow_exceeding_token_limit_rejected() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_tokens_per_flow: 10_000,
            ..Default::default()
        });
        limiter.record_flow_tokens("main", 8_000);
        let err = limiter.check_flow_tokens("main", 3_000).unwrap_err();
        assert_eq!(
            err,
            RateLimitError::FlowTokenLimitExceeded {
                flow_name: "main".to_string(),
                current: 8_000,
                max: 10_000,
            }
        );
    }

    #[test]
    fn global_limit_enforced() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_global_calls: 3,
            ..Default::default()
        });
        limiter.record_agent_call("a");
        limiter.record_agent_call("b");
        limiter.record_agent_call("c");
        let err = limiter.check_global_limit().unwrap_err();
        assert_eq!(
            err,
            RateLimitError::GlobalCallLimitExceeded {
                current: 3,
                max: 3,
            }
        );
    }

    #[test]
    fn reset_clears_all_counters() {
        let limiter = RateLimiter::new(RateLimitConfig {
            max_calls_per_agent: 2,
            max_tokens_per_flow: 1_000,
            max_global_calls: 5,
        });
        limiter.record_agent_call("worker");
        limiter.record_agent_call("worker");
        limiter.record_flow_tokens("main", 900);
        assert!(limiter.check_agent_limit("worker").is_err());

        limiter.reset();

        assert!(limiter.check_agent_limit("worker").is_ok());
        assert!(limiter.check_flow_tokens("main", 500).is_ok());
        assert!(limiter.check_global_limit().is_ok());
        let summary = limiter.usage_summary();
        assert!(summary.agent_calls.is_empty());
        assert!(summary.flow_tokens.is_empty());
        assert_eq!(summary.global_calls, 0);
    }

    #[test]
    fn usage_summary_reflects_state() {
        let limiter = RateLimiter::default();
        limiter.record_agent_call("alpha");
        limiter.record_agent_call("alpha");
        limiter.record_agent_call("beta");
        limiter.record_flow_tokens("pipeline", 1_500);

        let summary = limiter.usage_summary();
        assert_eq!(summary.agent_calls.get("alpha"), Some(&2));
        assert_eq!(summary.agent_calls.get("beta"), Some(&1));
        assert_eq!(summary.flow_tokens.get("pipeline"), Some(&1_500));
        assert_eq!(summary.global_calls, 3);
    }

    #[test]
    fn concurrent_access_is_safe() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(RateLimiter::new(RateLimitConfig {
            max_calls_per_agent: 1_000,
            max_tokens_per_flow: 1_000_000,
            max_global_calls: 10_000,
        }));

        let mut handles = Vec::new();
        for i in 0..10 {
            let limiter = Arc::clone(&limiter);
            handles.push(thread::spawn(move || {
                let name = format!("agent_{}", i);
                for _ in 0..100 {
                    limiter.record_agent_call(&name);
                    limiter.record_flow_tokens("shared", 10);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let summary = limiter.usage_summary();
        assert_eq!(summary.global_calls, 1_000);
        assert_eq!(summary.flow_tokens.get("shared"), Some(&10_000));
        for i in 0..10 {
            assert_eq!(
                summary.agent_calls.get(&format!("agent_{}", i)),
                Some(&100)
            );
        }
    }

    #[test]
    fn error_display_messages() {
        let err = RateLimitError::AgentCallLimitExceeded {
            agent_name: "bot".to_string(),
            current: 100,
            max: 100,
        };
        assert!(err.to_string().contains("RATE LIMIT"));
        assert!(err.to_string().contains("@bot"));

        let err = RateLimitError::FlowTokenLimitExceeded {
            flow_name: "pipeline".to_string(),
            current: 90_000,
            max: 100_000,
        };
        assert!(err.to_string().contains("pipeline"));

        let err = RateLimitError::GlobalCallLimitExceeded {
            current: 1_000,
            max: 1_000,
        };
        assert!(err.to_string().contains("global"));
    }

    #[test]
    fn unknown_agent_has_zero_calls() {
        let limiter = RateLimiter::default();
        assert!(limiter.check_agent_limit("never_seen").is_ok());

        let summary = limiter.usage_summary();
        assert_eq!(summary.agent_calls.get("never_seen"), None);
    }

    #[test]
    fn flow_token_check_is_prospective() {
        // check_flow_tokens should consider the *proposed* addition, not just current
        let limiter = RateLimiter::new(RateLimitConfig {
            max_tokens_per_flow: 100,
            ..Default::default()
        });
        limiter.record_flow_tokens("f", 50);
        // 50 + 50 = 100, exactly at limit — should pass
        assert!(limiter.check_flow_tokens("f", 50).is_ok());
        // 50 + 51 = 101, over limit — should fail
        assert!(limiter.check_flow_tokens("f", 51).is_err());
    }
}
