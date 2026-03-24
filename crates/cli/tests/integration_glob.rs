mod common;

use std::fs;

use common::{assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn glob_include_only_matching_files() {
    let root = fresh_dir("glob-include");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.log"), "hello\n").unwrap();
    fs::write(root.join("c.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.txt")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    let lines: Vec<_> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split(':').next().unwrap_or(l))
        .collect();
    assert!(
        lines.iter().all(|l| std::path::Path::new(l)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))),
        "expected only .txt files: {lines:?}"
    );
    assert!(
        !lines.iter().any(|l| std::path::Path::new(l)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))),
        "should not contain .log files: {lines:?}"
    );
}

#[test]
fn glob_exclude_pattern_excludes_matched_files() {
    let root = fresh_dir("glob-exclude");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.log"), "hello\n").unwrap();
    fs::write(root.join("c.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("!*.log")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    let lines: Vec<_> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.split(':').next().unwrap_or(l))
        .collect();
    assert!(
        !lines.iter().any(|l| std::path::Path::new(l)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))),
        "should not contain .log files: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| std::path::Path::new(l)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))),
        "should contain .txt files: {lines:?}"
    );
}

#[test]
fn glob_multiple_patterns_later_wins() {
    let root = fresh_dir("glob-multiple");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.txt")
        .arg("-g")
        .arg("!a*.txt")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains("b.txt"),
        "b.txt should be included: {stdout}"
    );
    assert!(
        !stdout.contains("a.txt"),
        "a.txt should be excluded by later ignore pattern: {stdout}"
    );
}

#[test]
fn glob_directory_matches_subtree() {
    let root = fresh_dir("glob-dir");
    fs::create_dir_all(root.join("foo/bar")).unwrap();
    fs::write(root.join("foo/bar/baz.txt"), "hello\n").unwrap();
    fs::write(root.join("foo/qux.log"), "hello\n").unwrap();
    fs::write(root.join("other.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("foo/**")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains("foo/bar/baz.txt"),
        "foo/** should match subdirectory: {stdout}"
    );
    assert!(
        stdout.contains("foo/qux.log"),
        "foo/** should match file in foo/: {stdout}"
    );
    assert!(
        !stdout.contains("other.txt"),
        "foo/** should not match outside: {stdout}"
    );
}

#[test]
fn glob_whitelist_then_exclude() {
    let root = fresh_dir("glob-whitelist-exclude");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    fs::write(root.join("c.log"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.txt")
        .arg("-g")
        .arg("!a*.txt")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains("b.txt"),
        "b.txt should be included: {stdout}"
    );
    assert!(
        !stdout.contains("a.txt"),
        "a.txt should be excluded: {stdout}"
    );
}

#[test]
fn glob_only_whitelist_none_match_excludes_all() {
    let root = fresh_dir("glob-no-match");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.log"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.xyz")
        .arg("hello")
        .output()
        .unwrap();

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.is_empty(),
        "no matching whitelist should exclude all: {stdout}"
    );
}

#[test]
fn glob_invalid_pattern_returns_error() {
    let root = fresh_dir("glob-invalid");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("[")
        .arg("hello")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid glob pattern"),
        "should report invalid glob: {stderr}"
    );
}

#[test]
fn glob_files_with_matches_includes_only_glob_matched() {
    let root = fresh_dir("glob-files-with");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.log"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.txt")
        .arg("-l")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("a.txt"), "should contain a.txt: {stdout}");
    assert!(
        !stdout.contains("b.log"),
        "should not contain b.log: {stdout}"
    );
}

#[test]
fn glob_combined_with_path_scope() {
    let root = fresh_dir("glob-path-scope");
    fs::create_dir_all(root.join("foo")).unwrap();
    fs::create_dir_all(root.join("bar")).unwrap();
    fs::write(root.join("foo/a.txt"), "hello\n").unwrap();
    fs::write(root.join("bar/b.txt"), "hello\n").unwrap();
    fs::write(root.join("bar/c.log"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-g")
        .arg("*.txt")
        .arg("hello")
        .arg(root.join("foo"))
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains("foo/a.txt"),
        "should find foo/a.txt: {stdout}"
    );
    assert!(
        !stdout.contains("bar"),
        "path scope should limit search: {stdout}"
    );
}
