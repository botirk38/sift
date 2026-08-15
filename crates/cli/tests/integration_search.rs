mod common;

use std::fs;

#[cfg(not(windows))]
use common::assert_stdout_eq;
use common::{
    TestProject, assert_exit_code, assert_stdout_contains, assert_stdout_not_contains,
    assert_success, normalize_stderr, rel_match,
};

#[test]
fn build_respects_gitignore() {
    let p = TestProject::new("build-gitignore");
    p.write(".gitignore", "ignored.txt\n");
    p.write("keep.txt", "needle\n");
    p.write("ignored.txt", "needle\n");
    p.build_index();

    let out = p.index_output(["needle"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("keep.txt", ""));
    assert_stdout_not_contains(&out, "ignored.txt");
}

#[test]
fn build_then_search_finds_line() {
    let p = TestProject::new("search-line");
    p.mkdir("src");
    p.write("src/lib.rs", "fn f() {\n  let y = 2;\n}\n");
    p.build_index();

    let out = p.index_output([r"let\s+y"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("src/lib.rs", ""));
    assert_stdout_contains(&out, "let y = 2;");
}

#[test]
fn search_no_match_exits_1() {
    let p = TestProject::new("search-no-match");
    p.write("a.txt", "nope\n");
    p.build_index();

    let status = p.index_status(["ZZZ_NOT_THERE"]);
    assert_eq!(status.code(), Some(1));
}

#[test]
fn fixed_string_ignore_case_finds_match() {
    let p = TestProject::new("search-fixed-ignore-case");
    p.write("t.txt", "hello world\n");
    p.build_index();

    let out = p.index_output(["-i", "-F", "HELLO"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("t.txt", ""));
    assert_stdout_contains(&out, "hello world");
}

#[test]
fn invert_match_returns_non_matching_lines() {
    let p = TestProject::new("search-invert-match");
    p.write("t.txt", "keep\nskip\nkeep too\n");
    p.build_index();

    let out = p.index_output(["-v", "skip"]);
    assert_success(&out);
    assert_stdout_contains(&out, "keep");
    assert_stdout_contains(&out, "keep too");
    assert_stdout_not_contains(&out, "t.txt:skip");
}

#[test]
fn word_regexp_matches_whole_words_only() {
    let p = TestProject::new("search-word-regexp");
    p.write("t.txt", "cat\nscatter\ncatnip\n");
    p.build_index();

    let out = p.index_output(["-w", "cat"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("t.txt", "cat"));
    assert_stdout_not_contains(&out, "scatter");
    assert_stdout_not_contains(&out, "catnip");
}

#[test]
fn line_regexp_matches_whole_lines_only() {
    let p = TestProject::new("search-line-regexp");
    p.write("t.txt", "cat\ncat dog\ndog cat\n");
    p.build_index();

    let out = p.index_output(["-x", "cat"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("t.txt", "cat"));
    assert_stdout_not_contains(&out, "cat dog");
    assert_stdout_not_contains(&out, "dog cat");
}

#[test]
fn missing_pattern_exits_2() {
    let p = TestProject::new("search-missing-pattern");
    p.write("t.txt", "hello\n");
    p.build_index();

    let out = p
        .sift()
        .arg("--sift-dir")
        .arg(p.root().join(".sift"))
        .output()
        .unwrap();
    assert_exit_code(&out, 2);
    assert!(normalize_stderr(&out).contains("no pattern"));
}

#[test]
fn search_literal_index_without_subcommand() {
    let p = TestProject::new("search-literal-index");
    p.write("t.txt", "word index here\n");
    p.build_index();

    let out = p.index_output(["-e", "index"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("t.txt", ""));
    assert_stdout_contains(&out, "index");
}

#[test]
fn index_build_rejects_file_path() {
    let p = TestProject::new("search-single-file");
    p.write("one.txt", "alpha\nbeta needle\n");

    let out = p.index_output(["index", "build", "--wait", "one.txt"]);
    assert_exit_code(&out, 2);
    assert!(normalize_stderr(&out).contains("index build requires a directory"));
}

#[test]
fn binary_files_are_skipped_by_default() {
    let p = TestProject::new("search-binary-skip");
    p.write("text.txt", "alpha βeta\n");
    p.write("bin.dat", b"prefix\0\xce\xb2\xce\xb5\xcf\x84\xce\xb1\n");
    p.build_index();

    let out = p.index_output([r"\p{Greek}+"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("text.txt", "alpha βeta"));
    assert_stdout_not_contains(&out, "bin.dat");
}

#[cfg(not(windows))]
#[test]
fn symlinked_files_are_not_searched_by_default() {
    use std::os::unix::fs::symlink;

    let p = TestProject::new("search-symlink-skip");
    p.mkdir("real");
    p.mkdir("link");
    p.write("real/target.txt", "needle here\n");
    symlink(
        p.root().join("real/target.txt"),
        p.root().join("link/target.txt"),
    )
    .unwrap();
    p.build_index();

    let out = p.index_output(["needle"]);
    assert_success(&out);
    assert_stdout_eq(
        &out,
        &format!("{}\n", rel_match("real/target.txt", "needle here")),
    );
}

#[cfg(not(windows))]
#[test]
fn follow_symlink_searches_linked_path() {
    use std::os::unix::fs::symlink;

    let p = TestProject::new("search-follow-symlink");
    p.mkdir("real");
    p.mkdir("link");
    p.write("real/target.txt", "needle here\n");
    symlink(
        p.root().join("real/target.txt"),
        p.root().join("link/target.txt"),
    )
    .unwrap();
    p.build_index_follow();

    let out = p.index_output(["--follow", "needle"]);
    assert_success(&out);
    assert_stdout_contains(&out, "needle here");
    assert_stdout_contains(&out, "link/target.txt");
}

#[test]
fn partial_index_missing_component_errors() {
    let p = TestProject::new("search-partial-index-error");
    p.write("found.txt", "unique_marker_partial_index\n");
    p.build_index();

    let current = fs::read_to_string(p.root().join(".sift/CURRENT")).unwrap();
    let snapshot = current.trim();
    let postings = p
        .root()
        .join(format!(".sift/snapshots/{snapshot}/ngram-3/postings.bin"));
    fs::remove_file(&postings).unwrap();

    let out = p.index_output(["unique_marker_partial_index"]);
    assert_exit_code(&out, 2);
}

#[test]
fn invalid_meta_errors() {
    let p = TestProject::new("search-invalid-meta-error");
    p.write("found.txt", "unique_marker_bad_meta\n");

    fs::create_dir_all(p.root().join(".sift")).unwrap();
    fs::write(p.root().join(".sift/CURRENT"), "nonexistent-snapshot\n").unwrap();

    let out = p.index_output(["unique_marker_bad_meta"]);
    assert_exit_code(&out, 2);
}
