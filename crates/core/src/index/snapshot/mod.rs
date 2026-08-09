pub mod artifact;
pub mod identity;
pub mod lease;
pub mod manifest;
pub mod store;

pub use artifact::ArtifactData;
pub use identity::SnapshotId;
pub use lease::SnapshotLease;
pub use manifest::SnapshotManifest;
pub use store::disk::DiskSnapshotStore;
pub use store::{SnapshotRead, SnapshotWrite};

use std::path::{Path, PathBuf};

use super::config::CorpusKind;
use super::error::IndexError;
use super::files::Files;
use super::record::Index;

/// An immutable opened snapshot: shared [`Files`], opened indexes, and lease.
pub struct Snapshot {
    id: Option<SnapshotId>,
    root: PathBuf,
    state: SnapshotState,
}

enum SnapshotState {
    Empty,
    Current(CurrentSnapshot),
}

struct CurrentSnapshot {
    files: Files,
    indexes: Vec<Box<dyn Index>>,
    _lease: SnapshotLease,
}

impl Snapshot {
    #[must_use]
    pub(crate) const fn empty(root: PathBuf) -> Self {
        Self {
            id: None,
            root,
            state: SnapshotState::Empty,
        }
    }

    #[must_use]
    pub(crate) fn committed(
        id: SnapshotId,
        root: PathBuf,
        files: Files,
        indexes: Vec<Box<dyn Index>>,
        lease: SnapshotLease,
    ) -> Self {
        Self {
            id: Some(id),
            root,
            state: SnapshotState::Current(CurrentSnapshot {
                files,
                indexes,
                _lease: lease,
            }),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn id(&self) -> Option<&SnapshotId> {
        self.id.as_ref()
    }

    #[must_use]
    pub(crate) fn indexes(&self) -> &[Box<dyn Index>] {
        match &self.state {
            SnapshotState::Empty => &[],
            SnapshotState::Current(c) => &c.indexes,
        }
    }

    /// Shared file table for this snapshot.
    #[must_use]
    pub const fn files(&self) -> Option<&Files> {
        match &self.state {
            SnapshotState::Empty => None,
            SnapshotState::Current(c) => Some(&c.files),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self.state, SnapshotState::Empty)
    }

    /// Open the current committed snapshot for search.
    ///
    /// Uses `root` and `corpus_kind` from the caller's metadata so that
    /// snapshot loading does not read `.sift/meta.json`.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest is malformed, an index kind is unknown,
    /// or the snapshot could not be opened after retry.
    pub fn open_current(
        sift_dir: &Path,
        root: PathBuf,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Self> {
        for attempt in 0..2 {
            let Some(current_id) = DiskSnapshotStore::read_current_id(sift_dir)? else {
                return Ok(Self::empty(root));
            };

            let snap_dir = sift_dir.join("snapshots").join(&current_id);

            let lease = SnapshotLease::create_file(sift_dir, &current_id)?;

            if !snap_dir.exists() {
                drop(lease);
                continue;
            }

            let manifest_path = snap_dir.join("manifest.json");
            let manifest_raw = match std::fs::read_to_string(&manifest_path) {
                Ok(raw) => raw,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempt == 0 => {
                    drop(lease);
                    continue;
                }
                Err(e) => return Err(crate::Error::Io(e)),
            };

            let manifest: SnapshotManifest = serde_json::from_str(&manifest_raw).map_err(|e| {
                crate::Error::Index(IndexError::InvalidManifest {
                    path: manifest_path.clone(),
                    source: e,
                })
            })?;

            let mut indexes: Vec<Box<dyn Index>> = Vec::new();
            let mut files = None;
            for record in &manifest.indexes {
                let namespace = record.name();
                let index_dir = snap_dir.join(&namespace);
                if !index_dir.exists() {
                    return Err(crate::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("index namespace {namespace} missing under snapshot"),
                    )));
                }
                if files.is_none() {
                    files = Some(Files::open(&index_dir, root.clone())?);
                }
                indexes.push(record.open(&index_dir, &root, corpus_kind)?);
            }

            let Some(files) = files else {
                return Ok(Self::empty(root));
            };

            return Ok(Self::committed(manifest.id, root, files, indexes, lease));
        }

        Err(crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "snapshot disappeared during open",
        )))
    }
}

#[cfg(test)]
mod tests;
