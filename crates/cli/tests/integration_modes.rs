mod common;

use std::fs;

use common::{
    abs, abs_match, assert_success, build_index, command, fresh_dir, line_path, normalized_stdout,
};

#[test]
fn files_without_match_only_non_matching_paths() {
    let root = fresh_dir("modes-files-without-match");
    fs::write(root.join("a.txt"), "hello world\n").unwrap();
    fs::write(root.join("b.txt"), "goodbye world\n").unwrap();
    fs::write(root.join("c.txt"), "hello again\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--files-without-match")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, [abs(&root, "b.txt")]);
}

#[test]
fn files_without_match_when_no_file_matches_prints_all_files() {
    let root = fresh_dir("modes-files-without-match-none");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--files-without-match")
        .arg("nomatch")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, [abs(&root, "a.txt"), abs(&root, "b.txt")]);
}

#[test]
fn files_without_match_all_match_prints_nothing_and_exits_1() {
    let root = fresh_dir("modes-files-without-match-all-match");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--files-without-match")
        .arg("hello")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));

    let stdout = normalized_stdout(&out);
    assert!(stdout.is_empty(), "expected no output, got: {stdout}");
}

#[test]
fn files_with_matches_wins_over_files_without_match() {
    let root = fresh_dir("modes-files-without-overrides-files-with");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--files-without-match")
        .arg("--files-with-matches")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, [abs(&root, "a.txt"), abs(&root, "b.txt")]);
}

#[test]
fn count_shows_zero_for_non_matching_files() {
    let root = fresh_dir("modes-count-zero");
    fs::write(root.join("a.txt"), "hit\nhit\n").unwrap();
    fs::write(root.join("b.txt"), "miss\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-c")
        .arg("hit")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "a.txt", "2")));
    assert!(
        !stdout.contains("b.txt"),
        "zero-count files should be omitted"
    );
}

#[test]
fn count_takes_precedence_when_last() {
    let root = fresh_dir("modes-count-suppressed");
    fs::write(root.join("a.txt"), "hello\n").unwrap();
    fs::write(root.join("b.txt"), "hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--files-without-match")
        .arg("--count")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        lines,
        [
            abs_match(&root, "a.txt", "1"),
            abs_match(&root, "b.txt", "1")
        ]
    );
}

#[test]
fn quiet_exit_code_0_on_match() {
    let root = fresh_dir("modes-quiet-exit-match");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let status = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-q")
        .arg("found")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn quiet_exit_code_1_on_no_match() {
    let root = fresh_dir("modes-quiet-exit-nomatch");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let status = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-q")
        .arg("notfound")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn quiet_no_output_on_match() {
    let root = fresh_dir("modes-quiet-no-output");
    fs::write(root.join("a.txt"), "hello world\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-q")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(
        stdout.is_empty(),
        "quiet should produce no output, got: {stdout}"
    );
}

#[test]
fn no_match_exit_code_1() {
    let root = fresh_dir("modes-no-match-exit");
    fs::write(root.join("a.txt"), "something\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let status = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("nothing")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
}

#[test]
fn match_exit_code_0() {
    let root = fresh_dir("modes-match-exit");
    fs::write(root.join("a.txt"), "something\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let status = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("some")
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(0));
}

#[test]
fn standard_output_with_line_numbers() {
    let root = fresh_dir("modes-line-numbers");
    fs::write(root.join("t.txt"), "line one\nline two\nline three\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-n")
        .arg("line two")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("2:line two"));
    assert!(!stdout.contains("1:line one"));
}

#[test]
fn standard_output_no_filename() {
    let root = fresh_dir("modes-no-filename");
    fs::write(root.join("t.txt"), "alpha\nbeta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--no-filename")
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(!stdout.contains("t.txt"));
}

#[test]
fn multiple_patterns_combined_with_or() {
    let root = fresh_dir("modes-multi-pattern");
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    fs::write(root.join("b.txt"), "beta\n").unwrap();
    fs::write(root.join("c.txt"), "gamma\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-e")
        .arg("alpha")
        .arg("-e")
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "a.txt", "alpha")));
    assert!(stdout.contains(&abs_match(&root, "b.txt", "beta")));
    assert!(!stdout.contains("c.txt"));
}

#[test]
fn empty_corpus_exits_gracefully() {
    let root = fresh_dir("modes-empty-corpus");
    fs::create_dir_all(root.join("empty")).unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("anything")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));

    let stdout = normalized_stdout(&out);
    assert!(stdout.is_empty());
}

