use crate::index::snapshot::SnapshotWrite;

/// Where index artifacts are written to.
pub enum IndexDestination<'a> {
    /// Write to a directory on disk.
    Directory(&'a std::path::Path),
    /// Write into a snapshot transaction.
    Snapshot {
        writer: &'a mut dyn SnapshotWrite,
        namespace: &'a str,
    },
}
