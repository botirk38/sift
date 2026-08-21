use std::collections::VecDeque;
use std::time::Instant;

use rayon::prelude::*;

use crate::Error;
use crate::search::bytes::Bytes;
use crate::search::error::Error as SearchError;
use crate::search::event::{BinaryEvent, ContextEvent, MatchEvent, SearchEvent, Span};
use crate::search::hit::Match;
use crate::search::input::{Input, Inputs, Origin};
use crate::search::line::Line;
use crate::search::matcher::Matcher;
use crate::search::mode::{Hit, SearchMode};
use crate::search::options::{Nul, SearchBound, SearchOptions};
use crate::search::query::Query;
use crate::search::report::{FileReport, Reports, SearchReport};
use crate::search::scan::{FileScan, Item};

#[derive(Debug, Clone)]
pub struct Searcher {
    query: Query,
    matcher: Matcher,
}

impl Searcher {
    /// Build a ready searcher by creating the query matcher.
    ///
    /// # Errors
    ///
    /// Returns an error if matcher construction fails.
    pub fn new(query: Query) -> Result<Self, SearchError> {
        let matcher = Matcher::compile(&query)?;
        let query = query.with_engine(matcher.resolved_engine());
        Ok(Self { query, matcher })
    }

    #[must_use]
    pub const fn query(&self) -> &Query {
        &self.query
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.query.patterns
    }

    #[must_use]
    pub const fn options(&self) -> &SearchOptions {
        &self.query.options
    }

    /// Search inputs and materialize a listing report.
    ///
    /// # Errors
    ///
    /// Returns an error if search execution fails.
    pub fn execute(&self, inputs: Inputs<'_>, mode: SearchMode) -> crate::Result<SearchReport> {
        if self.options().max_results == Some(0) {
            return Err(Error::Search(SearchError::InvalidMaxCount));
        }
        if inputs.is_empty() {
            return Ok(SearchReport::empty(mode));
        }

        let started = Instant::now();
        let items = inputs.into_vec();
        let reports = match mode {
            SearchMode::Paths => Reports::listed(&items),
            _ => match self.options().search_bound {
                SearchBound::Exhaustive => {
                    let files = items
                        .par_iter()
                        .filter_map(|input| self.search_one(input, mode).transpose())
                        .collect::<crate::Result<Vec<_>>>()?;
                    Reports::exhaustive(files, items.len())
                }
                SearchBound::FirstMatch => {
                    let mut reports = Reports::empty();
                    for input in &items {
                        if reports.tried(self.search_one(input, mode)?, mode) {
                            break;
                        }
                    }
                    reports
                }
            },
        };
        Ok(SearchReport::from(reports, mode, started.elapsed()))
    }

    /// Stream semantic events for each input. Pull to completion, then [`Events::into_report`].
    #[must_use]
    pub fn stream<'a>(&'a self, inputs: Inputs<'a>, mode: SearchMode) -> Events<'a> {
        Events::new(self, inputs, mode)
    }

    fn search_one(&self, input: &Input<'_>, mode: SearchMode) -> crate::Result<Option<FileReport>> {
        let Ok(mut loaded) = Bytes::open(input, self.options().io) else {
            return Ok(None);
        };
        loaded.decode(&self.options().input_encoding);
        let nul = self.nul(input);
        let binary = self.convert(&mut loaded, nul);
        let haystack = loaded.as_slice();
        let mut scan = FileScan::new(haystack, &self.matcher, self.options(), mode, nul, binary)?;
        let mut report = FileReport::new(input.origin().clone());
        while let Some(item) = scan.next_item() {
            if let Item::Hit(line) = item? {
                for event in self.match_events(input.origin().clone(), &line, &mut scan)? {
                    for listed in Self::listing(&event, mode) {
                        report.push_match(listed);
                    }
                    if matches!(mode, SearchMode::Print(_)) {
                        report.events.push(event);
                    }
                }
            }
        }
        report.matched = scan.matched();
        report.hits = scan.hits();
        report.bytes_searched = scan.bytes_searched();
        report.binary = scan.binary();
        Ok(Some(report))
    }

