// Copyright (c) 2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-25

//! TF-IDF semantic search over observations and session summaries.
//!
//! Provides self-contained search without external embedding services.
//! The index is built on-demand from the observation store and scored
//! using term frequency–inverse document frequency (TF-IDF).

use std::collections::HashMap;

use crate::observation_store::{Observation, ObservationStore};

/// A search result with its relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The observation that matched.
    pub observation: Observation,
    /// TF-IDF relevance score (higher = more relevant).
    pub score: f64,
}

/// A document in the search index, mapping to one observation.
struct Document {
    /// Index into the observations vector.
    obs_index: usize,
    /// Term frequencies for this document.
    term_freqs: HashMap<String, f64>,
    /// Total number of terms in the document.
    term_count: usize,
}

/// TF-IDF search index built from observations.
pub struct SearchIndex {
    documents: Vec<Document>,
    observations: Vec<Observation>,
    /// Inverse document frequency for each term.
    idf: HashMap<String, f64>,
}

impl SearchIndex {
    /// Build a search index from a set of observations.
    pub fn build(observations: Vec<Observation>) -> Self {
        let mut documents = Vec::with_capacity(observations.len());
        let mut doc_freq: HashMap<String, usize> = HashMap::new();

        for (i, obs) in observations.iter().enumerate() {
            let text = observation_text(obs);
            let tokens = tokenize(&text);
            let term_count = tokens.len();

            let mut term_freqs: HashMap<String, f64> = HashMap::new();
            for token in &tokens {
                *term_freqs.entry(token.clone()).or_default() += 1.0;
            }

            // Track which terms appear in this document (for IDF)
            for term in term_freqs.keys() {
                *doc_freq.entry(term.clone()).or_default() += 1;
            }

            documents.push(Document {
                obs_index: i,
                term_freqs,
                term_count,
            });
        }

        // Compute IDF: log(N / df)
        let n = observations.len().max(1) as f64;
        let idf: HashMap<String, f64> = doc_freq
            .into_iter()
            .map(|(term, df)| (term, (n / df as f64).ln()))
            .collect();

        Self {
            documents,
            observations,
            idf,
        }
    }

    /// Search for observations matching a query, returning top-k results.
    pub fn search(&self, query: &str, top_k: usize) -> Vec<SearchResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(usize, f64)> = self
            .documents
            .iter()
            .map(|doc| {
                let score = self.score_document(doc, &query_tokens);
                (doc.obs_index, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);

        scored
            .into_iter()
            .map(|(idx, score)| SearchResult {
                observation: self.observations[idx].clone(),
                score,
            })
            .collect()
    }

    /// Compute TF-IDF score for a document against query terms.
    fn score_document(&self, doc: &Document, query_tokens: &[String]) -> f64 {
        if doc.term_count == 0 {
            return 0.0;
        }

        query_tokens
            .iter()
            .map(|term| {
                let tf = doc.term_freqs.get(term).copied().unwrap_or(0.0) / doc.term_count as f64;
                let idf = self.idf.get(term).copied().unwrap_or(0.0);
                tf * idf
            })
            .sum()
    }

    /// Number of documents in the index.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }
}

/// Search an agent's recent observations.
pub fn search_agent_observations(
    store: &ObservationStore,
    agent: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let observations = store.get_agent_observations(agent, 1000)?;
    let index = SearchIndex::build(observations);
    Ok(index.search(query, top_k))
}

/// Extract searchable text from an observation.
fn observation_text(obs: &Observation) -> String {
    let mut parts = vec![obs.output.clone()];
    if let Some(tool) = &obs.tool {
        parts.push(tool.clone());
    }
    if let Some(input) = &obs.input {
        parts.push(input.clone());
    }
    parts.push(obs.agent.clone());
    parts.join(" ")
}

/// Tokenize text into lowercase terms, filtering short tokens and stopwords.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() > 2)
        .map(|s| s.to_lowercase())
        .filter(|s| !is_stopword(s))
        .collect()
}

/// Basic English stopwords.
fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "the" | "and" | "for" | "are" | "but" | "not" | "you" | "all"
            | "can" | "had" | "her" | "was" | "one" | "our" | "out"
            | "has" | "have" | "been" | "from" | "this" | "that"
            | "with" | "will" | "they" | "each" | "which" | "their"
            | "said" | "what" | "its" | "into" | "than" | "them"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation_store::{new_observation, ObservationKind};

    fn make_obs(agent: &str, tool: &str, output: &str) -> Observation {
        new_observation(
            "sess-search",
            agent,
            Some(tool),
            None,
            output,
            None,
            ObservationKind::ToolResult,
        )
    }

    #[test]
    fn search_finds_relevant_observations() {
        let observations = vec![
            make_obs("agent_a", "fetch_issues", "found 42 open issues about rendering"),
            make_obs("agent_a", "classify", "classified issues by diagram type"),
            make_obs("agent_a", "profile", "built persona profiles for developers"),
            make_obs("agent_a", "cluster", "identified 8 themes in the backlog"),
        ];

        let index = SearchIndex::build(observations);
        let results = index.search("rendering issues", 2);

        assert!(!results.is_empty());
        assert!(results[0].observation.output.contains("rendering"));
    }

    #[test]
    fn search_ranks_by_relevance() {
        let observations = vec![
            make_obs("a", "t1", "rust programming language safety features"),
            make_obs("a", "t2", "rust oxidation chemical process iron"),
            make_obs("a", "t3", "programming language design type safety"),
        ];

        let index = SearchIndex::build(observations);
        let results = index.search("rust programming", 3);

        // The first observation should rank highest (has both terms)
        assert!(results[0].score >= results[1].score);
        assert!(results[0].observation.output.contains("rust programming"));
    }

    #[test]
    fn empty_query_returns_nothing() {
        let observations = vec![make_obs("a", "t", "some data")];
        let index = SearchIndex::build(observations);
        assert!(index.search("", 5).is_empty());
    }

    #[test]
    fn search_no_matches() {
        let observations = vec![
            make_obs("a", "t", "completely unrelated content about cooking"),
        ];
        let index = SearchIndex::build(observations);
        let results = index.search("quantum physics", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn tokenize_filters_short_and_stopwords() {
        let tokens = tokenize("the quick brown fox and a lazy dog");
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"and".to_string()));
        assert!(tokens.contains(&"quick".to_string()));
        assert!(tokens.contains(&"brown".to_string()));
        assert!(tokens.contains(&"lazy".to_string()));
    }

    #[test]
    fn index_size() {
        let observations = vec![
            make_obs("a", "t1", "one"),
            make_obs("a", "t2", "two"),
        ];
        let index = SearchIndex::build(observations);
        assert_eq!(index.len(), 2);
        assert!(!index.is_empty());
    }

    #[test]
    fn search_with_store() {
        use crate::observation_store::ObservationStore;

        let store = ObservationStore::in_memory().unwrap();
        let sid = "sess-search-store";
        store.start_session(sid, "agent_s").unwrap();

        let obs = new_observation(
            sid, "agent_s", Some("fetch"), None, "found issues about flowchart rendering",
            None, ObservationKind::ToolResult,
        );
        store.record(&obs).unwrap();

        let obs = new_observation(
            sid, "agent_s", Some("classify"), None, "classified by persona and diagram",
            None, ObservationKind::ToolResult,
        );
        store.record(&obs).unwrap();

        let results = search_agent_observations(&store, "agent_s", "flowchart", 5).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].observation.output.contains("flowchart"));
    }
}
