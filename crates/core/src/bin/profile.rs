//! Hot-loop timings (tab-separated `metric` lines) and `cargo flamegraph` target for sift-core.
//!
//! Built only with `--features profile`. Prefer **`./scripts/profile.sh`** from the repo root.
//!
//! Scenarios (each mirrors a real benchsuite case):
//!
//! **Literal (narrowable)**:
//!   `literal`               beta
//!
//! **Word-boundary wrapped (sift currently disables index)**:
//!   `word_literal`          -w beta
//!
//! **Case-insensitive (sift currently disables index)**:
//!   `casei_literal`         -i beta
//!
//! **Mixed regex with required literal (sift falls back, ripgrep narrows)**:
//!   `required_literal`       [A-Z]+_RESUME
//!
//! **Unicode class (no required literal)**:
//!   `unicode_class`          \p{Greek}
//!
//! **No-literal regex (full scan)**:
//!   `no_literal`             \w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}
//!
//! **Literal alternation**:
//!   `alternation`            `ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT`
//!
//! **Case-insensitive alternation**:
//!   `alternation_casei`      -i `ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT`
//!
//! **Whole-line regex**:
//!   `line_regexp`           -x beta
//!
//! **Fixed string**:
//!   `fixed_string`          -F beta.gamma
//!
//! **Smart-case (lowercase, all-literal)**:
//!   `smart_case_lowercase`   -S beta  (all lowercase → case-insensitive)
//!
//! **Smart-case (uppercase, all-literal)**:
//!   `smart_case_uppercase`   -S Beta  (has uppercase → case-sensitive)
//!
//! **Corpus size**:
//!   Default: tiny **parity** fixture (2 files, ~2k iters).
//!   `SIFT_LARGE=1`: ~8k files × 100 lines across 256 crate dirs.
//!   `SIFT_CORPUS_FILES=N`: custom file count.
//!   `SIFT_CORPUS_LINES`, `SIFT_CORPUS_DIRS`: tune lines and fan-out.
//!
//! **Timing control**:
//!   `SIFT_ITERS`: fixed iteration count.
//!   `SIFT_LOOP_SECS`: run until N seconds elapsed (search modes only).