    const fn nul(&self, input: &Input<'_>) -> Nul {
        let options = self.options();
        options
            .binary_mode
            .nul(input.mention(), options.null_data())
    }

    fn convert(&self, loaded: &mut Bytes<'_>, nul: Nul) -> Option<u64> {
        let options = self.options();
        if matches!(nul, Nul::Convert) && options.multiline() {
            loaded.convert_nul(options.line_terminator())
        } else if matches!(nul, Nul::Convert) {
            memchr::memchr(0, loaded.as_slice()).map(|idx| u64::try_from(idx).unwrap_or(u64::MAX))
        } else {
            None
        }
    }

    fn match_events(
        &self,
        origin: Origin,
        line: &Line<'_>,
        scan: &mut FileScan<'_>,
    ) -> Result<Vec<MatchEvent>, SearchError> {
        let invert = self.options().invert_match();
        let template = self.options().replace.as_deref().map(str::as_bytes);
        if invert {
            return Ok(vec![MatchEvent {
                origin,
                line_number: line.number,
                absolute_byte_offset: Some(line.offset),
                bytes: line.bytes().to_vec(),
                spans: Vec::new(),
            }]);
        }
        if let Some(spans) = scan.spans() {
            return Ok(spans
                .starting_on(line)
                .map(|span| {
                    let bytes = scan.slice().get(span.range.clone()).unwrap_or(b"").to_vec();
                    MatchEvent {
                        origin: origin.clone(),
                        line_number: line.number,
                        absolute_byte_offset: Some(line.offset),
                        spans: vec![Span {
                            range: 0..bytes.len(),
                            replacement: span.replacement.clone(),
                        }],
                        bytes,
                    }
                })
                .collect());
        }
        let bytes = line.bytes().to_vec();
        let spans = self.matcher.spans(&bytes, template, scan.scratch())?;
        Ok(vec![MatchEvent {
            origin,
            line_number: line.number,
            absolute_byte_offset: Some(line.offset),
            bytes,
            spans,
        }])
    }

    fn listing(event: &MatchEvent, mode: SearchMode) -> Vec<Match> {
        if !matches!(mode, SearchMode::Print(_)) {
            return Vec::new();
        }
        let line = usize::try_from(event.line_number.unwrap_or(0)).unwrap_or(0);
        match mode.hit() {
            Some(Hit::Span) if event.spans.is_empty() => vec![Match {
                line,
                text: String::from_utf8_lossy(&event.bytes).into_owned(),
            }],
            Some(Hit::Span) => event
                .spans
                .iter()
                .map(|span| Match {
                    line,
                    text: String::from_utf8_lossy(span.text(&event.bytes)).into_owned(),
                })
                .collect(),
            Some(Hit::Line) => vec![Match {
                line,
                text: String::from_utf8_lossy(&event.line_bytes()).into_owned(),
            }],
            None => Vec::new(),
        }
    }
}

/// Streaming search output. Pull events, then [`Self::into_report`].
pub struct Events<'a> {
    searcher: &'a Searcher,
    mode: SearchMode,
    inputs: Vec<Input<'a>>,
    index: usize,
    pending: VecDeque<SearchEvent>,
    reports: Reports,
    started: Instant,
    invalid: bool,
    exhausted: bool,
}

impl<'a> Events<'a> {
    fn new(searcher: &'a Searcher, inputs: Inputs<'a>, mode: SearchMode) -> Self {
        Self {
            searcher,
            mode,
            inputs: inputs.into_vec(),
            index: 0,
            pending: VecDeque::new(),
            reports: if matches!(mode, SearchMode::Paths) {
                Reports::listed(&[])
            } else {
                Reports::empty()
            },
            started: Instant::now(),
            invalid: searcher.options().max_results == Some(0),
            exhausted: false,
        }
    }

