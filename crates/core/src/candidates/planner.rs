use crate::corpus::filter::FilterAdmission;
use crate::index::FileId;
use crate::index::IndexCoverage;
use crate::search::SearchQuery;

use crate::candidates::scope::{CandidateCoverage, ScanScope, SnapshotFreshness};
use crate::candidates::source::CandidateSource;

use super::plan::{CandidatePlan, PlannedDiscovery};

pub(crate) struct CandidatePlanner;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexStatus {
    Empty,
    Queryable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotStatus {
    Missing,
    FilterMismatch,
    TrustedComplete,
    TrustedLazy,
    StaleComplete,
}

impl CandidatePlanner {
    /// Pure decision: no filesystem or index reads beyond cheap metadata.
    pub(crate) fn plan(
        source: &CandidateSource<'_>,
        query: &SearchQuery,
        coverage: CandidateCoverage,
    ) -> CandidatePlan {
        let scope = source.scope;
        let narrowing = query.narrowing_allowed();
        let snapshot_status = Self::snapshot_status(source, scope);
        let index_status = Self::index_status(source);
        let freshness = scope.freshness().unwrap_or(SnapshotFreshness::Current);
        let discovery = Self::discovery(
            scope,
            coverage,
            narrowing,
            index_status,
            snapshot_status,
            freshness,
        );
        let file_ids = Self::file_ids_for(source, query, discovery, coverage);
        CandidatePlan {
            discovery,
            order: scope.order(),
            file_ids,
            indexed_corpus: source.indexes.indexed_corpus(),
        }
    }

    fn file_ids_for(
        source: &CandidateSource<'_>,
        query: &SearchQuery,
        discovery: PlannedDiscovery,
        coverage: CandidateCoverage,
    ) -> Vec<FileId> {
        match discovery {
            PlannedDiscovery::Empty | PlannedDiscovery::Walk => Vec::new(),
            PlannedDiscovery::Index { .. } | PlannedDiscovery::Merge { .. } => {
                if matches!(coverage, CandidateCoverage::Complete) {
                    source
                        .indexes
                        .all_indexed_file_ids(&source.indexes.indexed_corpus())
                } else {
                    source.indexes.query(query)
                }
            }
        }
    }

    fn snapshot_status(source: &CandidateSource<'_>, scope: ScanScope) -> SnapshotStatus {
        if !matches!(scope, ScanScope::Index { .. }) {
            return SnapshotStatus::Missing;
        }
        let freshness = scope.freshness().unwrap_or(SnapshotFreshness::Current);
        let Some(meta) = source.store_meta else {
            return SnapshotStatus::Missing;
        };
        if !meta.covers_candidate_filter(source.filter) {
            return SnapshotStatus::FilterMismatch;
        }
        match meta.coverage {
            IndexCoverage::Complete if freshness == SnapshotFreshness::Stale => {
                SnapshotStatus::StaleComplete
            }
            IndexCoverage::Complete => SnapshotStatus::TrustedComplete,
            IndexCoverage::Lazy => SnapshotStatus::TrustedLazy,
        }
    }

    fn index_status(source: &CandidateSource<'_>) -> IndexStatus {
        if source.indexes.usable() {
            IndexStatus::Queryable
        } else {
            IndexStatus::Empty
        }
    }

    const fn discovery(
        scope: ScanScope,
        coverage: CandidateCoverage,
        narrowing: bool,
        index_status: IndexStatus,
        snapshot_status: SnapshotStatus,
        freshness: SnapshotFreshness,
    ) -> PlannedDiscovery {
        match scope {
            ScanScope::StreamsOnly => PlannedDiscovery::Empty,
            ScanScope::Walk { .. } => PlannedDiscovery::Walk,
            ScanScope::Index { .. } => match coverage {
                CandidateCoverage::Complete => {
                    Self::complete_discovery(index_status, snapshot_status)
                }
                CandidateCoverage::PotentialMatches => {
                    Self::potential_discovery(index_status, narrowing, snapshot_status, freshness)
                }
            },
        }
    }

    const fn complete_discovery(
        index_status: IndexStatus,
        snapshot_status: SnapshotStatus,
    ) -> PlannedDiscovery {
        match (index_status, snapshot_status) {
            (IndexStatus::Empty, _)
            | (
                _,
                SnapshotStatus::FilterMismatch
                | SnapshotStatus::TrustedLazy
                | SnapshotStatus::StaleComplete,
            ) => PlannedDiscovery::Walk,
            (_, _) => PlannedDiscovery::Index {
                admission: Self::admission(snapshot_status),
            },
        }
    }

    const fn potential_discovery(
        index_status: IndexStatus,
        narrowing: bool,
        snapshot_status: SnapshotStatus,
        freshness: SnapshotFreshness,
    ) -> PlannedDiscovery {
        match (index_status, narrowing, freshness) {
            (IndexStatus::Empty, _, _) | (_, false, _) => PlannedDiscovery::Walk,
            (IndexStatus::Queryable, true, _) => match snapshot_status {
                SnapshotStatus::TrustedLazy => PlannedDiscovery::Merge {
                    admission: Self::admission(snapshot_status),
                },
                SnapshotStatus::FilterMismatch => PlannedDiscovery::Walk,
                SnapshotStatus::StaleComplete => PlannedDiscovery::Index {
                    admission: Self::admission(snapshot_status),
                },
                SnapshotStatus::Missing | SnapshotStatus::TrustedComplete => {
                    PlannedDiscovery::Index {
                        admission: Self::admission(snapshot_status),
                    }
                }
            },
        }
    }

    const fn admission(snapshot_status: SnapshotStatus) -> FilterAdmission {
        match snapshot_status {
            SnapshotStatus::TrustedComplete
            | SnapshotStatus::TrustedLazy
            | SnapshotStatus::StaleComplete => FilterAdmission::Indexed,
            SnapshotStatus::Missing | SnapshotStatus::FilterMismatch => FilterAdmission::Full,
        }
    }
}
