use crate::corpus::CandidateOrder;

/// Whether candidate resolution should cover every corpus file or only
/// index-narrowed potential matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateCoverage {
    /// Index may narrow to potential matches only.
    PotentialMatches,
    /// Every corpus file must be considered (`-L`, `--include-zero`).
    Complete,
}

/// Which corpus files this search may scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanScope {
    /// Streams/stdin only — no corpus file resolution.
    StreamsOnly,
    /// Filesystem walk under the filter.
    Walk { order: CandidateOrder },
    /// Index-backed discovery; planner chooses walk when indexes are unusable.
    Index {
        order: CandidateOrder,
        freshness: SnapshotFreshness,
    },
}

/// Whether the opened snapshot is safe to read for index-backed search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFreshness {
    /// On-disk snapshot is current (daemon confirmed, or no daemon to disagree).
    Current,
    /// Daemon reports a newer snapshot was committed; this id is behind.
    Stale,
}

impl CandidateCoverage {
    pub(crate) const fn from_mode(mode: crate::search::SearchMode) -> Self {
        use crate::search::{SearchMode, ZeroCounts};
        match mode {
            SearchMode::FilesWithoutMatch => Self::Complete,
            SearchMode::CountLines { zeros } | SearchMode::CountMatches { zeros } => match zeros {
                ZeroCounts::Include => Self::Complete,
                ZeroCounts::Omit => Self::PotentialMatches,
            },
            SearchMode::Lines | SearchMode::Matches | SearchMode::FilesWithMatches => {
                Self::PotentialMatches
            }
        }
    }
}

impl ScanScope {
    pub(crate) fn order(self) -> CandidateOrder {
        match self {
            Self::StreamsOnly => CandidateOrder::default(),
            Self::Walk { order } | Self::Index { order, .. } => order,
        }
    }

    pub(crate) const fn freshness(self) -> Option<SnapshotFreshness> {
        match self {
            Self::Index { freshness, .. } => Some(freshness),
            Self::StreamsOnly | Self::Walk { .. } => None,
        }
    }
}