    /// Finish the report after events have been pulled.
    #[must_use]
    pub fn into_report(mut self) -> SearchReport {
        if matches!(self.mode, SearchMode::Paths) {
            self.reports = Reports::listed(&self.inputs);
        }
        SearchReport::from(self.reports, self.mode, self.started.elapsed())
    }

    fn fill(&mut self) -> crate::Result<()> {
        while self.pending.is_empty() && !self.exhausted && self.index < self.inputs.len() {
            let i = self.index;
            self.index += 1;
            if matches!(self.mode, SearchMode::Paths) {
                continue;
            }
            if self.open_file(i)?.is_none() {
                self.reports.tried(None, self.mode);
            }
        }
        Ok(())
    }

    fn open_file(&mut self, i: usize) -> crate::Result<Option<()>> {
        let io = self.searcher.options().io;
        let encoding = self.searcher.options().input_encoding;
        let (origin, chunk, nul, binary) = {
            let input = &self.inputs[i];
            let Ok(mut loaded) = Bytes::open(input, io) else {
                return Ok(None);
            };
            loaded.decode(&encoding);
            let nul = self.searcher.nul(input);
            let binary = self.searcher.convert(&mut loaded, nul);
            (
                input.origin().clone(),
                loaded.as_slice().to_vec(),
                nul,
                binary,
            )
        };
        let mut scan = FileScan::new(
            &chunk,
            &self.searcher.matcher,
            self.searcher.options(),
            self.mode,
            nul,
            binary,
        )?;
        self.pending.push_back(SearchEvent::Begin(origin.clone()));
        let mut body = VecDeque::new();
        while let Some(item) = scan.next_item() {
            match item? {
                Item::Hit(line) => {
                    for event in self
                        .searcher
                        .match_events(origin.clone(), &line, &mut scan)?
                    {
                        body.push_back(SearchEvent::Match(event));
                    }
                }
                Item::Context { line, kind } => {
                    body.push_back(SearchEvent::Context(ContextEvent {
                        origin: origin.clone(),
                        kind,
                        line_number: line.number,
                        absolute_byte_offset: line.offset,
                        bytes: line.bytes().to_vec(),
                    }));
                }
                Item::Break => body.push_back(SearchEvent::ContextBreak),
            }
        }
        if scan.matched()
            && let Some(offset) = scan.binary()
        {
            self.pending.push_back(SearchEvent::Binary(BinaryEvent {
                origin: origin.clone(),
                absolute_byte_offset: offset,
            }));
        }
        self.pending.append(&mut body);
        self.pending.push_back(SearchEvent::End(origin.clone()));
        let mut report = FileReport::new(origin);
        report.matched = scan.matched();
        report.hits = scan.hits();
        report.bytes_searched = scan.bytes_searched();
        report.binary = scan.binary();
        if self.reports.tried(Some(report), self.mode)
            && matches!(
                self.searcher.options().search_bound,
                SearchBound::FirstMatch
            )
        {
            self.exhausted = true;
        }
        Ok(Some(()))
    }
}

