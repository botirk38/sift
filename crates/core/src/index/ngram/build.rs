//! Gram extraction and posting assembly for N-gram indexes.

use super::gram::{Gram, GramNorm, GramWidth};
use super::storage::lexicon::LexiconEntry;
use crate::index::postings::Postings;

use crate::index::Files;

/// Collected index data ready for persistence (kind artifacts only).
pub struct IndexTables {
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

impl IndexTables {
    pub fn assemble(width: GramWidth, norm: GramNorm, files: &Files) -> crate::Result<Self> {
        if width.get() <= 4 {
            Self::assemble_u64(width, norm, files)
        } else {
            Self::assemble_u128(width, norm, files)
        }
    }

    fn assemble_u64(width: GramWidth, norm: GramNorm, files: &Files) -> crate::Result<Self> {
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
                    .map(|gram| (gram.ordinal() << 32) | u64::from(fid))
                    .collect::<Vec<_>>())
            })
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(crate::Error::Io)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        rayon::slice::ParallelSliceMut::par_sort_unstable(&mut pairs[..]);
        let mut postings = Vec::with_capacity(pairs.len());
        let mut lexicon = Vec::new();
        let mut ids = Vec::new();
        let mut i = 0;
        while i < pairs.len() {
            let ordinal = pairs[i] >> 32;
            let start = i;
            while i < pairs.len() && pairs[i] >> 32 == ordinal {
                i += 1;
            }
            let gram = Gram::from_ordinal(width, ordinal)?;
            let offset = u64::try_from(postings.len()).map_err(|_| {
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
            ids.extend(pairs[start..i].iter().map(|pair| {
                u32::try_from(*pair & u64::from(u32::MAX)).expect("masked file id fits in u32")
            }));
            postings.extend_from_slice(&Postings::encode_list(&ids));
            lexicon.push(LexiconEntry { gram, offset, len });
        }
        Ok(Self { lexicon, postings })
    }

    fn assemble_u128(width: GramWidth, norm: GramNorm, files: &Files) -> crate::Result<Self> {
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
                    .map(|gram| (u128::from(gram.ordinal()) << 32) | u128::from(fid))
                    .collect::<Vec<_>>())
            })
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(crate::Error::Io)?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        rayon::slice::ParallelSliceMut::par_sort_unstable(&mut pairs[..]);

        let mut postings = Vec::with_capacity(pairs.len());
        let mut lexicon = Vec::new();
        let mut ids = Vec::new();
        let mut i = 0;
        while i < pairs.len() {
            let ordinal = u64::try_from(pairs[i] >> 32).expect("gram ordinal fits in u64");
            let start = i;
            while i < pairs.len()
                && u64::try_from(pairs[i] >> 32).expect("gram ordinal fits in u64") == ordinal
            {
                i += 1;
            }
            let gram = Gram::from_ordinal(width, ordinal)?;
            let offset = u64::try_from(postings.len()).map_err(|_| {
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
            ids.extend(pairs[start..i].iter().map(|pair| {
                u32::try_from(*pair & u128::from(u32::MAX)).expect("masked file id fits in u32")
            }));
            postings.extend_from_slice(&Postings::encode_list(&ids));
            lexicon.push(LexiconEntry { gram, offset, len });
        }
        Ok(Self { lexicon, postings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Files;
    use crate::index::meta::{CorpusMeta, FilterMeta, IndexCoverage, StoreMeta, WalkMeta};
    use crate::index::ngram::gram::GramWidth;
    use crate::index::record::IndexRecord;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn meta_for(root: PathBuf) -> StoreMeta {
        StoreMeta::new(
            CorpusMeta {
                root,
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
        let snapshot = tmp.path().join("snapshot");
        fs::create_dir(&snapshot).expect("snapshot");
        let files = Files::build(&meta_for(tmp.path().to_path_buf()), &snapshot).expect("files");
        let tables =
            IndexTables::assemble(GramWidth::TRIGRAM, GramNorm::Identity, &files).expect("tables");
        assert!(!tables.lexicon.is_empty());
    }
}
