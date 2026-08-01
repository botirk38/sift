use std::path::{Path, PathBuf};

use super::config::{CorpusKind, IndexConfig};
use super::contract::{Index, IndexWrite};
use super::error::IndexError;
use super::kinds::FileId;
use super::meta::StoreMeta;
use super::paths::IndexedCorpus;
use super::snapshot::store::SnapshotWrite;
use super::snapshot::{
    DiskSnapshotStore, Snapshot, SnapshotId, SnapshotManifest, SnapshotRead, SnapshotStore,
    SnapshotWriterSession,
};
use super::{IndexDestination, IndexSource};

use crate::corpus::Candidate;
use crate::corpus::filter::{CandidateFilter, FilterAdmission};
use crate::search::SearchQuery;

/// Composable snapshot store and query registry.
///
/// Combines index lifecycle (build, update, publish) with query-time candidate
/// narrowing. [`Self::open`] ensures store metadata is on disk and loads the
/// current snapshot; [`Self::load`] opens an existing store without creating
/// one. `build` / `update` acquire the write lock and publish a new committed
/// snapshot.
pub struct Indexes {
    sift_dir: PathBuf,
    snapshots: DiskSnapshotStore,
    meta: StoreMeta,
    snapshot: Snapshot,
}

impl Indexes {
    /// Open the store at `sift_dir`, writing `meta` when the store is new, and
    /// load the current committed snapshot.
    ///
    /// Use this for index lifecycle (build/update). For search-only loads that
    /// must not create a store, use [`Self::load`].
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created, metadata cannot be
    /// written, the snapshot store cannot be opened, or the current snapshot
    /// cannot be loaded.
    pub fn open(sift_dir: &Path, meta: &StoreMeta) -> crate::Result<Self> {
        std::fs::create_dir_all(sift_dir)?;
        if !StoreMeta::path(sift_dir).exists() {
            let guard = WriteLockGuard::acquire(sift_dir)?;
            if !StoreMeta::path(sift_dir).exists() {
                meta.write(sift_dir)?;
            }
            drop(guard);
        }
        let stored_meta = StoreMeta::read(sift_dir).unwrap_or_else(|_| meta.clone());
        Self::from_stored(sift_dir, stored_meta)
    }

    /// Load an existing store for search. Does not create meta or directories.
    ///
    /// Returns `Ok(None)` when no store exists. A dangling `CURRENT` without
    /// meta is treated as a broken store and returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata exists but cannot be read, the current
    /// snapshot cannot be opened, or the store is inconsistent.
    pub fn load(sift_dir: &Path) -> crate::Result<Option<Self>> {
        if !StoreMeta::path(sift_dir).exists() {
            if DiskSnapshotStore::read_current_id(sift_dir)?.is_some() {
                // CURRENT without meta is inconsistent — surface the open error.
                let _ = Snapshot::open_current(sift_dir, PathBuf::new(), CorpusKind::Directory)?;
            }
            return Ok(None);
        }
        let stored_meta = StoreMeta::read(sift_dir)?;
        Self::from_stored(sift_dir, stored_meta).map(Some)
    }

    fn from_stored(sift_dir: &Path, stored_meta: StoreMeta) -> crate::Result<Self> {
        let snapshots = DiskSnapshotStore::open(sift_dir)?;
        let snapshot = Snapshot::open_current(
            sift_dir,
            stored_meta.corpus.root.clone(),
            stored_meta.corpus.kind,
        )?;
        Ok(Self {
            sift_dir: sift_dir.to_path_buf(),
            snapshots,
            meta: stored_meta,
            snapshot,
        })
    }

    /// Metadata currently governing the store.
    #[must_use]
    pub const fn meta(&self) -> &StoreMeta {
        &self.meta
    }

    /// Overwrite the store metadata on disk and in-memory.
    ///
    /// # Errors
    ///
    /// Returns an error if writing `meta.json` fails.
    pub fn refresh_meta(&mut self, meta: &StoreMeta) -> crate::Result<()> {
        meta.write(&self.sift_dir)?;
        self.meta = meta.clone();
        Ok(())
    }

