//! Walk corpus, extract trigrams, write `files.bin`, `lexicon.bin`, `postings.bin`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::index::files;
use crate::index::trigram::extract_trigrams_utf8_lossy;
use crate::search::parallel_candidate_min_files;
use crate::storage::lexicon::{self, LexiconEntry};
use crate::storage::postings;
use crate::{FILES_BIN, LEXICON_BIN, POSTINGS_BIN};

/// Build trigram tables under `out_dir` for corpus `root` (canonicalized by caller).
///
/// # Errors
///
/// Propagates IO errors from walking, reading files, or writing tables.
pub fn build_trigram_index(root: &Path, out_dir: &Path) -> crate::Result<()> {
    // Walk once to collect corpus-relative paths, then sort for stable file ids (same `files.bin`
    // order as before: lexicographic relative paths). Trigram extraction runs in parallel when
    // there are enough files to amortize Rayon (same threshold as parallel candidate search).
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

    // trigram -> sorted unique file ids
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

    // Serialize postings: concatenated u32 LE; lexicon records offset + len (in u32 count).
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

    files::write_files_table(&out_dir.join(FILES_BIN), &rel_paths)?;
    postings::write_postings(&out_dir.join(POSTINGS_BIN), &posting_bytes)?;
    lexicon::write_lexicon(&out_dir.join(LEXICON_BIN), &lex_entries)?;
    Ok(())
}
