//! Walk corpus, extract trigrams, build in-memory index tables.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::index::trigram::extract_trigrams_utf8_lossy;
use crate::search::parallel_candidate_min_files;
use crate::storage::lexicon::LexiconEntry;

pub struct IndexTables {
    pub files: Vec<PathBuf>,
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

pub fn build_index_tables(root: &Path) -> crate::Result<IndexTables> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let walker = WalkBuilder::new(root).follow_links(false).build();
    for entry in walker {
        let entry = entry.map_err(crate::Error::Ignore)?;
        if !entry.path().is_file() {
            continue;
        }
        let path = entry.path();
        let display = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        paths.push(display);
    }
    paths.sort_unstable();

    let min_parallel = parallel_candidate_min_files();
    let per_file: Vec<(PathBuf, Vec<[u8; 3]>)> = if paths.len() >= min_parallel {
        paths
            .par_iter()
            .map(|display| {
                let path = root.join(display);
                let tris = fs::read(&path)
                    .map_or_else(|_| Vec::new(), |bytes| extract_trigrams_utf8_lossy(&bytes));
                (display.clone(), tris)
            })
            .collect()
    } else {
        paths
            .iter()
            .map(|display| {
                let path = root.join(display);
                let tris = fs::read(&path)
                    .map_or_else(|_| Vec::new(), |bytes| extract_trigrams_utf8_lossy(&bytes));
                (display.clone(), tris)
            })
            .collect()
    };
    let rel_paths: Vec<PathBuf> = per_file.iter().map(|(p, _)| p.clone()).collect();

    let mut map: BTreeMap<[u8; 3], Vec<u32>> = BTreeMap::new();

    for (id, (_rel, tris)) in per_file.iter().enumerate() {
        let id_u32: u32 = id.try_into().map_err(|_| {
            crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "too many indexed files",
            ))
        })?;
        for tri in tris {
            map.entry(*tri).or_default().push(id_u32);
        }
    }

    for ids in map.values_mut() {
        ids.sort_unstable();
        ids.dedup();
    }

    let mut posting_bytes: Vec<u8> = Vec::new();
    let mut lex_entries: Vec<LexiconEntry> = Vec::with_capacity(map.len());
    for (tri, ids) in map {
        let offset: u64 = posting_bytes.len().try_into().map_err(|_| {
            crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "postings offset overflow",
            ))
        })?;
        let len: u32 = ids.len().try_into().map_err(|_| {
            crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "posting list too long",
            ))
        })?;
        for fid in &ids {
            posting_bytes.extend_from_slice(&fid.to_le_bytes());
        }
        lex_entries.push(LexiconEntry {
            trigram: tri,
            offset,
            len,
        });
    }

    Ok(IndexTables {
        files: rel_paths,
        lexicon: lex_entries,
        postings: posting_bytes,
    })
}
