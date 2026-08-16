mod common;

use common::{
    TestProject, assert_stderr_empty, assert_stdout_contains, assert_stdout_not_contains,
    assert_success, normalize_stdout,
};

// ─── default (quit on NUL) ───────────────────────────────────────────────────

#[test]
fn default_skips_binary_file_walk() {
    let p = TestProject::new("binary-default-walk");
    p.write("binary.txt", b"abc\x00match_here\n");
    p.write("text.txt", "match_here\n");

    let out = p.walk_output(["match_here"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("text.txt"),
        "should find match in text file"
    );
    assert!(
        !stdout.contains("binary.txt:") || !stdout.contains("match_here"),
        "should not show match from binary file (NUL before match)"
    );
}

#[test]
fn default_skips_binary_file_index() {
    let p = TestProject::new("binary-default-index");
    p.write("binary.txt", b"abc\x00match_here\n");
    p.write("text.txt", "match_here\n");
    p.build_index();

    let out = p.index_output(["match_here"]);
    assert_success(&out);
    assert_stdout_contains(&out, "text.txt");
}

#[test]
fn default_recursive_suppresses_binary_match_before_nul() {
    let p = TestProject::new("binary-default-recursive-before-nul");
    p.write("binary.txt", b"findme\x00later\n");

    let out = p.walk_output(["findme"]);
    assert!(
        !out.status.success(),
        "recursive binary-only search should not match"
    );
    assert_stdout_not_contains(&out, "binary file matches");
    assert_stdout_not_contains(&out, "findme");
    assert_stderr_empty(&out);
}

// ─── -a / --text ─────────────────────────────────────────────────────────────

#[test]
fn text_flag_searches_binary_walk() {
    let p = TestProject::new("text-flag-walk");
    p.write("binary.txt", b"findme\n\x00other\n");

    let out = p.walk_output(["-a", "findme"]);
    assert_success(&out);
    assert_stdout_contains(&out, "binary.txt");
    assert_stdout_contains(&out, "findme");
}

#[test]
fn text_flag_searches_binary_index() {
    let p = TestProject::new("text-flag-index");
    p.write("binary.txt", b"findme\n\x00other\n");
    p.build_index();

    let out = p.index_output(["--text", "findme"]);
    assert_success(&out);
    assert_stdout_contains(&out, "binary.txt");
    assert_stdout_contains(&out, "findme");
}

#[test]
fn text_flag_finds_match_after_nul_walk() {
    let p = TestProject::new("text-after-nul-walk");
    p.write("binary.txt", b"\x00findme\n");

    let out = p.walk_output(["-a", "findme"]);
    assert_success(&out);
    assert_stdout_contains(&out, "findme");
}

// ─── --binary / explicit binary reporting ────────────────────────────────────

#[test]
fn binary_flag_reports_binary_match_walk() {
    let p = TestProject::new("binary-flag-walk");
    p.write("mixed.txt", b"findme\nmore\x00stuff\n");

    let out = p.walk_output(["--binary", "findme"]);
    assert_success(&out);
    assert_stdout_contains(
        &out,
        "mixed.txt: binary file matches (found NUL byte around offset 11)",
    );
    assert_stdout_not_contains(&out, "findme");
    assert_stderr_empty(&out);
}

#[test]
fn binary_flag_reports_binary_match_index() {
    let p = TestProject::new("binary-flag-index");
    p.write("mixed.txt", b"findme\nmore\x00stuff\n");
    p.build_index();

    let out = p.index_output(["--binary", "findme"]);
    assert_success(&out);
    assert_stdout_contains(
        &out,
        "mixed.txt: binary file matches (found NUL byte around offset 11)",
    );
    assert_stdout_not_contains(&out, "findme");
    assert_stderr_empty(&out);
}

#[test]
fn explicit_binary_file_reports_match_before_nul() {
    let p = TestProject::new("binary-explicit-file");
    p.write("binary.txt", b"findme\x00later\n");

    let out = p.walk_output(["findme", "binary.txt"]);
    assert_success(&out);
    assert_stdout_contains(
        &out,
        "binary.txt: binary file matches (found NUL byte around offset 6)",
    );
    assert_stdout_not_contains(&out, "findme");
    assert_stderr_empty(&out);
}

#[test]
fn indexed_explicit_binary_file_reports_match_before_nul() {
    let p = TestProject::new("binary-explicit-file-index");
    p.write("binary.txt", b"findme\x00later\n");
    p.build_index();

    let out = p.index_output(["findme", "binary.txt"]);
    assert_success(&out);
    assert_stdout_contains(
        &out,
        "binary.txt: binary file matches (found NUL byte around offset 6)",
    );
    assert_stdout_not_contains(&out, "findme");
    assert_stderr_empty(&out);
}

#[test]
fn no_messages_does_not_suppress_binary_match_output() {
    let p = TestProject::new("binary-no-messages");
    p.write("binary.txt", b"findme\x00later\n");

    let out = p.walk_output(["--no-messages", "findme", "binary.txt"]);
    assert_success(&out);
    assert_stdout_contains(
        &out,
        "binary.txt: binary file matches (found NUL byte around offset 6)",
    );
    assert_stderr_empty(&out);
}

#[test]
fn count_reports_explicit_binary_match_before_nul() {
    let p = TestProject::new("binary-count-explicit-file");
    p.write("binary.txt", b"findme\x00later\n");

    let out = p.walk_output(["-c", "findme", "binary.txt"]);
    assert_success(&out);
    assert_stdout_contains(&out, "1");
    assert_stderr_empty(&out);
}

#[test]
fn files_with_matches_reports_explicit_binary_match_before_nul() {
    let p = TestProject::new("binary-files-explicit-file");
    p.write("binary.txt", b"findme\x00later\n");

    let out = p.walk_output(["-l", "findme", "binary.txt"]);
    assert_success(&out);
    assert_stdout_contains(&out, "binary.txt");
    assert_stderr_empty(&out);
}

// ─── text overrides binary ───────────────────────────────────────────────────

#[test]
fn text_overrides_binary() {
    let p = TestProject::new("text-overrides-binary");
    p.write("binary.txt", b"\x00findme\n");

    let out = p.walk_output(["--binary", "-a", "findme"]);
    assert_success(&out);
    assert_stdout_contains(&out, "findme");
}

// ─── consistency: index vs walk ──────────────────────────────────────────────

#[test]
fn text_mode_consistent_index_and_walk() {
    let p = TestProject::new("text-consistent");
    p.write("binary.txt", b"findme\n\x00other\n");
    p.write("text.txt", "findme\n");
    p.build_index();

    let index_out = p.index_output(["-a", "findme"]);
    assert_success(&index_out);

    let walk_out = p.walk_output(["-a", "findme"]);
    assert_success(&walk_out);

    let mut index_lines: Vec<_> = normalize_stdout(&index_out)
        .lines()
        .map(str::to_string)
        .collect();
    let mut walk_lines: Vec<_> = normalize_stdout(&walk_out)
        .lines()
        .map(str::to_string)
        .collect();
    index_lines.sort();
    walk_lines.sort();
    assert_eq!(
        index_lines, walk_lines,
        "index and walk should produce same results with -a"
    );
}
