use std::{fs, path::Path};

use sift_core::search::CaseMode;
use sift_core::search::SearchOptions;
use sift_core::{
    FileFilter, FileFilterConfig, FileOrder, FilterAdmission, GramNorm, GramWidth, Hit,
    IndexRecord, Indexes, Plan, Query, Scan, ScanScope, SnapshotFreshness,
};
use tempfile::TempDir;

use super::super::common::{
    build_indexes, index_candidates, make_filter_corpus, make_parity_corpus, open_indexes,
    sample_store_meta,
};

#[test]
fn literal_query_returns_indexed_candidates() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let candidates = index_candidates(
        &indexes,
        &corpus,
        &["beta".to_string()],
        SearchOptions::default(),
        FilterAdmission::Full,
    );
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .any(|c| c.rel_path() == Path::new("a/x.txt"))
    );
}

#[test]
fn literal_query_matching_every_file_reports_no_narrowing() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.txt"), "shared beta\n").expect("write a");
    fs::write(corpus.join("b.txt"), "another beta\n").expect("write b");

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let candidates = index_candidates(
        &indexes,
        &corpus,
        &["beta".to_string()],
        SearchOptions::default(),
        FilterAdmission::Full,
    );
    assert_eq!(candidates.len(), 2);
}

#[test]
fn literal_candidates_narrow_to_expected_file() {
    let tmp = TempDir::new().expect("tempdir");
    make_filter_corpus(tmp.path());
    let sift_dir = tmp.path().join(".sift");
    build_indexes(tmp.path(), &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let candidates = index_candidates(
        &indexes,
        tmp.path(),
        &["beta".to_string()],
        SearchOptions::default(),
        FilterAdmission::Full,
    );
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .any(|c| c.rel_path() == Path::new("keep.txt"))
    );
    assert!(!candidates.iter().any(|c| c.rel_path().starts_with("skip")));
}

#[test]
fn identity_only_smart_case_lowercase_scans_all_indexed() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("hit.rs"), "let x = ERR_SYS;\n").expect("write hit");
    fs::write(corpus.join("miss.rs"), "let y = other_symbol;\n").expect("write miss");
    for i in 0..40 {
        fs::write(
            corpus.join(format!("noise{i}.rs")),
            format!("fn noise_{i}() {{ let v = {i}; }}\n"),
        )
        .expect("write noise");
    }

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let pattern = "err_sys".to_string();
    let options = SearchOptions {
        case_mode: CaseMode::Smart,
        ..SearchOptions::default()
    };
    let candidates = index_candidates(
        &indexes,
        &corpus,
        &[pattern],
        options,
        FilterAdmission::Full,
    );
    assert_eq!(candidates.len(), 42);
}

#[test]
fn identity_only_insensitive_scans_all_indexed() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("hit.rs"), "let x = ERR_SYS;\n").expect("write hit");
    fs::write(corpus.join("miss.rs"), "let y = other_symbol;\n").expect("write miss");
    for i in 0..40 {
        fs::write(
            corpus.join(format!("noise{i}.rs")),
            format!("fn noise_{i}() {{ let v = {i}; }}\n"),
        )
        .expect("write noise");
    }

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let pattern = "err_sys|pme_turn_off".to_string();
    let options = SearchOptions {
        case_mode: CaseMode::Insensitive,
        ..SearchOptions::default()
    };
    let candidates = index_candidates(
        &indexes,
        &corpus,
        &[pattern],
        options,
        FilterAdmission::Full,
    );
    assert_eq!(candidates.len(), 42);
}

#[test]
fn identity_only_insensitive_alternation_scans_all_indexed() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.rs"), "ERR_SYS\n").expect("write a");
    fs::write(corpus.join("b.rs"), "PME_TURN_OFF\n").expect("write b");
    fs::write(corpus.join("c.rs"), "LINK_REQ_RST\n").expect("write c");
    fs::write(corpus.join("d.rs"), "CFG_BME_EVT\n").expect("write d");
    for i in 0..80 {
        fs::write(
            corpus.join(format!("noise{i}.rs")),
            format!(
                "fn noise_{i}() {{ let err_code = 1; let cfg_x = 2; let link_y = 3; let pme_z = 4; }}\n"
            ),
        )
        .expect("write noise");
    }

    let sift_dir = tmp.path().join(".sift");
    build_indexes(&corpus, &sift_dir);

    let indexes = open_indexes(&sift_dir);
    let pattern = "ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string();
    let options = SearchOptions {
        case_mode: CaseMode::Insensitive,
        ..SearchOptions::default()
    };
    let candidates = index_candidates(
        &indexes,
        &corpus,
        &[pattern],
        options,
        FilterAdmission::Full,
    );
    assert_eq!(candidates.len(), 84);
}

