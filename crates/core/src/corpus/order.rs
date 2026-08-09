use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::File;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileOrderKey {
    #[default]
    None,
    Path,
    Modified,
    Accessed,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileOrderDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileOrder {
    pub key: FileOrderKey,
    pub direction: FileOrderDirection,
}

impl FileOrder {
    #[must_use]
    pub const fn new(key: FileOrderKey, direction: FileOrderDirection) -> Self {
        Self { key, direction }
    }

    #[must_use]
    pub const fn is_sorted(self) -> bool {
        !matches!(self.key, FileOrderKey::None)
    }

    /// Order candidates in place according to the configured key.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when filesystem metadata required by a timestamp
    /// ordering key cannot be read.
    pub fn order(self, candidates: &mut [File]) -> crate::Result<()> {
        if !self.is_sorted() {
            return Ok(());
        }

        let mut keyed = Vec::with_capacity(candidates.len());
        for candidate in candidates.iter().cloned() {
            keyed.push(FileOrderEntry::new(candidate, self.key)?);
        }

        keyed.sort_by(|a, b| {
            a.value
                .cmp(&b.value)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });
        if matches!(self.direction, FileOrderDirection::Descending) {
            keyed.reverse();
        }

        for (slot, entry) in candidates.iter_mut().zip(keyed) {
            *slot = entry.candidate;
        }

        Ok(())
    }
}

struct FileOrderEntry {
    value: FileOrderValue,
    rel_path: PathBuf,
    candidate: File,
}

impl FileOrderEntry {
    fn new(candidate: File, key: FileOrderKey) -> crate::Result<Self> {
        let rel_path = candidate.rel_path().to_path_buf();
        let value = match key {
            FileOrderKey::None | FileOrderKey::Path => FileOrderValue::Path(rel_path.clone()),
            FileOrderKey::Modified => FileOrderValue::Time(candidate_time(
                candidate.abs_path(),
                std::fs::Metadata::modified,
            )?),
            FileOrderKey::Accessed => FileOrderValue::Time(candidate_time(
                candidate.abs_path(),
                std::fs::Metadata::accessed,
            )?),
            FileOrderKey::Created => FileOrderValue::Time(candidate_time(
                candidate.abs_path(),
                std::fs::Metadata::created,
            )?),
        };

        Ok(Self {
            value,
            rel_path,
            candidate,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum FileOrderValue {
    Path(PathBuf),
    Time(SystemTime),
}

fn candidate_time(
    path: &Path,
    timestamp: impl FnOnce(&std::fs::Metadata) -> std::io::Result<SystemTime>,
) -> crate::Result<SystemTime> {
    Ok(timestamp(&std::fs::metadata(path)?)?)
}
