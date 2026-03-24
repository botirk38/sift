//! Hot-loop timings (tab-separated `metric` lines) and `cargo flamegraph` target for sift-core.
//!
//! Built only with `--features profile`. Prefer **`./scripts/profile.sh`** from the repo root.
//!
//! Manual: `cargo run --profile profiling -p sift-core --features profile --bin sift-profile -- narrow`
//!
//! Modes: `narrow` | `full_dotstar` | `full_ci` | `build`
//!
//! **Corpus size** (simulate a large codebase):
//! - Default: tiny **parity** fixture (2 files).
//! - **`SIFT_LARGE=1`** — defaults to ~8k files × ~100 lines, spread across 256 `crates/cXXXX/src/` trees.
//! - Or set **`SIFT_CORPUS_FILES=N`** explicitly (enables large layout; use `0` unset via omitting).
//! - **`SIFT_CORPUS_LINES`**, **`SIFT_CORPUS_DIRS`** — tune lines per file and crate fan-out.
//!
//! Other env: **`SIFT_ITERS`** (default lower on large corpora), **`SIFT_LOOP_SECS`** (overrides iters for search; not for `build`).

use std::fs;
use std::hint::black_box;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use sift_core::{
    CompiledSearch, Index, IndexBuilder, SearchMatchFlags, SearchMode, SearchOptions, SearchOutput,
};

#[derive(Clone, Debug)]
enum CorpusKind {
    Parity,
    Large {
        files: usize,
        lines_per_file: usize,
        dir_fanout: usize,
    },
}

