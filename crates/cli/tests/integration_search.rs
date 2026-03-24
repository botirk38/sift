mod common;

use std::fs;
use std::path::Path;

use common::{abs_match, assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn build_then_search_finds_line() {
    let root = fresh_dir("search-line");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn f() {\n  let y = 2;\n}\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg(r"let\s+y")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains(&abs_match(&root, "src/lib.rs", "")) && stdout.contains("let y = 2;"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn search_no_match_exits_1() {
    let root = fresh_dir("search-no-match");
    fs::write(root.join("a.txt"), "nope\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let status = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("ZZZ_NOT_THERE")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn fixed_string_ignore_case_finds_match() {
    let root = fresh_dir("search-fixed-ignore-case");
    fs::write(root.join("t.txt"), "hello world\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-i")
        .arg("-F")
        .arg("HELLO")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "t.txt", "")) && stdout.contains("hello world"));
}

#[test]
fn invert_match_returns_non_matching_lines() {
    let root = fresh_dir("search-invert-match");
    fs::write(root.join("t.txt"), "keep\nskip\nkeep too\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-v")
        .arg("skip")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("keep"));
    assert!(stdout.contains("keep too"));
    assert!(
        !stdout.contains("t.txt:skip"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn word_regexp_matches_whole_words_only() {
    let root = fresh_dir("search-word-regexp");
    fs::write(root.join("t.txt"), "cat\nscatter\ncatnip\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-w")
        .arg("cat")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "t.txt", "cat")));
    assert!(!stdout.contains("scatter"), "unexpected stdout: {stdout}");
    assert!(!stdout.contains("catnip"), "unexpected stdout: {stdout}");
}

#[test]
fn line_regexp_matches_whole_lines_only() {
    let root = fresh_dir("search-line-regexp");
    fs::write(root.join("t.txt"), "cat\ncat dog\ndog cat\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-x")
        .arg("cat")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "t.txt", "cat")));
    assert!(!stdout.contains("cat dog"), "unexpected stdout: {stdout}");
    assert!(!stdout.contains("dog cat"), "unexpected stdout: {stdout}");
}

#[test]
fn missing_pattern_exits_2() {
    let root = fresh_dir("search-missing-pattern");
    fs::write(root.join("t.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None).arg("--sift-dir").arg(&idx).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no patterns"));
}

#[test]
fn search_literal_index_without_subcommand() {
    let root = fresh_dir("search-literal-index");
    fs::write(root.join("t.txt"), "word index here\n").unwrap();
    let idx = root.join(".sift");

    build_index(Some(&root), &idx, Path::new("."));

    let out = command(Some(&root))
        .arg("--sift-dir")
        .arg(&idx)
        .arg("index")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "t.txt", "")) && stdout.contains("index"));
}

#[test]
fn build_single_file_then_search_finds_match() {
    let root = fresh_dir("search-single-file");
    let file = root.join("one.txt");
    fs::write(&file, "alpha\nbeta needle\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("needle")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains(&abs_match(&root, "one.txt", "beta needle")),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn build_single_file_then_search_path_scope_accepts_that_file() {
    let root = fresh_dir("search-single-file-scope");
    let file = root.join("one.txt");
    fs::write(&file, "needle here\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("needle")
        .arg(&file)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains(&abs_match(&root, "one.txt", "needle here")),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn binary_files_are_skipped_by_default() {
    let root = fresh_dir("search-binary-skip");
    fs::write(root.join("text.txt"), "alpha βeta\n").unwrap();
    fs::write(
        root.join("bin.dat"),
        b"prefix\0\xce\xb2\xce\xb5\xcf\x84\xce\xb1\n",
    )
    .unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg(r"\p{Greek}+")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.contains(&abs_match(&root, "text.txt", "alpha βeta")),
        "unexpected stdout: {stdout}"
    );
    assert!(
        !stdout.contains("bin.dat"),
        "binary file should be skipped: {stdout}"
    );
}

#[cfg(not(windows))]
#[test]
fn symlinked_files_are_not_searched_by_default() {
    use std::os::unix::fs::symlink;

    let root = fresh_dir("search-symlink-skip");
    fs::create_dir_all(root.join("real")).unwrap();
    fs::create_dir_all(root.join("link")).unwrap();
    fs::write(root.join("real/target.txt"), "needle here\n").unwrap();
    symlink(root.join("real/target.txt"), root.join("link/target.txt")).unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("needle")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, [abs_match(&root, "real/target.txt", "needle here")]);
}
