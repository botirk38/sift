//! Snapshot-owned shared [`FileId`] → path table (`files.bin`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::corpus::File;
use crate::corpus::walk::{FileWalk, LinkTraversal, WalkFile, WalkSelector};
use crate::index::FileId;
use crate::index::meta::StoreMeta;
use crate::index::mmap::mmap_open;

enum TableBytes {
    Owned(Arc<[u8]>),
    Mmap(memmap2::Mmap),
}

impl std::fmt::Debug for TableBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Owned(bytes) => f.debug_tuple("Owned").field(&bytes.len()).finish(),
            Self::Mmap(mmap) => f.debug_tuple("Mmap").field(&mmap.len()).finish(),
        }
    }
}

impl AsRef<[u8]> for TableBytes {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Owned(bytes) => bytes,
            Self::Mmap(mmap) => mmap.as_ref(),
        }
    }
}

/// On-disk artifact name for the shared file table.
pub const FILES_BIN: &str = "files.bin";

const FILES_MAGIC: [u8; 8] = *b"SIFTFIL2";

/// Shared `FileId` space for a committed snapshot.
#[derive(Debug)]
pub struct Files {
    root: PathBuf,
    table: FileTable,
    paths: OnceLock<HashSet<PathBuf>>,
}

impl Files {
    /// Walk the corpus described by `meta` and build an in-memory table.
    ///
    /// # Errors
    ///
    /// Returns an error if walking or stating files fails.
    pub fn build(meta: &StoreMeta) -> crate::Result<Self> {
        let root = meta.corpus.root.canonicalize().map_err(|source| {
            crate::Error::Index(crate::index::IndexError::Io {
                path: meta.corpus.root.clone(),
                source,
            })
        })?;
        let paths = walk_paths(meta)?;
        let rows = collect_rows(&root, &paths)?;
        let bytes = FileTable::encode(&rows).map_err(crate::Error::Io)?;
        let table = FileTable::from_bytes(bytes).map_err(crate::Error::Io)?;
        Ok(Self {
            root,
            table,
            paths: OnceLock::new(),
        })
    }

    /// Open `files.bin` from a snapshot directory.
    ///
    /// # Errors
    ///
    /// Returns an error if the table is missing or malformed.
    pub fn open(snapshot_dir: &Path, root: PathBuf) -> crate::Result<Self> {
        let table = FileTable::open(&snapshot_dir.join(FILES_BIN)).map_err(crate::Error::Io)?;
        table.validate_paths().map_err(crate::Error::Io)?;
        Ok(Self {
            root,
            table,
            paths: OnceLock::new(),
        })
    }

    /// Write `files.bin` into `snapshot_dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding or writing fails.
    pub fn write(&self, snapshot_dir: &Path) -> crate::Result<()> {
        let bytes = self.table.bytes();
        std::fs::write(snapshot_dir.join(FILES_BIN), bytes).map_err(crate::Error::Io)
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.table.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
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

    /// Whether `rel_path` is present in this table.
    #[must_use]
    pub fn contains(&self, rel_path: &Path) -> bool {
        self.path_set().contains(rel_path)
    }

    /// Drop paths already present in this table.
    #[must_use]
    pub fn retain_unindexed(&self, paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
        paths
            .into_iter()
            .filter(|path| !self.contains(path))
            .collect()
    }

    /// Walk selector that keeps only paths absent from this table.
    #[must_use]
    pub const fn unindexed(&self) -> Unindexed<'_> {
        Unindexed { files: self }
    }

    /// Corpus-relative path for `id`, if present.
    #[must_use]
    pub fn rel_path(&self, id: FileId) -> Option<&str> {
        self.table.row(id.get()).ok().map(|row| row.path)
    }

    fn path_set(&self) -> &HashSet<PathBuf> {
        self.paths.get_or_init(|| {
            (0..self.table.len())
                .filter_map(|id| self.table.row(id).ok().map(|row| PathBuf::from(row.path)))
                .collect()
        })
    }
}

/// Walk selector for paths not present in [`Files`].
#[derive(Clone, Copy)]
pub struct Unindexed<'a> {
    files: &'a Files,
}