#[test]
fn single_file_match() {
    let root = fresh_dir("modes-single-file");
    fs::write(root.join("only.txt"), "unique content\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("unique")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "only.txt", "unique content")));
}

#[test]
fn invert_match_excludes_matching_lines() {
    let root = fresh_dir("modes-invert-match");
    fs::write(root.join("t.txt"), "keep\ndrop\nkeep\ndrop\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-v")
        .arg("drop")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("keep"));
    assert!(!stdout.contains("drop"));
}

#[test]
fn word_regexp_respects_word_boundaries() {
    let root = fresh_dir("modes-word-boundary");
    fs::write(
        root.join("t.txt"),
        "cat\ncategory\n bobcat  \ncategorically\n",
    )
    .unwrap();
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
    assert!(stdout.contains("cat"));
    assert!(
        !stdout.contains("category"),
        "category should not match: {stdout}"
    );
    assert!(
        !stdout.contains("bobcat"),
        "bobcat should not match: {stdout}"
    );
    assert!(
        !stdout.contains("categorically"),
        "categorically should not match: {stdout}"
    );
}

#[test]
fn word_regexp_with_unicode() {
    let root = fresh_dir("modes-word-unicode");
    fs::write(root.join("t.txt"), "αβγ\nαβ\nβγδ\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-w")
        .arg("αβ")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("αβ"));
    assert!(!stdout.contains("αβγ"), "αβγ should not match: {stdout}");
    assert!(!stdout.contains("βγδ"), "βγδ should not match: {stdout}");
}

#[test]
fn line_regexp_exact_line_match() {
    let root = fresh_dir("modes-line-regexp");
    fs::write(root.join("t.txt"), "exact\nnot exact\nnot exact either\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-x")
        .arg("exact")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "t.txt", "exact")));
    assert!(
        stdout.lines().count() == 1,
        "only exact line should match: {stdout}"
    );
}

#[test]
fn case_insensitive_unicode() {
    let root = fresh_dir("modes-case-unicode");
    fs::write(root.join("t.txt"), "ΑΛΦΑ\nαλφα\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-i")
        .arg("αλφα")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("ΑΛΦΑ"));
    assert!(stdout.contains("αλφα"));
}

#[test]
fn fixed_strings_literal_match() {
    let root = fresh_dir("modes-fixed-literal");
    fs::write(root.join("t.txt"), "hello.world\nhelloworld\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-F")
        .arg("hello.world")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("hello.world"));
    assert!(
        !stdout.contains("helloworld"),
        "helloworld should not match: {stdout}"
    );
}

#[test]
fn fixed_strings_with_case_insensitive() {
    let root = fresh_dir("modes-fixed-ignore-case");
    fs::write(root.join("t.txt"), "HELLO world\nhello WORLD\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-i")
        .arg("-F")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("HELLO world"));
    assert!(stdout.contains("hello WORLD"));
}

#[test]
fn ascii_mode_restricts_unicode() {
    let root = fresh_dir("modes-ascii-mode");
    fs::write(root.join("t.txt"), "café\ncafe\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("(?-u)cafe")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("cafe"));
    assert!(
        !stdout.contains("café"),
        "café should not match ASCII \\w: {stdout}"
    );
}

#[test]
fn regex_metacharacters_work_as_regex() {
    let root = fresh_dir("modes-regex-literal");
    fs::write(root.join("t.txt"), "file1.txt\nfile2.txt\nfile[1].txt\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("file[12].txt")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(
        !stdout.contains("file[1].txt"),
        "file[1].txt should not match char class [12]: {stdout}"
    );
}

#[test]
fn max_count_stops_after_n_matches() {
    let root = fresh_dir("modes-max-count");
    fs::write(root.join("t.txt"), "match\nmatch\nmatch\nmatch\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--max-count")
        .arg("2")
        .arg("match")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2, "expected 2 lines, got: {lines:?}");
}