#[test]
fn default_catalog_casei_alternation_matches_sensitive_count() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("a.rs"), "ERR_SYS\n").expect("write a");
    fs::write(corpus.join("b.rs"), "PME_TURN_OFF\n").expect("write b");
    fs::write(corpus.join("c.rs"), "LINK_REQ_RST\n").expect("write c");
    fs::write(corpus.join("d.rs"), "CFG_BME_EVT\n").expect("write d");
    for i in 0..80 {
        fs::write(
            corpus.join(format!("noise{i}.rs")),
            format!(
                "fn noise_{i}() {{ let err_code = 1; let cfg_x = 2; let link_y = 3; let pme_z = 4; }}\n"
            ),
        )
        .expect("write noise");
    }

    let sift_dir = tmp.path().join(".sift");
    let root = corpus.canonicalize().unwrap_or_else(|_| corpus.clone());
    let meta = sample_store_meta(root, IndexRecord::default_catalog());
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open indexes");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build default catalog");

    let indexes = open_indexes(&sift_dir);
    let pattern = "ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string();
    let insensitive = index_candidates(
        &indexes,
        &corpus,
        std::slice::from_ref(&pattern),
        SearchOptions {
            case_mode: CaseMode::Insensitive,
            ..SearchOptions::default()
        },
        FilterAdmission::Full,
    );
    let sensitive = index_candidates(
        &indexes,
        &corpus,
        &[pattern],
        SearchOptions {
            case_mode: CaseMode::Sensitive,
            ..SearchOptions::default()
        },
        FilterAdmission::Full,
    );
    assert_eq!(insensitive.len(), sensitive.len());
    assert_eq!(sensitive.len(), 4);
}

#[test]
fn dual_indexes_intersect_identity_all_with_ascii_lower_matched() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("create corpus");
    fs::write(corpus.join("hit.rs"), "let x = ERR_SYS;\n").expect("write hit");
    for i in 0..60 {
        fs::write(
            corpus.join(format!("noise{i}.rs")),
            format!(
                "fn noise_{i}() {{ let err=1; let rr_=1; let r_s=1; let _sy=1; let sys=1; \
                 let pme=1; let me_=1; let e_t=1; let _tu=1; let tur=1; let urn=1; let rn_=1; let n_o=1; let _of=1; let off=1; \
                 let lin=1; let ink=1; let nk_=1; let k_r=1; let _re=1; let req=1; let eq_=1; let q_r=1; let _rs=1; let rst=1; \
                 let cfg=1; let fg_=1; let g_b=1; let _bm=1; let bme=1; let me_=2; let e_e=1; let _ev=1; let evt=1; }}\n"
            ),
        )
        .expect("write noise");
    }

    let sift_dir = tmp.path().join(".sift");
    let root = corpus.canonicalize().unwrap_or_else(|_| corpus.clone());
    let records = [
        IndexRecord::ngram(GramWidth::TRIGRAM),
        IndexRecord::ngram_norm(GramWidth::new(5), GramNorm::AsciiLower),
    ];
    let meta = sample_store_meta(root, records.to_vec());
    let mut indexes = Indexes::open(&sift_dir, &meta).expect("open indexes");
    indexes.refresh_meta(&meta).expect("refresh meta");
    indexes.build().expect("build dual indexes");

    let indexes = open_indexes(&sift_dir);
    let pattern = "ERR_SYS|PME_TURN_OFF|LINK_REQ_RST|CFG_BME_EVT".to_string();

    let insensitive = index_candidates(
        &indexes,
        &corpus,
        &[pattern],
        SearchOptions {
            case_mode: CaseMode::Insensitive,
            ..SearchOptions::default()
        },
        FilterAdmission::Full,
    );
    assert_eq!(insensitive.len(), 1);
    assert_eq!(insensitive[0].rel_path(), Path::new("hit.rs"));

    let sensitive = index_candidates(
        &indexes,
        &corpus,
        &["ERR_SYS".to_string()],
        SearchOptions {
            case_mode: CaseMode::Sensitive,
            ..SearchOptions::default()
        },
        FilterAdmission::Full,
    );
    assert_eq!(sensitive.len(), 1);
    assert_eq!(sensitive[0].rel_path(), Path::new("hit.rs"));
}

#[test]
fn corrupt_postings_fail_at_query_not_load() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    make_parity_corpus(&corpus);
    let sift_dir = tmp.path().join(".sift");
    drop(build_indexes(&corpus, &sift_dir));

    let id = fs::read_to_string(sift_dir.join("CURRENT"))
        .expect("CURRENT")
        .trim()
        .to_string();
    let postings = sift_dir
        .join("snapshots")
        .join(id)
        .join("ngram-3")
        .join("postings.bin");
    let mut bytes = fs::read(&postings).expect("read postings");
    assert!(bytes.len() > 12, "postings header plus payload");
    for byte in &mut bytes[12..] {
        *byte = 0xff;
    }
    fs::write(&postings, bytes).expect("corrupt payload");

    let indexes = Indexes::load(&sift_dir)
        .expect("load")
        .expect("store present");
    let filter = FileFilter::new(&FileFilterConfig::default(), &corpus).expect("filter");
    let source = Scan::new(
        Some(&indexes),
        &filter,
        None,
        ScanScope::Index {
            order: FileOrder::default(),
            freshness: SnapshotFreshness::Current,
        },
    );
    let query = Query::new(vec!["beta".into()], SearchOptions::default()).expect("query");
    let searcher = sift_core::Searcher::new(query).expect("searcher");
    let result = Plan::new(
        &source,
        searcher.query(),
        sift_core::SearchMode::Print(Hit::Line).coverage(),
    )
    .resolve(&source);
    assert!(result.is_err(), "corrupt postings fail at query");
}
