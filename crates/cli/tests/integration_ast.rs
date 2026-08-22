//! `--index ast` lifecycle: building and rebuilding the AST namespace.

mod common;

use std::fs;
use std::path::Path;

use common::{TestProject, assert_exit_code, assert_stdout_contains};

fn write_corpus(p: &TestProject) {
    p.write("main.rs", "fn main() { needle_alpha(); }\n");
    p.write("app.py", "def needle_alpha():\n    return 1\n");
    p.write("notes.md", "needle_alpha in prose\n");
}

fn current_snapshot_dir(p: &TestProject) -> std::path::PathBuf {
    let current = fs::read_to_string(p.root().join(".sift/CURRENT")).expect("read CURRENT");
    p.root().join(".sift/snapshots").join(current.trim())
}

#[test]
fn index_build_with_ast_writes_namespace_and_search_works() {
    let p = TestProject::new("ast-build");
    write_corpus(&p);
    p.build_index_with(
        Path::new("."),
        false,
        &[],
        &["--index", "ngram", "--index", "ast"],
    );

    let snap = current_snapshot_dir(&p);
    assert!(snap.join("files.bin").is_file());
    assert!(snap.join("ngram-3").join("postings.bin").is_file());
    assert!(snap.join("ast").join("kinds.bin").is_file());
    assert!(snap.join("ast").join("postings.bin").is_file());
    assert!(!snap.join("ast").join("files.bin").exists());

    // Regex search over the dual-kind store is unchanged: the AST kind
    // returns every shared id, so the intersection is a no-op.
    let out = p.index_output(["needle_alpha"]);
    assert_exit_code(&out, 0);
    assert_stdout_contains(&out, "main.rs");
    assert_stdout_contains(&out, "app.py");
    assert_stdout_contains(&out, "notes.md");
}

#[test]
fn index_update_rebuilds_with_ast_namespace() {
    let p = TestProject::new("ast-update");
    write_corpus(&p);
    p.build_index();

    let before = current_snapshot_dir(&p);
    assert!(!before.join("ast").exists());

    let out = p.index_output([
        "index", "update", "--wait", "--index", "ngram", "--index", "ast", ".",
    ]);
    assert_exit_code(&out, 0);

    let after = current_snapshot_dir(&p);
    assert!(after.join("ast").join("kinds.bin").is_file());
    assert!(after.join("ast").join("postings.bin").is_file());

    let out = p.index_output(["needle_alpha"]);
    assert_exit_code(&out, 0);
    assert_stdout_contains(&out, "main.rs");
}

#[test]
fn ast_kind_rejects_width_and_norm() {
    let p = TestProject::new("ast-rejects-knobs");
    write_corpus(&p);

    let out = p.index_output([
        "index", "build", "--wait", "--index", "ast", "--width", "3", ".",
    ]);
    assert_exit_code(&out, 2);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--width"), "stderr: {stderr}");

    let out = p.index_output([
        "index",
        "build",
        "--wait",
        "--index",
        "ast",
        "--norm",
        "ascii-lower",
        ".",
    ]);
    assert_exit_code(&out, 2);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--norm"), "stderr: {stderr}");
}

#[test]
fn unknown_index_kind_names_the_expected_set() {
    let p = TestProject::new("ast-unknown-kind");
    write_corpus(&p);

    let out = p.index_output(["index", "build", "--wait", "--index", "bogus", "."]);
    assert_exit_code(&out, 2);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("possible values: ngram, ast"),
        "stderr: {stderr}"
    );
}

#[test]
fn ast_only_store_serves_regex_search_via_full_fallback() {
    let p = TestProject::new("ast-only");
    write_corpus(&p);
    p.build_index_with(Path::new("."), false, &[], &["--index", "ast"]);

    let snap = current_snapshot_dir(&p);
    assert!(snap.join("ast").join("kinds.bin").is_file());
    assert!(!snap.join("ngram-3").exists());

    // The single-kind store cannot narrow yet; every query falls back to the
    // full shared id space and still returns correct results.
    let out = p.index_output(["needle_alpha"]);
    assert_exit_code(&out, 0);
    assert_stdout_contains(&out, "main.rs");
    assert_stdout_contains(&out, "notes.md");
}