#[test]
fn output_order_deterministic() {
    let root = fresh_dir("modes-output-order");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::create_dir_all(root.join("c")).unwrap();
    fs::write(root.join("a/z.txt"), "found\n").unwrap();
    fs::write(root.join("b/m.txt"), "found\n").unwrap();
    fs::write(root.join("c/a.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("found")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 3);
    let expected = [
        abs(&root, "a/z.txt"),
        abs(&root, "b/m.txt"),
        abs(&root, "c/a.txt"),
    ];
    let paths: Vec<_> = lines
        .iter()
        .map(|l| line_path(l, &expected).to_string())
        .collect();
    assert_eq!(paths, expected, "output should be sorted: {paths:?}");
}

#[test]
fn build_command_error_exit_code() {
    let root = fresh_dir("modes-build-error");
    let idx = root.join(".sift");

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("build")
        .arg("/nonexistent/path")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn quiet_with_files_with_matches_match() {
    let root = fresh_dir("modes-quiet-l-match");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-l")
        .arg("-q")
        .arg("found")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_with_matches_no_match() {
    let root = fresh_dir("modes-quiet-l-nomatch");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-l")
        .arg("-q")
        .arg("notfound")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_without_match_match() {
    let root = fresh_dir("modes-quiet-L-match");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    fs::write(root.join("b.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-L")
        .arg("-q")
        .arg("found")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_without_match_no_match() {
    let root = fresh_dir("modes-quiet-L-nomatch");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    fs::write(root.join("b.txt"), "miss\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-L")
        .arg("-q")
        .arg("found")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_flag_order_independent() {
    let root = fresh_dir("modes-quiet-order");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    fs::write(root.join("b.txt"), "found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    for args in &[&["-q", "-L"][..], &["-L", "-q"][..]] {
        let out = command(None)
            .arg("--sift-dir")
            .arg(&idx)
            .args(*args)
            .arg("found")
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "-L -q should exit 1 when all files match"
        );
        assert!(normalized_stdout(&out).is_empty());
    }
}

#[test]
fn quiet_count_no_output() {
    let root = fresh_dir("modes-quiet-count");
    fs::write(root.join("a.txt"), "found\nfound\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-c")
        .arg("-q")
        .arg("found")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_only_matching_match() {
    let root = fresh_dir("modes-quiet-o-match");
    fs::write(root.join("a.txt"), "hello world\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-o")
        .arg("-q")
        .arg("hello")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn quiet_only_matching_no_match() {
    let root = fresh_dir("modes-quiet-o-nomatch");
    fs::write(root.join("a.txt"), "hello world\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-o")
        .arg("-q")
        .arg("notfound")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(normalized_stdout(&out).is_empty());
}

#[test]
fn single_file_search_defaults_to_no_filename() {
    let root = fresh_dir("modes-single-default");
    let file = root.join("only.txt");
    fs::write(&file, "hello world\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("hello world"));
    assert!(
        !stdout.contains("only.txt"),
        "single-file index search should default to no filename"
    );
}

#[test]
fn single_file_o_defaults_to_no_filename() {
    let root = fresh_dir("modes-single-o-default");
    let file = root.join("only.txt");
    fs::write(&file, "alpha beta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-o")
        .arg("alpha")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("alpha"));
    assert!(
        !stdout.contains("only.txt"),
        "single-file -o should default to no filename"
    );
}

#[test]
fn single_file_count_bare() {
    let root = fresh_dir("modes-single-count");
    let file = root.join("only.txt");
    fs::write(&file, "hello\nhello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("-c")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert_eq!(stdout.trim(), "2");
    assert!(
        !stdout.contains("only.txt"),
        "single-file -c should print bare count"
    );
}

#[test]
fn single_file_count_matches_bare() {
    let root = fresh_dir("modes-single-count-matches");
    let file = root.join("only.txt");
    fs::write(&file, "hello hello hello\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &file);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--count-matches")
        .arg("hello")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert_eq!(stdout.trim(), "3");
    assert!(
        !stdout.contains("only.txt"),
        "single-file --count-matches should print bare count"
    );
}

#[test]
fn no_filename_flag_still_works_for_line_prefix() {
    let root = fresh_dir("modes-no-filename-explicit");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    fs::write(root.join("b.txt"), "also found\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--no-filename")
        .arg("found")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("found"));
    assert!(!stdout.contains("a.txt"));
    assert!(!stdout.contains("b.txt"));
}

#[test]
fn files_with_matches_still_path_mode_under_no_filename() {
    let root = fresh_dir("modes-l-no-filename");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    fs::write(root.join("b.txt"), "found\n").unwrap();
    fs::write(root.join("c.txt"), "miss\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--no-filename")
        .arg("-l")
        .arg("found")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines
        .iter()
        .all(|l| l.contains("a.txt") || l.contains("b.txt")));
}

#[test]
fn files_without_match_still_path_mode_under_no_filename() {
    let root = fresh_dir("modes-L-no-filename");
    fs::write(root.join("a.txt"), "hit\n").unwrap();
    fs::write(root.join("b.txt"), "hit\n").unwrap();
    fs::write(root.join("c.txt"), "miss\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("--no-filename")
        .arg("-L")
        .arg("hit")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("c.txt"));
}
