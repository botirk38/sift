//! Shared fixtures and helpers for sift-core benchmarks.
//!
//! All fixtures are deterministic and use temporary directories that are
//! automatically cleaned up. Search/open/candidate benches build fixtures
//! outside `b.iter`; build benches materialize inside `b.iter`.
//!
//! Only functions used by more than one bench binary live here.
//! Bench-specific helpers live in the bench file itself so no binary
//! compiles dead code.

use std::fs;
use std::io::Write;
use std::path::Path;

use sift_core::{
    CorpusMeta, FilterMeta, GramWidth, IndexCoverage, IndexRecord, Indexes, StoreMeta,
    VisibilityConfig, WalkMeta,
};

// ─── Corpus materializers ────────────────────────────────────────────────────

pub fn make_filter_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("a/.secret")).unwrap();
    fs::create_dir_all(root.join("subdir")).unwrap();
    fs::create_dir_all(root.join("skip")).unwrap();
    fs::create_dir_all(root.join("also_skip")).unwrap();

    fs::write(root.join("a/x.txt"), "alpha beta gamma\n").unwrap();
    fs::write(root.join("a/.hidden.txt"), "beta in hidden file\n").unwrap();
    fs::write(root.join("a/data.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("a/.secret/log"), "beta in hidden dir\n").unwrap();
    fs::write(root.join("subdir/a.txt"), "beta in subdir\n").unwrap();
    fs::write(root.join("subdir/b.log"), "no match here\n").unwrap();
    fs::write(root.join("root.txt"), "beta at root level\n").unwrap();
    fs::write(root.join("skip/ignored.txt"), "beta gitignored\n").unwrap();
    fs::write(root.join("also_skip/omit.txt"), "beta in .ignore\n").unwrap();
    fs::write(root.join("keep.txt"), "beta outside ignore rules\n").unwrap();

    fs::write(root.join(".gitignore"), "skip/**\n").unwrap();
    fs::write(root.join(".ignore"), "also_skip/**\n").unwrap();
}

pub fn materialize_large_corpus(
    root: &Path,
    files: usize,
    lines_per_file: usize,
    dir_fanout: usize,
) {
    let fanout = dir_fanout.max(1);
    for i in 0..files {
        let c = i % fanout;
        let path = root
            .join("crates")
            .join(format!("c{c:04}"))
            .join("src")
            .join(format!("module_{i}.rs"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let f = fs::File::create(&path).unwrap();
        let mut f = std::io::BufWriter::new(f);
        for line in 0..lines_per_file {
            let mid = if line % 47 == 3 {
                " beta "
            } else if line % 91 == 7 {
                " RESUME "
            } else if line % 31 == 11 {
                " ERR_SYS "
            } else {
                " xval "
            };
            writeln!(
                f,
                "// {i}:{line} fn sym_{line}(){mid} struct Row{{ id: u32 }}"
            )
            .unwrap();
        }
    }
}

fn build_index_store(corpus: &Path, sift_dir: &Path) {
    let root = corpus
        .canonicalize()
        .unwrap_or_else(|_| corpus.to_path_buf());
    let meta = StoreMeta::new(
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
            visibility: VisibilityConfig::default(),
        },
        vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
    );
    let mut indexes = Indexes::open(sift_dir, &meta).unwrap();
    indexes.refresh_meta(&meta).unwrap();
    indexes.build().unwrap();
}

pub fn open_large_indexes() -> (tempfile::TempDir, Indexes) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");
    materialize_large_corpus(&corpus, 8_000, 100, 256);
    let sift_dir = tmp.path().join(".sift");
    build_index_store(&corpus, &sift_dir);
    let meta = StoreMeta::read(&sift_dir).unwrap();
    let indexes = Indexes::open(&sift_dir, &meta).unwrap();
    (tmp, indexes)
}
