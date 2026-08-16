use std::collections::VecDeque;
use std::ops::Range;
use std::time::Instant;

use rayon::prelude::*;

use crate::Error;
use crate::search::error::Error as SearchError;
use crate::search::event::{ContextKind, Events};
use crate::search::haystack::Haystack;
use crate::search::input::{Input, Inputs};
use crate::search::line::{Line, Lines};
use crate::search::matcher::Matcher;
use crate::search::mode::SearchMode;
use crate::search::options::{SearchBound, SearchOptions};
use crate::search::query::Query;
use crate::search::report::{Buffer, FileReport, SearchReport};
use crate::search::stats::StatsMode;

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

    /// Search inputs and optionally emit semantic events.
    ///
    /// # Errors
    ///
    /// Returns an error if search execution or sink handling fails.
    pub fn execute(
        &self,
        inputs: Inputs<'_>,
        stats: StatsMode,
        mode: SearchMode,
        events: Events<'_>,
    ) -> crate::Result<SearchReport> {
        if self.options().max_results == Some(0) {
            return Err(Error::Search(SearchError::InvalidMaxCount));
        }
        if inputs.is_empty() {
            return Ok(SearchReport::empty(stats, mode));
        }

        let started = Instant::now();
        let buffer = Buffer::from(&events);
        let items = inputs.into_vec();
        let (mut reports, files_searched) = match mode {
            SearchMode::Paths => (
                items
                    .iter()
                    .map(|input| FileReport::listed(input.origin().clone()))
                    .collect(),
                items.len(),
            ),
            _ => match self.options().search_bound {
                SearchBound::Exhaustive => (
                    items
                        .par_iter()
                        .filter_map(|input| self.search(input, mode, buffer).transpose())
                        .collect::<crate::Result<Vec<_>>>()?,
                    items.len(),
                ),
                SearchBound::FirstMatch => {
                    let mut reports = Vec::new();
                    let mut searched = 0usize;
                    for input in &items {
                        searched += 1;
                        let Some(report) = self.search(input, mode, buffer)? else {
                            continue;
                        };
                        if mode.settles(report.matched) {
                            reports.push(report);
                            break;
                        }
                    }
                    (reports, searched)
                }
            },
        };
        Self::emit(events, &mut reports)?;
        Ok(SearchReport::from(
            reports,
            mode,
            stats,
            files_searched,
            started.elapsed(),
        ))
    }

    fn emit(events: Events<'_>, reports: &mut [FileReport]) -> crate::Result<()> {
        let Events::Emit(sink) = events else {
            return Ok(());
        };
        for event in reports.iter_mut().flat_map(FileReport::drain_events) {
            sink.event(event)?;
        }
        Ok(())
    }

    fn search(
        &self,
        input: &Input<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<Option<FileReport>> {
        let Ok(mut haystack) = Haystack::open(input, self.options().io) else {
            return Ok(None);
        };
        let origin = input.origin();
        let explicit = input.explicit();
        let options = self.options();
        let mut report = FileReport::new();
        report.begin(origin, buffer);
        haystack.decode(&options.input_encoding);
        if options.binary_mode.converts(explicit, options.null_data())
            && let Some(offset) = haystack.convert_nul(options.line_terminator())
        {
            report.binary(origin, offset, explicit, buffer);
        }
        let chunk = haystack.bytes();
        let searched = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        let ranges = options
            .multiline()
            .then(|| self.match_ranges(chunk))
            .transpose()?;
        let passthru = options.passthru();
        let before_n = if passthru { 0 } else { options.before_context };
        let after_n = if passthru { 0 } else { options.after_context };
        let context_on = before_n > 0 || after_n > 0;
        let limit = match mode {
            SearchMode::FilesWithMatches | SearchMode::FilesWithoutMatch | SearchMode::Paths => {
                Some(1usize)
            }
            _ => options.max_results,
        };
        let mut before: VecDeque<Line<'static>> = VecDeque::new();
        let mut after_left = 0usize;
        let mut has_sunk = false;
        let mut last_end = 0u64;
        let mut hits = 0usize;
        for line in Lines::new(chunk, options.line_terminator(), !mode.is_path_mode()) {
            if options.binary_mode.quits(explicit, options.null_data())
                && let Some(idx) = memchr::memchr(0, line.bytes())
            {
                let offset = line.offset + u64::try_from(idx).unwrap_or(u64::MAX);
                report.binary(origin, offset, explicit, buffer);
                break;
            }
            let matched = match ranges.as_deref() {
                None => self
                    .matcher
                    .is_match(line.without_terminator(options.line_terminator(), options.crlf()))?,
                Some(ranges) => {
                    let start = usize::try_from(line.offset).unwrap_or(usize::MAX);
                    let line_end = start.saturating_add(line.bytes().len());
                    ranges
                        .iter()
                        .any(|range| start < range.end && line_end > range.start)
                }
            };
            let hit = matched != options.invert_match();
            let accepting = limit.is_none_or(|n| hits < n);
            if hit && accepting {
                if context_on && has_sunk && last_end < line.offset {
                    report.gap(buffer);
                }
                while let Some(held) = before.pop_front() {
                    report.context(origin, ContextKind::Before, &held, buffer);
                }
                report.hit(origin, &line, &self.matcher, mode, buffer, options);
                hits += 1;
                after_left = after_n;
                has_sunk = true;
                last_end = line.offset + u64::try_from(line.bytes().len()).unwrap_or(u64::MAX);
                if limit.is_some_and(|n| hits >= n) && after_left == 0 {
                    break;
                }
                continue;
            }
            if passthru {
                report.context(origin, ContextKind::Other, &line, buffer);
                has_sunk = true;
                last_end = line.offset + u64::try_from(line.bytes().len()).unwrap_or(u64::MAX);
                continue;
            }
            if after_left > 0 {
                report.context(origin, ContextKind::After, &line, buffer);
                after_left -= 1;
                has_sunk = true;
                last_end = line.offset + u64::try_from(line.bytes().len()).unwrap_or(u64::MAX);
                if limit.is_some_and(|n| hits >= n) && after_left == 0 {
                    break;
                }
                continue;
            }
            if before_n > 0 {
                if before.len() == before_n {
                    before.pop_front();
                }
                before.push_back(line.into_owned());
            }
        }
        report.finish(origin.clone(), mode, buffer, searched);
        Ok(Some(report))
    }

    fn match_ranges(&self, bytes: &[u8]) -> Result<Vec<Range<usize>>, SearchError> {
        let term = self.options().line_terminator();
        let mut ranges = Vec::new();
        let mut pos = 0usize;
        while pos < bytes.len() {
            let Some(found) = self.matcher.find(&bytes[pos..])? else {
                break;
            };
            let start = pos + found.start();
            let end = pos + found.end();
            ranges.push(Lines::covering(bytes, term, start..end));
            pos = if found.end() == 0 { pos + 1 } else { end };
        }
        Lines::merge(&mut ranges);
        Ok(ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::File;
    use crate::search::event::{SearchEvent, SearchSink};
    use crate::search::hit::Listing;
    use crate::search::input::Origin;
    use crate::search::options::{BinaryMode, Io, RegexEngine, SearchFlags};
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

    struct Collect(Vec<SearchEvent>);

    impl SearchSink for Collect {
        fn event(&mut self, event: SearchEvent) -> crate::Result<()> {
            self.0.push(event);
            Ok(())
        }
    }

    fn stream(bytes: &[u8], explicit: bool) -> Inputs<'_> {
        let mut inputs = Inputs::empty();
        inputs.push(Input::Bytes {
            origin: Origin::stream("t"),
            bytes: Cow::Borrowed(bytes),
            explicit,
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
            stream(b"findme\0later\n", false),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Discard,
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
            stream(b"findme\0later\n", false),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Discard,
        )
        .expect("execute");
        assert!(report.found());
        let Listing::Lines(files) = &report.listed else {
            panic!("expected Lines");
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
            stream(b"findme\0later\n", false),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Discard,
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
            stream(b"needle\nneedle\nneedle\n", true),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Discard,
        )
        .expect("execute");
        let Listing::Lines(files) = &report.listed else {
            panic!("expected Lines");
        };
        assert_eq!(files[0].matches.len(), 2);
    }

    #[test]
    fn passthru_emits_non_matching_lines() {
        let mut sink = Collect(Vec::new());
        searcher(
            "needle",
            SearchOptions {
                flags: SearchFlags::PASSTHRU,
                ..SearchOptions::default()
            },
        )
        .execute(
            stream(b"keep\nneedle\n", true),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Emit(&mut sink),
        )
        .expect("execute");
        assert!(
            sink.0.iter().any(|event| matches!(
                event,
                SearchEvent::Context(ctx) if ctx.bytes == b"keep\n"
            )),
            "passthru should emit the non-matching line"
        );
        assert!(
            sink.0
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
            stream(b"foo\nbar\n", true),
            StatsMode::Off,
            SearchMode::Lines,
            Events::Discard,
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
            .execute(inputs, StatsMode::On, SearchMode::Lines, Events::Discard)
            .expect("execute");
        assert!(!report.found());
        let stats = report.stats.as_ref().expect("stats");
        assert_eq!(stats.files_searched, 1);
        assert_eq!(stats.bytes_searched, 0);
        assert!(matches!(report.listed, Listing::Lines(ref files) if files.is_empty()));
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
            .execute(inputs, StatsMode::Off, SearchMode::Lines, Events::Discard)
            .expect("execute");
            assert!(report.found(), "{io:?}");
        }
    }
}