impl Iterator for Events<'_> {
    type Item = crate::Result<SearchEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.invalid {
            self.invalid = false;
            self.exhausted = true;
            return Some(Err(Error::Search(SearchError::InvalidMaxCount)));
        }
        if let Some(event) = self.pending.pop_front() {
            return Some(Ok(event));
        }
        match self.fill() {
            Err(err) => {
                self.exhausted = true;
                Some(Err(err))
            }
            Ok(()) => self.pending.pop_front().map(Ok),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::File;
    use crate::search::event::SearchEvent;
    use crate::search::hit::Listing;
    use crate::search::input::{Mention, Origin};
    use crate::search::mode::{Hit, SearchMode, ZeroCounts};
    use crate::search::options::{BinaryMode, Io, RegexEngine, SearchBound, SearchFlags};
    use crate::search::query::Query;
    use std::borrow::Cow;
    use std::path::PathBuf;

    #[test]
    fn auto_engine_fallback_to_pcre2_disables_narrowing() {
        let query = Query::new(
            vec![r"(?<=foo)bar".into()],
            SearchOptions {
                regex_engine: RegexEngine::Auto,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.narrowing(), crate::search::Narrowing::Allowed);
        let searcher = Searcher::new(query).expect("compile via Auto→PCRE2");
        assert_eq!(searcher.query().options().regex_engine, RegexEngine::Pcre2);
        assert_eq!(
            searcher.query().narrowing(),
            crate::search::Narrowing::Disabled
        );
    }

    fn collect(
        searcher: &Searcher,
        inputs: Inputs<'_>,
        mode: SearchMode,
    ) -> (Vec<SearchEvent>, SearchReport) {
        let mut events = searcher.stream(inputs, mode);
        let mut out = Vec::new();
        for event in events.by_ref() {
            out.push(event.expect("event"));
        }
        (out, events.into_report())
    }

    fn stream(bytes: &[u8], mention: Mention) -> Inputs<'_> {
        let mut inputs = Inputs::empty();
        inputs.push(Input::Bytes {
            origin: Origin::stream("t"),
            bytes: Cow::Borrowed(bytes),
            mention,
        });
        inputs
    }

    fn searcher(pattern: &str, options: SearchOptions) -> Searcher {
        let query = Query::new(vec![pattern.into()], options).expect("query");
        Searcher::new(query).expect("searcher")
    }

    #[test]
    fn quit_stops_before_nul_on_discovered_input() {
        let report = searcher(
            "later",
            SearchOptions {
                binary_mode: BinaryMode::Quit,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"findme\0later\n", Mention::Discovered),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(!report.found());
    }

    #[test]
    fn convert_rewrites_nul_so_later_line_matches() {
        let report = searcher(
            "later",
            SearchOptions {
                binary_mode: BinaryMode::Binary,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"findme\0later\n", Mention::Discovered),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(report.found());
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert!(files[0].matches.iter().any(|m| m.text.contains("later")));
    }

    #[test]
    fn as_text_matches_across_nul_on_one_line() {
        let report = searcher(
            "later",
            SearchOptions {
                binary_mode: BinaryMode::AsText,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"findme\0later\n", Mention::Discovered),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(report.found());
    }

    #[test]
    fn max_results_stops_after_n_hits() {
        let report = searcher(
            "needle",
            SearchOptions {
                max_results: Some(2),
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"needle\nneedle\nneedle\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files[0].matches.len(), 2);
    }

    #[test]
    fn execute_print_keeps_match_event_spans() {
        let report = searcher("needle", SearchOptions::default())
            .execute(
                stream(b"needle here\n", Mention::Explicit),
                SearchMode::Print(Hit::Line),
            )
            .expect("execute");
        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].events.len(), 1);
        let event = &report.files[0].events[0];
        assert_eq!(event.spans[0].range, 0..6);
        assert_eq!(&event.bytes[event.spans[0].range.clone()], b"needle");
    }

    #[test]
    fn passthru_emits_non_matching_lines() {
        let (events, _) = collect(
            &searcher(
                "needle",
                SearchOptions {
                    flags: SearchFlags::PASSTHRU,
                    ..SearchOptions::default()
                },
            ),
            stream(b"keep\nneedle\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        );
        assert!(
            events.iter().any(|event| matches!(
                event,
                SearchEvent::Context(ctx) if ctx.bytes == b"keep\n"
            )),
            "passthru should emit the non-matching line"
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SearchEvent::Match(_))),
            "passthru should still emit the matching line"
        );
    }

    #[test]
    fn multiline_matches_across_line_terminator() {
        let report = searcher(
            r"foo\nbar",
            SearchOptions {
                flags: SearchFlags::MULTILINE,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"foo\nbar\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(report.found());
    }

    #[test]
    fn omitted_unreadable_path_is_absent() {
        let file = File::new(
            PathBuf::from("missing.txt"),
            PathBuf::from("/tmp/sift-missing-file-that-does-not-exist"),
        );
        let mut inputs = Inputs::empty();
        inputs.push(Input::from_file(file, &[]));
        let report = searcher("needle", SearchOptions::default())
            .execute(inputs, SearchMode::Print(Hit::Line))
            .expect("execute");
        assert!(!report.found());
        let stats = &report.stats;
        assert_eq!(stats.files_searched, 1);
        assert_eq!(stats.bytes_searched, 0);
        assert!(matches!(report.listed, Listing::Matches(ref files) if files.is_empty()));
    }

    #[test]
    fn sync_and_mmap_find_the_same_hit() {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), b"needle\n").expect("write");
        let file = File::new(PathBuf::from("t.txt"), tmp.path().to_path_buf());
        for io in [Io::Sync, Io::Mmap] {
            let mut inputs = Inputs::empty();
            inputs.push(Input::from_file(file.clone(), &[]));
            let report = searcher(
                "needle",
                SearchOptions {
                    io,
                    ..SearchOptions::default()
                },
            )
            .execute(inputs, SearchMode::Print(Hit::Line))
            .expect("execute");
            assert!(report.found(), "{io:?}");
        }
    }

    #[test]
    fn first_match_counts_bytes_from_files_before_settling() {
        let mut inputs = Inputs::empty();
        inputs.push(Input::Bytes {
            origin: Origin::stream("miss"),
            bytes: Cow::Borrowed(b"aaaa\n"),
            mention: Mention::Explicit,
        });
        inputs.push(Input::Bytes {
            origin: Origin::stream("hit"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        });
        let report = searcher(
            "needle",
            SearchOptions {
                search_bound: SearchBound::FirstMatch,
                ..SearchOptions::default()
            },
        )
        .execute(inputs, SearchMode::Print(Hit::Line))
        .expect("execute");
        assert!(report.found());
        let stats = &report.stats;
        assert_eq!(stats.files_searched, 2);
        assert_eq!(stats.bytes_searched, 5 + 7);
    }

    #[test]
    fn stream_exhaustive_keeps_opening_after_a_hit() {
        let mut inputs = Inputs::empty();
        inputs.push(Input::Bytes {
            origin: Origin::stream("a"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        });
        inputs.push(Input::Bytes {
            origin: Origin::stream("b"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        });
        let (events, report) = collect(
            &searcher("needle", SearchOptions::default()),
            inputs,
            SearchMode::Print(Hit::Line),
        );
        let matches = events
            .iter()
            .filter(|event| matches!(event, SearchEvent::Match(_)))
            .count();
        assert_eq!(matches, 2);
        assert_eq!(report.stats.files_searched, 2);
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn stream_first_match_stops_after_settling() {
        let mut inputs = Inputs::empty();
        inputs.push(Input::Bytes {
            origin: Origin::stream("hit"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        });
        inputs.push(Input::Bytes {
            origin: Origin::stream("later"),
            bytes: Cow::Borrowed(b"needle\n"),
            mention: Mention::Explicit,
        });
        let (events, report) = collect(
            &searcher(
                "needle",
                SearchOptions {
                    search_bound: SearchBound::FirstMatch,
                    ..SearchOptions::default()
                },
            ),
            inputs,
            SearchMode::Print(Hit::Line),
        );
        let matches = events
            .iter()
            .filter(|event| matches!(event, SearchEvent::Match(_)))
            .count();
        assert_eq!(matches, 1);
        assert_eq!(report.stats.files_searched, 1);
    }

    #[test]
    fn convert_without_pattern_hit_emits_no_binary_event() {
        let (events, report) = collect(
            &searcher(
                "zzz",
                SearchOptions {
                    binary_mode: BinaryMode::Binary,
                    ..SearchOptions::default()
                },
            ),
            stream(b"abc\0def\n", Mention::Discovered),
            SearchMode::Print(Hit::Line),
        );
        assert!(!report.found());
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SearchEvent::Binary(_))),
            "Binary is only announced when the file matched"
        );
    }

    #[test]
    fn multiline_count_matches_counts_spans_not_overlapping_lines() {
        let options = SearchOptions {
            flags: SearchFlags::MULTILINE,
            ..SearchOptions::default()
        };
        let lines = searcher(r"foo\nbar", options.clone())
            .execute(
                stream(b"foo\nbar\n", Mention::Explicit),
                SearchMode::Count {
                    hit: Hit::Line,
                    zeros: ZeroCounts::Omit,
                },
            )
            .expect("execute");
        let Listing::Counts(counts) = &lines.listed else {
            panic!("expected Counts");
        };
        assert_eq!(counts[0].hits, 2);

        let spans = searcher(r"foo\nbar", options)
            .execute(
                stream(b"foo\nbar\n", Mention::Explicit),
                SearchMode::Count {
                    hit: Hit::Span,
                    zeros: ZeroCounts::Omit,
                },
            )
            .expect("execute");
        let Listing::Counts(counts) = &spans.listed else {
            panic!("expected Counts");
        };
        assert_eq!(counts[0].hits, 1);
    }

    #[test]
    fn multiline_only_matching_emits_full_span() {
        let report = searcher(
            r"foo\nbar",
            SearchOptions {
                flags: SearchFlags::MULTILINE,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"foo\nbar\n", Mention::Explicit),
            SearchMode::Print(Hit::Span),
        )
        .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files[0].matches.len(), 1);
        assert_eq!(files[0].matches[0].text, "foo\nbar");
    }

    #[test]
    fn multiline_replace_collapses_span_to_replacement() {
        let report = searcher(
            r"foo\nbar",
            SearchOptions {
                flags: SearchFlags::MULTILINE,
                replace: Some("X".into()),
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"foo\nbar\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files[0].matches.len(), 1);
        assert_eq!(files[0].matches[0].text.trim(), "X");
    }

    #[test]
    fn multiline_replace_interpolates_from_haystack() {
        let searcher = searcher(
            r"\Bfoo",
            SearchOptions {
                flags: SearchFlags::MULTILINE,
                replace: Some("Y".into()),
                ..SearchOptions::default()
            },
        );
        let report = searcher
            .execute(
                stream(b"xfoo\n", Mention::Explicit),
                SearchMode::Print(Hit::Line),
            )
            .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files[0].matches[0].text.trim(), "Y");

        let (events, _) = collect(
            &searcher,
            stream(b"xfoo\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        );
        let Some(SearchEvent::Match(event)) = events
            .iter()
            .find(|event| matches!(event, SearchEvent::Match(_)))
        else {
            panic!("expected Match");
        };
        assert_eq!(
            event
                .spans
                .first()
                .and_then(|span| span.replacement.as_deref()),
            Some(b"Y".as_slice())
        );
    }

    #[test]
    fn multiline_max_results_caps_matching_lines() {
        let report = searcher(
            r"foo\nbar",
            SearchOptions {
                flags: SearchFlags::MULTILINE,
                max_results: Some(1),
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"foo\nbar\nfoo\nbar\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert_eq!(files[0].matches.len(), 1);
    }

    #[test]
    fn pcre2_word_matches_non_word_edges() {
        let report = searcher(
            "-2",
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                flags: SearchFlags::WORD_REGEXP,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"abc -2 foo\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(report.found());
    }

    #[test]
    fn pcre2_crlf_lets_end_anchor_match() {
        let crlf = searcher(
            "abc$",
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                flags: SearchFlags::CRLF,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"abc\r\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(crlf.found());

        let newline = searcher(
            "abc$",
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"abc\r\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        assert!(!newline.found());
    }

    #[test]
    fn pcre2_replace_interpolates_capture_groups() {
        let report = searcher(
            "(foo)(\\d+)",
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                replace: Some("${1}_${2}".into()),
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"foo123bar\n", Mention::Explicit),
            SearchMode::Print(Hit::Line),
        )
        .expect("execute");
        let Listing::Matches(files) = &report.listed else {
            panic!("expected Matches");
        };
        assert!(
            files[0].matches[0].text.contains("foo_123"),
            "got {}",
            files[0].matches[0].text
        );
    }
}