fn corpus_kind() -> CorpusKind {
    let large = std::env::var("SIFT_LARGE")
        .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let files = std::env::var("SIFT_CORPUS_FILES")
        .ok()
        .and_then(|s| s.parse().ok());

    if large && files.is_none() {
        return CorpusKind::Large {
            files: 8_000,
            lines_per_file: std::env::var("SIFT_CORPUS_LINES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            dir_fanout: std::env::var("SIFT_CORPUS_DIRS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(256),
        };
    }

    match files {
        None | Some(0) => CorpusKind::Parity,
        Some(n) => CorpusKind::Large {
            files: n,
            lines_per_file: std::env::var("SIFT_CORPUS_LINES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            dir_fanout: std::env::var("SIFT_CORPUS_DIRS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(256),
        },
    }
}

fn make_parity_corpus(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha beta\n").unwrap();
    fs::write(root.join("b/y.txt"), "gamma delta\n").unwrap();
}

/// Monorepo-shaped tree: `crates/cNNNN/src/module_M.rs` with many lines of pseudo-Rust.
fn materialize_large_corpus(root: &Path, files: usize, lines_per_file: usize, dir_fanout: usize) {
    let fanout = dir_fanout.max(1);
    for i in 0..files {
        let c = i % fanout;
        let path = root
            .join("crates")
            .join(format!("c{c:04}"))
            .join("src")
            .join(format!("module_{i}.rs"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = fs::File::create(&path).unwrap();
        for line in 0..lines_per_file {
            // Occasional "beta" for narrow / CI patterns; rest fills trigrams.
            let mid = if line % 47 == 3 {
                " beta "
            } else if line % 91 == 7 {
                " impl Trait "
            } else {
                " xval "
            };
            writeln!(
                f,
                "// {i}:{line} fn sym_{line}(){mid} struct Row{{ id: u32 }}",
            )
            .unwrap();
        }
    }
}

fn make_many_files_corpus(root: &Path, n: usize) {
    for i in 0..n {
        let dir = root.join(format!("d{}", i % 10));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("f{i}.txt")),
            format!("line one line two content {i}\n"),
        )
        .unwrap();
    }
}

fn materialize_search_corpus(root: &Path, kind: &CorpusKind) {
    match kind {
        CorpusKind::Parity => make_parity_corpus(root),
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => materialize_large_corpus(root, *files, *lines_per_file, *dir_fanout),
    }
}

/// `Parity` kind: 2 files for search profiling; 32 small files for `build` mode.
fn materialize_build_corpus(root: &Path, kind: &CorpusKind) {
    match kind {
        CorpusKind::Parity => make_many_files_corpus(root, 32),
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => materialize_large_corpus(root, *files, *lines_per_file, *dir_fanout),
    }
}

fn open_corpus_index(kind: &CorpusKind) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let corpus = tmp.path().join("corpus");

    let t_mat = Instant::now();
    materialize_search_corpus(&corpus, kind);
    let mat_ms = t_mat.elapsed().as_secs_f64() * 1e3;
    println!("metric\tphase_materialize_corpus_ms\t{mat_ms:.3}");

    match kind {
        CorpusKind::Parity => {
            println!("metric\tcorpus_kind\tparity");
            println!("metric\tcorpus_files\t2");
        }
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => {
            println!("metric\tcorpus_kind\tlarge");
            println!("metric\tcorpus_files\t{files}");
            println!("metric\tcorpus_lines_per_file\t{lines_per_file}");
            println!("metric\tcorpus_dir_fanout\t{dir_fanout}");
        }
    }

    let idx = tmp.path().join("idx");
    let t0 = Instant::now();
    let _ = IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;
    let t1 = Instant::now();
    let index = Index::open(&idx).unwrap();
    let open_ms = t1.elapsed().as_secs_f64() * 1e3;
    println!("metric\tphase_build_index_ms\t{build_ms:.3}");
    println!("metric\tphase_open_index_ms\t{open_ms:.3}");
    (tmp, index)
}

enum Loop {
    Timed(Duration),
    Iters(usize),
}

/// Mean nanoseconds per iteration (integer; avoids lossy `usize`/`u128`→`f64` casts).
fn ns_per_iter(elapsed: Duration, iters: usize) -> u128 {
    if iters == 0 {
        return 0;
    }
    let iters_u128 = u128::try_from(iters).unwrap_or(u128::MAX);
    elapsed.as_nanos() / iters_u128
}

const fn default_search_iters(kind: &CorpusKind) -> usize {
    match kind {
        CorpusKind::Parity => 2_000_000,
        CorpusKind::Large { .. } => 5_000,
    }
}

fn loop_config(kind: &CorpusKind) -> Loop {
    if let Ok(s) = std::env::var("SIFT_LOOP_SECS") {
        let secs: u64 = s.parse().unwrap_or(15);
        return Loop::Timed(Duration::from_secs(secs));
    }
    let iters: usize = std::env::var("SIFT_ITERS")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or_else(|| default_search_iters(kind));
    Loop::Iters(iters)
}

fn run_narrow(index: &Index, loop_cfg: &Loop) {
    let opts = SearchOptions::default();
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let t0 = Instant::now();
    let iters = match loop_cfg {
        Loop::Timed(d) => {
            let deadline = Instant::now() + *d;
            let mut n = 0usize;
            while Instant::now() < deadline {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
                n += 1;
            }
            n
        }
        Loop::Iters(n) => {
            for _ in 0..*n {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
            }
            *n
        }
    };
    let elapsed = t0.elapsed();
    let ns = ns_per_iter(elapsed, iters);
    println!("metric\tmode\tnarrow");
    println!("metric\titers\t{iters}");
    println!("metric\ttotal_ms\t{:.3}", elapsed.as_secs_f64() * 1e3);
    println!("metric\tns_per_iter\t{ns}");
}

fn run_full_dotstar(index: &Index, loop_cfg: &Loop) {
    let opts = SearchOptions::default();
    let pat = vec![".*".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let t0 = Instant::now();
    let iters = match loop_cfg {
        Loop::Timed(d) => {
            let deadline = Instant::now() + *d;
            let mut n = 0usize;
            while Instant::now() < deadline {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
                n += 1;
            }
            n
        }
        Loop::Iters(n) => {
            for _ in 0..*n {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
            }
            *n
        }
    };
    let elapsed = t0.elapsed();
    let ns = ns_per_iter(elapsed, iters);
    println!("metric\tmode\tfull_dotstar");
    println!("metric\titers\t{iters}");
    println!("metric\ttotal_ms\t{:.3}", elapsed.as_secs_f64() * 1e3);
    println!("metric\tns_per_iter\t{ns}");
}

fn run_full_ci(index: &Index, loop_cfg: &Loop) {
    let mut flags = SearchMatchFlags::empty();
    flags |= SearchMatchFlags::CASE_INSENSITIVE;
    let opts = SearchOptions {
        flags,
        max_results: None,
    };
    let pat = vec!["beta".to_string()];
    let query = CompiledSearch::new(&pat, opts).unwrap();
    let t0 = Instant::now();
    let iters = match loop_cfg {
        Loop::Timed(d) => {
            let deadline = Instant::now() + *d;
            let mut n = 0usize;
            while Instant::now() < deadline {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
                n += 1;
            }
            n
        }
        Loop::Iters(n) => {
            for _ in 0..*n {
                black_box(
                    query
                        .run_index(
                            index,
                            &[],
                            SearchOutput {
                                mode: SearchMode::Quiet,
                                with_filename: false,
                                line_number: false,
                            },
                        )
                        .unwrap(),
                );
            }
            *n
        }
    };
    let elapsed = t0.elapsed();
    let ns = ns_per_iter(elapsed, iters);
    println!("metric\tmode\tfull_ci");
    println!("metric\titers\t{iters}");
    println!("metric\ttotal_ms\t{:.3}", elapsed.as_secs_f64() * 1e3);
    println!("metric\tns_per_iter\t{ns}");
}

const fn default_build_iters(kind: &CorpusKind) -> usize {
    match kind {
        CorpusKind::Parity => 500,
        CorpusKind::Large { .. } => 2,
    }
}

fn run_build(iters: usize, kind: &CorpusKind) {
    let max_cap = match kind {
        CorpusKind::Parity => 500,
        CorpusKind::Large { .. } => 20,
    };
    let iters = iters.clamp(1, max_cap);
    let t0 = Instant::now();
    for _ in 0..iters {
        let tmp = tempfile::tempdir().unwrap();
        let corpus = tmp.path().join("corpus");
        materialize_build_corpus(&corpus, kind);
        let idx = tmp.path().join("idx");
        let _ = IndexBuilder::new(&corpus).with_dir(&idx).build().unwrap();
    }
    let elapsed = t0.elapsed();
    let ns = ns_per_iter(elapsed, iters);
    match kind {
        CorpusKind::Parity => {
            println!("metric\tcorpus_kind\tparity");
            println!("metric\tcorpus_files\t32");
            println!("metric\tmode\tbuild_small_32files");
        }
        CorpusKind::Large {
            files,
            lines_per_file,
            dir_fanout,
        } => {
            println!("metric\tcorpus_kind\tlarge");
            println!("metric\tcorpus_files\t{files}");
            println!("metric\tcorpus_lines_per_file\t{lines_per_file}");
            println!("metric\tcorpus_dir_fanout\t{dir_fanout}");
            println!("metric\tmode\tbuild_large");
        }
    }
    println!("metric\titers\t{iters}");
    println!("metric\ttotal_ms\t{:.3}", elapsed.as_secs_f64() * 1e3);
    println!("metric\tns_per_iter\t{ns}");
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "narrow".to_string());
    let kind = corpus_kind();

    match mode.as_str() {
        "build" => {
            if std::env::var("SIFT_LOOP_SECS").is_ok() {
                eprintln!("build mode: use SIFT_ITERS (SIFT_LOOP_SECS not supported)");
                std::process::exit(2);
            }
            let iters = std::env::var("SIFT_ITERS")
                .ok()
                .and_then(|x| x.parse().ok())
                .unwrap_or_else(|| default_build_iters(&kind));
            run_build(iters, &kind);
        }
        "narrow" | "full_dotstar" | "full_ci" => {
            let (_tmp, index) = open_corpus_index(&kind);
            let loop_cfg = loop_config(&kind);
            match mode.as_str() {
                "narrow" => run_narrow(&index, &loop_cfg),
                "full_dotstar" => run_full_dotstar(&index, &loop_cfg),
                "full_ci" => run_full_ci(&index, &loop_cfg),
                _ => unreachable!(),
            }
        }
        _ => {
            eprintln!("usage: sift-profile [narrow|full_dotstar|full_ci|build]");
            std::process::exit(2);
        }
    }
}