    /// Committed snapshot id when present.
    #[must_use]
    pub fn current_id(&self) -> Option<&str> {
        self.snapshots.current_id().map(SnapshotId::as_str)
    }

    /// Filesystem directory of a snapshot (used by CLI diagnostics).
    #[must_use]
    pub fn snapshot_dir(&self, id: &str) -> PathBuf {
        self.sift_dir.join("snapshots").join(id)
    }

    /// Whether any opened index is available for candidate narrowing.
    #[must_use]
    pub fn queryable(&self) -> bool {
        !self.snapshot.indexes().is_empty()
    }

    /// Corpus root recorded in metadata.
    #[must_use]
    pub fn corpus_root(&self) -> &Path {
        self.meta.corpus.root.as_path()
    }

    /// Corpus kind recorded in metadata.
    #[must_use]
    pub const fn corpus_kind(&self) -> CorpusKind {
        self.meta.corpus.kind
    }

    /// Committed snapshot id when present.
    #[must_use]
    pub const fn snapshot_id(&self) -> Option<&SnapshotId> {
        self.snapshot.id()
    }

    /// Corpus-relative paths covered by every opened index in this snapshot.
    #[must_use]
    pub fn indexed_corpus(&self) -> IndexedCorpus {
        IndexedCorpus::intersection(self.snapshot.indexes().iter().map(|idx| idx.coverage()))
    }

    /// Build a new snapshot using the given configured indexes.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer session cannot be acquired, the build
    /// fails, or publishing fails.
    pub fn build(
        &mut self,
        indexes: &[Box<dyn Index>],
        config: &IndexConfig<'_>,
        paths: &[PathBuf],
    ) -> crate::Result<String> {
        let mut writer = self.snapshots.writer()?;
        let mut txn = writer.begin()?;

        for index in indexes {
            let namespace = index.name();
            index.build(IndexWrite {
                dest: IndexDestination::Snapshot {
                    writer: &mut txn,
                    namespace: &namespace,
                },
                config,
                paths,
            })?;
        }

        let manifest = SnapshotManifest {
            id: txn.id().clone(),
            indexes: indexes.iter().map(|idx| idx.to_record()).collect(),
        };
        let id = writer.publish(txn, manifest)?;
        drop(writer);
        self.reload_snapshot()?;
        Ok(id.to_string())
    }

    /// Refresh the current snapshot, rebuilding only indexes whose corpus
    /// changed.
    ///
    /// # Errors
    ///
    /// Returns an error if there is no current snapshot, the writer session
    /// cannot be acquired, the update fails, or publishing fails.
    pub fn update(
        &mut self,
        indexes: &[Box<dyn Index>],
        paths: &[PathBuf],
    ) -> crate::Result<Option<String>> {
        let config = self.meta.write_config();
        let mut writer = self.snapshots.writer()?;

        let Some(current) = writer.current()? else {
            return Err(crate::Error::Index(IndexError::Io {
                path: self.sift_dir.clone(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no current snapshot; run build first",
                ),
            }));
        };

        let mut txn = writer.begin()?;

        let changed: Vec<bool> = indexes
            .iter()
            .map(|index| {
                let namespace = index.name();
                let is_present = current
                    .manifest()
                    .indexes
                    .iter()
                    .any(|record| record.name() == namespace);
                if !is_present {
                    index.build(IndexWrite {
                        dest: IndexDestination::Snapshot {
                            writer: &mut txn,
                            namespace: &namespace,
                        },
                        config: &config,
                        paths,
                    })?;
                    return Ok(true);
                }
                let opened_source = IndexSource::Snapshot {
                    reader: &current as &dyn SnapshotRead,
                    namespace: &namespace,
                };
                let opened = index.open(opened_source, config.corpus.root, config.corpus.kind)?;
                opened.update(IndexWrite {
                    dest: IndexDestination::Snapshot {
                        writer: &mut txn,
                        namespace: &namespace,
                    },
                    config: &config,
                    paths,
                })
            })
            .collect::<crate::Result<_>>()?;

        if !changed.iter().any(|&c| c) {
            return Ok(None);
        }

        for (index, did_change) in indexes.iter().zip(&changed) {
            if !did_change {
                let namespace = index.name();
                for artifact_name in current.artifacts(&namespace)? {
                    let data = current.artifact(&namespace, &artifact_name)?;
                    let bytes = data.as_ref().to_vec();
                    txn.put_artifact(&namespace, &artifact_name, bytes)?;
                }
            }
        }

        let manifest = SnapshotManifest {
            id: txn.id().clone(),
            indexes: indexes.iter().map(|idx| idx.to_record()).collect(),
        };
        let id = writer.publish(txn, manifest)?;
        drop(writer);
        drop(current);
        self.reload_snapshot()?;
        Ok(Some(id.to_string()))
    }

