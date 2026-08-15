mod common;

use common::{TestProject, assert_success, line_path, normalize_stdout, rel_match};

#[test]
fn files_without_match_only_non_matching_paths() {
    let p = TestProject::new("modes-files-without-match");
    p.write("a.txt", "hello world\n");
    p.write("b.txt", "goodbye world\n");
    p.write("c.txt", "hello again\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "hello"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines, ["b.txt".to_string()]);
}

#[test]
fn files_without_match_when_no_file_matches_prints_all_files() {
    let p = TestProject::new("modes-files-without-match-none");
    p.write("a.txt", "hello\n");
    p.write("b.txt", "hello\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "nomatch"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines, ["a.txt".to_string(), "b.txt".to_string()]);
}

#[test]
fn files_without_match_all_match_prints_nothing_and_exits_1() {
    let p = TestProject::new("modes-files-without-match-all-match");
    p.write("a.txt", "hello\n");
    p.write("b.txt", "hello\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "hello"]);
    assert_eq!(out.status.code(), Some(1));

    let stdout = normalize_stdout(&out);
    assert!(stdout.is_empty(), "expected no output, got: {stdout}");
}

#[test]
fn files_with_matches_wins_over_files_without_match() {
    let p = TestProject::new("modes-files-without-overrides-files-with");
    p.write("a.txt", "hello\n");
    p.write("b.txt", "hello\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "--files-with-matches", "hello"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines, ["a.txt".to_string(), "b.txt".to_string()]);
}

#[test]
fn count_shows_zero_for_non_matching_files() {
    let p = TestProject::new("modes-count-zero");
    p.write("a.txt", "hit\nhit\n");
    p.write("b.txt", "miss\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-c", "hit"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("a.txt", "2")));
    assert!(
        !stdout.contains("b.txt"),
        "zero-count files should be omitted"
    );
}

#[test]
fn count_takes_precedence_when_last() {
    let p = TestProject::new("modes-count-suppressed");
    p.write("a.txt", "hello\n");
    p.write("b.txt", "hello\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "--count", "hello"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines, [rel_match("a.txt", "1"), rel_match("b.txt", "1")]);
}

#[test]
fn quiet_exit_code_0_on_match() {
    let p = TestProject::new("modes-quiet-exit-match");
    p.write("a.txt", "found\n");
    p.build_index_at(p.root());

    let status = p.index_status(["-q", "found"]);
    assert_eq!(status.code(), Some(0));
}

#[test]
fn quiet_exit_code_1_on_no_match() {
    let p = TestProject::new("modes-quiet-exit-nomatch");
    p.write("a.txt", "found\n");
    p.build_index_at(p.root());

    let status = p.index_status(["-q", "notfound"]);
    assert_eq!(status.code(), Some(1));
}

#[test]
fn quiet_no_output_on_match() {
    let p = TestProject::new("modes-quiet-no-output");
    p.write("a.txt", "hello world\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-q", "hello"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(
        stdout.is_empty(),
        "quiet should produce no output, got: {stdout}"
    );
}

#[test]
fn quiet_stats_stops_after_first_match() {
    let p = TestProject::new("modes-quiet-first-match-stats");
    p.write("a.txt", "hit\n");
    p.write("b.txt", "miss\n");
    p.write("c.txt", "hit\n");
    p.build_index();

    let out = p.index_output(["-q", "-E", "none", "--stats", "hit"]);
    assert_success(&out);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("1 files searched"),
        "expected quiet to stop after first match, got: {stderr:?}"
    );
}

#[test]
fn no_match_exit_code_1() {
    let p = TestProject::new("modes-no-match-exit");
    p.write("a.txt", "something\n");
    p.build_index_at(p.root());

    let status = p.index_status(["nothing"]);
    assert_eq!(status.code(), Some(1));
}

#[test]
fn match_exit_code_0() {
    let p = TestProject::new("modes-match-exit");
    p.write("a.txt", "something\n");
    p.build_index_at(p.root());

    let status = p.index_status(["some"]);
    assert_eq!(status.code(), Some(0));
}

#[test]
fn standard_output_with_line_numbers() {
    let p = TestProject::new("modes-line-numbers");
    p.write("t.txt", "line one\nline two\nline three\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-n", "line two"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("2:line two"));
    assert!(!stdout.contains("1:line one"));
}

#[test]
fn standard_output_no_filename() {
    let p = TestProject::new("modes-no-filename");
    p.write("t.txt", "alpha\nbeta\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--no-filename", "beta"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(!stdout.contains("t.txt"));
}

#[test]
fn multiple_patterns_combined_with_or() {
    let p = TestProject::new("modes-multi-pattern");
    p.write("a.txt", "alpha\n");
    p.write("b.txt", "beta\n");
    p.write("c.txt", "gamma\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-e", "alpha", "-e", "beta"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("a.txt", "alpha")));
    assert!(stdout.contains(&rel_match("b.txt", "beta")));
    assert!(!stdout.contains("c.txt"));
}

#[test]
fn empty_corpus_exits_gracefully() {
    let p = TestProject::new("modes-empty-corpus");
    p.mkdir("empty");
    p.build_index_at(p.root());

    let out = p.index_output(["anything"]);
    assert_eq!(out.status.code(), Some(1));

    let stdout = normalize_stdout(&out);
    assert!(stdout.is_empty());
}

#[test]
fn single_file_match() {
    let p = TestProject::new("modes-single-file");
    p.write("only.txt", "unique content\n");
    p.build_index_at(p.root());

    let out = p.index_output(["unique"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("only.txt", "unique content")));
}

#[test]
fn invert_match_excludes_matching_lines() {
    let p = TestProject::new("modes-invert-match");
    p.write("t.txt", "keep\ndrop\nkeep\ndrop\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-v", "drop"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep"));
    assert!(!stdout.contains("drop"));
}

#[test]
fn word_regexp_respects_word_boundaries() {
    let p = TestProject::new("modes-word-boundary");
    p.write("t.txt", "cat\ncategory\n bobcat  \ncategorically\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "cat"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
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
    let p = TestProject::new("modes-word-unicode");
    p.write(
        "t.txt",
        "\u{3b1}\u{3b2}\u{3b3}\n\u{3b1}\u{3b2}\n\u{3b2}\u{3b3}\u{3b4}\n",
    );
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "\u{3b1}\u{3b2}"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("\u{3b1}\u{3b2}"));
    assert!(
        !stdout.contains("\u{3b1}\u{3b2}\u{3b3}"),
        "\u{3b1}\u{3b2}\u{3b3} should not match: {stdout}"
    );
    assert!(
        !stdout.contains("\u{3b2}\u{3b3}\u{3b4}"),
        "\u{3b2}\u{3b3}\u{3b4} should not match: {stdout}"
    );
}

#[test]
fn line_regexp_exact_line_match() {
    let p = TestProject::new("modes-line-regexp");
    p.write("t.txt", "exact\nnot exact\nnot exact either\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-x", "exact"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("t.txt", "exact")));
    assert!(
        stdout.lines().count() == 1,
        "only exact line should match: {stdout}"
    );
}

#[test]
fn case_insensitive_unicode() {
    let p = TestProject::new("modes-case-unicode");
    p.write(
        "t.txt",
        "\u{0391}\u{039b}\u{03a6}\u{0391}\n\u{03b1}\u{03bb}\u{03c6}\u{03b1}\n",
    );
    p.build_index_at(p.root());

    let out = p.index_output(["-i", "\u{03b1}\u{03bb}\u{03c6}\u{03b1}"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("\u{0391}\u{039b}\u{03a6}\u{0391}"));
    assert!(stdout.contains("\u{03b1}\u{03bb}\u{03c6}\u{03b1}"));
}

#[test]
fn fixed_strings_literal_match() {
    let p = TestProject::new("modes-fixed-literal");
    p.write("t.txt", "hello.world\nhelloworld\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-F", "hello.world"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("hello.world"));
    assert!(
        !stdout.contains("helloworld"),
        "helloworld should not match: {stdout}"
    );
}

#[test]
fn fixed_strings_with_case_insensitive() {
    let p = TestProject::new("modes-fixed-ignore-case");
    p.write("t.txt", "HELLO world\nhello WORLD\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-i", "-F", "hello"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("HELLO world"));
    assert!(stdout.contains("hello WORLD"));
}

#[test]
fn ascii_mode_restricts_unicode() {
    let p = TestProject::new("modes-ascii-mode");
    p.write("t.txt", "caf\u{00e9}\ncafe\n");
    p.build_index_at(p.root());

    let out = p.index_output(["(?-u)cafe"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("cafe"));
    assert!(
        !stdout.contains("caf\u{00e9}"),
        "caf\u{00e9} should not match ASCII \\\\w: {stdout}"
    );
}

#[test]
fn regex_metacharacters_work_as_regex() {
    let p = TestProject::new("modes-regex-literal");
    p.write("t.txt", "file1.txt\nfile2.txt\nfile[1].txt\n");
    p.build_index_at(p.root());

    let out = p.index_output(["file[12].txt"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("file1.txt"));
    assert!(stdout.contains("file2.txt"));
    assert!(
        !stdout.contains("file[1].txt"),
        "file[1].txt should not match char class [12]: {stdout}"
    );
}

#[test]
fn max_count_stops_after_n_matches() {
    let p = TestProject::new("modes-max-count");
    p.write("t.txt", "match\nmatch\nmatch\nmatch\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--max-count", "2", "match"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 2, "expected 2 lines, got: {lines:?}");
}

#[test]
fn output_order_deterministic() {
    let p = TestProject::new("modes-output-order");
    p.mkdir("a");
    p.mkdir("b");
    p.mkdir("c");
    p.write("a/z.txt", "found\n");
    p.write("b/m.txt", "found\n");
    p.write("c/a.txt", "found\n");
    p.build_index_at(p.root());

    let out = p.index_output(["found"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 3);
    let expected = ["a/z.txt", "b/m.txt", "c/a.txt"].map(String::from).to_vec();
    let paths: Vec<_> = lines
        .iter()
        .map(|l| line_path(l, &expected).to_string())
        .collect();
    assert_eq!(paths, expected, "output should be sorted: {paths:?}");
}

#[test]
fn build_command_error_exit_code() {
    let p = TestProject::new("modes-build-error");

    let out = p
        .sift()
        .arg("--sift-dir")
        .arg(p.root().join(".sift"))
        .args(["index", "build"])
        .arg("/nonexistent/path")
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn quiet_with_files_with_matches_match() {
    let p = TestProject::new("modes-quiet-l-match");
    p.write("a.txt", "found\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-l", "-q", "found"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_with_matches_no_match() {
    let p = TestProject::new("modes-quiet-l-nomatch");
    p.write("a.txt", "found\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-l", "-q", "notfound"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_without_match_match() {
    let p = TestProject::new("modes-quiet-L-match");
    p.write("a.txt", "found\n");
    p.write("b.txt", "found\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "-q", "found"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_with_files_without_match_no_match() {
    let p = TestProject::new("modes-quiet-L-nomatch");
    p.write("a.txt", "found\n");
    p.write("b.txt", "miss\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--files-without-match", "-q", "found"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_flag_order_independent() {
    let p = TestProject::new("modes-quiet-order");
    p.write("a.txt", "found\n");
    p.write("b.txt", "found\n");
    p.build_index_at(p.root());

    for raw_args in &[
        &["-q", "--files-without-match"][..],
        &["--files-without-match", "-q"][..],
    ] {
        let args: Vec<&str> = raw_args
            .iter()
            .copied()
            .chain(std::iter::once("found"))
            .collect();
        let out = p.index_output(&args);
        assert_eq!(
            out.status.code(),
            Some(1),
            "--files-without-match -q should exit 1 when all files match"
        );
        assert!(normalize_stdout(&out).is_empty());
    }
}

#[test]
fn quiet_count_no_output() {
    let p = TestProject::new("modes-quiet-count");
    p.write("a.txt", "found\nfound\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-c", "-q", "found"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_only_matching_match() {
    let p = TestProject::new("modes-quiet-o-match");
    p.write("a.txt", "hello world\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-o", "-q", "hello"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn quiet_only_matching_no_match() {
    let p = TestProject::new("modes-quiet-o-nomatch");
    p.write("a.txt", "hello world\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-o", "-q", "notfound"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(normalize_stdout(&out).is_empty());
}

#[test]
fn no_filename_flag_still_works_for_line_prefix() {
    let p = TestProject::new("modes-no-filename-explicit");
    p.write("a.txt", "found\n");
    p.write("b.txt", "also found\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--no-filename", "found"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("found"));
    assert!(!stdout.contains("a.txt"));
    assert!(!stdout.contains("b.txt"));
}

#[test]
fn files_with_matches_still_path_mode_under_no_filename() {
    let p = TestProject::new("modes-l-no-filename");
    p.write("a.txt", "found\n");
    p.write("b.txt", "found\n");
    p.write("c.txt", "miss\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--no-filename", "-l", "found"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 2);
    assert!(
        lines
            .iter()
            .all(|l| l.contains("a.txt") || l.contains("b.txt"))
    );
}

#[test]
fn files_without_match_still_path_mode_under_no_filename() {
    let p = TestProject::new("modes-L-no-filename");
    p.write("a.txt", "hit\n");
    p.write("b.txt", "hit\n");
    p.write("c.txt", "miss\n");
    p.build_index_at(p.root());

    let out = p.index_output(["--no-filename", "--files-without-match", "hit"]);
    assert_success(&out);

    let lines: Vec<_> = normalize_stdout(&out).lines().map(str::to_string).collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("c.txt"));
}

#[test]
fn word_regexp_with_alternation() {
    let p = TestProject::new("modes-word-alt");
    p.write("t.txt", "cat\ndog\ncatdog\ncategory\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "cat|dog"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("cat"));
    assert!(stdout.contains("dog"));
    assert!(
        !stdout.contains("catdog"),
        "catdog should not match: {stdout}"
    );
    assert!(
        !stdout.contains("category"),
        "category should not match: {stdout}"
    );
}

#[test]
fn line_regexp_with_alternation() {
    let p = TestProject::new("modes-line-alt");
    p.write("t.txt", "cat\ndog\ncatdog\ncat\r\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-x", "cat|dog"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    let lines: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(
        lines.len(),
        2,
        "only exact cat or dog lines should match: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .all(|l| l.ends_with("cat") || l.ends_with("dog")),
        "both lines should be exact match: {lines:?}"
    );
}

#[test]
fn word_and_line_regexp_line_takes_precedence() {
    let p = TestProject::new("modes-wx-precedence");
    p.write("t.txt", "cat\ncat dog\ndog cat\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "-x", "cat"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    let lines: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(
        lines.len(),
        1,
        "line anchor should take precedence, only exact line matches: {lines:?}"
    );
    assert!(
        stdout.contains("cat"),
        "exact line cat should match: {stdout}"
    );
    assert!(
        !stdout.contains("cat dog"),
        "cat dog should not match: {stdout}"
    );
    assert!(
        !stdout.contains("dog cat"),
        "dog cat should not match: {stdout}"
    );
}

#[test]
fn line_regexp_only_matching() {
    let p = TestProject::new("modes-line-only");
    p.write("t.txt", "cat\ncat dog\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-x", "-o", "cat"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    let lines: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(
        lines.len(),
        1,
        "only exact match span should print: {lines:?}"
    );
    assert!(stdout.contains("cat"), "should print cat span: {stdout}");
}

#[test]
fn word_regexp_only_matching() {
    let p = TestProject::new("modes-word-only");
    p.write("t.txt", "cat\ncat dog\nconcatenate\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "-o", "cat"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    let lines: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(
        lines.len(),
        2,
        "both whole-word cat matches should print: {lines:?}"
    );
    assert!(
        !stdout.contains("concatenate"),
        "concatenate should not match: {stdout}"
    );
}

#[test]
fn word_line_regexp_only_matching() {
    let p = TestProject::new("modes-wx-only");
    p.write("t.txt", "cat\ncat dog\ndog cat\n");
    p.build_index_at(p.root());

    let out = p.index_output(["-w", "-x", "-o", "cat"]);
    assert_success(&out);

    let stdout = normalize_stdout(&out);
    let lines: Vec<_> = stdout.lines().map(str::to_string).collect();
    assert_eq!(
        lines.len(),
        1,
        "line anchor takes precedence, only exact match prints: {lines:?}"
    );
    assert!(
        stdout.contains("cat"),
        "exact match span should print: {stdout}"
    );
}
