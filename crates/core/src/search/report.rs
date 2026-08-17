use std::time::Duration;

use crate::search::hit::{Count, Listing, Match, MatchedFile};
use crate::search::input::{Input, Origin};
use crate::search::mode::SearchMode;
use crate::search::stats::Stats;

pub(super) struct FileReport {
    pub origin: Origin,
    pub matched: bool,
    pub hits: usize,
    pub matches: Vec<Match>,
    pub bytes_searched: u64,
    pub binary: Option<u64>,
}

impl FileReport {
    pub(super) const fn new(origin: Origin) -> Self {
        Self {
            origin,
            matched: false,
            hits: 0,
            matches: Vec::new(),
            bytes_searched: 0,
            binary: None,
        }
    }

    pub(super) const fn listed(origin: Origin) -> Self {
        Self {
            origin,
            matched: true,
            hits: 0,
            matches: Vec::new(),
            bytes_searched: 0,
            binary: None,
        }
    }

    pub(super) fn push_match(&mut self, m: Match) {
        self.matches.push(m);
    }
}

/// Result of searching a set of inputs.
pub(super) struct Reports {
    items: Vec<FileReport>,
    searched: usize,
    bytes: u64,
}

impl Reports {
    pub(super) fn listed(inputs: &[Input<'_>]) -> Self {
        Self::new(
            inputs
                .iter()
                .map(|input| FileReport::listed(input.origin().clone()))
                .collect(),
            inputs.len(),
            0,
        )
    }

    pub(super) fn exhaustive(items: Vec<FileReport>, searched: usize) -> Self {
        let bytes = items.iter().fold(0u64, |bytes, report| {
            bytes.saturating_add(report.bytes_searched)
        });
        Self::new(items, searched, bytes)
    }

    pub(super) const fn empty() -> Self {
        Self::new(Vec::new(), 0, 0)
    }

    /// Account for one attempted input. Returns whether `FirstMatch` should stop.
    pub(super) fn tried(&mut self, report: Option<FileReport>, mode: SearchMode) -> bool {
        self.searched += 1;
        let Some(report) = report else {
            return false;
        };
        self.bytes = self.bytes.saturating_add(report.bytes_searched);
        if !mode.settles(report.matched) {
            return false;
        }
        self.items.push(report);
        true
    }

    const fn new(items: Vec<FileReport>, searched: usize, bytes: u64) -> Self {
        Self {
            items,
            searched,
            bytes,
        }
    }
}

impl Listing {
    fn push(&mut self, report: FileReport) {
        match self {
            Self::MatchingPaths(v) | Self::NonMatchingPaths(v) => v.push(report.origin),
            Self::Counts(v) => v.push(Count {
                origin: report.origin,
                hits: report.hits,
            }),
            Self::Matches(v) => v.push(MatchedFile {
                origin: report.origin,
                matches: report.matches,
            }),
        }
    }
}

/// Result of a search run.
pub struct SearchReport {
    pub listed: Listing,
    pub stats: Stats,
}

impl SearchReport {
    pub(crate) fn empty(mode: SearchMode) -> Self {
        Self {
            listed: Listing::empty(mode),
            stats: Stats::default(),
        }
    }

    pub(super) fn from(reports: Reports, mode: SearchMode, elapsed: Duration) -> Self {
        let mut listed = Listing::empty(mode);
        let mut files_with_matches = 0usize;
        let mut hits = 0usize;

        for report in reports.items {
            if report.matched {
                files_with_matches += 1;
            }
            hits = hits.saturating_add(report.hits);
            if mode.admits(report.matched) {
                listed.push(report);
            }
        }

        let stats = Stats {
            matches: match mode {
                SearchMode::Print(_) | SearchMode::Count { .. } => Some(hits),
                SearchMode::FilesWithMatches | SearchMode::FilesWithoutMatch => {
                    Some(files_with_matches)
                }
                SearchMode::Paths => None,
            },
            files_with_matches: match mode {
                SearchMode::Paths => 0,
                _ => files_with_matches,
            },
            files_searched: reports.searched,
            bytes_printed: 0,
            bytes_searched: reports.bytes,
            elapsed,
        };

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
            Listing::Matches(v) => !v.is_empty(),
            Listing::Counts(v) => v.iter().any(|count| count.hits > 0),
        }
    }
}
