use crate::corpus::File;
use crate::corpus::filter::{FileFilter, FilterAdmission};
use crate::index::{FileId, Indexes};

/// Corpus files ready for search.
pub struct Candidates<'a>(pub(crate) Inner<'a>);

pub(crate) enum Inner<'a> {
    /// Walk, merge residual, or sorted resolve: paths already materialized.
    Resolved(Vec<File>),
    /// Index-narrowed file ids; search opens one file at a time.
    Indexed {
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    },
    /// Lazy snapshot: index hits stay as ids; unindexed walk paths are resolved.
    Mixed {
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
        unindexed: Vec<File>,
    },
}

/// Iterator over candidates (opens index rows as it goes).
pub enum IntoIter<'a> {
    Resolved(std::vec::IntoIter<File>),
    Indexed {
        ids: std::vec::IntoIter<FileId>,
        indexes: &'a Indexes,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    },
    Mixed {
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
        Self(Inner::Resolved(Vec::new()))
    }

    pub(crate) const fn indexed(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
    ) -> Self {
        Self(Inner::Indexed {
            indexes,
            file_ids,
            filter,
            admission,
        })
    }

    pub(crate) const fn mixed(
        indexes: &'a Indexes,
        file_ids: Vec<FileId>,
        filter: &'a FileFilter,
        admission: FilterAdmission,
        unindexed: Vec<File>,
    ) -> Self {
        Self(Inner::Mixed {
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
            Inner::Resolved(items) => items.is_empty(),
            Inner::Indexed { file_ids, .. } => file_ids.is_empty(),
            Inner::Mixed {
                file_ids,
                unindexed,
                ..
            } => file_ids.is_empty() && unindexed.is_empty(),
        }
    }

    /// Materialize every candidate. Index-backed rows open in parallel.
    #[must_use = "materialized candidates are consumed by search"]
    pub fn into_vec(self) -> Vec<File> {
        match self.0 {
            Inner::Resolved(items) => items,
            Inner::Indexed {
                indexes,
                file_ids,
                filter,
                admission,
            } => indexes.candidates(&file_ids, filter, admission),
            Inner::Mixed {
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
        Self(Inner::Resolved(items))
    }
}

impl Iterator for IntoIter<'_> {
    type Item = File;

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
                if let Some(candidate) = indexes.file(id, filter, *admission) {
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
    type Item = File;
    type IntoIter = IntoIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        match self.0 {
            Inner::Resolved(items) => IntoIter::Resolved(items.into_iter()),
            Inner::Indexed {
                indexes,
                file_ids,
                filter,
                admission,
            } => IntoIter::Indexed {
                ids: file_ids.into_iter(),
                indexes,
                filter,
                admission,
            },
            Inner::Mixed {
                indexes,
                file_ids,
                filter,
                admission,
                unindexed,
            } => IntoIter::Mixed {
                ids: file_ids.into_iter(),
                indexes,
                filter,
                admission,
                unindexed: unindexed.into_iter(),
            },
        }
    }
}
