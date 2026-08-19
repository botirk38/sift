use std::borrow::Cow;
use std::ops::Range;

use crate::search::input::Origin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    Begin(Origin),
    Match(MatchEvent),
    Context(ContextEvent),
    ContextBreak,
    Binary(BinaryEvent),
    End(Origin),
}

/// One match in a haystack, with optional replacement text for that range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub replacement: Option<Vec<u8>>,
}

impl Span {
    #[must_use]
    pub fn text<'a>(&'a self, haystack: &'a [u8]) -> &'a [u8] {
        self.replacement
            .as_deref()
            .unwrap_or_else(|| haystack.get(self.range.clone()).unwrap_or(&[]))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvent {
    pub origin: Origin,
    pub line_number: Option<u64>,
    pub absolute_byte_offset: Option<u64>,
    pub bytes: Vec<u8>,
    pub spans: Vec<Span>,
}

impl MatchEvent {
    /// Line bytes after interpolating replacements, or the original bytes.
    #[must_use]
    pub fn line_bytes(&self) -> Cow<'_, [u8]> {
        if self.spans.iter().all(|span| span.replacement.is_none()) {
            Cow::Borrowed(&self.bytes)
        } else {
            let mut out = Vec::new();
            let mut last = 0;
            for span in &self.spans {
                let start = span.range.start.min(self.bytes.len());
                out.extend_from_slice(&self.bytes[last.min(self.bytes.len())..start]);
                out.extend_from_slice(span.text(&self.bytes));
                last = span.range.end.min(self.bytes.len());
            }
            out.extend_from_slice(&self.bytes[last.min(self.bytes.len())..]);
            Cow::Owned(out)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEvent {
    pub origin: Origin,
    pub kind: ContextKind,
    pub line_number: Option<u64>,
    pub absolute_byte_offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextKind {
    Before,
    After,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinaryEvent {
    pub origin: Origin,
    pub absolute_byte_offset: u64,
}
