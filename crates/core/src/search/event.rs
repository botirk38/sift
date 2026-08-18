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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replacement {
    pub text: Vec<u8>,
    pub matches: Vec<Vec<u8>>,
}

impl Replacement {
    /// A replacement whose haystack is a single match span.
    #[must_use]
    pub fn one(text: Vec<u8>) -> Self {
        Self {
            text: text.clone(),
            matches: vec![text],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvent {
    pub origin: Origin,
    pub line_number: Option<u64>,
    pub absolute_byte_offset: Option<u64>,
    pub bytes: Vec<u8>,
    pub ranges: Vec<Range<usize>>,
    pub replacement: Option<Replacement>,
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
