use crate::corpus::Candidate;
use crate::corpus::filter::FilterAdmission;
use crate::corpus::order::{CandidateOrder, CandidateOrderKey};
use crate::corpus::walk::FileWalk;
use crate::index::{FileId, IndexCoverage, IndexedCorpus, Indexes};
use crate::search::{Narrowing, SearchQuery};

use crate::candidates::scope::{Coverage, ScanScope, SnapshotFreshness};
use crate::candidates::source::CandidateSource;

use super::output::{Candidates, Inner as CandidatesInner};

/// Pure discovery decision for candidate resolution.
#[must_use]
pub(crate) struct Plan {
    pub discovery: Discovery,
    pub order: CandidateOrder,
    pub coverage: Coverage,
    pub query: SearchQuery,
}

/// How candidate discovery will run at resolve time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Discovery {
    Empty,
    Walk,
    Index {
        admission: FilterAdmission,
    },
    /// Index hits merged with a walk of unindexed paths (lazy snapshots).
    Merge {
        admission: FilterAdmission,
    },
}

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

impl Plan {
    /// Pure decision over discovery shape — no index query I/O.
    pub(crate) fn new(
        source: &CandidateSource<'_>,
        query: &SearchQuery,
        coverage: Coverage,
    ) -> Self {
        let scope = source.scope;
        let narrowing = query.narrowing();
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
        Self {
            discovery,
            order: scope.order(),
            coverage,
            query: query.clone(),
        }
    }

    /// Run the plan: index query, filesystem walk, hydrate, and order.
    ///
    /// # Errors
    ///
    /// Returns an error if filesystem walking or ordering fails.
    pub(crate) fn resolve<'a>(
        self,
        source: &'a CandidateSource<'a>,
    ) -> crate::Result<Candidates<'a>> {
        let Self {
            discovery,
            order,
            coverage,
            query,
        } = self;
        let candidates = match discovery {
            Discovery::Empty => Candidates::empty(),
            Discovery::Walk => Candidates::from(Self::walk(source)?),
            Discovery::Index { admission } => {
                source.indexes.map_or_else(Candidates::empty, |indexes| {
                    let file_ids = Self::file_ids(indexes, &query, coverage);
                    Candidates::indexed(indexes, file_ids, source.filter, admission)
                })
            }
            Discovery::Merge { admission } => source.indexes.map_or_else(
                || Ok(Candidates::empty()),
                |indexes| {
                    let file_ids = Self::file_ids(indexes, &query, coverage);
                    let indexed_corpus = indexes.indexed_corpus();
                    Self::merge(source, indexes, file_ids, admission, &indexed_corpus)
                },
            )?,
        };
        Self::order(candidates, order)
    }

    fn file_ids(indexes: &Indexes, query: &SearchQuery, coverage: Coverage) -> Vec<FileId> {
        match coverage {
            Coverage::Complete => indexes.all_indexed_file_ids(&indexes.indexed_corpus()),
            Coverage::PotentialMatches => indexes.query(query),
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
        match source.indexes {
            Some(indexes) if indexes.queryable() => IndexStatus::Queryable,
            Some(_) | None => IndexStatus::Empty,
        }
    }

    const fn discovery(
        scope: ScanScope,
        coverage: Coverage,
        narrowing: Narrowing,
        index_status: IndexStatus,
        snapshot_status: SnapshotStatus,
        freshness: SnapshotFreshness,
    ) -> Discovery {
        match scope {
            ScanScope::StreamsOnly => Discovery::Empty,
            ScanScope::Walk { .. } => Discovery::Walk,
            ScanScope::Index { .. } => match coverage {
                Coverage::Complete => Self::complete_discovery(index_status, snapshot_status),
                Coverage::PotentialMatches => {
                    Self::potential_discovery(index_status, narrowing, snapshot_status, freshness)
                }
            },
        }
    }

    const fn complete_discovery(
        index_status: IndexStatus,
        snapshot_status: SnapshotStatus,
    ) -> Discovery {
        match (index_status, snapshot_status) {
            (IndexStatus::Empty, _)
            | (
                _,
                SnapshotStatus::FilterMismatch
                | SnapshotStatus::TrustedLazy
                | SnapshotStatus::StaleComplete,
            ) => Discovery::Walk,
            (_, _) => Discovery::Index {
                admission: Self::admission(snapshot_status),
            },
        }
    }

    const fn potential_discovery(
        index_status: IndexStatus,
        narrowing: Narrowing,
        snapshot_status: SnapshotStatus,
        freshness: SnapshotFreshness,
    ) -> Discovery {
        match (index_status, narrowing, freshness) {
            (IndexStatus::Empty, _, _) | (_, Narrowing::Disabled, _) => Discovery::Walk,
            (IndexStatus::Queryable, Narrowing::Allowed, _) => match snapshot_status {
                SnapshotStatus::TrustedLazy => Discovery::Merge {
                    admission: Self::admission(snapshot_status),
                },
                SnapshotStatus::FilterMismatch => Discovery::Walk,
                SnapshotStatus::Missing
                | SnapshotStatus::TrustedComplete
                | SnapshotStatus::StaleComplete => Discovery::Index {
                    admission: Self::admission(snapshot_status),
                },
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

    fn walk(source: &CandidateSource<'_>) -> crate::Result<Vec<Candidate>> {
        let walked = FileWalk::from_filter(source.filter).candidates()?;
        Ok(source.filter.retain(walked, FilterAdmission::Full))
    }

    fn merge<'a>(
        source: &'a CandidateSource<'a>,
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        admission: FilterAdmission,
        indexed_corpus: &IndexedCorpus,
    ) -> crate::Result<Candidates<'a>> {
        let walked = FileWalk::from_filter(source.filter)
            .candidates_matching(indexed_corpus.unindexed_files())?;
        let unindexed = source.filter.retain(walked, FilterAdmission::Full);
        Ok(Candidates::mixed(
            indexes,
            file_ids,
            source.filter,
            admission,
            unindexed,
        ))
    }

    fn order(candidates: Candidates<'_>, order: CandidateOrder) -> crate::Result<Candidates<'_>> {
        if !order.is_sorted() {
            return Ok(candidates);
        }
        if matches!(order.key, CandidateOrderKey::Path) {
            match candidates.0 {
                CandidatesInner::Indexed {
                    indexes,
                    mut file_ids,
                    filter,
                    admission,
                } => {
                    if matches!(
                        order.direction,
                        crate::corpus::order::CandidateOrderDirection::Descending
                    ) {
                        file_ids.reverse();
                    }
                    return Ok(Candidates(CandidatesInner::Indexed {
                        indexes,
                        file_ids,
                        filter,
                        admission,
                    }));
                }
                other => {
                    let mut items = Candidates(other).into_vec();
                    order.order(&mut items)?;
                    return Ok(Candidates::from(items));
                }
            }
        }
        let mut items = candidates.into_vec();
        order.order(&mut items)?;
        Ok(Candidates::from(items))
    }
}
