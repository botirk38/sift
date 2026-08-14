#![no_main]

use libfuzzer_sys::fuzz_target;
use sift_core::candidates::{Scan, ScanScope, SnapshotFreshness};
use sift_core::search::{
    Events, Query, SearchFlags, SearchInputs, SearchMode, SearchOptions, Searcher, StatsMode,
};
use sift_core::{
    CorpusKind, CorpusMeta, FileFilter, FileFilterConfig, FilterMeta, GramWidth, IndexCoverage,
    IndexRecord, Indexes, Inputs, Plan, StoreMeta, VisibilityConfig, WalkMeta,
};
use std::fs;
use std::sync::OnceLock;

const MAX_PATTERN_LEN: usize = 512;

struct IndexHolder {
    _temp: tempfile::TempDir,
    indexes: Indexes,
    root: std::path::PathBuf,
}

static INDEXES: OnceLock<IndexHolder> = OnceLock::new();

fn indexed() -> &'static IndexHolder {
    INDEXES.get_or_init(|| {
        let tmp = tempfile::tempdir().expect("tempdir");
        let corpus = tmp.path().join("c");
        fs::create_dir_all(&corpus).expect("mkdir");
        fs::write(corpus.join("a.txt"), b"hello world\nfoo bar\n").expect("a.txt");
        fs::write(corpus.join("b.txt"), b"baz\nquux line\n").expect("b.txt");
        let sift_dir = tmp.path().join(".sift");
        let meta = StoreMeta::new(
            CorpusMeta {
                root: corpus.clone(),
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
        );
        let mut indexes = Indexes::open(&sift_dir, &meta).expect("open_index");
        indexes.refresh_meta(&meta).expect("refresh_meta");
        indexes.build().expect("build_index");
        let root = indexes.corpus_root().to_path_buf();
        IndexHolder {
            _temp: tmp,
            indexes,
            root,
        }
    })
}

fn lossy_pattern(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .chars()
        .take(MAX_PATTERN_LEN)
        .collect()
}

fn opts_from_bytes(data: &[u8]) -> SearchOptions {
    let flags = data
        .first()
        .map(|b| SearchFlags::from_bits_truncate(u16::from(*b)))
        .unwrap_or_default();
    let max_results = data.get(1).map(|b| (*b as usize).min(10_000));
    SearchOptions {
        flags,
        max_results,
        ..SearchOptions::default()
    }
}

fn run_search(holder: &IndexHolder, patterns: &[String], opts: &SearchOptions) {
    let Ok(query) = Query::new(patterns.to_vec(), opts.clone()) else {
        return;
    };
    let Ok(searcher) = Searcher::new(query) else {
        return;
    };
    let filter = FileFilter::new(&FileFilterConfig::default(), &holder.root).unwrap();
    let source = Scan::new(
        Some(&holder.indexes),
        &filter,
        None,
        ScanScope::Index {
            order: Default::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let Ok(candidates) =
        Plan::new(&source, searcher.query(), SearchMode::Lines.coverage()).resolve(&source)
    else {
        return;
    };
    let inputs = SearchInputs {
        candidates,
        streams: Inputs::empty(),
        explicit: &[],
    };
    let _ = searcher.execute(inputs, StatsMode::Off, SearchMode::Lines, Events::Discard);
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    let opts = opts_from_bytes(data);
    let holder = indexed();

    let pat1 = lossy_pattern(&data[2..]);
    run_search(holder, &[pat1], &opts);

    if data.len() > 4 {
        let mid = 2 + (data.len() - 2) / 2;
        let p_a = lossy_pattern(&data[2..mid]);
        let p_b = lossy_pattern(&data[mid..]);
        run_search(holder, &[p_a, p_b], &opts);
    }

    let p = lossy_pattern(&data[2..]);
    let _ = compile_with_flags(&[&p], &opts);
    if data.len() > 4 {
        let mid = 2 + (data.len() - 2) / 2;
        let p_a = lossy_pattern(&data[2..mid]);
        let p_b = lossy_pattern(&data[mid..]);
        let _ = compile_with_flags(&[&p_a, &p_b], &opts);
    }
});

fn compile_with_flags(patterns: &[&str], opts: &SearchOptions) -> Result<(), ()> {
    let query = Query::new(
        patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect(),
        opts.clone(),
    )
    .map_err(|_| ())?;
    Searcher::new(query).map(|_| ()).map_err(|_| ())
}
