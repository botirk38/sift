//! Fast indexed regex search over codebases — core engine.
//!
//! **Walking:** [`WalkBuilder`] from the [`ignore`] crate (ripgrep-class ignore rules).

mod index;
mod planner;
mod prefilter;
mod query;
mod search;
mod storage;
mod verify;

pub use storage::{lexicon, postings};
pub use verify::{compile_pattern, compile_search_pattern};

pub use planner::TrigramPlan;
pub use search::{walk_file_paths, CompiledSearch, Match, SearchMatchFlags, SearchOptions};

/// Re-export for convenience.
pub use ignore::{Walk, WalkBuilder};

pub use index::trigram::extract_trigrams;

use std::path::{Path, PathBuf};

use lexicon::LexiconEntry;
use thiserror::Error;

/// Filename written under the index directory pointing at the corpus root (first line, UTF-8 path).
pub const META_FILENAME: &str = "sift.meta";

/// `files.bin` under the index directory (file id → relative path).
pub const FILES_BIN: &str = "files.bin";
/// `lexicon.bin` — sorted trigram → postings slice.
pub const LEXICON_BIN: &str = "lexicon.bin";
/// `postings.bin` — concatenated little-endian file id payloads.
pub const POSTINGS_BIN: &str = "postings.bin";

/// Errors from index operations and search.
#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ignore walk error: {0}")]
    Ignore(#[from] ignore::Error),

    #[error("regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("search patterns must not be empty")]
    EmptyPatterns,

    #[error("invalid index metadata: {0}")]
    InvalidMeta(PathBuf),

    #[error("index not initialized (missing {0})")]
    MissingMeta(PathBuf),

    #[error("index component missing: {0}")]
    MissingComponent(PathBuf),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Record corpus location, build the trigram index, and write tables under `out_dir`.
///
/// # Errors
///
/// Returns [`Error::Io`] if paths cannot be read or written.
pub fn build_index(path: &Path, out_dir: &Path) -> Result<()> {
    let root = path.canonicalize()?;
    std::fs::create_dir_all(out_dir)?;
    let meta_path = out_dir.join(META_FILENAME);
    let root_display = root.display();
    std::fs::write(&meta_path, format!("{root_display}\n"))?;
    index::build_trigram_index(&root, out_dir)?;
    Ok(())
}

/// Open handle to an index directory (metadata + in-memory trigram tables).
#[derive(Debug)]
pub struct Index {
    /// Corpus root to search.
    pub root: PathBuf,
    dir: PathBuf,
    /// File id → corpus-relative path (same order as `files.bin` on disk).
    pub files: Vec<PathBuf>,
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

impl Index {
    /// Open an index directory produced by [`build_index`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::MissingMeta`] if `sift.meta` is absent, [`Error::MissingComponent`] if a
    /// trigram table file is missing, [`Error::Io`] on read failure, or [`Error::InvalidMeta`] if
    /// metadata is empty or malformed.
    pub fn open(path: &Path) -> Result<Self> {
        let index_dir = path.to_path_buf();
        let meta_path = index_dir.join(META_FILENAME);
        if !meta_path.is_file() {
            return Err(Error::MissingMeta(meta_path));
        }
        let raw = std::fs::read_to_string(&meta_path)?;
        let line = raw
            .lines()
            .next()
            .ok_or_else(|| Error::InvalidMeta(meta_path.clone()))?;
        let root = PathBuf::from(line);
        let paths = [
            index_dir.join(FILES_BIN),
            index_dir.join(LEXICON_BIN),
            index_dir.join(POSTINGS_BIN),
        ];
        for p in &paths {
            if !p.is_file() {
                return Err(Error::MissingComponent(p.clone()));
            }
        }
        let files = index::files::read_files_table(&paths[0])?;
        let lex = lexicon::read_lexicon(&paths[1])?;
        let postings_blob = postings::read_postings(&paths[2])?;
        Ok(Self {
            root,
            dir: index_dir,
            files,
            lexicon: lex,
            postings: postings_blob,
        })
    }

    /// Directory holding `sift.meta` and index files.
    #[must_use]
    pub fn index_dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn explain(&self, pattern: &str) -> QueryPlan {
        let mode =
            match TrigramPlan::for_patterns(&[pattern.to_string()], &SearchOptions::default()) {
                TrigramPlan::FullScan => "full_scan",
                TrigramPlan::Narrow { .. } => "indexed_candidates",
            };
        QueryPlan {
            pattern: pattern.to_string(),
            mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    pub pattern: String,
    pub mode: &'static str,
}

#[cfg(test)]
impl Index {
    pub(crate) fn test_stub(lexicon: Vec<LexiconEntry>, postings: Vec<u8>) -> Self {
        Self {
            root: PathBuf::from("/"),
            dir: PathBuf::from("/"),
            files: Vec::new(),
            lexicon,
            postings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn build_open_search_finds_line() {
        let tmp = std::env::temp_dir().join(format!("sift-core-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join("src/lib.rs"), "fn hello() {\n  let x = 1;\n}\n").unwrap();

        let idx = tmp.join(".index");
        build_index(&tmp, &idx).unwrap();

        let index = Index::open(&idx).unwrap();
        assert!(!index.lexicon.is_empty());
        let pat = vec![r"let\s+x".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.search_index(&index).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].file.ends_with("src/lib.rs"));
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn open_missing_meta_errors() {
        let tmp = std::env::temp_dir().join(format!("sift-missing-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        assert!(matches!(Index::open(&tmp), Err(Error::MissingMeta(_))));
    }

    #[test]
    fn open_missing_table_errors() {
        let tmp = std::env::temp_dir().join(format!("sift-missing-table-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(META_FILENAME), "/tmp/foo\n").unwrap();
        assert!(matches!(Index::open(&tmp), Err(Error::MissingComponent(_))));
    }

    #[test]
    fn open_empty_meta_errors() {
        let tmp = std::env::temp_dir().join(format!("sift-empty-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(META_FILENAME), "").unwrap();
        assert!(matches!(Index::open(&tmp), Err(Error::InvalidMeta(_))));
    }

    #[test]
    fn explain_returns_naive_plan() {
        let tmp = std::env::temp_dir().join(format!("sift-explain-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let idx = tmp.join(".index");
        build_index(&tmp, &idx).unwrap();
        let index = Index::open(&idx).unwrap();
        let plan = index.explain("foo.*");
        assert_eq!(plan.pattern, "foo.*");
        assert_eq!(plan.mode, "full_scan");
    }

    #[test]
    fn indexed_search_matches_naive_for_literal() {
        let tmp = std::env::temp_dir().join(format!("sift-idx-parity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("a")).unwrap();
        fs::create_dir_all(tmp.join("b")).unwrap();
        fs::write(tmp.join("a/x.txt"), "alpha beta\n").unwrap();
        fs::write(tmp.join("b/y.txt"), "gamma delta\n").unwrap();

        let idx = tmp.join(".index");
        build_index(&tmp, &idx).unwrap();
        let index = Index::open(&idx).unwrap();

        let pat = vec!["beta".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let naive = q.search_walk(&tmp, None).unwrap();
        let indexed = q.search_index(&index).unwrap();
        assert_eq!(indexed, naive);
    }

    #[test]
    fn full_scan_parallel_candidate_path_finds_all_files() {
        let tmp = std::env::temp_dir().join(format!("sift-parallel-fs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("d")).unwrap();

        let min_parallel = crate::search::parallel_candidate_min_files();
        let n_files = if min_parallel == usize::MAX {
            3
        } else {
            min_parallel.clamp(2, 64)
        };
        for i in 0..n_files {
            fs::write(
                tmp.join("d").join(format!("f{i}.txt")),
                format!("line {i} needle\n"),
            )
            .unwrap();
        }
        let idx = tmp.join(".index");
        build_index(&tmp, &idx).unwrap();
        let index = Index::open(&idx).unwrap();
        assert_eq!(index.files.len(), n_files);

        let pat = vec!["needle".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let hits = q.search_index(&index).unwrap();
        assert_eq!(hits.len(), n_files);
    }

    #[test]
    fn full_scan_uses_files_bin_same_hits_as_fresh_walk() {
        let tmp = std::env::temp_dir().join(format!("sift-fullscan-parity-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("keep")).unwrap();
        fs::write(tmp.join("keep/a.txt"), "one\ntwo beta\n").unwrap();
        fs::write(tmp.join("keep/b.txt"), "three\n").unwrap();
        fs::write(tmp.join(".ignore"), "ignored\n").unwrap();
        fs::create_dir_all(tmp.join("ignored")).unwrap();
        fs::write(tmp.join("ignored/hidden.txt"), "beta skip\n").unwrap();

        let idx = tmp.join(".index");
        build_index(&tmp, &idx).unwrap();
        let index = Index::open(&idx).unwrap();

        let pat = vec![".*".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let mut from_index = q.search_index(&index).unwrap();
        let mut from_walk = q.search_walk(&tmp, None).unwrap();
        from_index.sort_by(|a, b| (&a.file, a.line, &a.text).cmp(&(&b.file, b.line, &b.text)));
        from_walk.sort_by(|a, b| (&a.file, a.line, &a.text).cmp(&(&b.file, b.line, &b.text)));
        assert_eq!(from_index, from_walk);
    }
}
