use std::time::Instant;

use rayon::prelude::*;

use crate::Error;
use crate::GrepError;
use crate::candidates::{Candidates, CandidatesInner};
use crate::corpus::Candidate;
use crate::corpus::filter::{CandidateFilter, FilterAdmission};
use crate::index::{FileId, Indexes};
use crate::search::event::Events;
use crate::search::input::{Input, InputConversion, Inputs, SearchInputs};
use crate::search::matcher::{Matcher, MatcherBuilder};
use crate::search::mode::SearchMode;
use crate::search::options::{SearchBound, SearchOptions};
use crate::search::query::SearchQuery;
use crate::search::report::{Report, SearchSummary};
use crate::search::stats::StatsMode;
use crate::search::task::{Buffer, FileSearch, SearchTask};

#[derive(Debug, Clone)]
pub struct Searcher {
    pub(crate) query: SearchQuery,
    matcher: Matcher,
}

impl Searcher {
    /// Build a ready searcher by creating the query matcher.
    ///
    /// # Errors
    ///
    /// Returns an error if matcher construction fails.
    pub fn new(query: SearchQuery) -> Result<Self, GrepError> {
        let matcher = MatcherBuilder::new(&query).build()?;
        let query = query.with_engine(matcher.resolved_engine());
        Ok(Self { query, matcher })
    }
}

