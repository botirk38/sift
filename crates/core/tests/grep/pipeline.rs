use std::borrow::Cow;
use std::fs;

use sift_core::candidates::{CandidateSource, ScanScope, SnapshotFreshness};
use sift_core::grep::{
    ByteInput, CandidateFilter, CandidateFilterConfig, CandidateOrder, CandidateOrderDirection,
    CandidateOrderKey, Grep, GrepRequest, PathDisplay,
};
use sift_core::search::{
    InputConversion, Inputs, Listing, SearchEvent, SearchMode, SearchOptions, SearchQueryBuilder,
    SearchSink, Searcher, StatsMode,
};
use tempfile::TempDir;

use super::common::{make_parity_corpus, open_indexes};

const fn index_scope(order: CandidateOrder) -> ScanScope {
    ScanScope::Index {
        order,
        freshness: SnapshotFreshness::Current,
    }
}

#[test]
fn grep_finds_match_in_indexed_corpus() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let query = SearchQueryBuilder::new(vec!["beta".to_string()])
        .options(SearchOptions::default())
        .build()
        .expect("query");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );
    let request = GrepRequest {
        query: query.clone(),
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
        mode: SearchMode::Lines,
        stats: StatsMode::Off,
    };
    let grep = Grep::new(source);
    let candidates = grep.resolve_candidates(&request).expect("candidates");
    let searcher = Searcher::new(query).expect("searcher");
    let inputs = sift_core::search::SearchInputs {
        candidates,
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
    };

    let report = searcher.search(inputs, StatsMode::Off).expect("grep run");
    assert!(report.found());
}

#[test]
fn candidate_planner_all_indexed_uses_index_when_metadata_missing() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let query = SearchQueryBuilder::new(vec!["alpha|beta|gamma|delta".to_string()])
        .options(SearchOptions::default())
        .build()
        .expect("query");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );
    let request = GrepRequest {
        query,
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
        mode: SearchMode::Lines,
        stats: StatsMode::Off,
    };

    let grep = Grep::new(source);
    let candidates = grep.resolve_candidates(&request).expect("candidates");

    assert_eq!(candidates.into_vec().len(), 2);
}

#[test]
fn high_level_grep_search_resolves_candidates_and_reports_matches() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["beta".to_string()])
                .options(SearchOptions::default())
                .build()
                .expect("query"),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::Lines,
            stats: StatsMode::On,
        })
        .expect("grep search");

    assert!(report.found());
    let Listing::Lines(files) = &report.listed else {
        panic!("expected Lines listing");
    };
    assert!(!files.is_empty());
    assert!(files.iter().any(|f| !f.matches.is_empty()));
    assert!(!report.listed.corpus_hit_paths().is_empty());
    assert!(report.stats.is_some());
}

#[test]
fn high_level_grep_stream_emits_events_without_collecting_matches() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );
    let mut sink = EventRecorder::default();

    let report = Grep::new(source)
        .stream(
            GrepRequest {
                query: SearchQueryBuilder::new(vec!["beta".to_string()])
                    .options(SearchOptions::default())
                    .build()
                    .expect("query"),
                streams: Inputs::empty(),
                conversion: InputConversion::new(&[], PathDisplay::Relative, None),
                mode: SearchMode::Lines,
                stats: StatsMode::Off,
            },
            &mut sink,
        )
        .expect("grep stream");

    assert!(report.found());
    let Listing::Lines(files) = &report.listed else {
        panic!("expected Lines listing");
    };
    assert!(files.iter().all(|f| f.matches.is_empty()));
    assert!(sink.matches > 0);
}

#[test]
fn high_level_grep_files_without_match_selects_nonmatching_files() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: CandidateOrder::default(),
        },
    );

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["beta".to_string()])
                .options(SearchOptions::default())
                .build()
                .expect("query"),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::FilesWithoutMatch,
            stats: StatsMode::Off,
        })
        .expect("grep search");

    assert!(report.found());
    let Listing::NonMatchingPaths(files) = &report.listed else {
        panic!("expected NonMatchingPaths");
    };
    assert!(!files.is_empty());
}

#[test]
fn high_level_grep_files_without_match_uses_full_corpus_with_index() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.txt"), "hello\n").expect("write a");
    fs::write(corpus.join("b.txt"), "goodbye\n").expect("write b");

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["hello".to_string()])
                .options(SearchOptions::default())
                .build()
                .expect("query"),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::FilesWithoutMatch,
            stats: StatsMode::Off,
        })
        .expect("grep search");

    assert!(report.found());
    let Listing::NonMatchingPaths(files) = &report.listed else {
        panic!("expected NonMatchingPaths");
    };
    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("b.txt"));
}

#[test]
fn high_level_grep_files_without_match_is_not_selected_when_all_files_match() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["alpha|beta|gamma|delta".to_string()])
                .options(SearchOptions::default())
                .build()
                .expect("query"),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::FilesWithoutMatch,
            stats: StatsMode::Off,
        })
        .expect("grep search");

    assert!(!report.found());
    let Listing::NonMatchingPaths(files) = &report.listed else {
        panic!("expected NonMatchingPaths");
    };
    assert!(files.is_empty());
}

#[derive(Default)]
struct EventRecorder {
    matches: usize,
}

