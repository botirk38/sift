//! Fast indexed regex search over codebases — core engine.
//!
//! **Walking:** [`WalkBuilder`] from the [`ignore`] crate (ripgrep-class ignore rules).

mod index;
mod planner;
mod search;
pub mod storage;
mod verify;

pub use index::{Index, IndexBuilder, QueryPlan};
pub use storage::{lexicon, postings};
pub use verify::{compile_pattern, compile_search_pattern};

pub use planner::TrigramPlan;
pub use search::{
    walk_file_paths, CompiledSearch, Match, SearchMatchFlags, SearchMode, SearchOptions,
    SearchOutput,
};

pub use ignore::{Walk, WalkBuilder};

pub use index::trigram::extract_trigrams;

use std::path::PathBuf;

use thiserror::Error;

pub const SIFT_DIR: &str = ".sift";
pub const INDEX_SUBDIR: &str = ".index";
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
    Regex(#[from] Box<regex_automata::meta::BuildError>),

    #[error("regex build error: {0}")]
    RegexBuild(String),

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

    fn normalized_path(p: &std::path::Path) -> std::path::PathBuf {
        let s = p.display().to_string();
        #[cfg(windows)]
        let s = s.strip_prefix("\\\\?\\").unwrap_or(&s).to_string();
        #[cfg(target_os = "macos")]
        let s = s.replace("/private", "");
        std::path::PathBuf::from(s)
    }

    #[test]
    fn build_open_search_finds_line() {
        let tmp = std::env::temp_dir().join(format!("sift-core-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("src")).unwrap();
        fs::write(tmp.join("src/lib.rs"), "fn hello() {\n  let x = 1;\n}\n").unwrap();

        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();

        let index = Index::open(&idx).unwrap();
        assert!(index.file_count() > 0);
        let pat = vec![r"let\s+x".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.collect_index_matches(&index).unwrap();
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
        let root_path = std::env::temp_dir().join("sift-test-root");
        let meta = format!(
            r#"{{
  "root": "{}",
  "kind": "directory"
}}"#,
            root_path.display()
        );
        fs::write(tmp.join(META_FILENAME), meta).unwrap();
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
    fn explain_returns_indexed_plan_for_literal_prefix() {
        let tmp = std::env::temp_dir().join(format!("sift-explain-indexed-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("a.txt"), "alpha beta\ngamma delta\n").unwrap();
        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();
        let plan = index.explain("foo.*");
        assert_eq!(plan.pattern, "foo.*");
        assert_eq!(plan.mode, "indexed_candidates");
    }

    #[test]
    fn explain_returns_full_scan_for_true_no_literal() {
        let tmp =
            std::env::temp_dir().join(format!("sift-explain-fullscan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("a.txt"), "alpha beta\ngamma delta\n").unwrap();
        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();
        let plan = index.explain(r"\w{5}\s+\w{5}");
        assert_eq!(plan.pattern, r"\w{5}\s+\w{5}");
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

        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();

        let pat = vec!["beta".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let naive = q.collect_walk_matches(&tmp).unwrap();
        let indexed = q.collect_index_matches(&index).unwrap();
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
        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();
        assert_eq!(index.file_count(), n_files);

        let pat = vec!["needle".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let hits = q.collect_index_matches(&index).unwrap();
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

        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&tmp).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();

        let pat = vec![".*".to_string()];
        let opts = SearchOptions::default();
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let mut from_index = q.collect_index_matches(&index).unwrap();
        let mut from_walk = q.collect_walk_matches(&tmp).unwrap();
        from_index.sort_by(|a, b| (&a.file, a.line, &a.text).cmp(&(&b.file, b.line, &b.text)));
        from_walk.sort_by(|a, b| (&a.file, a.line, &a.text).cmp(&(&b.file, b.line, &b.text)));
        assert_eq!(from_index, from_walk);
    }

    #[test]
    fn build_open_single_file_search_finds_line() {
        let tmp = std::env::temp_dir().join(format!("sift-single-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("one.txt");
        fs::write(&file, "alpha\nbeta needle\n").unwrap();

        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&file).with_dir(&idx).build().unwrap();
        let index = Index::open(&idx).unwrap();

        let expected_root = file.canonicalize().unwrap().parent().unwrap().to_path_buf();
        assert_eq!(
            normalized_path(&index.root),
            normalized_path(&expected_root)
        );
        assert!(matches!(index.corpus_kind, index::CorpusKind::File { .. }));
        assert_eq!(index.file_count(), 1);
        assert_eq!(index.file_path(0).unwrap(), std::path::Path::new("one.txt"));

        let pat = vec!["needle".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.collect_index_matches(&index).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            normalized_path(&hits[0].file),
            normalized_path(&file.canonicalize().unwrap())
        );
        assert_eq!(hits[0].line, 2);
    }

    #[test]
    fn single_file_meta_is_json_with_explicit_kind() {
        let tmp =
            std::env::temp_dir().join(format!("sift-single-file-meta-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("one.txt");
        fs::write(&file, "alpha\n").unwrap();

        let idx = tmp.join(".sift");
        let _ = IndexBuilder::new(&file).with_dir(&idx).build().unwrap();
        let meta = fs::read_to_string(idx.join(META_FILENAME)).unwrap();

        assert!(
            meta.contains("\"kind\": \"file\""),
            "unexpected meta: {meta}"
        );
        assert!(meta.contains("\"entries\""), "unexpected meta: {meta}");
        assert!(meta.contains("\"one.txt\""), "unexpected meta: {meta}");
    }
}
