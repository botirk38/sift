use std::ops::Range;
use std::sync::Arc;

use crate::search::input::FileIdentity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEvent {
    Begin(FileEvent),
    Match(MatchEvent),
    Context(ContextEvent),
    ContextBreak,
    Binary(BinaryEvent),
    End(FileEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEvent {
    pub file: Arc<FileIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchEvent {
    pub file: Arc<FileIdentity>,
    pub line_number: Option<u64>,
    pub absolute_byte_offset: Option<u64>,
    pub bytes: Vec<u8>,
    pub ranges: Vec<Range<usize>>,
    pub replacement: Option<Vec<u8>>,
    pub replacement_matches: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEvent {
    pub file: Arc<FileIdentity>,
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
    pub file: Arc<FileIdentity>,
    pub absolute_byte_offset: u64,
    pub explicit: bool,
}

pub trait SearchSink {
    /// Receive one semantic search event.
    ///
    /// # Errors
    ///
    /// Returns an error if the sink cannot accept the event.
    fn event(&mut self, event: SearchEvent) -> crate::Result<()>;
}

/// Whether search records semantic events.
pub enum Events<'a> {
    /// Drop events during search.
    Discard,
    /// Buffer events during search, then deliver to the sink.
    Emit(&'a mut dyn SearchSink),
}
