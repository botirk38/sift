//! Snapshot-owned file id → candidate map.

use std::path::{Path, PathBuf};

use crate::corpus::File;
use crate::index::FileId;
use crate::index::ngram::files::FileTable;

/// Shared `FileId` space for a committed snapshot.
#[derive(Debug)]
pub struct Files {
    root: PathBuf,
    table: FileTable,
}

impl Files {
    /// Load `files.bin` from an index namespace directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the file table is missing or malformed.
    pub(crate) fn open(index_dir: &Path, root: PathBuf) -> crate::Result<Self> {
        let table = FileTable::open(&index_dir.join(crate::FILES_BIN)).map_err(crate::Error::Io)?;
        table.validate_paths().map_err(crate::Error::Io)?;
        Ok(Self { root, table })
    }

    /// One indexed row as a [`File`]. Missing id → `None`.
    #[must_use]
    pub fn file(&self, id: FileId) -> Option<File> {
        let row = self.table.row(id.get()).ok()?;
        let rel = PathBuf::from(row.path);
        let abs = self.root.join(&rel);
        Some(File::with_metadata(rel, abs, Some(row.size), None))
    }

    /// Every file id in this table.
    #[must_use]
    pub fn all_file_ids(&self) -> Vec<FileId> {
        (0..self.table.len()).map(FileId::new).collect()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.table.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
