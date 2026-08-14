//! Concrete on-disk snapshot publish: `CURRENT`, tmp rename, leases, GC.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::IndexError;
use super::record::IndexRecord;

const SNAPSHOTS_DIR: &str = "snapshots";
const CURRENT_FILE: &str = "CURRENT";
const WRITE_LOCK: &str = "write.lock";
const LEASES_DIR: &str = "leases";
const MANIFEST_FORMAT: u32 = 1;

/// Opaque snapshot identity written to `CURRENT`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotId(String);

impl SnapshotId {
    #[must_use]
    pub const fn new(id: String) -> Self {
        Self(id)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub format: u32,
    pub id: SnapshotId,
    pub indexes: Vec<IndexRecord>,
}

impl SnapshotManifest {
    #[must_use]
    pub const fn new(id: SnapshotId, indexes: Vec<IndexRecord>) -> Self {
        Self {
            format: MANIFEST_FORMAT,
            id,
            indexes,
        }
    }

    fn validate(&self) -> Result<(), String> {
        if self.format != MANIFEST_FORMAT {
            return Err(format!(
                "unsupported snapshot manifest format {}; expected {MANIFEST_FORMAT}",
                self.format
            ));
        }
        Ok(())
    }
}

/// Disk-backed snapshot store.
pub struct DiskStore {
    dir: PathBuf,
    current_id: Option<SnapshotId>,
}

impl DiskStore {
    /// # Errors
    ///
    /// Returns an error if `CURRENT` exists but cannot be read.
    pub fn open(dir: &Path) -> crate::Result<Self> {
        let current_path = dir.join(CURRENT_FILE);
        let current_id = if current_path.exists() {
            Some(SnapshotId::new(Self::read_current(&current_path)?))
        } else {
            None
        };
        Ok(Self {
            dir: dir.to_path_buf(),
            current_id,
        })
    }

    /// # Errors
    ///
    /// Returns an error if `CURRENT` exists but cannot be read.
    pub fn read_current_id(dir: &Path) -> crate::Result<Option<String>> {
        let current_path = dir.join(CURRENT_FILE);
        if current_path.exists() {
            Self::read_current(&current_path).map(Some)
        } else {
            Ok(None)
        }
    }

    fn snapshots_dir(&self) -> PathBuf {
        self.dir.join(SNAPSHOTS_DIR)
    }

    fn current_path(&self) -> PathBuf {
        self.dir.join(CURRENT_FILE)
    }

    fn read_current(path: &Path) -> crate::Result<String> {
        let raw = std::fs::read_to_string(path)?;
        Ok(raw.trim().to_string())
    }

