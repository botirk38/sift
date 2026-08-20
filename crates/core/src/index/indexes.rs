//! Store facade: open / load / build / query over shared [`Files`] and runtime kinds.

use std::path::{Path, PathBuf};

use super::disk::{DiskStore, SnapshotHandle, SnapshotId, SnapshotManifest};
use super::files::Files;
use super::kinds::FileId;
use super::meta::{STORE_VERSION, StoreMeta};
use super::record::Kind;

use crate::corpus::File;
use crate::corpus::filter::{FileFilter, FilterAdmission};
use crate::search::Query;

/// Committed snapshot held for search.
struct Snapshot {
    files: Files,
    kinds: Vec<Kind>,
    /// On-disk snapshot directory + soft pin.
    handle: SnapshotHandle,
}

/// Composable snapshot store and query registry.
pub struct Indexes {
    sift_dir: PathBuf,
    store: DiskStore,
    meta: StoreMeta,
    current: Option<Snapshot>,
}

impl Indexes {
    /// Open the store at `sift_dir`, writing `meta` when the store is new.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created, metadata cannot be
    /// written, or the current snapshot cannot be loaded.
    pub fn open(sift_dir: &Path, meta: &StoreMeta) -> crate::Result<Self> {
        std::fs::create_dir_all(sift_dir)?;
        if !StoreMeta::path(sift_dir).exists() {
            let guard = WriteLockGuard::acquire(sift_dir)?;
            if !StoreMeta::path(sift_dir).exists() {
                let mut meta = meta.clone();
                meta.version = STORE_VERSION;
                meta.write(sift_dir)?;
            }
            drop(guard);
        }
        let stored_meta = StoreMeta::read(sift_dir)?;
        Self::from_stored(sift_dir, stored_meta)
    }

    /// Load an existing store for search. Does not create meta or directories.
    ///
    /// Returns `Ok(None)` when no store exists. A dangling `CURRENT` without
    /// meta is treated as a broken store and returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata exists but cannot be read or the store is
    /// inconsistent.
    pub fn load(sift_dir: &Path) -> crate::Result<Option<Self>> {
        if !StoreMeta::path(sift_dir).exists() {
            if DiskStore::read_current_id(sift_dir)?.is_some() {
                let _ = SnapshotHandle::current(sift_dir)?;
            }
            return Ok(None);
        }
        let stored_meta = StoreMeta::read(sift_dir)?;
        Self::from_stored(sift_dir, stored_meta).map(Some)
    }

    fn from_stored(sift_dir: &Path, stored_meta: StoreMeta) -> crate::Result<Self> {
        let store = DiskStore::open(sift_dir)?;
        let current = Self::load_current(sift_dir, &stored_meta.corpus.root)?;
        Ok(Self {
            sift_dir: sift_dir.to_path_buf(),
            store,
            meta: stored_meta,
            current,
        })
    }

    fn load_current(sift_dir: &Path, root: &Path) -> crate::Result<Option<Snapshot>> {
        let Some(handle) = SnapshotHandle::current(sift_dir)? else {
            return Ok(None);
        };
        let files = Files::open(handle.dir(), root.to_path_buf())?;
        let file_count = files.len();
        let mut kinds = Vec::with_capacity(handle.manifest().indexes.len());
        for record in &handle.manifest().indexes {
            let ns = handle.dir().join(record.name());
            kinds.push(record.open(&ns, file_count)?);
        }
        Ok(Some(Snapshot {
            files,
            kinds,
            handle,
        }))
    }

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
        let mut meta = meta.clone();
        meta.version = STORE_VERSION;
        meta.write(&self.sift_dir)?;
        self.meta = meta;
        Ok(())
    }

    #[must_use]
    pub fn current_id(&self) -> Option<&str> {
        self.current.as_ref().map(|c| c.handle.id().as_str())
    }

    #[must_use]
    pub fn snapshot_dir(&self, id: &str) -> PathBuf {
        self.sift_dir.join("snapshots").join(id)
    }

    #[must_use]
    pub fn queryable(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|c| !c.files.is_empty() && !c.kinds.is_empty())
    }

    #[must_use]
    pub fn corpus_root(&self) -> &Path {
        &self.meta.corpus.root
    }

    #[must_use]
    pub fn snapshot_id(&self) -> Option<&SnapshotId> {
        self.current.as_ref().map(|c| c.handle.id())
    }

    #[must_use]
    pub const fn files(&self) -> Option<&Files> {
        match &self.current {
            Some(current) => Some(&current.files),
            None => None,
        }
    }

    /// Build a new snapshot from the store catalog and corpus config.
    ///
    /// # Errors
    ///
    /// Returns an error if walking, building, or publishing fails.
    pub fn build(&mut self) -> crate::Result<String> {
        let records = self.meta.indexes.clone();

        let mut writer = self.store.writer()?;
        let txn = writer.begin()?;
        let files = Files::build(&self.meta, txn.dir())?;

        for record in &records {
            let ns = txn.namespace_dir(&record.name())?;
            record.build(&ns, &files)?;
        }
        // Release the files.bin mmap before renaming the snapshot temp dir.
        // Windows denies directory rename while a mapping is open (ERROR_ACCESS_DENIED).
        drop(files);

        let manifest = SnapshotManifest::new(txn.id().clone(), records);
        let id = writer.publish(txn, &manifest)?;
        drop(writer);
        self.reload()?;
        Ok(id.to_string())
    }

    fn reload(&mut self) -> crate::Result<()> {
        self.store = DiskStore::open(&self.sift_dir)?;
        self.current = Self::load_current(&self.sift_dir, &self.meta.corpus.root)?;
        Ok(())
    }

    pub(crate) fn query(&self, query: &Query) -> crate::Result<Vec<FileId>> {
        let Some(current) = &self.current else {
            return Ok(Vec::new());
        };
        if current.kinds.is_empty() {
            return Ok(Vec::new());
        }
        if current.kinds.len() == 1 {
            return current.kinds[0].query(query);
        }
        let mut plans: Vec<Vec<FileId>> = current
            .kinds
            .iter()
            .map(|idx| idx.query(query))
            .collect::<crate::Result<_>>()?;
        plans.sort_by_key(Vec::len);
        let mut cur = plans.remove(0);
        for next in plans {
            cur = Self::intersect_sorted(&cur, &next);
            if cur.is_empty() {
                break;
            }
        }
        Ok(cur)
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

    pub(crate) fn file(
        &self,
        id: FileId,
        filter: &FileFilter,
        admission: FilterAdmission,
    ) -> Option<File> {
        let candidate = self.files()?.file(id)?;
        candidate.matches(filter, admission).then_some(candidate)
    }

    pub(crate) fn candidates(
        &self,
        file_ids: &[FileId],
        filter: &FileFilter,
        admission: FilterAdmission,
    ) -> Vec<File> {
        use rayon::prelude::*;
        let Some(files) = self.files() else {
            return Vec::new();
        };
        file_ids
            .par_iter()
            .filter_map(|id| {
                let candidate = files.file(*id)?;
                candidate.matches(filter, admission).then_some(candidate)
            })
            .collect()
    }

    pub(crate) fn all_indexed_file_ids(&self) -> Vec<FileId> {
        self.files().map_or_else(Vec::new, Files::all_file_ids)
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
