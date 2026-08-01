pub mod disk;

use super::artifact::ArtifactData;
use super::identity::SnapshotId;
use super::manifest::SnapshotManifest;

/// A readable snapshot with access to its manifest and named artifacts.
pub trait SnapshotRead {
    fn manifest(&self) -> &SnapshotManifest;

    /// Load a named artifact from the snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace or artifact is missing, or I/O/mmap fails.
    fn artifact(&self, namespace: &str, name: &str) -> crate::Result<ArtifactData>;

    /// List every artifact name stored under `namespace`.
    ///
    /// # Errors
    ///
    /// Returns an error if the namespace cannot be inspected.
    fn artifacts(&self, namespace: &str) -> crate::Result<Vec<String>>;
}

/// A writable snapshot transaction that accepts named byte artifacts.
pub trait SnapshotWrite {
    fn id(&self) -> &SnapshotId;

    /// Store a named artifact in the in-progress snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if the artifact cannot be written.
    fn put_artifact(&mut self, namespace: &str, name: &str, bytes: Vec<u8>) -> crate::Result<()>;
}
