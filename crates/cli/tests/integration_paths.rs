mod common;

use std::fs;
use std::path::Path;

use common::{assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn relative_path_scope_limits_matches() {
    let root = fresh_dir("paths-relative-scope");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "ONLY_IN_A\n").unwrap();
    fs::write(root.join("b/y.txt"), "ONLY_IN_B\n").unwrap();
    let idx = root.join(".idx");

    build_index(Some(&root), &idx, Path::new("."));

    let out = command(Some(&root))
        .arg("--index")
        .arg(&idx)
        .arg("ONLY_IN_")
        .arg("a")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("a/x.txt") && stdout.contains("ONLY_IN_A"));
    assert!(!stdout.contains("b/y.txt"), "unexpected stdout: {stdout}");

    let out_both = command(Some(&root))
        .arg("--index")
        .arg(&idx)
        .arg("ONLY_IN_")
        .arg("a")
        .arg("b")
        .output()
        .unwrap();
    assert_success(&out_both);

    let stdout_both = normalized_stdout(&out_both);
    assert!(stdout_both.contains("a/x.txt") && stdout_both.contains("b/y.txt"));
}

#[test]
fn absolute_path_scope_within_corpus_works() {
    let root = fresh_dir("paths-absolute-scope");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "alpha\n").unwrap();
    fs::write(root.join("b/y.txt"), "alpha\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("alpha")
        .arg(root.join("a"))
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("a/x.txt"));
    assert!(!stdout.contains("b/y.txt"), "unexpected stdout: {stdout}");
}

#[test]
fn search_path_outside_corpus_exits_2() {
    let root = fresh_dir("paths-outside-corpus");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    let outside = fresh_dir("paths-outside-corpus-elsewhere");
    fs::write(outside.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("hello")
        .arg(outside)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not under indexed corpus root"));
}