use std::fs;
use std::hint::black_box;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sift_core::{
    CaseMode, CompiledSearch, Index, IndexBuilder, SearchMatchFlags, SearchMode, SearchOptions,
    SearchOutput, TrigramPlan,
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

fn external_corpus_paths() -> Option<(PathBuf, PathBuf)> {
    let corpus = std::env::var_os("SIFT_PROFILE_CORPUS").map(PathBuf::from)?;
    let index = std::env::var_os("SIFT_PROFILE_INDEX").map_or_else(
        || PathBuf::from(format!("{}.sift", corpus.display())),
        PathBuf::from,
    );
    Some((corpus, index))
}

fn open_corpus_index(kind: &CorpusKind) -> (tempfile::TempDir, Index) {
    if let Some((corpus, index_dir)) = external_corpus_paths() {
        let t_open = Instant::now();
        let index = Index::open(&index_dir).unwrap();
        let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
        println!("metric\tcorpus_kind\texternal");
        println!("metric\tcorpus_root\t{}", corpus.display());
        println!("metric\tindex_root\t{}", index_dir.display());
        println!("metric\tphase_open_index_ms\t{open_ms:.3}");
        return (tempfile::tempdir().unwrap(), index);
    }

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

    let idx = tmp.path().join(".sift");
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

#[derive(Clone, Debug)]
struct Scenario {
    name: &'static str,
    patterns: Vec<String>,
    opts: SearchOptions,
}

impl Scenario {
    fn literal() -> Self {
        Self {
            name: "literal",
            patterns: vec!["beta".to_string()],
            opts: SearchOptions::default(),
        }
    }

    fn word_literal() -> Self {
        Self {
            name: "word_literal",
            patterns: vec!["beta".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::WORD_REGEXP,
                case_mode: CaseMode::Sensitive,
                max_results: None,
            },
        }
    }

    fn casei_literal() -> Self {
        Self {
            name: "casei_literal",
            patterns: vec!["beta".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::default(),
                case_mode: CaseMode::Insensitive,
                max_results: None,
            },
        }
    }

    fn required_literal() -> Self {
        Self {
            name: "required_literal",
            patterns: vec!["[A-Z]+_RESUME".to_string()],
            opts: SearchOptions::default(),
        }
    }

    fn unicode_class() -> Self {
        Self {
            name: "unicode_class",
            patterns: vec![r"\p{Greek}".to_string()],
            opts: SearchOptions::default(),
        }
    }

    fn no_literal() -> Self {
        Self {
            name: "no_literal",
            patterns: vec![r"\w{5}\s+\w{5}\s+\w{5}\s+\w{5}\s+\w{5}".to_string()],
            opts: SearchOptions::default(),
        }
    }

    fn alternation() -> Self {
        Self {
            name: "alternation",
            patterns: vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()],
            opts: SearchOptions::default(),
        }
    }

    fn alternation_casei() -> Self {
        Self {
            name: "alternation_casei",
            patterns: vec!["ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::default(),
                case_mode: CaseMode::Insensitive,
                max_results: None,
            },
        }
    }

    fn line_regexp() -> Self {
        Self {
            name: "line_regexp",
            patterns: vec!["beta".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::LINE_REGEXP,
                case_mode: CaseMode::Sensitive,
                max_results: None,
            },
        }
    }

    fn fixed_string() -> Self {
        Self {
            name: "fixed_string",
            patterns: vec!["beta.gamma".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::FIXED_STRINGS,
                case_mode: CaseMode::Sensitive,
                max_results: None,
            },
        }
    }

    fn smart_case_lowercase() -> Self {
        Self {
            name: "smart_case_lowercase",
            patterns: vec!["beta".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::default(),
                case_mode: CaseMode::Smart,
                max_results: None,
            },
        }
    }

    fn smart_case_uppercase() -> Self {
        Self {
            name: "smart_case_uppercase",
            patterns: vec!["Beta".to_string()],
            opts: SearchOptions {
                flags: SearchMatchFlags::default(),
                case_mode: CaseMode::Smart,
                max_results: None,
            },
        }
    }
}

#[allow(clippy::type_complexity)]
const ALL_SCENARIOS: &[(&str, fn() -> Scenario)] = &[
    ("literal", Scenario::literal),
    ("word_literal", Scenario::word_literal),
    ("casei_literal", Scenario::casei_literal),
    ("required_literal", Scenario::required_literal),
    ("unicode_class", Scenario::unicode_class),
    ("no_literal", Scenario::no_literal),
    ("alternation", Scenario::alternation),
    ("alternation_casei", Scenario::alternation_casei),
    ("line_regexp", Scenario::line_regexp),
    ("fixed_string", Scenario::fixed_string),
    ("smart_case_lowercase", Scenario::smart_case_lowercase),
    ("smart_case_uppercase", Scenario::smart_case_uppercase),
];

fn find_scenario(name: &str) -> Option<Scenario> {
    for (n, f) in ALL_SCENARIOS {
        if *n == name {
            return Some(f());
        }
    }
    None
}

fn run_scenario(index: &Index, scenario: &Scenario, loop_cfg: &Loop) {
    let t_plan = Instant::now();
    let query = CompiledSearch::new(&scenario.patterns, scenario.opts).unwrap();
    let plan_us = t_plan.elapsed().as_micros();

    let plan_kind = match &query.plan {
        TrigramPlan::Narrow { .. } => "narrow",
        TrigramPlan::FullScan => "full_scan",
    };

    let total_files = index.file_count();

    let t_candidates = Instant::now();
    let candidate_ids = query.candidate_file_ids(index, &[], None, false);
    let candidates_us = t_candidates.elapsed().as_micros();
    let candidate_count = candidate_ids.len();

    let t_matcher = Instant::now();
    let _matcher = query.build_matcher().unwrap();
    let matcher_us = t_matcher.elapsed().as_micros();

    let t_search = Instant::now();
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
                            None,
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
                            None,
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
    let search_elapsed = t_search.elapsed();
    let search_us = search_elapsed.as_micros();
    let ns = ns_per_iter(search_elapsed, iters);

    println!("metric\tscenario\t{}", scenario.name);
    println!("metric\titers\t{iters}");
    println!("metric\tplan_kind\t{plan_kind}");
    println!("metric\ttotal_files\t{total_files}");
    println!("metric\tcandidate_files\t{candidate_count}");
    println!("metric\tphase_plan_us\t{plan_us}");
    println!("metric\tphase_candidate_us\t{candidates_us}");
    println!("metric\tphase_matcher_us\t{matcher_us}");
    println!("metric\tphase_search_us\t{search_us}");
    println!(
        "metric\ttotal_ms\t{:.3}",
        search_elapsed.as_secs_f64() * 1e3
    );
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
        let idx = tmp.path().join(".sift");
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

fn list_scenarios() {
    for (name, _) in ALL_SCENARIOS {
        println!("{name}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map_or("literal", |s| s.as_str());
    let kind = corpus_kind();

    if mode == "list" {
        list_scenarios();
        return;
    }

    if mode == "build" {
        if std::env::var("SIFT_LOOP_SECS").is_ok() {
            eprintln!("build mode: use SIFT_ITERS (SIFT_LOOP_SECS not supported)");
            std::process::exit(2);
        }
        let iters: usize = std::env::var("SIFT_ITERS")
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or_else(|| default_build_iters(&kind));
        run_build(iters, &kind);
        return;
    }

    let scenario = find_scenario(mode).unwrap_or_else(|| {
        eprintln!(
            "usage: sift-profile [list|build|{}]",
            ALL_SCENARIOS
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join("|")
        );
        std::process::exit(2);
    });

    let (_tmp, index) = open_corpus_index(&kind);
    let loop_cfg = loop_config(&kind);
    run_scenario(&index, &scenario, &loop_cfg);
}
