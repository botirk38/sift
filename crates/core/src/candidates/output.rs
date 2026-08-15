use crate::corpus::File;
use crate::corpus::filter::{FileFilter, FilterAdmission};
use crate::index::{FileId, Indexes};

/// Corpus files ready for search.
pub struct Candidates<'a>(pub(crate) Origin<'a>);

pub enum Origin<'a> {
    /// Walk, merge residual, or sorted resolve: paths already materialized.
    Walk(Vec<File>),
    /// Index-narrowed file ids; search opens one file at a time.
    Index {
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    },
    /// Lazy snapshot: index hits stay as ids; unindexed walk paths are resolved.
    Merge {
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
        unindexed: Vec<File>,
    },
}

/// Iterator over candidates (opens index rows as it goes).
pub enum Iter<'a> {
    Walk(std::vec::IntoIter<File>),
    Index {
        ids: std::vec::IntoIter<FileId>,
        indexes: &'a Indexes,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    },
    Merge {
        ids: std::vec::IntoIter<FileId>,
        indexes: &'a Indexes,
        filter: &'a FileFilter,
        admission: FilterAdmission,
        unindexed: std::vec::IntoIter<File>,
    },
}

impl<'a> Candidates<'a> {
    #[must_use]
    pub const fn empty() -> Self {
        Self(Origin::Walk(Vec::new()))
    }

    pub(crate) const fn index(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    ) -> Self {
        Self(Origin::Index {
            indexes,
            file_ids,
            filter,
            admission,
        })
    }

    pub(crate) const fn merge(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
        unindexed: Vec<File>,
    ) -> Self {
        Self(Origin::Merge {
            indexes,
            file_ids,
            filter,
            admission,
            unindexed,
        })
    }

    /// Returns `true` when no candidates will be yielded.
    ///
    /// For index-backed rows, `false` means the id set may still filter to nothing during
    /// iteration.
    #[must_use = "candidate emptiness affects whether search runs"]
    pub const fn is_empty(&self) -> bool {
        match &self.0 {
            Origin::Walk(items) => items.is_empty(),
            Origin::Index { file_ids, .. } => file_ids.is_empty(),
            Origin::Merge {
                file_ids,
                unindexed,
                ..
            } => file_ids.is_empty() && unindexed.is_empty(),
        }
    }

    /// Cheap upper bound on candidates before hydration / admission filtering.
    ///
    /// For index-backed rows this is the id count (some ids may still drop during
    /// iteration). Walk rows are exact.
    #[must_use]
    pub const fn bound(&self) -> usize {
        match &self.0 {
            Origin::Walk(items) => items.len(),
            Origin::Index { file_ids, .. } => file_ids.len(),
            Origin::Merge {
                file_ids,
                unindexed,
                ..
            } => file_ids.len().saturating_add(unindexed.len()),
        }
    }

    /// Materialize every candidate. Index-backed rows open in parallel.
    #[must_use = "materialized candidates are consumed by search"]
    pub fn into_vec(self) -> Vec<File> {
        match self.0 {
            Origin::Walk(items) => items,
            Origin::Index {
                indexes,
                file_ids,
                filter,
                admission,
            } => indexes.candidates(&file_ids, filter, admission),
            Origin::Merge {
                indexes,
                file_ids,
                filter,
                admission,
                mut unindexed,
            } => {
                let mut items = indexes.candidates(&file_ids, filter, admission);
                items.append(&mut unindexed);
                items
            }
        }
    }
}

impl From<Vec<File>> for Candidates<'_> {
    fn from(items: Vec<File>) -> Self {
        Self(Origin::Walk(items))
    }
}

impl Iterator for Iter<'_> {
    type Item = File;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Walk(iter) => iter.next(),
            Self::Index {
                ids,
                indexes,
                filter,
                admission,
            } => loop {
                let id = ids.next()?;
                if let Some(candidate) = indexes.file(id, filter, *admission) {
                    return Some(candidate);
                }
            },
            Self::Merge {
                ids,
                indexes,
                filter,
                admission,
                unindexed,
            } => loop {
                match ids.next() {
                    Some(id) => {
                        if let Some(candidate) = indexes.file(id, filter, *admission) {
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
            Self::Walk(iter) => iter.size_hint(),
            Self::Index { ids, .. } => (0, ids.size_hint().1),
            Self::Merge { ids, unindexed, .. } => {
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
    type Item = File;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self.0 {
            Origin::Walk(items) => Iter::Walk(items.into_iter()),
            Origin::Index {
                indexes,
                file_ids,
                filter,
                admission,
            } => Iter::Index {
                ids: file_ids.into_iter(),
                indexes,
                filter,
                admission,
            },
            Origin::Merge {
                indexes,
                file_ids,
                filter,
                admission,
                unindexed,
            } => Iter::Merge {
                ids: file_ids.into_iter(),
                indexes,
                filter,
                admission,
                unindexed: unindexed.into_iter(),
            },
        }
    }
}
