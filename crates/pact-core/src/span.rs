// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-03-15

//! Source location tracking for error reporting.
//!
//! Every token and AST node carries a [`Span`] that records its byte range
//! in the original source text. The [`SourceMap`] maps [`SourceId`]s to the
//! full source strings so that diagnostic reporters (miette) can display
//! annotated source snippets.

use std::collections::HashMap;

/// An opaque identifier for a source file loaded into the [`SourceMap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceId(
    /// Numeric index into the source map.
    pub u32,
);

/// A half-open byte range `[start, end)` inside a single source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// The source file this span belongs to.
    pub source: SourceId,
    /// Byte offset of the first character (inclusive).
    pub start: usize,
    /// Byte offset past the last character (exclusive).
    pub end: usize,
}

impl Span {
    /// Create a new span covering `[start, end)` in the given source.
    pub fn new(source: SourceId, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    /// Return the byte length of this span.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Return true if this span covers zero bytes.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Merge two spans into one that covers both (they must share a source).
    pub fn merge(self, other: Span) -> Span {
        debug_assert_eq!(self.source, other.source);
        Span {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// Stores source text keyed by [`SourceId`] for diagnostic rendering.
#[derive(Debug, Default)]
pub struct SourceMap {
    sources: HashMap<SourceId, NamedSource>,
    next_id: u32,
}

/// A source file with a name (for display) and its full text.
#[derive(Debug)]
struct NamedSource {
    name: String,
    text: String,
}

impl SourceMap {
    /// Create an empty source map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a source file and return its id.
    pub fn add(&mut self, name: impl Into<String>, text: impl Into<String>) -> SourceId {
        let id = SourceId(self.next_id);
        self.next_id += 1;
        self.sources.insert(
            id,
            NamedSource {
                name: name.into(),
                text: text.into(),
            },
        );
        id
    }

    /// Retrieve the source text for a given id.
    pub fn text(&self, id: SourceId) -> &str {
        &self.sources[&id].text
    }

    /// Retrieve the file name for a given id.
    pub fn name(&self, id: SourceId) -> &str {
        &self.sources[&id].name
    }

    /// Build a miette `NamedSource` for use in diagnostics.
    pub fn miette_source(&self, id: SourceId) -> miette::NamedSource<String> {
        let src = &self.sources[&id];
        miette::NamedSource::new(&src.name, src.text.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_map_round_trip() {
        let mut sm = SourceMap::new();
        let id = sm.add("test.pact", "hello world");
        assert_eq!(sm.text(id), "hello world");
        assert_eq!(sm.name(id), "test.pact");
    }

    #[test]
    fn span_merge() {
        let src = SourceId(0);
        let a = Span::new(src, 0, 5);
        let b = Span::new(src, 3, 10);
        let merged = a.merge(b);
        assert_eq!(merged.start, 0);
        assert_eq!(merged.end, 10);
    }

    #[test]
    fn span_len_and_empty() {
        let src = SourceId(0);
        let span = Span::new(src, 5, 10);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());

        let empty = Span::new(src, 5, 5);
        assert!(empty.is_empty());
    }
}