impl WalkSelector for Unindexed<'_> {
    fn includes(&self, rel_path: &Path) -> bool {
        !self.files.contains(rel_path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRowOwned {
    path: PathBuf,
    size: u64,
}

/// Borrowed file row from the on-disk table.
#[derive(Debug, Clone, Copy)]
struct FileRow<'a> {
    path: &'a str,
    size: u64,
}

struct FileTable {
    data: TableBytes,
    count: usize,
    offset_table_start: usize,
}

impl std::fmt::Debug for FileTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTable")
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

impl FileTable {
    /// Bytes after the path: `size` (8).
    const SIZE_LEN: usize = 8;

    fn encode(rows: &[FileRowOwned]) -> std::io::Result<Vec<u8>> {
        let count = rows.len();
        let offset_table_start = FILES_MAGIC.len() + 4;
        let blob_start = offset_table_start + count * 4;

        let mut offsets = Vec::<u32>::with_capacity(count);
        let mut blob = Vec::<u8>::new();

        for row in rows {
            let path_bytes = row.path.to_string_lossy();
            let path_bytes = path_bytes.as_bytes();
            let path_len = u32::try_from(path_bytes.len()).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "file path exceeds u32::MAX",
                )
            })?;
            let abs_off = u32::try_from(blob_start + blob.len()).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "files blob offset exceeds u32::MAX",
                )
            })?;
            offsets.push(abs_off);
            blob.extend_from_slice(&path_len.to_le_bytes());
            blob.extend_from_slice(path_bytes);
            blob.extend_from_slice(&row.size.to_le_bytes());
        }

        let mut out = Vec::with_capacity(blob_start + blob.len());
        out.extend_from_slice(&FILES_MAGIC);
        let count = u32::try_from(count).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "files count exceeds u32::MAX",
            )
        })?;
        out.extend_from_slice(&count.to_le_bytes());
        for off in &offsets {
            out.extend_from_slice(&off.to_le_bytes());
        }
        out.extend_from_slice(&blob);
        Ok(out)
    }

    fn from_bytes(bytes: Vec<u8>) -> std::io::Result<Self> {
        let (count, offset_table_start) = Self::validate(&bytes)?;
        Ok(Self {
            data: TableBytes::Owned(bytes.into()),
            count,
            offset_table_start,
        })
    }

    fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = mmap_open(path)?;
        let (count, offset_table_start) = Self::validate(mmap.as_ref())?;
        Ok(Self {
            data: TableBytes::Mmap(mmap),
            count,
            offset_table_start,
        })
    }

    fn bytes(&self) -> &[u8] {
        self.data.as_ref()
    }

    const fn len(&self) -> usize {
        self.count
    }

    fn validate(bytes: &[u8]) -> std::io::Result<(usize, usize)> {
        let magic_len = FILES_MAGIC.len();
        if bytes.len() < magic_len + 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "files table too short for magic+count",
            ));
        }
        if bytes[..magic_len] != FILES_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "unexpected files table magic",
            ));
        }
        let count = read_u32_le(bytes, magic_len) as usize;
        let offset_table_start = magic_len + 4;
        let blob_start = offset_table_start + count * 4;
        if bytes.len() < blob_start {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "files table too short for offset table",
            ));
        }
        for i in 0..count {
            let off = read_u32_le(bytes, offset_table_start + i * 4) as usize;
            if off < blob_start {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("offset table[{i}] points before blob start"),
                ));
            }
            if off + 4 > bytes.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("offset table[{i}] path_len prefix extends past end"),
                ));
            }
            let path_len = read_u32_le(bytes, off) as usize;
            let entry_end = off + 4 + path_len + Self::SIZE_LEN;
            if entry_end > bytes.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("offset table[{i}] entry extends past end"),
                ));
            }
        }
        Ok((count, offset_table_start))
    }

    fn row(&self, id: usize) -> std::io::Result<FileRow<'_>> {
        if id >= self.count {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "file id out of range",
            ));
        }
        let bytes = self.bytes();
        let off = read_u32_le(bytes, self.offset_table_start + id * 4) as usize;
        let path_len = read_u32_le(bytes, off) as usize;
        let path_start = off + 4;
        let path_end = path_start + path_len;
        let path = bytes.get(path_start..path_end).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("path {id} extends past files table end"),
            )
        })?;
        let path = std::str::from_utf8(path).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("path {id} is not valid UTF-8: {err}"),
            )
        })?;
        let size = read_u64_le(bytes, path_end);
        Ok(FileRow { path, size })
    }

    fn validate_paths(&self) -> std::io::Result<()> {
        for id in 0..self.count {
            let row = self.row(id)?;
            let path = row.path.as_bytes();
            if path.is_empty()
                || path.starts_with(b"/")
                || path
                    .split(|&b| b == b'/' || b == b'\\')
                    .any(|component| component == b"..")
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid file path in index at id {id}"),
                ));
            }
        }
        Ok(())
    }
}

