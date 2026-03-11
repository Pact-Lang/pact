// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2025-03-22

//! Character stream with position tracking for the lexer.
//!
//! [`Cursor`] wraps a source string and provides peek/advance operations
//! that track the current byte offset. The lexer uses the byte offsets to
//! construct [`Span`](crate::span::Span)s for every token.

/// A cursor over a source string that tracks the current byte position.
#[derive(Debug)]
pub struct Cursor<'src> {
    /// The full source text as a byte slice.
    src: &'src str,
    /// Iterator over `(byte_offset, char)` pairs.
    chars: std::str::CharIndices<'src>,
    /// The most recently consumed character and its byte offset, if any.
    prev: Option<(usize, char)>,
    /// The next character and its byte offset, peeked from the iterator.
    current: Option<(usize, char)>,
}

impl<'src> Cursor<'src> {
    /// Create a new cursor positioned at the start of `src`.
    pub fn new(src: &'src str) -> Self {
        let mut chars = src.char_indices();
        let current = chars.next();
        Self {
            src,
            chars,
            prev: None,
            current,
        }
    }

    /// Return the current byte offset in the source.
    ///
    /// If the cursor is at EOF, returns the length of the source.
    pub fn offset(&self) -> usize {
        match self.current {
            Some((off, _)) => off,
            None => self.src.len(),
        }
    }

    /// Peek at the next character without consuming it.
    pub fn peek(&self) -> Option<char> {
        self.current.map(|(_, c)| c)
    }

    /// Peek at the character after the current one.
    pub fn peek_next(&self) -> Option<char> {
        let mut clone = self.chars.clone();
        clone.next().map(|(_, c)| c)
    }

    /// Consume and return the current character, advancing the cursor.
    pub fn advance(&mut self) -> Option<char> {
        let current = self.current?;
        self.prev = Some(current);
        self.current = self.chars.next();
        Some(current.1)
    }

    /// Return `true` if the cursor has reached the end of the source.
    pub fn is_at_end(&self) -> bool {
        self.current.is_none()
    }

    /// Extract a substring from the source by byte range.
    pub fn slice(&self, start: usize, end: usize) -> &'src str {
        &self.src[start..end]
    }

    /// Advance while a predicate holds, returning the consumed characters.
    pub fn eat_while(&mut self, pred: impl Fn(char) -> bool) {
        while let Some(c) = self.peek() {
            if pred(c) {
                self.advance();
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_advance() {
        let mut c = Cursor::new("abc");
        assert_eq!(c.offset(), 0);
        assert_eq!(c.peek(), Some('a'));
        assert_eq!(c.advance(), Some('a'));
        assert_eq!(c.offset(), 1);
        assert_eq!(c.advance(), Some('b'));
        assert_eq!(c.advance(), Some('c'));
        assert!(c.is_at_end());
        assert_eq!(c.advance(), None);
    }

    #[test]
    fn peek_next() {
        let c = Cursor::new("ab");
        assert_eq!(c.peek(), Some('a'));
        assert_eq!(c.peek_next(), Some('b'));
    }

    #[test]
    fn eat_while_digits() {
        let mut c = Cursor::new("123abc");
        c.eat_while(|ch| ch.is_ascii_digit());
        assert_eq!(c.offset(), 3);
        assert_eq!(c.peek(), Some('a'));
    }

    #[test]
    fn empty_source() {
        let c = Cursor::new("");
        assert!(c.is_at_end());
        assert_eq!(c.peek(), None);
        assert_eq!(c.offset(), 0);
    }
}
