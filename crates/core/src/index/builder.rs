//! Walk corpus, extract trigrams, build in-memory index tables.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use memmap2::Mmap;
use rayon::prelude::*;

use crate::index::{trigram::extract_unique_trigrams_utf8_lossy, CorpusKind};
use crate::search::parallel_candidate_min_files;
use crate::storage::lexicon::LexiconEntry;
use crate::storage::mmap::open_mmap;

pub struct IndexTables {
    pub files: Vec<PathBuf>,
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

fn collect_paths(root: &Path) -> crate::Result<(CorpusKind, Vec<PathBuf>)> {
    if root.is_file() {
        let Some(name) = root.file_name() else {
            return Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "single-file corpus must have a file name",
            )));
        };
        let entry = PathBuf::from(name);
        return Ok((
            CorpusKind::File {
                entries: vec![entry.clone()],
            },
            vec![entry],
        ));
    }

    let mut paths: Vec<PathBuf> = Vec::new();
    let walker = WalkBuilder::new(root)
        .follow_links(false)
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_global(false)
        .git_ignore(false)
        .git_exclude(false)
        .require_git(false)
        .build();
    for entry in walker {
        let entry = entry.map_err(crate::Error::Ignore)?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let display = path.strip_prefix(root).unwrap_or(path).to_path_buf();
        paths.push(display);
    }
    Ok((CorpusKind::Directory, paths))
}

fn open_corpus_bytes(path: &Path) -> crate::Result<Mmap> {
    open_mmap(path).map_err(crate::Error::Io)
}

fn unique_trigrams_for_file(path: &Path) -> crate::Result<Vec<[u8; 3]>> {
    let mmap = open_corpus_bytes(path)?;
    let mut tris: Vec<[u8; 3]> = extract_unique_trigrams_utf8_lossy(mmap.as_ref())
        .into_iter()
        .collect();
    tris.sort_unstable();
    Ok(tris)
}

fn actual_path(root: &Path, corpus_kind: &CorpusKind, display: &Path) -> PathBuf {
    match corpus_kind {
        CorpusKind::Directory => root.join(display),
        CorpusKind::File { .. } => root.to_path_buf(),
    }
}

pub fn build_index_tables(root: &Path) -> crate::Result<(CorpusKind, IndexTables)> {
    let (corpus_kind, mut paths) = collect_paths(root)?;
    paths.sort_unstable();

    let min_parallel = parallel_candidate_min_files();
    let per_file: Vec<(PathBuf, Vec<[u8; 3]>)> = if paths.len() >= min_parallel {
        paths
            .par_iter()
            .map(|display| {
                let path = actual_path(root, &corpus_kind, display);
                unique_trigrams_for_file(&path).map(|tris| (display.clone(), tris))
            })
            .collect::<crate::Result<Vec<_>>>()?
    } else {
        paths
            .iter()
            .map(|display| {
                let path = actual_path(root, &corpus_kind, display);
                unique_trigrams_for_file(&path).map(|tris| (display.clone(), tris))
            })
            .collect::<crate::Result<Vec<_>>>()?
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

    Ok((
        corpus_kind,
        IndexTables {
            files: rel_paths,
            lexicon: lex_entries,
            postings: posting_bytes,
        },
    ))
}