impl SearchSink for EventRecorder {
    fn event(&mut self, event: SearchEvent) -> sift_core::Result<()> {
        if let SearchEvent::Match(event) = event {
            self.matches += 1;
            assert!(!event.bytes.is_empty());
            assert!(event.absolute_byte_offset.is_some());
        }
        Ok(())
    }
}

#[test]
fn grep_finds_match_in_stdin_stream() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");

    let query = SearchQueryBuilder::new(vec!["needle".to_string()])
        .options(SearchOptions::default())
        .build()
        .expect("query");

    let indexes = open_indexes(&tmp.path().join(".sift"));
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(Some(&indexes), &filter, None, ScanScope::StreamsOnly);
    let request = GrepRequest {
        query: query.clone(),
        streams: Inputs::empty(),
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
        mode: SearchMode::Lines,
        stats: StatsMode::Off,
    };
    let grep = Grep::new(source);
    let candidates = grep.resolve_candidates(&request).expect("candidates");
    let searcher = Searcher::new(query).expect("searcher");

    let streams = Inputs::empty().with_stream(ByteInput {
        path: Cow::Borrowed("<stdin>"),
        bytes: Cow::Borrowed(b"hello needle world\n"),
        explicit: false,
    });
    let inputs = sift_core::search::SearchInputs {
        candidates,
        streams,
        conversion: InputConversion::new(&[], PathDisplay::Relative, None),
    };

    let report = searcher.search(inputs, StatsMode::Off).expect("grep run");
    assert!(report.found());
    let Listing::Lines(files) = &report.listed else {
        panic!("expected Lines listing");
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].matches.len(), 1);
    assert!(files[0].matches[0].text.contains("needle"));
}

#[test]
fn count_include_zero_lists_zeros_but_found_requires_hits() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.txt"), "alpha\n").expect("write a");
    fs::write(corpus.join("b.txt"), "beta\n").expect("write b");

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: CandidateOrder::default(),
        },
    );

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["nomatch".to_string()])
                .options(SearchOptions::default())
                .build()
                .expect("query"),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::CountLines {
                zeros: sift_core::ZeroCounts::Include,
            },
            stats: StatsMode::Off,
        })
        .expect("grep search");

    assert!(!report.found());
    let Listing::LineCounts(counts) = &report.listed else {
        panic!("expected LineCounts");
    };
    assert!(!counts.is_empty());
    assert!(counts.iter().all(|c| c.lines == 0));
}

#[test]
fn stream_begin_path_shares_arc_with_listed_file() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(CandidateOrder::default()),
    );
    let mut sink = PathRecorder::default();

    let report = Grep::new(source)
        .stream(
            GrepRequest {
                query: SearchQueryBuilder::new(vec!["beta".to_string()])
                    .options(SearchOptions::default())
                    .build()
                    .expect("query"),
                streams: Inputs::empty(),
                conversion: InputConversion::new(&[], PathDisplay::Relative, None),
                mode: SearchMode::Lines,
                stats: StatsMode::Off,
            },
            &mut sink,
        )
        .expect("grep stream");

    let Listing::Lines(files) = &report.listed else {
        panic!("expected Lines");
    };
    assert!(!files.is_empty());
    assert!(!sink.begin_paths.is_empty());
    assert!(sink.begin_paths.iter().any(|begin| {
        files
            .iter()
            .any(|f| std::sync::Arc::ptr_eq(begin, &f.file.path))
    }));
}

#[derive(Default)]
struct PathRecorder {
    begin_paths: Vec<std::sync::Arc<std::path::Path>>,
}

impl SearchSink for PathRecorder {
    fn event(&mut self, event: SearchEvent) -> sift_core::Result<()> {
        if let SearchEvent::Begin(event) = event {
            self.begin_paths.push(event.path);
        }
        Ok(())
    }
}

#[test]
fn first_match_settles_on_pattern_hit_not_include_zero() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.txt"), "aaa\n").expect("write a");
    fs::write(corpus.join("b.txt"), "bbb\n").expect("write b");
    fs::write(corpus.join("c.txt"), "needle\n").expect("write c");

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = CandidateFilter::new(&CandidateFilterConfig::default(), &corpus).expect("filter");
    let source = CandidateSource::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: CandidateOrder::new(CandidateOrderKey::Path, CandidateOrderDirection::Ascending),
        },
    );

    let options = SearchOptions {
        search_bound: sift_core::SearchBound::FirstMatch,
        ..SearchOptions::default()
    };

    let report = Grep::new(source)
        .search(GrepRequest {
            query: SearchQueryBuilder::new(vec!["needle".to_string()])
                .options(options)
                .build()
                .expect("query")
                .with_narrowing(sift_core::Narrowing::Disabled),
            streams: Inputs::empty(),
            conversion: InputConversion::new(&[], PathDisplay::Relative, None),
            mode: SearchMode::CountLines {
                zeros: sift_core::ZeroCounts::Include,
            },
            stats: StatsMode::On,
        })
        .expect("grep search");

    assert!(report.found());
    let Listing::LineCounts(counts) = &report.listed else {
        panic!("expected LineCounts");
    };
    assert_eq!(counts.len(), 1);
    assert!(counts[0].lines > 0);
    assert!(counts[0].file.path.ends_with("c.txt"));
    let stats = report.stats.as_ref().expect("stats");
    assert!(stats.files_searched >= 1);
    assert!(stats.files_searched <= 3);
}
