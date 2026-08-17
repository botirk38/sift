use crate::corpus::FileOrder;

/// Whether candidate resolution should cover every corpus file or only
/// index-narrowed potential matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
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
    Walk { order: FileOrder },
    /// Index-backed discovery. Callers request [`Walk`](Self::Walk) when no
    /// queryable index is available; Plan still falls back to walk when Index
    /// cannot be used.
    Index {
        order: FileOrder,
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

impl ScanScope {
    pub(crate) fn order(self) -> FileOrder {
        match self {
            Self::StreamsOnly => FileOrder::default(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{Hit, SearchMode, ZeroCounts};

    #[test]
    fn count_lines_omit_uses_potential_matches() {
        assert_eq!(
            SearchMode::Count {
                hit: Hit::Line,
                zeros: ZeroCounts::Omit
            }
            .coverage(),
            Coverage::PotentialMatches
        );
    }

    #[test]
    fn count_lines_include_uses_complete() {
        assert_eq!(
            SearchMode::Count {
                hit: Hit::Line,
                zeros: ZeroCounts::Include
            }
            .coverage(),
            Coverage::Complete
        );
    }

    #[test]
    fn count_matches_omit_uses_potential_matches() {
        assert_eq!(
            SearchMode::Count {
                hit: Hit::Span,
                zeros: ZeroCounts::Omit
            }
            .coverage(),
            Coverage::PotentialMatches
        );
    }

    #[test]
    fn files_without_match_uses_complete() {
        assert_eq!(SearchMode::FilesWithoutMatch.coverage(), Coverage::Complete);
    }
}
