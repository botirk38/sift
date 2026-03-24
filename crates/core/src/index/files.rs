//! File table: sequential file id → relative path (UTF-8).
//!
//! Format v2 (SIFTFIL2):
//!   magic (8) | count (4) | offsets[count] (4*count) | blob
//!
//! blob is concatenation of length-prefixed paths:
//!   for each: `path_len` (4) | `path_bytes` (`path_len`)
//!
//! This makes `get(id)` O(1) — two array indexing ops and one slice decode.

use std::path::{Path, PathBuf};

use memmap2::Mmap;

use crate::storage::format::FILES_MAGIC;
use crate::storage::mmap::open_mmap;

#[derive(Debug)]
pub struct MappedFilesView {
    backing: Backing,
    count: usize,
    offset_table_start: usize,
}

#[derive(Debug)]
enum Backing {
    Mmap(Mmap),
    Owned(Vec<u8>),
}

impl MappedFilesView {
    fn bytes(&self) -> &[u8] {
        match &self.backing {
            Backing::Mmap(m) => m.as_ref(),
            Backing::Owned(v) => v.as_slice(),
        }
    }

    pub fn open(path: &Path) -> std::io::Result<Self> {
        let mmap = open_mmap(path)?;
        let bytes = mmap.as_ref();
        let (count, offset_table_start) = Self::validate(bytes)?;
        Ok(Self {
            backing: Backing::Mmap(mmap),
            count,
            offset_table_start,
        })
    }

    pub fn from_paths(paths: &[PathBuf]) -> Self {
        let count = paths.len();
        let offset_table_start = FILES_MAGIC.len() + 4 + count * 4;
        let blob_start = offset_table_start;

        let mut offsets = Vec::<u32>::with_capacity(count);
        let mut blob = Vec::<u8>::new();

        for p in paths {
            let s = p.to_string_lossy();
            let bytes = s.as_bytes();
            let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            let abs_off = u32::try_from(blob_start + blob.len()).unwrap_or(u32::MAX);
            offsets.push(abs_off);
            blob.extend_from_slice(&len.to_le_bytes());
            blob.extend_from_slice(bytes);
        }

        let mut file_bytes = Vec::with_capacity(offset_table_start + blob.len());
        file_bytes.extend_from_slice(&FILES_MAGIC);
        file_bytes.extend_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
        for off in &offsets {
            file_bytes.extend_from_slice(&off.to_le_bytes());
        }
        file_bytes.extend_from_slice(&blob);

        Self {
            backing: Backing::Owned(file_bytes),
            count,
            offset_table_start,
        }
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
        let count =
            u32::from_le_bytes(bytes[magic_len..magic_len + 4].try_into().unwrap()) as usize;
        let offset_table_len = count * 4;
        let offset_table_start = magic_len + 4;
        let blob_start = offset_table_start + offset_table_len;
        if bytes.len() < blob_start {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "files table too short for offset table",
            ));
        }
        for i in 0..count {
            let off = u32::from_le_bytes(
                bytes[offset_table_start + i * 4..offset_table_start + (i + 1) * 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
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
            let path_len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            if off + 4 + path_len > bytes.len() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("offset table[{i}] path extends past end"),
                ));
            }
        }
        Ok((count, offset_table_start))
    }

    pub const fn len(&self) -> usize {
        self.count
    }

    pub fn to_path_bufs(&self) -> std::io::Result<Vec<PathBuf>> {
        let mut out = Vec::with_capacity(self.count);
        let bytes = self.bytes();
        for id in 0..self.count {
            let off = u32::from_le_bytes(
                bytes[self.offset_table_start + id * 4..self.offset_table_start + (id + 1) * 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let path_len = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            let path_start = off + 4;
            let path_end = path_start + path_len;
            let path_bytes = bytes.get(path_start..path_end).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("path {id} extends past files table end"),
                )
            })?;
            let path = std::str::from_utf8(path_bytes).map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("path {id} is not valid UTF-8: {err}"),
                )
            })?;
            out.push(PathBuf::from(path));
        }
        Ok(out)
    }

    pub fn backing_slice(&self) -> &[u8] {
        self.bytes()
    }
}
