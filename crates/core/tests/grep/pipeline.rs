use std::borrow::Cow;
use std::fs;

use sift_core::{
    Candidates, FileFilter, FileFilterConfig, FileOrder, FileOrderDirection, FileOrderKey, Hit,
    Input, Inputs, Listing, Mention, Narrowing, Origin, PathDisplay, Plan, Query, Scan, ScanScope,
    SearchEvent, SearchMode, SearchOptions, Searcher, SnapshotFreshness, ZeroCounts,
};
use tempfile::TempDir;

use super::common::{make_parity_corpus, open_indexes};

fn search_inputs(candidates: Candidates<'_>) -> Inputs<'_> {
    candidates
        .into_vec()
        .into_iter()
        .map(|file| Input::from_file(file, &[]))
        .collect()
}

const fn index_scope(order: FileOrder) -> ScanScope {
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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let query = Query::new(vec!["beta".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );
    let candidates = Plan::new(
        &source,
        searcher.query(),
        SearchMode::Print(Hit::Line).coverage(),
    )
    .resolve(&source)
    .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), SearchMode::Print(Hit::Line))
        .expect("grep run");
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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let query = Query::new(
        vec!["alpha|beta|gamma|delta".to_string()],
        SearchOptions::default(),
    )
    .expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let candidates = Plan::new(
        &source,
        searcher.query(),
        SearchMode::Print(Hit::Line).coverage(),
    )
    .resolve(&source)
    .expect("candidates");

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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let query = Query::new(vec!["beta".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::Print(Hit::Line);
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
        .expect("grep search");

    assert!(report.found());
    let Listing::Matches(files) = &report.listed else {
        panic!("expected Matches");
    };
    assert!(!files.is_empty());
    assert!(files.iter().any(|f| !f.matches.is_empty()));
    assert!(!report.listed.corpus_hit_paths().is_empty());
}

#[test]
fn high_level_grep_stream_emits_events_without_collecting_matches() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let query = Query::new(vec!["beta".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::Print(Hit::Line);
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let mut events = searcher.stream(search_inputs(candidates), mode);
    let mut matches = 0usize;
    for event in events.by_ref() {
        if let SearchEvent::Match(event) = event.expect("event") {
            matches += 1;
            assert!(!event.bytes.is_empty());
            assert!(event.absolute_byte_offset.is_some());
        }
    }
    let report = events.into_report();

    assert!(report.found());
    let Listing::Matches(files) = &report.listed else {
        panic!("expected Matches");
    };
    assert!(files.iter().all(|f| f.matches.is_empty()));
    assert!(matches > 0);
}

#[test]
fn high_level_grep_files_without_match_selects_nonmatching_files() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: FileOrder::default(),
        },
    );

    let query = Query::new(vec!["beta".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::FilesWithoutMatch;
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let query = Query::new(vec!["hello".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::FilesWithoutMatch;
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
        .expect("grep search");

    assert!(report.found());
    let Listing::NonMatchingPaths(files) = &report.listed else {
        panic!("expected NonMatchingPaths");
    };
    assert_eq!(files.len(), 1);
    assert!(files[0].display(PathDisplay::Relative).ends_with("b.txt"));
}

#[test]
fn high_level_grep_files_without_match_is_not_selected_when_all_files_match() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let query = Query::new(
        vec!["alpha|beta|gamma|delta".to_string()],
        SearchOptions::default(),
    )
    .expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::FilesWithoutMatch;
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
        .expect("grep search");

    assert!(!report.found());
    let Listing::NonMatchingPaths(files) = &report.listed else {
        panic!("expected NonMatchingPaths");
    };
    assert!(files.is_empty());
}

#[test]
fn grep_finds_match_in_stdin_stream() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");

    let query = Query::new(vec!["needle".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");

    let indexes = open_indexes(&tmp.path().join(".sift"));
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(Some(&indexes), &filter, None, ScanScope::StreamsOnly);
    let candidates = Plan::new(
        &source,
        searcher.query(),
        SearchMode::Print(Hit::Line).coverage(),
    )
    .resolve(&source)
    .expect("candidates");

    let mut inputs = search_inputs(candidates);
    inputs.push(Input::Bytes {
        origin: Origin::stream("<stdin>"),
        bytes: Cow::Borrowed(b"hello needle world\n"),
        mention: Mention::Discovered,
    });

    let report = searcher
        .execute(inputs, SearchMode::Print(Hit::Line))
        .expect("grep run");
    assert!(report.found());
    let Listing::Matches(files) = &report.listed else {
        panic!("expected Matches");
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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: FileOrder::default(),
        },
    );

    let query = Query::new(vec!["nomatch".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::Count {
        hit: Hit::Line,
        zeros: ZeroCounts::Include,
    };
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
        .expect("grep search");

    assert!(!report.found());
    let Listing::Counts(counts) = &report.listed else {
        panic!("expected Counts");
    };
    assert!(!counts.is_empty());
    assert!(counts.iter().all(|c| c.hits == 0));
}

#[test]
fn stream_begin_path_shares_arc_with_listed_file() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    super::common::build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        index_scope(FileOrder::default()),
    );

    let query = Query::new(vec!["beta".to_string()], SearchOptions::default()).expect("query");
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::Print(Hit::Line);
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let mut events = searcher.stream(search_inputs(candidates), mode);
    let mut begin_identities = Vec::new();
    for event in events.by_ref() {
        if let SearchEvent::Begin(origin) = event.expect("event") {
            begin_identities.push(origin);
        }
    }
    let report = events.into_report();

    let Listing::Matches(files) = &report.listed else {
        panic!("expected Matches");
    };
    assert!(!files.is_empty());
    assert!(!begin_identities.is_empty());
    assert!(
        begin_identities
            .iter()
            .any(|begin| files.iter().any(|f| begin == &f.origin))
    );
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
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Walk {
            order: FileOrder::new(FileOrderKey::Path, FileOrderDirection::Ascending),
        },
    );

    let options = SearchOptions {
        search_bound: sift_core::SearchBound::FirstMatch,
        ..SearchOptions::default()
    };

    let query = Query::new(vec!["needle".to_string()], options)
        .expect("query")
        .with_narrowing(Narrowing::Disabled);
    let searcher = Searcher::new(query).expect("searcher");
    let mode = SearchMode::Count {
        hit: Hit::Line,
        zeros: ZeroCounts::Include,
    };
    let candidates = Plan::new(&source, searcher.query(), mode.coverage())
        .resolve(&source)
        .expect("candidates");

    let report = searcher
        .execute(search_inputs(candidates), mode)
        .expect("grep search");

    assert!(report.found());
    let Listing::Counts(counts) = &report.listed else {
        panic!("expected Counts");
    };
    assert_eq!(counts.len(), 1);
    assert!(counts[0].hits > 0);
    assert!(
        counts[0]
            .origin
            .display(PathDisplay::Relative)
            .ends_with("c.txt")
    );
    assert!(report.stats.files_searched >= 1);
    assert!(report.stats.files_searched <= 3);
}
