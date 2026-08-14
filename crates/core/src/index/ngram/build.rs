//! Gram extraction and posting assembly for N-gram indexes.

use super::gram::{Gram, GramNorm, GramWidth};
use super::storage::lexicon::LexiconEntry;
use super::storage::postings::Postings;

use crate::index::Files;

/// Collected index data ready for persistence (kind artifacts only).
pub struct IndexTables {
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

/// Assembles gram → file ID posting lists from per-file gram sets.
pub struct PostingTables {
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

/// Packed `(gram ordinal, file id)` sort key.
trait PostingKey: Copy + Ord + Send {
    fn pack(ordinal: u64, file_id: u32) -> Self;
    fn ordinal(self) -> u64;
    fn file_id(self) -> u32;
}

impl PostingKey for u64 {
    fn pack(ordinal: u64, file_id: u32) -> Self {
        (ordinal << 32) | Self::from(file_id)
    }
    fn ordinal(self) -> u64 {
        self >> 32
    }
    fn file_id(self) -> u32 {
        u32::try_from(self & Self::from(u32::MAX)).expect("masked file id fits in u32")
    }
}

impl PostingKey for u128 {
    fn pack(ordinal: u64, file_id: u32) -> Self {
        (Self::from(ordinal) << 32) | Self::from(file_id)
    }
    fn ordinal(self) -> u64 {
        u64::try_from(self >> 32).expect("gram ordinal fits u64")
    }
    fn file_id(self) -> u32 {
        u32::try_from(self & Self::from(u32::MAX)).expect("masked file id fits in u32")
    }
}

impl PostingTables {
    pub fn assemble(width: GramWidth, norm: GramNorm, files: &Files) -> crate::Result<Self> {
        if width.get() <= 4 {
            Self::assemble_packed::<u64>(width, norm, files)
        } else {
            Self::assemble_packed::<u128>(width, norm, files)
        }
    }

    fn assemble_packed<K: PostingKey>(
        width: GramWidth,
        norm: GramNorm,
        files: &Files,
    ) -> crate::Result<Self> {
        use rayon::prelude::*;

        if files.len() > u32::MAX as usize {
            return Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "too many indexed files",
            )));
        }

        let root = files.root();
        let mut pairs = (0..files.len())
            .into_par_iter()
            .map(|id| {
                let rel = files
                    .rel_path(crate::index::FileId::new(id))
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("missing path for file id {id}"),
                        )
                    })?;
                let bytes = std::fs::read(root.join(rel))?;
                let fid = u32::try_from(id).expect("file count checked above");
                let mut grams = rustc_hash::FxHashSet::with_capacity_and_hasher(
                    bytes.len() / 8,
                    rustc_hash::FxBuildHasher,
                );
                let width_usize = width.get();
                let mut window = [0u8; 8];
                if bytes.len() >= width_usize {
                    for offset in 0..=bytes.len() - width_usize {
                        window[..width_usize].copy_from_slice(&bytes[offset..offset + width_usize]);
                        norm.normalize_window(&mut window[..width_usize]);
                        grams.insert(Gram::from_window(&window[..width_usize]));
                    }
                }
                Ok(grams
                    .into_iter()
                    .map(|gram| K::pack(gram.ordinal(), fid))
                    .collect::<Vec<_>>())
            })
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(crate::Error::Io)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        rayon::slice::ParallelSliceMut::par_sort_unstable(&mut pairs[..]);
        Self::encode_pairs(width, &pairs)
    }

    fn encode_pairs<K: PostingKey>(width: GramWidth, pairs: &[K]) -> crate::Result<Self> {
        let mut posting_bytes = Vec::with_capacity(pairs.len());
        let mut lexicon = Vec::new();
        let mut ids: Vec<u32> = Vec::new();
        let mut i = 0;
        while i < pairs.len() {
            let gram_key = pairs[i].ordinal();
            let start = i;
            while i < pairs.len() && pairs[i].ordinal() == gram_key {
                i += 1;
            }
            let gram = Gram::from_ordinal(width, gram_key)?;
            let offset: u64 = posting_bytes.len().try_into().map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "postings offset overflow",
                ))
            })?;
            let len = u32::try_from(i - start).map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "posting list too long",
                ))
            })?;
            ids.clear();
            ids.extend(pairs[start..i].iter().map(|p| p.file_id()));
            posting_bytes.extend_from_slice(&Postings::encode_list(&ids));
            lexicon.push(LexiconEntry { gram, offset, len });
        }
        Ok(Self {
            lexicon,
            postings: posting_bytes,
        })
    }
}

impl IndexTables {
    /// Build kind tables over the shared [`Files`] identity.
    ///
    /// # Errors
    ///
    /// Returns an error if reading corpus files or encoding postings fails.
    pub fn build(width: GramWidth, norm: GramNorm, files: &Files) -> crate::Result<Self> {
        let tables = PostingTables::assemble(width, norm, files)?;
        Ok(Self {
            lexicon: tables.lexicon,
            postings: tables.postings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::meta::{CorpusMeta, FilterMeta, IndexCoverage, StoreMeta, WalkMeta};
    use crate::index::ngram::gram::GramWidth;
    use crate::index::record::IndexRecord;
    use crate::index::{CorpusKind, Files};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn meta_for(root: PathBuf) -> StoreMeta {
        StoreMeta::new(
            CorpusMeta {
                root,
                kind: CorpusKind::Directory,
                include_paths: Vec::new(),
                exclude_paths: Vec::new(),
            },
            IndexCoverage::Complete,
            WalkMeta {
                follow_links: false,
                one_file_system: false,
                max_depth: None,
                max_filesize: None,
            },
            FilterMeta {
                visibility: crate::corpus::filter::VisibilityConfig {
                    ignore: crate::corpus::filter::IgnoreConfig::disabled(),
                    ..Default::default()
                },
            },
            IndexRecord::default_catalog(),
        )
    }

    #[test]
    fn build_tables_over_shared_files() {
        let tmp = TempDir::new().expect("tmp");
        fs::write(tmp.path().join("a.rs"), b"fn foo() {}").expect("write");
        fs::write(tmp.path().join("b.rs"), b"fn bar() {}").expect("write");
        let files = Files::build(&meta_for(tmp.path().to_path_buf())).expect("files");
        let tables =
            IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &files).expect("tables");
        assert!(!tables.lexicon.is_empty());
    }
}
