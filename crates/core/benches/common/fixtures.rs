//! Shared corpus and index fixtures for sift-core benchmarks.
//!
//! Large search/planner fixtures are sized for monorepo-scale signal (tens of
//! thousands of files, millions of lines) and cached under the Cargo target
//! directory so setup is paid once per machine/scale.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sift_core::{
    CorpusKind, CorpusMeta, Files, FilterMeta, GramNorm, GramWidth, IndexCoverage, IndexRecord,
    NGramIndex, StoreMeta, VisibilityConfig, WalkMeta,
};

/// Dimensions of a synthetic on-disk corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorpusScale {
    pub files: usize,
    pub lines_per_file: usize,
    pub dir_fanout: usize,
}

impl CorpusScale {
    /// Medium corpus for index *build* benches that rematerialize inside `iter`.
    pub const BUILD: Self = Self {
        files: 8_000,
        lines_per_file: 100,
        dir_fanout: 256,
    };

    /// Default search/planner corpus: ~32k files × 160 lines ≈ 5.1M lines.
    /// Order-of-magnitude closer to a mid-size monorepo than the old 8k×100 fixture.
    pub const SEARCH: Self = Self {
        files: 32_000,
        lines_per_file: 160,
        dir_fanout: 512,
    };

    /// Stress scale (~kernel-ish file count): 64k × 200 ≈ 12.8M lines.
    pub const STRESS: Self = Self {
        files: 64_000,
        lines_per_file: 200,
        dir_fanout: 512,
    };

    fn cache_key(self) -> String {
        format!(
            "search-f{}-l{}-d{}",
            self.files, self.lines_per_file, self.dir_fanout
        )
    }

    fn approx_lines(self) -> u64 {
        u64::try_from(self.files)
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(self.lines_per_file).unwrap_or(u64::MAX))
    }
}

/// Resolve scale from `SIFT_BENCH_SCALE` (`ci` | `default` | `stress`).
///
/// - `ci` — [`CorpusScale::BUILD`] (fast CI / smoke)
/// - `default` / unset — [`CorpusScale::SEARCH`]
/// - `stress` — [`CorpusScale::STRESS`]
#[must_use]
pub fn search_scale() -> CorpusScale {
    match env::var("SIFT_BENCH_SCALE")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "ci" | "small" => CorpusScale::BUILD,
        "stress" | "large" => CorpusScale::STRESS,
        _ => CorpusScale::SEARCH,
    }
}

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
            } else if line % 53 == 17 {
                " PME_TURN_OFF "
            } else {
                " xval "
            };
            writeln!(
                f,
                "// {i}:{line} fn sym_{line}(){mid} struct Row{{ id: u32 }} // padding_{i}_{line}"
            )
            .unwrap();
        }
    }
}

pub fn materialize_scale(root: &Path, scale: CorpusScale) {
    materialize_large_corpus(root, scale.files, scale.lines_per_file, scale.dir_fanout);
}

/// Trigram-specialized N-gram tables written directly under `idx_dir`.
pub fn build_index(corpus: &Path, idx_dir: &Path) -> NGramIndex {
    let (root, kind, include_paths) = if corpus.is_file() {
        let parent = corpus.parent().unwrap_or(corpus);
        let filename = corpus.file_name().map(PathBuf::from).unwrap_or_default();
        (parent.to_path_buf(), CorpusKind::SingleFile, vec![filename])
    } else {
        (corpus.to_path_buf(), CorpusKind::Directory, vec![])
    };
    let abs_root = root.canonicalize().unwrap_or(root);
    let meta = StoreMeta::new(
        CorpusMeta {
            root: abs_root,
            kind,
            include_paths,
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
    let files = Files::build(&meta).unwrap();
    NGramIndex::build(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, &files).unwrap();
    NGramIndex::open(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, files.len()).unwrap()
}

pub fn open_index(idx_dir: &Path, file_count: usize) -> NGramIndex {
    NGramIndex::open(GramWidth::TRIGRAM, GramNorm::Identity, idx_dir, file_count).unwrap()
}

fn workspace_target_dir() -> PathBuf {
    if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target")
}

fn ensure_cached_search_fixture(scale: CorpusScale) -> PathBuf {
    let root = workspace_target_dir()
        .join("sift-bench-fixtures")
        .join(scale.cache_key());
    let ready = root.join("READY");
    let corpus = root.join("corpus");
    let idx = root.join(".sift");
    if ready.is_file() && corpus.is_dir() && idx.is_dir() {
        return root;
    }
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&corpus).unwrap();
    eprintln!(
        "sift-bench: materializing {} ({} files × {} lines/file ≈ {:.1}M lines) under {}",
        scale.cache_key(),
        scale.files,
        scale.lines_per_file,
        scale.approx_lines() as f64 / 1_000_000.0,
        root.display()
    );
    materialize_scale(&corpus, scale);
    let index = build_index(&corpus, &idx);
    fs::write(root.join("FILE_COUNT"), index.file_count().to_string()).unwrap();
    fs::write(&ready, b"ok\n").unwrap();
    root
}

/// Paths for the cached search-scale fixture.
#[derive(Debug, Clone)]
pub struct LargeFixturePaths {
    pub corpus: PathBuf,
    pub index_dir: PathBuf,
}

/// Ensure the cached search-scale corpus + index exist; return their paths.
pub fn large_fixture_paths() -> LargeFixturePaths {
    static PATHS: OnceLock<LargeFixturePaths> = OnceLock::new();
    PATHS
        .get_or_init(|| {
            let root = ensure_cached_search_fixture(search_scale());
            LargeFixturePaths {
                corpus: root.join("corpus"),
                index_dir: root.join(".sift"),
            }
        })
        .clone()
}

/// Persistent large corpus + warm trigram index for search/planner benches.
///
/// Returns `(corpus_root, index)`. The fixture lives under
/// `$CARGO_TARGET_DIR/sift-bench-fixtures/` and is reused across runs.
pub fn open_large_index() -> (PathBuf, NGramIndex) {
    let paths = large_fixture_paths();
    let root = paths
        .index_dir
        .parent()
        .expect("index_dir has parent");
    let file_count = fs::read_to_string(root.join("FILE_COUNT"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| {
            // Rebuild FILE_COUNT for fixtures materialized before this field existed.
            let files = Files::build(&StoreMeta::new(
                CorpusMeta {
                    root: paths.corpus.clone(),
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
                    visibility: VisibilityConfig::default(),
                },
                vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
            ))
            .unwrap();
            let n = files.len();
            let _ = fs::write(root.join("FILE_COUNT"), n.to_string());
            n
        });
    let index = open_index(&paths.index_dir, file_count);
    (paths.corpus, index)
}
