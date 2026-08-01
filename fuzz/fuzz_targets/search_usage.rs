#![no_main]

use libfuzzer_sys::fuzz_target;
use sift_core::candidates::{CandidateSource, ScanScope, SnapshotFreshness};
use sift_core::grep::{
    CandidateFilter, CandidateFilterConfig, Grep, GrepRequest, PathDisplay, VisibilityConfig,
};
use sift_core::search::{
    InputConversion, SearchFlags, SearchInputs, SearchOptions, SearchQueryBuilder, Searcher,
    StatsMode,
};
use sift_core::{
    CorpusKind, CorpusMeta, CorpusSpec, FilterMeta, GramWidth, IndexConfig, IndexCoverage,
    IndexRecord, IndexWalkConfig, Indexes, Inputs, NGramIndex, StoreMeta, WalkMeta,
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
        let trigram_dir = sift_dir.join("trigram");
        let config = IndexConfig {
            corpus: CorpusSpec {
                root: &corpus,
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        NGramIndex::new()
            .width(GramWidth::TRIGRAM)
            .build(&config, &trigram_dir, &[])
            .expect("build_index");
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
        let indexes = Indexes::open(&sift_dir, &meta).expect("open_index");
        let root = indexes
            .corpus_root()
            .expect("indexed corpus")
            .to_path_buf();
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
    let Ok(query) = SearchQueryBuilder::new(patterns.to_vec())
        .options(opts.clone())
        .build()
    else {
        return;
    };
    let Ok(searcher) = Searcher::new(query.clone()) else {
        return;
    };
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &holder.root).unwrap();
    let source = CandidateSource::new(
        &holder.indexes,
        &filter,
        None,
        ScanScope::Index {
            order: Default::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let request = GrepRequest {
        query: query.clone(),
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
        mode: sift_core::search::SearchMode::Lines,
        stats: StatsMode::Off,
    };
    let Ok(candidates) = Grep::new(source).resolve_candidates(&request) else {
        return;
    };
    let inputs = SearchInputs {
        candidates,
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
    };
    let _ = searcher.search(inputs, StatsMode::Off);
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
    let query = SearchQueryBuilder::new(
        patterns
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect(),
    )
    .options(opts.clone())
    .build()
    .map_err(|_| ())?;
    Searcher::new(query).map(|_| ()).map_err(|_| ())
}