impl Searcher {
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
        inputs: SearchInputs<'_>,
        stats: StatsMode,
        mode: SearchMode,
        events: Events<'_>,
    ) -> crate::Result<Report> {
        if self.options().max_results == Some(0) {
            return Err(Error::Search(GrepError::InvalidMaxCount));
        }
        if inputs.is_empty() {
            return Ok(Report::empty(stats, mode));
        }

        let search_start = Instant::now();
        let buffer = Buffer::from(&events);
        let options = self.options();
        let (mut searches, inputs_searched, bytes_searched) = match options.search_bound {
            SearchBound::Exhaustive => self.search_exhaustive(inputs, mode, buffer)?,
            SearchBound::FirstMatch => self.search_first_match(inputs, mode, buffer)?,
        };
        let summary = SearchSummary {
            mode,
            stats,
            inputs_len: inputs_searched,
            bytes_searched,
            elapsed: search_start.elapsed(),
        };
        emit_events(events, &mut searches)?;
        Ok(Report::from_searches(searches, summary))
    }

    fn search_exhaustive(
        &self,
        inputs: SearchInputs<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<(Vec<FileSearch>, usize, u64)> {
        let SearchInputs {
            candidates,
            streams,
            conversion,
        } = inputs;

        let (mut results, mut files_searched, mut bytes) =
            self.search_candidates(candidates, &conversion, mode, buffer)?;

        let stream_results = self.search_inputs(streams.as_slice(), mode, buffer);
        files_searched += streams.len();
        bytes = bytes.saturating_add(streams.byte_count());
        results.extend(stream_results);

        Ok((results, files_searched, bytes))
    }

    fn search_candidates(
        &self,
        candidates: Candidates<'_>,
        conversion: &InputConversion<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<(Vec<FileSearch>, usize, u64)> {
        match candidates.0 {
            CandidatesInner::Resolved(items) => {
                self.search_resolved(&items, conversion, mode, buffer)
            }
            CandidatesInner::Indexed {
                indexes,
                file_ids,
                filter,
                admission,
            } => self.search_indexed(
                IndexedFiles {
                    indexes,
                    file_ids: &file_ids,
                    filter,
                    admission,
                },
                conversion,
                mode,
                buffer,
            ),
            CandidatesInner::Mixed {
                indexes,
                file_ids,
                filter,
                admission,
                unindexed,
            } => {
                let (mut indexed_results, indexed_count, indexed_bytes) = self.search_indexed(
                    IndexedFiles {
                        indexes,
                        file_ids: &file_ids,
                        filter,
                        admission,
                    },
                    conversion,
                    mode,
                    buffer,
                )?;
                let (resolved_results, resolved_count, resolved_bytes) =
                    self.search_resolved(&unindexed, conversion, mode, buffer)?;
                indexed_results.extend(resolved_results);
                Ok((
                    indexed_results,
                    indexed_count.saturating_add(resolved_count),
                    indexed_bytes.saturating_add(resolved_bytes),
                ))
            }
        }
    }

    fn search_resolved(
        &self,
        candidates: &[Candidate],
        conversion: &InputConversion<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<(Vec<FileSearch>, usize, u64)> {
        let mut corpus_inputs = Inputs::with_capacity(candidates.len());
        for candidate in candidates {
            match conversion.open(candidate)? {
                Input::Path {
                    path,
                    identity,
                    explicit,
                } => corpus_inputs.push_path(path, identity, explicit),
                Input::Bytes {
                    path,
                    bytes,
                    identity,
                    explicit,
                } => {
                    if explicit {
                        corpus_inputs.push_explicit_bytes(path, bytes, identity);
                    } else {
                        corpus_inputs.push_bytes(path, bytes, identity);
                    }
                }
            }
        }
        let results = self.search_inputs(corpus_inputs.as_slice(), mode, buffer);
        let len = corpus_inputs.len();
        let bytes = corpus_inputs.byte_count();
        Ok((results, len, bytes))
    }

    fn search_indexed(
        &self,
        files: IndexedFiles<'_>,
        conversion: &InputConversion<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<(Vec<FileSearch>, usize, u64)> {
        let options = self.options();
        let outcomes: crate::Result<Vec<Option<FileSearch>>> = files
            .file_ids
            .par_iter()
            .map_init(
                || SearchTask::discovered_searcher(options, mode),
                |grep, &id| {
                    let Some(candidate) =
                        files.indexes.candidate(id, files.filter, files.admission)
                    else {
                        return Ok(None);
                    };
                    let input = conversion.open(&candidate)?;
                    Ok(Some(
                        SearchTask::new(&self.matcher, options, mode, buffer, &input).execute(grep),
                    ))
                },
            )
            .collect();
        let results: Vec<FileSearch> = outcomes?.into_iter().flatten().collect();
        let files_searched = results.len();
        let bytes = results.iter().fold(0u64, |acc, search| {
            acc.saturating_add(search.bytes_searched)
        });
        Ok((results, files_searched, bytes))
    }

    fn search_inputs(
        &self,
        inputs: &[Input<'_>],
        mode: SearchMode,
        buffer: Buffer,
    ) -> Vec<FileSearch> {
        let options = self.options();
        inputs
            .par_iter()
            .map_init(
                || SearchTask::discovered_searcher(options, mode),
                |grep, input| {
                    SearchTask::new(&self.matcher, options, mode, buffer, input).execute(grep)
                },
            )
            .collect()
    }

    fn search_first_match(
        &self,
        inputs: SearchInputs<'_>,
        mode: SearchMode,
        buffer: Buffer,
    ) -> crate::Result<(Vec<FileSearch>, usize, u64)> {
        let options = self.options();
        let SearchInputs {
            candidates,
            streams,
            conversion,
        } = inputs;
        let mut settled = Vec::new();
        let mut files_searched = 0usize;
        let mut bytes = 0u64;
        let mut grep = SearchTask::discovered_searcher(options, mode);

        for candidate in candidates {
            files_searched += 1;
            let input = conversion.open(&candidate)?;
            let search =
                SearchTask::new(&self.matcher, options, mode, buffer, &input).execute(&mut grep);
            bytes = bytes.saturating_add(search.bytes_searched);
            if mode.settles(search.matched) {
                settled.push(search);
                return Ok((settled, files_searched, bytes));
            }
        }
        for input in streams.as_slice() {
            files_searched += 1;
            let search =
                SearchTask::new(&self.matcher, options, mode, buffer, input).execute(&mut grep);
            bytes = bytes.saturating_add(search.bytes_searched);
            if mode.settles(search.matched) {
                settled.push(search);
                break;
            }
        }

        Ok((settled, files_searched, bytes))
    }
}

#[derive(Clone, Copy)]
struct IndexedFiles<'a> {
    indexes: &'a Indexes,
    file_ids: &'a [FileId],
    filter: &'a CandidateFilter,
    admission: FilterAdmission,
}

fn emit_events(events: Events<'_>, searches: &mut [FileSearch]) -> crate::Result<()> {
    let Events::Emit(sink) = events else {
        return Ok(());
    };
    for event in searches.iter_mut().flat_map(FileSearch::drain_events) {
        sink.event(event)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::options::{RegexEngine, SearchOptions};
    use crate::search::query::SearchQuery;

    #[test]
    fn auto_engine_fallback_to_pcre2_disables_narrowing() {
        // Lookaround is unsupported by the Rust regex engine.
        let query = SearchQuery::new(
            vec![r"(?<=foo)bar".into()],
            SearchOptions {
                regex_engine: RegexEngine::Auto,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.narrowing(), crate::search::Narrowing::Allowed);
        let searcher = Searcher::new(query).expect("compile via Auto→PCRE2");
        assert_eq!(searcher.query.options().regex_engine, RegexEngine::Pcre2);
        assert_eq!(
            searcher.query.narrowing(),
            crate::search::Narrowing::Disabled
        );
    }
}