    fn reload_snapshot(&mut self) -> crate::Result<()> {
        self.snapshots = DiskSnapshotStore::open(&self.sift_dir)?;
        self.snapshot = Snapshot::open_current(
            &self.sift_dir,
            self.meta.corpus.root.clone(),
            self.meta.corpus.kind,
        )?;
        Ok(())
    }

    #[must_use]
    pub(crate) fn query(&self, query: &SearchQuery) -> Vec<FileId> {
        let indexes = self.snapshot.indexes();
        if indexes.is_empty() {
            return Vec::new();
        }
        if indexes.len() == 1 {
            return indexes[0].query(query);
        }
        let mut plans: Vec<Vec<FileId>> = indexes.iter().map(|idx| idx.query(query)).collect();
        plans.sort_by_key(Vec::len);
        let mut cur = plans.remove(0);
        for next in plans {
            cur = Self::intersect_sorted(&cur, &next);
            if cur.is_empty() {
                break;
            }
        }
        cur
    }

    fn intersect_sorted(a: &[FileId], b: &[FileId]) -> Vec<FileId> {
        let mut out = Vec::with_capacity(a.len().min(b.len()));
        let (mut i, mut j) = (0usize, 0usize);
        while i < a.len() && j < b.len() {
            match a[i].cmp(&b[j]) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    out.push(a[i]);
                    i += 1;
                    j += 1;
                }
            }
        }
        out
    }

    pub(crate) fn candidate(
        &self,
        id: FileId,
        filter: &CandidateFilter,
        admission: FilterAdmission,
    ) -> Option<Candidate> {
        let lead = self.lead_index()?;
        let candidate = lead.candidate(id)?;
        candidate.matches(filter, admission).then_some(candidate)
    }

    pub(crate) fn candidates(
        &self,
        file_ids: &[FileId],
        filter: &CandidateFilter,
        admission: FilterAdmission,
    ) -> Vec<Candidate> {
        use rayon::prelude::*;
        let Some(lead) = self.lead_index() else {
            return Vec::new();
        };
        file_ids
            .par_iter()
            .filter_map(|id| {
                let candidate = lead.candidate(*id)?;
                candidate.matches(filter, admission).then_some(candidate)
            })
            .collect()
    }

    pub(crate) fn all_indexed_file_ids(&self, corpus: &IndexedCorpus) -> Vec<FileId> {
        let Some(lead) = self.lead_index() else {
            return Vec::new();
        };
        if self.snapshot.indexes().len() == 1 {
            return lead.all_file_ids();
        }
        lead.all_file_ids()
            .into_iter()
            .filter(|id| {
                lead.candidate(*id)
                    .is_some_and(|c| corpus.contains(c.rel_path()))
            })
            .collect()
    }

    fn lead_index(&self) -> Option<&dyn Index> {
        self.snapshot.indexes().first().map(AsRef::as_ref)
    }
}

struct WriteLockGuard {
    file: fslock::LockFile,
}

impl WriteLockGuard {
    fn acquire(sift_dir: &Path) -> crate::Result<Self> {
        let lock_path = sift_dir.join("write.lock");
        let mut lock_file = fslock::LockFile::open(&lock_path)?;
        lock_file.lock()?;
        Ok(Self { file: lock_file })
    }
}

impl Drop for WriteLockGuard {
    fn drop(&mut self) {
        let _ = &mut self.file;
    }
}
