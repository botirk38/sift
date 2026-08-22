use std::fs;
use std::path::Path;

use sift_core::{AstIndex, AstLanguage, Files, IndexRecord};
use tempfile::TempDir;

use crate::common::sample_store_meta;

fn write_polyglot_corpus(root: &Path) {
    fs::create_dir_all(root).expect("mkdir");
    fs::write(root.join("main.rs"), "fn main() { println!(\"hi\"); }\n").expect("write rs");
    fs::write(root.join("app.py"), "def run():\n    return 1\n").expect("write py");
    fs::write(root.join("index.js"), "function run() { return 1; }\n").expect("write js");
    fs::write(
        root.join("lib.ts"),
        "function run(): number { return 1; }\n",
    )
    .expect("write ts");
    fs::write(root.join("App.tsx"), "const x = <div>hi</div>;\n").expect("write tsx");
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc run() int { return 1 }\n",
    )
    .expect("write go");
    fs::write(
        root.join("Main.java"),
        "class Main { int run() { return 1; } }\n",
    )
    .expect("write java");
    fs::write(root.join("README.md"), "no language here\n").expect("write md");
    fs::write(root.join("invalid.rs"), [0xE2u8, 0x28, 0xA1, 0x0A]).expect("write invalid utf-8");
}

/// Build a shared file table over `corpus` and the AST artifacts beside it.
fn build_ast(tmp: &TempDir, corpus: &Path) -> (Files, std::path::PathBuf) {
    let root = corpus.canonicalize().expect("canonicalize");
    let meta = sample_store_meta(
        root,
        vec![IndexRecord::ngram(sift_core::GramWidth::TRIGRAM)],
    );
    let snapshot = tmp.path().join("snapshot");
    fs::create_dir_all(&snapshot).expect("snapshot dir");
    let files = Files::build(&meta, &snapshot).expect("files");
    let ast_dir = tmp.path().join("ast");
    AstIndex::build(&ast_dir, &files).expect("build");
    (files, ast_dir)
}

#[test]
fn build_and_reopen_maps_languages_by_extension() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);
    let (files, ast_dir) = build_ast(&tmp, &corpus);

    let index = AstIndex::open(&ast_dir, files.len()).expect("open");
    assert_eq!(index.file_count(), files.len());
    assert!(index.stale_languages().is_empty());

    let of = |lang: AstLanguage| -> Vec<String> {
        index
            .files_of(lang)
            .into_iter()
            .filter_map(|id| files.rel_path(id).map(str::to_string))
            .collect()
    };
    assert_eq!(of(AstLanguage::Rust), vec!["main.rs".to_string()]);
    assert_eq!(of(AstLanguage::Python), vec!["app.py".to_string()]);
    assert_eq!(of(AstLanguage::JavaScript), vec!["index.js".to_string()]);
    // `.ts` and `.tsx` are distinct languages with distinct grammars.
    assert_eq!(of(AstLanguage::TypeScript), vec!["lib.ts".to_string()]);
    assert_eq!(of(AstLanguage::Tsx), vec!["App.tsx".to_string()]);
    assert_eq!(of(AstLanguage::Go), vec!["main.go".to_string()]);
    assert_eq!(of(AstLanguage::Java), vec!["Main.java".to_string()]);
}

#[test]
fn unsupported_and_non_utf8_files_have_no_language() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);
    let (files, ast_dir) = build_ast(&tmp, &corpus);

    let index = AstIndex::open(&ast_dir, files.len()).expect("open");
    // invalid.rs is not valid UTF-8: present in the shared table, never a
    // structural candidate.
    let rust_files: Vec<String> = index
        .files_of(AstLanguage::Rust)
        .into_iter()
        .filter_map(|id| files.rel_path(id).map(str::to_string))
        .collect();
    assert!(!rust_files.contains(&"invalid.rs".to_string()));
    // README.md has no grammar; the shared table still covers every file even
    // though only some carry a language.
    let with_language: usize = AstLanguage::ALL
        .iter()
        .map(|&lang| index.files_of(lang).len())
        .sum();
    assert!(with_language < files.len());
}

#[test]
fn empty_corpus_builds_and_reopens() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    fs::create_dir_all(&corpus).expect("mkdir");
    let (files, ast_dir) = build_ast(&tmp, &corpus);

    assert_eq!(files.len(), 0);
    // Artifacts must exist even with nothing to index: a namespace with
    // missing artifacts would make the whole snapshot unopenable.
    assert!(ast_dir.join("kinds.bin").is_file());
    assert!(ast_dir.join("postings.bin").is_file());
    let index = AstIndex::open(&ast_dir, 0).expect("open empty");
    assert_eq!(index.file_count(), 0);
}

#[test]
fn ids_are_ascending_and_within_the_shared_space() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);
    let (files, ast_dir) = build_ast(&tmp, &corpus);
    let index = AstIndex::open(&ast_dir, files.len()).expect("open");

    for &lang in &AstLanguage::ALL {
        let ids = index.files_of(lang);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted, "{lang:?} ids must be ascending and unique");
        assert!(ids.iter().all(|id| id.get() < files.len()));
    }
}

#[test]
fn languages_lists_every_compiled_language() {
    let tmp = TempDir::new().expect("tempdir");
    let corpus = tmp.path().join("corpus");
    write_polyglot_corpus(&corpus);
    let (files, ast_dir) = build_ast(&tmp, &corpus);

    let index = AstIndex::open(&ast_dir, files.len()).expect("open");
    assert_eq!(index.languages(), AstLanguage::ALL.to_vec());
}