fn walk_paths(meta: &StoreMeta) -> crate::Result<Vec<PathBuf>> {
    use crate::index::config::CorpusKind;

    match meta.corpus.kind {
        CorpusKind::SingleFile => {
            if meta.corpus.include_paths.is_empty() {
                return Err(crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "SingleFile corpus must specify the file in include_paths",
                )));
            }
            Ok(meta.corpus.include_paths.clone())
        }
        CorpusKind::Directory => {
            let paths = FileWalk::new(&meta.corpus.root)
                .scopes(&meta.corpus.include_paths)
                .excludes(&meta.corpus.exclude_paths)
                .visibility(meta.filters.visibility.clone())
                .links(if meta.walk.follow_links {
                    LinkTraversal::Follow
                } else {
                    LinkTraversal::DoNotFollow
                })
                .one_file_system(meta.walk.one_file_system)
                .max_depth(meta.walk.max_depth)
                .max_filesize(meta.walk.max_filesize)
                .files()?
                .into_iter()
                .map(WalkFile::into_rel_path)
                .collect();
            Ok(paths)
        }
    }
}

fn collect_rows(root: &Path, paths: &[PathBuf]) -> crate::Result<Vec<FileRowOwned>> {
    use rayon::prelude::*;
    paths
        .par_iter()
        .map(|rel| {
            let abs = root.join(rel);
            let meta = std::fs::metadata(&abs)?;
            Ok(FileRowOwned {
                path: rel.clone(),
                size: meta.len(),
            })
        })
        .collect()
}

fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("slice is exactly 4 bytes"),
    )
}

fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("slice is exactly 8 bytes"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn encode_open_round_trip() {
        let rows = vec![
            FileRowOwned {
                path: PathBuf::from("a.txt"),
                size: 42,
            },
            FileRowOwned {
                path: PathBuf::from("sub/b.txt"),
                size: 100,
            },
        ];
        let bytes = FileTable::encode(&rows).expect("encode");
        let table = FileTable::from_bytes(bytes).expect("decode");
        assert_eq!(table.len(), 2);
        assert_eq!(table.row(0).expect("row0").path, "a.txt");
        assert_eq!(table.row(0).expect("row0").size, 42);
        assert_eq!(table.row(1).expect("row1").path, "sub/b.txt");
    }

    #[test]
    fn write_open_on_disk() {
        let tmp = TempDir::new().expect("tmp");
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join("a.txt"), b"hi").expect("write");
        let meta = StoreMeta::new(
            crate::index::meta::CorpusMeta {
                root: root.clone(),
                kind: crate::index::config::CorpusKind::Directory,
                include_paths: Vec::new(),
                exclude_paths: Vec::new(),
            },
            crate::index::meta::IndexCoverage::Complete,
            crate::index::meta::WalkMeta {
                follow_links: false,
                one_file_system: false,
                max_depth: None,
                max_filesize: None,
            },
            crate::index::meta::FilterMeta {
                visibility: crate::corpus::filter::VisibilityConfig::default(),
            },
            crate::index::record::IndexRecord::default_catalog(),
        );
        let files = Files::build(&meta).expect("build");
        let snap = tmp.path().join("snap");
        std::fs::create_dir_all(&snap).expect("snap");
        files.write(&snap).expect("write");
        let opened = Files::open(&snap, root).expect("open");
        assert_eq!(opened.len(), 1);
        assert!(opened.contains(Path::new("a.txt")));
    }
}
