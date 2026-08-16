mod common;

use common::{
    TestProject, assert_stderr_empty, assert_stdout_eq, assert_success, normalize_stdout, rel_match,
};

// ─── --replace / -r ──────────────────────────────────────────────────────────

#[test]
fn replace_literal_walk() {
    let p = TestProject::new("replace-literal-walk");
    p.write("a.txt", "hello world\n");
    let out = p.walk_output(["-r", "planet", "world"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello planet"),
        "expected replacement output, got: {stdout}"
    );
}

#[test]
fn replace_literal_consistent_index_and_walk() {
    let p = TestProject::new("replace-literal-both");
    p.write("a.txt", "hello world\n");
    p.build_index();
    let args = ["-r", "planet", "world"];
    let expected = format!("{}\n", rel_match("a.txt", "hello planet"));

    let index = p.index_output(args);
    assert_success(&index);
    assert_stdout_eq(&index, &expected);
    assert_stderr_empty(&index);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stdout_eq(&walk, &expected);
    assert_stderr_empty(&walk);
}

#[test]
fn replace_with_capture_groups_walk() {
    let p = TestProject::new("replace-capture-walk");
    p.write("a.txt", "foo123bar\n");
    let out = p.walk_output(["-r", "${1}_${2}", "(foo)(\\d+)", "a.txt"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("foo_123bar"),
        "expected capture-group replacement, got: {stdout}"
    );
}

#[test]
fn replace_with_capture_groups_index() {
    let p = TestProject::new("replace-capture-index");
    p.write("a.txt", "foo123bar\n");
    p.build_index();
    let out = p.index_output(["-r", "${1}_${2}", "(foo)(\\d+)"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("foo_123bar"),
        "expected capture-group replacement, got: {stdout}"
    );
}

#[test]
fn replace_multiline_collapses_span_walk() {
    let p = TestProject::new("replace-multiline-walk");
    p.write("a.txt", "foo\nbar\n");
    let out = p.walk_output(["-U", "-r", "X", r"foo\nbar"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains('X'), "expected replacement, got: {stdout}");
    assert!(
        !stdout.contains("foo") && !stdout.contains("bar"),
        "replaced span should not print original lines, got: {stdout}"
    );
}

// ─── --trim ──────────────────────────────────────────────────────────────────

#[test]
fn trim_removes_leading_whitespace_walk() {
    let p = TestProject::new("trim-walk");
    p.write("a.txt", "    indented line\n");
    let out = p.walk_output(["--trim", "indented"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("indented line"),
        "expected trimmed output, got: {stdout}"
    );
    assert!(
        !stdout.contains("    indented"),
        "leading whitespace should be removed, got: {stdout}"
    );
}

#[test]
fn trim_consistent_index_and_walk() {
    let p = TestProject::new("trim-both");
    p.write("a.txt", "    hello world\n");
    p.build_index();
    let args = ["--trim", "hello"];
    let expected = format!("{}\n", rel_match("a.txt", "hello world"));

    let index = p.index_output(args);
    assert_success(&index);
    assert_stdout_eq(&index, &expected);
    assert_stderr_empty(&index);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stdout_eq(&walk, &expected);
    assert_stderr_empty(&walk);
}

// ─── --byte-offset / -b ─────────────────────────────────────────────────────

#[test]
fn byte_offset_shows_position_walk() {
    let p = TestProject::new("byte-offset-walk");
    p.write("a.txt", "first line\nhello world\n");
    let out = p.walk_output(["-b", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("11:"),
        "expected byte offset 11, got: {stdout}"
    );
}

#[test]
fn byte_offset_consistent_index_and_walk() {
    let p = TestProject::new("byte-offset-both");
    p.write("a.txt", "abc\nhello\n");
    p.build_index();
    let args = ["-b", "hello"];
    let expected = format!("{}\n", rel_match("a.txt", "4:hello"));

    let index = p.index_output(args);
    assert_success(&index);
    assert_stdout_eq(&index, &expected);
    assert_stderr_empty(&index);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stdout_eq(&walk, &expected);
    assert_stderr_empty(&walk);
}

// ─── --passthru ─────────────────────────────────────────────────────────────

#[test]
fn passthru_shows_all_lines_walk() {
    let p = TestProject::new("passthru-walk");
    p.write("a.txt", "first\nhello\nthird\n");
    let out = p.walk_output(["--passthru", "hello", "a.txt"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("first"),
        "passthru should show non-matching lines, got: {stdout}"
    );
    assert!(
        stdout.contains("hello"),
        "passthru should show matching lines, got: {stdout}"
    );
    assert!(
        stdout.contains("third"),
        "passthru should show non-matching lines, got: {stdout}"
    );
}

#[test]
fn passthru_alias_passthrough_walk() {
    let p = TestProject::new("passthrough-alias-walk");
    p.write("a.txt", "first\nhello\nthird\n");
    let out = p.walk_output(["--passthrough", "hello", "a.txt"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("first") && stdout.contains("third"),
        "passthrough alias should show all lines, got: {stdout}"
    );
}

// ─── --include-zero ─────────────────────────────────────────────────────────

#[test]
fn include_zero_in_count_mode_walk() {
    let p = TestProject::new("include-zero-walk");
    p.write("a.txt", "match\n");
    p.write("b.txt", "nothing here\n");
    let out = p.walk_output(["--count", "--include-zero", "match"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("a.txt:1"),
        "should show a.txt with count 1, got: {stdout}"
    );
    assert!(
        stdout.contains("b.txt:0"),
        "should show b.txt with count 0 when --include-zero, got: {stdout}"
    );
}

#[test]
fn include_zero_in_count_mode_index() {
    let p = TestProject::new("include-zero-index");
    p.write("a.txt", "match\n");
    p.write("b.txt", "nothing here\n");
    p.build_index();
    let out = p.index_output(["--count", "--include-zero", "match"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("a.txt:1"),
        "should show a.txt with count 1, got: {stdout}"
    );
    assert!(
        stdout.contains("b.txt:0"),
        "should show b.txt with count 0 when --include-zero, got: {stdout}"
    );
}

#[test]
fn count_without_include_zero_omits_zero_files_walk() {
    let p = TestProject::new("no-include-zero-walk");
    p.write("a.txt", "match\n");
    p.write("b.txt", "nothing here\n");
    let out = p.walk_output(["--count", "match"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("a.txt:1"),
        "should show a.txt with count 1, got: {stdout}"
    );
    assert!(
        !stdout.contains("b.txt"),
        "should NOT show b.txt without --include-zero, got: {stdout}"
    );
}
