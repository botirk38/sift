use std::time::Duration;

use grep_matcher::{Captures, Matcher as GrepMatcher};

use crate::search::event::{
    BinaryEvent, ContextEvent, ContextKind, Events, FileEvent, MatchEvent, SearchEvent,
};
use crate::search::hit::{
    LineCount, ListedFile, ListedRow, Listing, Match, MatchedFile, SpanCount,
};
use crate::search::input::Origin;
use crate::search::line::Line;
use crate::search::matcher::Matcher;
use crate::search::mode::SearchMode;
use crate::search::options::SearchOptions;
use crate::search::stats::{MatchTotals, Stats, StatsMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Buffer {
    Discard,
    Collect,
}

impl Buffer {
    pub(super) const fn from(events: &Events<'_>) -> Self {
        match events {
            Events::Discard => Self::Discard,
            Events::Emit(_) => Self::Collect,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchEmission {
    Presence,
    LineCount,
    Lines,
    Spans,
}

impl MatchEmission {
    const fn from(mode: SearchMode, options: &SearchOptions) -> Self {
        if options.replace.is_some() {
            return if matches!(mode, SearchMode::Matches) || options.only_matching() {
                Self::Spans
            } else {
                Self::Lines
            };
        }
        match mode {
            SearchMode::FilesWithMatches | SearchMode::FilesWithoutMatch | SearchMode::Paths => {
                Self::Presence
            }
            SearchMode::CountLines { .. } => Self::LineCount,
            SearchMode::CountMatches { .. } | SearchMode::Matches => Self::Spans,
            SearchMode::Lines if options.only_matching() => Self::Spans,
            SearchMode::Lines => Self::Lines,
        }
    }
}

pub(super) struct FileReport {
    pub(crate) matched: bool,
    pub(crate) row: Option<ListedRow>,
    pub(crate) events: Vec<SearchEvent>,
    pub(crate) line_matches: usize,
    pub(crate) match_spans: usize,
    pub(crate) bytes_searched: u64,
    matches: Vec<Match>,
    binary_byte_offset: Option<u64>,
}

impl FileReport {
    pub(super) const fn new() -> Self {
        Self {
            matched: false,
            row: None,
            events: Vec::new(),
            line_matches: 0,
            match_spans: 0,
            bytes_searched: 0,
            matches: Vec::new(),
            binary_byte_offset: None,
        }
    }

    pub(super) const fn listed(origin: Origin) -> Self {
        Self {
            matched: true,
            row: Some(ListedRow::MatchingPath(ListedFile {
                origin,
                binary_byte_offset: None,
            })),
            events: Vec::new(),
            line_matches: 0,
            match_spans: 0,
            bytes_searched: 0,
            matches: Vec::new(),
            binary_byte_offset: None,
        }
    }

    pub(super) fn drain_events(&mut self) -> impl Iterator<Item = SearchEvent> + '_ {
        self.events.drain(..)
    }

    pub(super) fn begin(&mut self, origin: &Origin, buffer: Buffer) {
        match buffer {
            Buffer::Discard => {}
            Buffer::Collect => self.events.push(SearchEvent::Begin(FileEvent {
                origin: origin.clone(),
            })),
        }
    }

    pub(super) fn hit(
        &mut self,
        origin: &Origin,
        line: &Line<'_>,
        matcher: &Matcher,
        mode: SearchMode,
        buffer: Buffer,
        options: &SearchOptions,
    ) {
        self.line_matches += 1;
        self.matched = true;
        match matcher {
            Matcher::Rust(inner) => {
                self.record_match(origin, line, inner, mode, buffer, options);
            }
            Matcher::Pcre2(inner) => {
                self.record_match(origin, line, inner, mode, buffer, options);
            }
        }
    }

    pub(super) fn context(
        &mut self,
        origin: &Origin,
        kind: ContextKind,
        line: &Line<'_>,
        buffer: Buffer,
    ) {
        match buffer {
            Buffer::Discard => {}
            Buffer::Collect => self.events.push(SearchEvent::Context(ContextEvent {
                origin: origin.clone(),
                kind,
                line_number: line.number,
                absolute_byte_offset: line.offset,
                bytes: line.bytes().to_vec(),
            })),
        }
    }

    pub(super) fn gap(&mut self, buffer: Buffer) {
        match buffer {
            Buffer::Discard => {}
            Buffer::Collect => self.events.push(SearchEvent::ContextBreak),
        }
    }

    pub(super) fn binary(&mut self, origin: &Origin, offset: u64, explicit: bool, buffer: Buffer) {
        self.binary_byte_offset.get_or_insert(offset);
        match buffer {
            Buffer::Discard => {}
            Buffer::Collect => self.events.push(SearchEvent::Binary(BinaryEvent {
                origin: origin.clone(),
                absolute_byte_offset: offset,
                explicit,
            })),
        }
    }

    pub(super) fn finish(
        &mut self,
        origin: Origin,
        mode: SearchMode,
        buffer: Buffer,
        bytes_searched: u64,
    ) {
        self.bytes_searched = bytes_searched;
        match buffer {
            Buffer::Discard => {}
            Buffer::Collect => self.events.push(SearchEvent::End(FileEvent {
                origin: origin.clone(),
            })),
        }
        let listed_file = ListedFile {
            origin,
            binary_byte_offset: self.binary_byte_offset,
        };
        self.row = if mode.admits(self.matched) {
            Some(match mode {
                SearchMode::FilesWithMatches | SearchMode::Paths => {
                    ListedRow::MatchingPath(listed_file)
                }
                SearchMode::FilesWithoutMatch => ListedRow::NonMatchingPath(listed_file),
                SearchMode::CountLines { .. } => ListedRow::LineCount(LineCount {
                    file: listed_file,
                    lines: self.line_matches,
                }),
                SearchMode::CountMatches { .. } => ListedRow::SpanCount(SpanCount {
                    file: listed_file,
                    spans: self.match_spans,
                }),
                SearchMode::Lines => ListedRow::Lines(MatchedFile {
                    file: listed_file,
                    matches: std::mem::take(&mut self.matches),
                }),
                SearchMode::Matches => ListedRow::Spans(MatchedFile {
                    file: listed_file,
                    matches: std::mem::take(&mut self.matches),
                }),
            })
        } else {
            None
        };
    }

    fn record_match<M: GrepMatcher>(
        &mut self,
        origin: &Origin,
        line: &Line<'_>,
        matcher: &M,
        mode: SearchMode,
        buffer: Buffer,
        options: &SearchOptions,
    ) {
        let emission = MatchEmission::from(mode, options);
        let replacement = options.replace.as_deref().map(str::as_bytes);
        let line_no = usize::try_from(line.number.unwrap_or(0)).unwrap_or(0);
        let line_bytes = line.bytes();
        match emission {
            MatchEmission::Presence | MatchEmission::LineCount => {
                if matches!(buffer, Buffer::Collect) {
                    self.events.push(SearchEvent::Match(MatchEvent {
                        origin: origin.clone(),
                        line_number: line.number,
                        absolute_byte_offset: Some(line.offset),
                        bytes: Vec::new(),
                        ranges: Vec::new(),
                        replacement: None,
                        replacement_matches: Vec::new(),
                    }));
                }
            }
            MatchEmission::Lines if matches!(buffer, Buffer::Discard) => {
                if replacement.is_some() {
                    self.count_spans(matcher, line_bytes);
                }
                self.matches.push(Match {
                    line: line_no,
                    text: String::from_utf8_lossy(line_bytes).into_owned(),
                });
            }
            MatchEmission::Lines => {
                let expanded = replacement.and_then(|replacement| {
                    Replacement::expand(matcher, line_bytes, replacement).ok()
                });
                let mut ranges = Vec::new();
                let _ = matcher.find_iter(line_bytes, |m: grep_matcher::Match| {
                    ranges.push(m.start()..m.end());
                    self.match_spans += 1;
                    true
                });
                self.events.push(SearchEvent::Match(MatchEvent {
                    origin: origin.clone(),
                    line_number: line.number,
                    absolute_byte_offset: Some(line.offset),
                    bytes: line_bytes.to_vec(),
                    ranges,
                    replacement: expanded
                        .as_ref()
                        .map(|replacement| replacement.line.clone()),
                    replacement_matches: expanded.map_or_else(Vec::new, |r| r.matches),
                }));
            }
            MatchEmission::Spans if matches!(buffer, Buffer::Discard) => match mode {
                SearchMode::CountMatches { .. } | SearchMode::CountLines { .. } => {
                    self.count_spans(matcher, line_bytes);
                }
                _ if replacement.is_some() => self.count_spans(matcher, line_bytes),
                _ => self.collect_span_matches(matcher, line_no, line_bytes),
            },
            MatchEmission::Spans => {
                self.emit_span_event(origin, matcher, line, line_bytes, replacement);
            }
        }
    }

    fn count_spans<M: GrepMatcher>(&mut self, matcher: &M, line_bytes: &[u8]) {
        let _ = matcher.find_iter(line_bytes, |_m: grep_matcher::Match| {
            self.match_spans += 1;
            true
        });
    }

    fn collect_span_matches<M: GrepMatcher>(
        &mut self,
        matcher: &M,
        line: usize,
        line_bytes: &[u8],
    ) {
        let _ = matcher.find_iter(line_bytes, |m: grep_matcher::Match| {
            self.match_spans += 1;
            self.matches.push(Match {
                line,
                text: String::from_utf8_lossy(&line_bytes[m.start()..m.end()]).into_owned(),
            });
            true
        });
    }

    fn emit_span_event<M: GrepMatcher>(
        &mut self,
        origin: &Origin,
        matcher: &M,
        line: &Line<'_>,
        line_bytes: &[u8],
        replacement: Option<&[u8]>,
    ) {
        let expanded = replacement
            .and_then(|replacement| Replacement::expand(matcher, line_bytes, replacement).ok());
        let mut ranges = Vec::new();
        let _ = matcher.find_iter(line_bytes, |m: grep_matcher::Match| {
            ranges.push(m.start()..m.end());
            self.match_spans += 1;
            true
        });
        self.events.push(SearchEvent::Match(MatchEvent {
            origin: origin.clone(),
            line_number: line.number,
            absolute_byte_offset: Some(line.offset),
            bytes: line_bytes.to_vec(),
            ranges,
            replacement: expanded
                .as_ref()
                .map(|replacement| replacement.line.clone()),
            replacement_matches: expanded.map_or_else(Vec::new, |r| r.matches),
        }));
    }
}

struct Replacement {
    line: Vec<u8>,
    matches: Vec<Vec<u8>>,
}

impl Replacement {
    fn expand<M: GrepMatcher>(
        matcher: &M,
        bytes: &[u8],
        replacement: &[u8],
    ) -> Result<Self, M::Error> {
        let mut caps = matcher.new_captures()?;
        let mut line = Vec::new();
        let mut spans = Vec::new();
        matcher.replace_with_captures(bytes, &mut caps, &mut line, |captures, dst| {
            let start = dst.len();
            captures.interpolate(|name| matcher.capture_index(name), bytes, replacement, dst);
            spans.push(dst[start..].to_vec());
            true
        })?;
        Ok(Self {
            line,
            matches: spans,
        })
    }
}

/// Result of a search run.
pub struct SearchReport {
    pub listed: Listing,
    pub stats: Option<Stats>,
}

impl SearchReport {
    pub(crate) fn empty(stats: StatsMode, mode: SearchMode) -> Self {
        Self {
            listed: Listing::empty(mode),
            stats: stats.collect().then(Stats::default),
        }
    }

    pub(super) fn from(
        reports: Vec<FileReport>,
        mode: SearchMode,
        stats: StatsMode,
        inputs_len: usize,
        elapsed: Duration,
    ) -> Self {
        let mut listed = Listing::empty(mode);
        let mut files_with_matches = 0usize;
        let mut match_lines = 0usize;
        let mut match_spans = 0usize;
        let mut bytes_searched = 0u64;

        for report in reports {
            if report.matched {
                files_with_matches += 1;
            }
            match_lines = match_lines.saturating_add(report.line_matches);
            match_spans = match_spans.saturating_add(report.match_spans);
            bytes_searched = bytes_searched.saturating_add(report.bytes_searched);
            if let Some(row) = report.row {
                listed.push_row(row);
            }
        }

        let stats = stats.collect().then_some(Stats {
            matches: match mode {
                SearchMode::FilesWithMatches | SearchMode::FilesWithoutMatch => {
                    MatchTotals::Lines(files_with_matches)
                }
                SearchMode::Paths => MatchTotals::None,
                SearchMode::CountMatches { .. } | SearchMode::Matches => {
                    MatchTotals::Spans(match_spans)
                }
                SearchMode::Lines | SearchMode::CountLines { .. } => {
                    MatchTotals::Lines(match_lines)
                }
            },
            files_with_matches: match mode {
                SearchMode::Paths => 0,
                _ => files_with_matches,
            },
            files_searched: inputs_len,
            bytes_printed: 0,
            bytes_searched,
            elapsed,
        });

        Self { listed, stats }
    }

    /// Whether the search should exit successfully (ripgrep-compatible).
    ///
    /// Not the same as [`Listing::is_empty`]: count `--include-zero` may list
    /// zeros while this returns false when no pattern hits occurred.
    #[must_use]
    pub fn found(&self) -> bool {
        match &self.listed {
            Listing::MatchingPaths(v) | Listing::NonMatchingPaths(v) => !v.is_empty(),
            Listing::Lines(v) | Listing::Spans(v) => !v.is_empty(),
            Listing::LineCounts(v) => v.iter().any(|c| c.lines > 0),
            Listing::SpanCounts(v) => v.iter().any(|c| c.spans > 0),
        }
    }

    #[must_use]
    pub const fn stats(&self) -> Option<&Stats> {
        self.stats.as_ref()
    }
}
