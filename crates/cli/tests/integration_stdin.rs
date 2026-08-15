mod common;

use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Output, Stdio};

use common::{
    TestProject, assert_exit_code, assert_stderr_empty, assert_stdout_contains, assert_stdout_eq,
    assert_stdout_not_contains, assert_success, normalize_stdout,
};

fn output_with_stdin<I, S>(mut cmd: Command, args: I, stdin: &[u8]) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn sift");
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(stdin)
        .expect("write stdin");
    child.wait_with_output().expect("wait sift")
}

#[test]
fn piped_stdin_is_searched_when_no_paths_are_given() {
    let p = TestProject::new("stdin-implicit");
    let out = output_with_stdin(p.sift(), ["needle"], b"alpha\nneedle\nomega\n");

    assert_success(&out);
    assert_stdout_eq(&out, "needle\n");
}

#[test]
fn empty_piped_stdin_still_searches_walk_corpus() {
    let p = TestProject::new("stdin-empty-pipe-walk");
    p.write("hay.txt", "needle\n");
    let mut cmd = p.sift();
    cmd.arg("--sift-dir").arg(p.root().join(".sift-not-found"));
    let out = output_with_stdin(cmd, ["needle"], b"");

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "hay.txt:needle\n");
}

#[test]
fn piped_stdin_does_not_replace_indexed_default_corpus() {
    let p = TestProject::new("stdin-indexed-default-corpus");
    p.write("a.txt", "needle\n");
    p.build_index();
    let mut cmd = p.sift();
    cmd.arg("--sift-dir").arg(p.sift_dir());

    let out = output_with_stdin(cmd, ["needle"], b"no-match\n");

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "a.txt:needle\n");
}

#[test]
fn explicit_dash_searches_stdin() {
    let p = TestProject::new("stdin-explicit");
    let out = output_with_stdin(p.sift(), ["needle", "-"], b"alpha\nneedle\nomega\n");

    assert_success(&out);
    assert_stdout_eq(&out, "needle\n");
}

#[test]
fn explicit_dash_searches_stdin_with_other_paths() {
    let p = TestProject::new("stdin-explicit-with-path");
    p.write("hay.txt", "needle\n");

    let out = output_with_stdin(p.sift(), ["needle", "hay.txt", "-"], b"needle\n");

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "hay.txt:needle\n<stdin>:needle\n");
}

#[test]
fn stdin_with_filename_uses_ripgrep_stdin_name() {
    let p = TestProject::new("stdin-filename");
    let out = output_with_stdin(p.sift(), ["-H", "needle"], b"needle\n");

    assert_success(&out);
    assert_stdout_eq(&out, "<stdin>:needle\n");
}

#[test]
fn stdin_count_counts_stream_matches() {
    let p = TestProject::new("stdin-count");
    let out = output_with_stdin(p.sift(), ["--count", "needle", "-"], b"needle\nneedle\n");

    assert_success(&out);
    assert_stdout_eq(&out, "2\n");
}

#[test]
fn stdin_files_with_matches_prints_stdin_name() {
    let p = TestProject::new("stdin-files-with-matches");
    let out = output_with_stdin(
        p.sift(),
        ["--files-with-matches", "needle", "-"],
        b"needle\n",
    );

    assert_success(&out);
    assert_stdout_eq(&out, "<stdin>\n");
}

#[test]
fn binary_stdin_reports_match_before_nul() {
    let p = TestProject::new("stdin-binary-before-nul");
    let out = output_with_stdin(p.sift(), ["findme"], b"findme\0later\n");

    assert_success(&out);
    assert_stdout_contains(
        &out,
        "binary file matches (found \"/0\" byte around offset 6)",
    );
    assert_stdout_not_contains(&out, "findme");
    assert_stdout_not_contains(&out, "<stdin>");
    assert_stderr_empty(&out);
}

#[test]
fn binary_stdin_count_reports_match_before_nul() {
    let p = TestProject::new("stdin-binary-count-before-nul");
    let out = output_with_stdin(p.sift(), ["--count", "findme"], b"findme\0later\n");

    assert_success(&out);
    assert_stdout_eq(&out, "1\n");
    assert_stderr_empty(&out);
}

#[test]
fn stdin_json_uses_stdin_name() {
    let p = TestProject::new("stdin-json");
    let out = output_with_stdin(p.sift(), ["--json", "needle", "-"], b"needle\n");

    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#""text":"<stdin>""#),
        "expected JSON path to use <stdin>, got: {stdout}"
    );
}

#[test]
fn mixed_file_and_stdin_json_has_one_summary() {
    let p = TestProject::new("stdin-json-mixed");
    p.write("hay.txt", "needle\n");
    let out = output_with_stdin(p.sift(), ["--json", "needle", "hay.txt", "-"], b"needle\n");

    assert_success(&out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("hay.txt"), "stdout: {stdout}");
    assert!(stdout.contains(r#""text":"<stdin>""#), "stdout: {stdout}");
    assert_eq!(
        stdout.matches(r#""type":"summary""#).count(),
        1,
        "expected one JSON summary for the combined search: {stdout}"
    );
}

#[test]
fn pattern_file_dash_reads_patterns_from_stdin() {
    let p = TestProject::new("stdin-pattern-file");
    p.write("hay.txt", "alpha\nneedle\nomega\n");

    let out = output_with_stdin(p.sift(), ["-f", "-", "hay.txt"], b"needle\n");

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "hay.txt:needle\n");
}

#[test]
fn missing_stdin_match_exits_one() {
    let p = TestProject::new("stdin-no-match");
    let out = output_with_stdin(p.sift(), ["needle"], b"alpha\nomega\n");

    assert_exit_code(&out, 1);
    assert_stdout_eq(&out, "");
}