    fn write_atomic(path: &Path, contents: &str) -> crate::Result<()> {
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    fn generate_id() -> String {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{:010x}-{:08x}", d.as_secs(), d.subsec_nanos())
    }

    fn active_lease_ids(dir: &Path) -> crate::Result<Vec<String>> {
        let leases_dir = dir.join(LEASES_DIR);
        let Ok(entries) = std::fs::read_dir(&leases_dir) else {
            return Ok(Vec::new());
        };
        let stale_threshold = std::time::Duration::from_hours(1);
        let mut ids = Vec::new();
        for entry in entries {
            let entry = entry?;
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            if let Ok(metadata) = entry.metadata()
                && let Ok(mtime) = metadata.modified()
                && let Ok(age) = mtime.elapsed()
                && age > stale_threshold
            {
                let _ = std::fs::remove_file(entry.path());
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                let id = raw.trim().to_string();
                if !id.is_empty() {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn gc(snapshots_dir: &Path, store_dir: &Path, keep: &[String]) -> crate::Result<()> {
        let Ok(entries) = std::fs::read_dir(snapshots_dir) else {
            return Ok(());
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("tmp-") {
                let _ = std::fs::remove_dir_all(entry.path());
                continue;
            }
            if keep.iter().any(|k| *k == name_str.as_ref()) {
                continue;
            }
            if let Ok(leased) = Self::active_lease_ids(store_dir)
                && leased.iter().any(|id| id == name_str.as_ref())
            {
                continue;
            }
            let _ = std::fs::remove_dir_all(entry.path());
        }
        Ok(())
    }

    /// Acquire exclusive write access.
    ///
    /// # Errors
    ///
    /// Returns an error if the write lock cannot be acquired.
    pub fn writer(&mut self) -> crate::Result<DiskWriter<'_>> {
        let lock_path = self.dir.join(WRITE_LOCK);
        let mut lock = fslock::LockFile::open(&lock_path)?;
        lock.lock()?;
        Ok(DiskWriter { store: self, lock })
    }
}

/// Writer session holding the exclusive store lock.
pub struct DiskWriter<'a> {
    store: &'a mut DiskStore,
    lock: fslock::LockFile,
}

impl Drop for DiskWriter<'_> {
    fn drop(&mut self) {
        let _ = &mut self.lock;
    }
}

impl DiskWriter<'_> {
    /// Begin a new in-progress snapshot directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the temp directory cannot be created.
    pub fn begin(&self) -> crate::Result<SnapshotTxn> {
        let snapshots_dir = self.store.snapshots_dir();
        std::fs::create_dir_all(&snapshots_dir)?;
        let id_str = DiskStore::generate_id();
        let id = SnapshotId::new(id_str);
        let tmp_dir = snapshots_dir.join(format!("tmp-{}", id.as_str()));
        std::fs::create_dir_all(&tmp_dir)?;
        Ok(SnapshotTxn {
            id,
            dir: tmp_dir,
            committed: false,
        })
    }

    /// Commit `txn` and point `CURRENT` at it.
    ///
    /// # Errors
    ///
    /// Returns an error if publishing fails.
    pub fn publish(
        &mut self,
        mut txn: SnapshotTxn,
        manifest: &SnapshotManifest,
    ) -> crate::Result<SnapshotId> {
        manifest.validate().map_err(|msg| {
            crate::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
        })?;
        let snapshots_dir = self.store.snapshots_dir();
        let manifest_json = serde_json::to_vec_pretty(manifest).map_err(|e| {
            crate::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        std::fs::write(txn.dir.join("manifest.json"), manifest_json)?;

        let final_dir = snapshots_dir.join(txn.id.as_str());
        std::fs::rename(&txn.dir, &final_dir)?;
        txn.committed = true;

        DiskStore::write_atomic(&self.store.current_path(), txn.id.as_str())?;

        let old_current = self.store.current_id.replace(txn.id.clone());
        let mut keep: Vec<String> = vec![txn.id.to_string()];
        if let Some(ref old_id) = old_current {
            keep.push(old_id.to_string());
        }
        DiskStore::gc(&snapshots_dir, &self.store.dir, &keep)?;
        Ok(txn.id.clone())
    }
}

/// In-progress snapshot write (temp directory).
pub struct SnapshotTxn {
    id: SnapshotId,
    dir: PathBuf,
    committed: bool,
}

impl SnapshotTxn {
    #[must_use]
    pub const fn id(&self) -> &SnapshotId {
        &self.id
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Namespace directory for a kind (`snapshots/tmp-…/<name>/`).
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub fn namespace_dir(&self, name: &str) -> crate::Result<PathBuf> {
        let dir = self.dir.join(name);
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
}

impl Drop for SnapshotTxn {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// Opened current snapshot directory, soft-pinned by a lease file.
pub struct OpenedSnapshot {
    id: SnapshotId,
    dir: PathBuf,
    manifest: SnapshotManifest,
    lease_path: PathBuf,
}

impl OpenedSnapshot {
    #[must_use]
    pub const fn id(&self) -> &SnapshotId {
        &self.id
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub const fn manifest(&self) -> &SnapshotManifest {
        &self.manifest
    }

    /// Soft pin so GC does not delete a snapshot still held by a reader.
    fn create_lease(sift_dir: &Path, snapshot_id: &str) -> crate::Result<PathBuf> {
        let leases_dir = sift_dir.join(LEASES_DIR);
        std::fs::create_dir_all(&leases_dir)?;
        let pid = std::process::id();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let leaf = format!("{pid}-{now}");
        let lease_path = leases_dir.join(&leaf);
        let tmp_path = leases_dir.join(format!("{leaf}.tmp"));
        std::fs::write(&tmp_path, snapshot_id)?;
        std::fs::rename(&tmp_path, &lease_path)?;
        Ok(lease_path)
    }

    /// Open the current committed snapshot, if any.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest is invalid or the snapshot disappears mid-open.
    pub fn open_current(sift_dir: &Path) -> crate::Result<Option<Self>> {
        for attempt in 0..2 {
            let Some(current_id) = DiskStore::read_current_id(sift_dir)? else {
                return Ok(None);
            };
            let snap_dir = sift_dir.join(SNAPSHOTS_DIR).join(&current_id);
            let lease_path = Self::create_lease(sift_dir, &current_id)?;
            if !snap_dir.exists() {
                let _ = std::fs::remove_file(&lease_path);
                continue;
            }
            let manifest_path = snap_dir.join("manifest.json");
            let manifest_raw = match std::fs::read_to_string(&manifest_path) {
                Ok(raw) => raw,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && attempt == 0 => {
                    let _ = std::fs::remove_file(&lease_path);
                    continue;
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&lease_path);
                    return Err(crate::Error::Io(e));
                }
            };
            let manifest: SnapshotManifest = match serde_json::from_str(&manifest_raw) {
                Ok(m) => m,
                Err(e) => {
                    let _ = std::fs::remove_file(&lease_path);
                    return Err(crate::Error::Index(IndexError::InvalidManifest {
                        path: manifest_path,
                        source: e,
                    }));
                }
            };
            if let Err(msg) = manifest.validate() {
                let _ = std::fs::remove_file(&lease_path);
                return Err(crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    msg,
                )));
            }
            return Ok(Some(Self {
                id: SnapshotId::new(current_id),
                dir: snap_dir,
                manifest,
                lease_path,
            }));
        }
        Err(crate::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "snapshot disappeared during open",
        )))
    }
}

impl Drop for OpenedSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.lease_path);
    }
}
