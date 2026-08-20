use crate::corpus::File;
use crate::corpus::filter::FilterAdmission;
use crate::corpus::order::{FileOrder, FileOrderKey};
use crate::corpus::walk::FileWalk;
use crate::index::{FileId, Files, IndexCoverage, Indexes};
use crate::search::{Narrowing, Query};

use crate::candidates::scan::Scan;
use crate::candidates::scope::{Coverage, ScanScope, SnapshotFreshness};

use super::output::{Candidates, Origin as CandidatesOrigin};

/// Pure discovery decision for candidate resolution.
#[must_use]
pub struct Plan {
    pub(crate) discovery: Discovery,
    pub(crate) order: FileOrder,
    pub(crate) coverage: Coverage,
    pub(crate) query: Query,
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

impl Plan {
    /// Pure decision over discovery shape — no index query I/O.
    pub fn new(source: &Scan<'_>, query: &Query, coverage: Coverage) -> Self {
        let scope = source.scope;
        let narrowing = query.narrowing();
        let discovery = Self::discovery(source, coverage, narrowing);
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
    pub fn resolve<'a>(self, source: &'a Scan<'a>) -> crate::Result<Candidates<'a>> {
        let Self {
            discovery,
            order,
            coverage,
            query,
        } = self;
        let candidates = match discovery {
            Discovery::Empty => Candidates::empty(),
            Discovery::Walk => Candidates::from(Self::walk(source)?),
            Discovery::Index { admission } => match source.indexes {
                None => Candidates::empty(),
                Some(indexes) => {
                    let file_ids = Self::file_ids(indexes, &query, coverage)?;
                    Candidates::index(indexes, file_ids, source.filter, admission)
                }
            },
            Discovery::Merge { admission } => source.indexes.map_or_else(
                || Ok(Candidates::empty()),
                |indexes| {
                    let file_ids = Self::file_ids(indexes, &query, coverage)?;
                    indexes.files().map_or_else(
                        || Ok(Candidates::empty()),
                        |files| Self::merge(source, indexes, file_ids, admission, files),
                    )
                },
            )?,
        };
        Self::order(candidates, order)
    }

    fn file_ids(
        indexes: &Indexes,
        query: &Query,
        coverage: Coverage,
    ) -> crate::Result<Vec<FileId>> {
        match coverage {
            Coverage::Complete => Ok(indexes.all_indexed_file_ids()),
            Coverage::PotentialMatches => indexes.query(query),
        }
    }

    fn discovery(source: &Scan<'_>, coverage: Coverage, narrowing: Narrowing) -> Discovery {
        match source.scope {
            ScanScope::StreamsOnly => Discovery::Empty,
            ScanScope::Walk { .. } => Discovery::Walk,
            ScanScope::Index { .. } => match coverage {
                Coverage::Complete => Self::complete_discovery(source),
                Coverage::PotentialMatches => Self::potential_discovery(source, narrowing),
            },
        }
    }

    fn complete_discovery(source: &Scan<'_>) -> Discovery {
        let Some(indexes) = source.indexes else {
            return Discovery::Walk;
        };
        if !indexes.queryable() {
            return Discovery::Walk;
        }
        let Some(meta) = source.store_meta else {
            return Discovery::Index {
                admission: FilterAdmission::Full,
            };
        };
        if !meta.covers_candidate_filter(source.filter) {
            return Discovery::Walk;
        }
        match (meta.coverage, source.scope.freshness()) {
            (IndexCoverage::Lazy, _)
            | (IndexCoverage::Complete, Some(SnapshotFreshness::Stale)) => Discovery::Walk,
            (IndexCoverage::Complete, _) => Discovery::Index {
                admission: FilterAdmission::Indexed,
            },
        }
    }

    fn potential_discovery(source: &Scan<'_>, narrowing: Narrowing) -> Discovery {
        let Some(indexes) = source.indexes else {
            return Discovery::Walk;
        };
        if !indexes.queryable() || matches!(narrowing, Narrowing::Disabled) {
            return Discovery::Walk;
        }
        let Some(meta) = source.store_meta else {
            return Discovery::Index {
                admission: FilterAdmission::Full,
            };
        };
        if !meta.covers_candidate_filter(source.filter) {
            return Discovery::Walk;
        }
        match meta.coverage {
            IndexCoverage::Complete => Discovery::Index {
                admission: FilterAdmission::Indexed,
            },
            IndexCoverage::Lazy => Discovery::Merge {
                admission: FilterAdmission::Indexed,
            },
        }
    }

    fn walk(source: &Scan<'_>) -> crate::Result<Vec<File>> {
        let walked = FileWalk::from_filter(source.filter).candidates()?;
        Ok(source.filter.retain(walked, FilterAdmission::Full))
    }

    fn merge<'a>(
        source: &'a Scan<'a>,
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        admission: FilterAdmission,
        files: &Files,
    ) -> crate::Result<Candidates<'a>> {
        let walked: Vec<_> = FileWalk::from_filter(source.filter)
            .candidates()?
            .into_iter()
            .filter(|file| !files.contains(file.rel_path()))
            .collect();
        let unindexed = source.filter.retain(walked, FilterAdmission::Full);
        Ok(Candidates::merge(
            indexes,
            file_ids,
            source.filter,
            admission,
            unindexed,
        ))
    }

    fn order(candidates: Candidates<'_>, order: FileOrder) -> crate::Result<Candidates<'_>> {
        if !order.is_sorted() {
            return Ok(candidates);
        }
        if matches!(order.key, FileOrderKey::Path) {
            match candidates.0 {
                CandidatesOrigin::Index {
                    indexes,
                    mut file_ids,
                    filter,
                    admission,
                } => {
                    if matches!(
                        order.direction,
                        crate::corpus::order::FileOrderDirection::Descending
                    ) {
                        file_ids.reverse();
                    }
                    return Ok(Candidates(CandidatesOrigin::Index {
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
