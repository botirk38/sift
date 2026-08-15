use std::time::Duration;

use crate::search::hit::Listing;
use crate::search::mode::SearchMode;
use crate::search::stats::{MatchTotals, Stats, StatsMode};
use crate::search::task::FileSearch;

/// Result of a search run.
pub struct Report {
    pub listed: Listing,
    pub stats: Option<Stats>,
}

#[derive(Clone, Copy)]
pub(crate) struct SearchSummary {
    pub mode: SearchMode,
    pub stats: StatsMode,
    pub inputs_len: usize,
    pub bytes_searched: u64,
    pub elapsed: Duration,
}

impl Report {
    pub(crate) fn empty(stats: StatsMode, mode: SearchMode) -> Self {
        Self {
            listed: Listing::empty(mode),
            stats: stats.collect().then(Stats::default),
        }
    }

    pub(crate) fn from_searches(searches: Vec<FileSearch>, summary: SearchSummary) -> Self {
        let mut listed = Listing::empty(summary.mode);
        let mut files_with_matches = 0usize;
        let mut match_lines = 0usize;
        let mut match_spans = 0usize;

        for search in searches {
            if search.matched {
                files_with_matches += 1;
            }
            match_lines = match_lines.saturating_add(search.line_matches);
            match_spans = match_spans.saturating_add(search.match_spans);
            if let Some(row) = search.row {
                listed.push_row(row);
            }
        }

        let stats = summary.stats.collect().then_some(Stats {
            matches: match summary.mode {
                // `-l` / `--files-without-match` stop after the first hit per file
                // (`max_matches = 1`), so the tally is the number of files that
                // contained a hit — not a full span scan. `--files` does not search.
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
            files_with_matches,
            files_searched: summary.inputs_len,
            bytes_printed: 0,
            bytes_searched: summary.bytes_searched,
            elapsed: summary.elapsed,
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
