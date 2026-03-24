//! Fast indexed regex search over codebases — core engine.
//!
//! **Walking:** [`WalkBuilder`] from the [`ignore`] crate (ripgrep-class ignore rules).

mod index;
mod planner;
mod search;
mod storage;
mod verify;

pub use index::{Index, IndexBuilder, QueryPlan};
pub use storage::{lexicon, postings};
pub use verify::{compile_pattern, compile_search_pattern};

pub use planner::TrigramPlan;
pub use search::{walk_file_paths, CompiledSearch, Match, SearchMatchFlags, SearchOptions};

pub use ignore::{Walk, WalkBuilder};

pub use index::trigram::extract_trigrams;

use std::path::PathBuf;

use thiserror::Error;

pub const META_FILENAME: &str = "sift.meta";
pub const FILES_BIN: &str = "files.bin";
pub const LEXICON_BIN: &str = "lexicon.bin";
pub const POSTINGS_BIN: &str = "postings.bin";

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
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();

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
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
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
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
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
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
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
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
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
