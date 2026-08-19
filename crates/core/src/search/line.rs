use std::borrow::Cow;
#[cfg(test)]
use std::ops::Range;

use memchr::{memchr, memchr2};

use crate::search::event::Span;

pub(super) struct Line<'a> {
    pub number: Option<u64>,
    pub offset: u64,
    pub bytes: Cow<'a, [u8]>,
}

/// How searchable bytes are split into lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Break {
    Term(u8),
    /// Convert NULs without rewriting the bytes: split on the terminator or NUL.
    TermOrNul(u8),
}

impl Break {
    pub(super) const fn term(self) -> u8 {
        match self {
            Self::Term(term) | Self::TermOrNul(term) => term,
        }
    }

    const fn nul(self) -> bool {
        matches!(self, Self::TermOrNul(_))
    }
}

/// Absolute regex spans in the searchable bytes for a multiline search.
pub(super) struct Multiline {
    spans: Vec<Span>,
}

impl Multiline {
    pub(super) const fn new(spans: Vec<Span>) -> Self {
        Self { spans }
    }

    pub(super) const fn count(&self) -> usize {
        self.spans.len()
    }

    pub(super) fn keep_first(&mut self, n: usize) {
        self.spans.truncate(n);
    }

    pub(super) fn overlaps(&self, line: &Line<'_>) -> bool {
        let (start, end) = Self::bounds(line);
        self.spans.iter().any(|span| {
            let range = &span.range;
            start < range.end && end > range.start
        })
    }

    /// Spans whose start lies on this line.
    pub(super) fn starting_on(&self, line: &Line<'_>) -> impl Iterator<Item = &Span> + '_ {
        let (start, end) = Self::bounds(line);
        self.spans.iter().filter(move |span| {
            let range = &span.range;
            range.start >= start && range.start < end
        })
    }

    /// Whether a replace already consumed this line as the middle of a span.
    pub(super) fn consumed(&self, line: &Line<'_>, term: u8, crlf: bool) -> bool {
        let start = usize::try_from(line.offset).unwrap_or(usize::MAX);
        let content_end = start.saturating_add(line.without_terminator(term, crlf).len());
        self.spans
            .iter()
            .any(|span| span.range.start < start && span.range.end >= content_end)
    }

    fn bounds(line: &Line<'_>) -> (usize, usize) {
        let start = usize::try_from(line.offset).unwrap_or(usize::MAX);
        (start, start.saturating_add(line.bytes().len()))
    }
}

pub(super) struct Lines<'a> {
    chunk: &'a [u8],
    brk: Break,
    numbered: bool,
    number: u64,
    offset: u64,
    pos: usize,
}

impl<'a> Lines<'a> {
    pub(super) const fn new(chunk: &'a [u8], brk: Break, numbered: bool) -> Self {
        Self {
            chunk,
            brk,
            numbered,
            number: 1,
            offset: 0,
            pos: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn covering(bytes: &[u8], term: u8, span: Range<usize>) -> Range<usize> {
        let start = memchr::memrchr(term, &bytes[..span.start]).map_or(0, |i| i + 1);
        let end =
            memchr::memchr(term, &bytes[span.end..]).map_or(bytes.len(), |i| span.end + i + 1);
        start..end
    }

    const fn next_number(&mut self) -> Option<u64> {
        if !self.numbered {
            return None;
        }
        let number = self.number;
        self.number += 1;
        Some(number)
    }
}

impl<'a> Iterator for Lines<'a> {
    type Item = Line<'a>;

    fn next(&mut self) -> Option<Line<'a>> {
        if self.pos >= self.chunk.len() {
            return None;
        }
        let start = self.pos;
        let rel = if self.brk.nul() {
            memchr2(self.brk.term(), 0, &self.chunk[start..])
                .map_or(self.chunk.len() - start, |i| i + 1)
        } else {
            memchr(self.brk.term(), &self.chunk[start..])
                .map_or(self.chunk.len() - start, |i| i + 1)
        };
        self.pos = start + rel;
        let offset = self.offset;
        self.offset += rel as u64;
        Some(Line {
            number: self.next_number(),
            offset,
            bytes: Cow::Borrowed(&self.chunk[start..self.pos]),
        })
    }
}

impl Line<'_> {
    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn end(&self) -> u64 {
        self.offset + u64::try_from(self.bytes().len()).unwrap_or(u64::MAX)
    }

    pub(super) fn into_owned(self) -> Line<'static> {
        Line {
            number: self.number,
            offset: self.offset,
            bytes: Cow::Owned(self.bytes.into_owned()),
        }
    }

    pub(super) fn without_terminator(&self, term: u8, crlf: bool) -> &[u8] {
        self.without_break(Break::Term(term), crlf)
    }

    pub(super) fn without_break(&self, brk: Break, crlf: bool) -> &[u8] {
        let bytes = self.bytes();
        let Some((last, rest)) = bytes.split_last() else {
            return bytes;
        };
        let term = brk.term();
        if *last != term && !(brk.nul() && *last == 0) {
            return bytes;
        }
        if crlf
            && *last == term
            && let Some((before, stripped)) = rest.split_last()
            && *before == b'\r'
        {
            return stripped;
        }
        rest
    }
}

#[cfg(test)]
mod tests {
    use super::{Break, Lines};

    #[test]
    fn splits_on_newline() {
        let lines: Vec<_> = Lines::new(b"a\nb\n", Break::Term(b'\n'), true).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].number, Some(1));
        assert_eq!(lines[0].bytes(), b"a\n");
        assert_eq!(lines[1].bytes(), b"b\n");
    }

    #[test]
    fn last_item_may_lack_terminator() {
        let lines: Vec<_> = Lines::new(b"hello\nworld", Break::Term(b'\n'), true).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].bytes(), b"world");
        assert_eq!(lines[1].number, Some(2));
    }

    #[test]
    fn without_terminator_strips_crlf() {
        let line = super::Line {
            number: Some(1),
            offset: 0,
            bytes: std::borrow::Cow::Borrowed(b"hi\r\n"),
        };
        assert_eq!(line.without_terminator(b'\n', true), b"hi");
        assert_eq!(line.without_terminator(b'\n', false), b"hi\r");
    }

    #[test]
    fn covering_expands_to_line() {
        let bytes = b"aa\nneedle\nzz\n";
        let span = Lines::covering(bytes, b'\n', 4..8);
        assert_eq!(&bytes[span.clone()], b"needle\n");
        assert_eq!(span, 3..10);
    }

    #[test]
    fn term_or_nul_splits_without_rewrite() {
        let lines: Vec<_> = Lines::new(b"a\0b\n", Break::TermOrNul(b'\n'), true).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].bytes(), b"a\0");
        assert_eq!(lines[1].bytes(), b"b\n");
        assert_eq!(lines[0].without_break(Break::TermOrNul(b'\n'), false), b"a");
    }
}
