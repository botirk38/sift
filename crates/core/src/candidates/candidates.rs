use crate::corpus::Candidate;
use crate::corpus::filter::{CandidateFilter, FilterAdmission};
use crate::index::{FileId, Indexes};

/// Index-narrowed file ids with the filter used when each row is opened.
pub struct IndexedCandidates<'a> {
    pub(crate) indexes: &'a Indexes,
    pub(crate) file_ids: Vec<FileId>,
    pub(crate) filter: &'a CandidateFilter,
    pub(crate) admission: FilterAdmission,
}

/// Corpus files ready for search.
///
/// [`Resolved`](Self::Resolved) paths are already materialized. [`Indexed`](Self::Indexed)
/// keeps file ids until the searcher opens each row. [`Mixed`](Self::Mixed) pairs deferred
/// index hits with already-resolved unindexed walk paths (lazy snapshots).
pub enum Candidates<'a> {
    /// Walk, merge residual, or sorted resolve: paths already materialized.
    Resolved(Vec<Candidate>),
    /// Index-narrowed file ids; search hydrates one file at a time.
    Indexed(IndexedCandidates<'a>),
    /// Lazy snapshot: index hits stay as ids; unindexed walk paths are resolved.
    Mixed {
        indexed: IndexedCandidates<'a>,
        unindexed: Vec<Candidate>,
    },
}

/// Iterator over resolved candidates (hydrates index rows as it goes).
pub enum IntoIter<'a> {
    Resolved(std::vec::IntoIter<Candidate>),
    Indexed {
        ids: std::vec::IntoIter<FileId>,
        indexes: &'a Indexes,
        filter: &'a CandidateFilter,
        admission: FilterAdmission,
    },
    Mixed {
        ids: std::vec::IntoIter<FileId>,
        indexes: &'a Indexes,
        filter: &'a CandidateFilter,
        admission: FilterAdmission,
        unindexed: std::vec::IntoIter<Candidate>,
    },
}

impl<'a> IndexedCandidates<'a> {
    pub(crate) const fn new(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a CandidateFilter,
        admission: FilterAdmission,
    ) -> Self {
        Self {
            indexes,
            file_ids,
            filter,
            admission,
        }
    }

    #[must_use]
    pub(crate) fn file_ids(&self) -> &[FileId] {
        &self.file_ids
    }
}

impl<'a> Candidates<'a> {
    pub(crate) const fn empty() -> Self {
        Self::Resolved(Vec::new())
    }

    pub(crate) const fn indexed(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a CandidateFilter,
        admission: FilterAdmission,
    ) -> Self {
        Self::Indexed(IndexedCandidates::new(indexes, file_ids, filter, admission))
    }

    pub(crate) const fn mixed(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a CandidateFilter,
        admission: FilterAdmission,
        unindexed: Vec<Candidate>,
    ) -> Self {
        Self::Mixed {
            indexed: IndexedCandidates::new(indexes, file_ids, filter, admission),
            unindexed,
        }
    }

    /// Returns `true` when no candidates will be yielded.
    ///
    /// For index-backed rows, `false` means the id set may still filter to nothing during
    /// iteration.
    #[must_use = "candidate emptiness affects whether search runs"]
    pub const fn is_empty(&self) -> bool {
        match self {
            Self::Resolved(items) => items.is_empty(),
            Self::Indexed(indexed) => indexed.file_ids.is_empty(),
            Self::Mixed { indexed, unindexed } => {
                indexed.file_ids.is_empty() && unindexed.is_empty()
            }
        }
    }

    /// Materialize every candidate. Index-backed rows materialize in parallel.
    #[must_use = "materialized candidates are consumed by search"]
    pub fn into_vec(self) -> Vec<Candidate> {
        match self {
            Self::Resolved(items) => items,
            Self::Indexed(indexed) => {
                indexed
                    .indexes
                    .candidates(&indexed.file_ids, indexed.filter, indexed.admission)
            }
            Self::Mixed {
                indexed,
                mut unindexed,
            } => {
                let mut items = indexed.indexes.candidates(
                    &indexed.file_ids,
                    indexed.filter,
                    indexed.admission,
                );
                items.append(&mut unindexed);
                items
            }
        }
    }
}

impl From<Vec<Candidate>> for Candidates<'_> {
    fn from(items: Vec<Candidate>) -> Self {
        Self::Resolved(items)
    }
}

impl<'a> From<Candidates<'a>> for Vec<Candidate> {
    fn from(candidates: Candidates<'a>) -> Self {
        candidates.into_vec()
    }
}

impl Iterator for IntoIter<'_> {
    type Item = Candidate;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Resolved(iter) => iter.next(),
            Self::Indexed {
                ids,
                indexes,
                filter,
                admission,
            } => loop {
                let id = ids.next()?;
                if let Some(candidate) = indexes.candidate(id, filter, *admission) {
                    return Some(candidate);
                }
            },
            Self::Mixed {
                ids,
                indexes,
                filter,
                admission,
                unindexed,
            } => loop {
                match ids.next() {
                    Some(id) => {
                        if let Some(candidate) = indexes.candidate(id, filter, *admission) {
                            return Some(candidate);
                        }
                    }
                    None => return unindexed.next(),
                }
            },
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Resolved(iter) => iter.size_hint(),
            Self::Indexed { ids, .. } => (0, ids.size_hint().1),
            Self::Mixed { ids, unindexed, .. } => {
                let (unindexed_lo, unindexed_hi) = unindexed.size_hint();
                let (_, ids_hi) = ids.size_hint();
                (
                    unindexed_lo,
                    match (ids_hi, unindexed_hi) {
                        (Some(a), Some(b)) => Some(a.saturating_add(b)),
                        _ => None,
                    },
                )
            }
        }
    }
}

impl<'a> IntoIterator for Candidates<'a> {
    type Item = Candidate;
    type IntoIter = IntoIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::Resolved(items) => IntoIter::Resolved(items.into_iter()),
            Self::Indexed(indexed) => IntoIter::Indexed {
                ids: indexed.file_ids.into_iter(),
                indexes: indexed.indexes,
                filter: indexed.filter,
                admission: indexed.admission,
            },
            Self::Mixed { indexed, unindexed } => IntoIter::Mixed {
                ids: indexed.file_ids.into_iter(),
                indexes: indexed.indexes,
                filter: indexed.filter,
                admission: indexed.admission,
                unindexed: unindexed.into_iter(),
            },
        }
    }
}
