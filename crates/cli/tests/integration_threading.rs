mod common;

use common::{TestProject, assert_success, normalize_stdout};

// ─── --threads / -j ────────────────────────────────────────────────────────────

#[test]
fn threads_flag_runs_search_walk() {
    let p = TestProject::new("threads-walk");
    p.write("a.txt", "hello world\n");
    let out = p.walk_output(["-j", "1", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello world"),
        "expected match with -j 1, got: {stdout}"
    );
}

#[test]
fn threads_flag_accepts_larger_value_walk() {
    let p = TestProject::new("threads-large-walk");
    p.write("a.txt", "line\n");
    let out = p.walk_output(["--threads", "4", "line"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("line"),
        "expected match with --threads 4, got: {stdout}"
    );
}

// ─── --line-buffered / --block-buffered ────────────────────────────────────────

#[test]
fn line_buffered_accepted_walk() {
    let p = TestProject::new("line-buffered-walk");
    p.write("a.txt", "hello\n");
    let out = p.walk_output(["--line-buffered", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello"),
        "expected output with --line-buffered, got: {stdout}"
    );
}

#[test]
fn block_buffered_accepted_walk() {
    let p = TestProject::new("block-buffered-walk");
    p.write("a.txt", "hello\n");
    let out = p.walk_output(["--block-buffered", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello"),
        "expected output with --block-buffered, got: {stdout}"
    );
}

// ─── --path-separator ──────────────────────────────────────────────────────────

#[test]
fn path_separator_replaces_separator_walk() {
    let p = TestProject::new("path-sep-walk");
    p.write("sub/a.txt", "match\n");
    let out = p.walk_output(["--path-separator", ":", "match"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("sub:a.txt"),
        "expected path with ':' separator, got: {stdout}"
    );
}

// ─── --one-file-system ─────────────────────────────────────────────────────────

#[test]
fn one_file_system_accepted_walk() {
    let p = TestProject::new("one-fs-walk");
    p.write("a.txt", "data\n");
    let out = p.walk_output(["--one-file-system", "data"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("data"),
        "expected match with --one-file-system, got: {stdout}"
    );
}

// ─── --io ────────────────────────────────────────────────────────────────────

#[test]
fn io_mmap_flag_accepted_walk() {
    let p = TestProject::new("io-mmap-walk");
    p.write("a.txt", "hello\n");
    let out = p.walk_output(["--io", "mmap", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello"),
        "expected output with --io mmap, got: {stdout}"
    );
}

#[test]
fn io_sync_flag_accepted_walk() {
    let p = TestProject::new("io-sync-walk");
    p.write("a.txt", "hello\n");
    let out = p.walk_output(["--io", "sync", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello"),
        "expected output with --io sync, got: {stdout}"
    );
}

// ─── -U/--multiline ────────────────────────────────────────────────────────────

#[test]
fn multiline_matches_across_lines_walk() {
    let p = TestProject::new("multiline-walk");
    p.write("a.txt", "foo\nbar\n");
    let out = p.walk_output(["-U", r"foo\nbar"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("foo") && stdout.contains("bar"),
        "expected multiline match, got: {stdout}"
    );
}

#[test]
fn multiline_long_flag_walk() {
    let p = TestProject::new("multiline-long-walk");
    p.write("a.txt", "alpha\nbeta\n");
    let out = p.walk_output(["--multiline", r"alpha\nbeta"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("alpha") && stdout.contains("beta"),
        "expected multiline match, got: {stdout}"
    );
}

#[test]
fn multiline_only_matching_prints_span_walk() {
    let p = TestProject::new("multiline-only-matching");
    p.write("a.txt", "foo\nbar\n");
    let out = p.walk_output(["-U", "-o", "--no-filename", r"foo\nbar"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("foo") && stdout.contains("bar"),
        "expected full multiline span, got: {stdout}"
    );
}

// ─── --multiline-dotall ────────────────────────────────────────────────────────

#[test]
fn multiline_dotall_dot_matches_newline_walk() {
    let p = TestProject::new("dotall-walk");
    p.write("a.txt", "start\nend\n");
    let out = p.walk_output(["-U", "--multiline-dotall", "start.end"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("start") && stdout.contains("end"),
        "expected dotall match, got: {stdout}"
    );
}

// ─── --crlf ────────────────────────────────────────────────────────────────────

#[test]
fn crlf_flag_matches_with_carriage_return_walk() {
    let p = TestProject::new("crlf-walk");
    p.write("a.txt", "hello world\r\n");
    let out = p.walk_output(["--crlf", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("hello"),
        "expected match with --crlf, got: {stdout}"
    );
}

#[test]
fn crlf_word_boundary_respects_cr_walk() {
    let p = TestProject::new("crlf-word-walk");
    p.write("a.txt", "hello world\r\n");
    let out = p.walk_output(["--crlf", "world$"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("world"),
        "expected 'world' with --crlf and $, got: {stdout}"
    );
}

// ─── Index + walk consistency ──────────────────────────────────────────────────

#[test]
fn crlf_consistent_index_and_walk() {
    let p = TestProject::new("crlf-consistency");
    p.write("a.txt", "hello world\r\n");
    p.build_index();
    let index_out = p.index_output(["--crlf", "hello"]);
    let walk_out = p.walk_output(["--crlf", "hello"]);
    assert_success(&index_out);
    assert_success(&walk_out);
    assert_eq!(
        normalize_stdout(&index_out),
        normalize_stdout(&walk_out),
        "index and walk --crlf results differ"
    );
}

#[test]
fn multiline_dotall_consistent_index_and_walk() {
    let p = TestProject::new("dotall-consistency");
    p.write("a.txt", "start\nend\n");
    p.build_index();
    let index_out = p.index_output(["-U", "--multiline-dotall", "start.end"]);
    let walk_out = p.walk_output(["-U", "--multiline-dotall", "start.end"]);
    assert_success(&index_out);
    assert_success(&walk_out);
    assert_eq!(
        normalize_stdout(&index_out),
        normalize_stdout(&walk_out),
        "index and walk --multiline-dotall results differ"
    );
}

#[test]
fn multiline_consistent_index_and_walk() {
    let p = TestProject::new("multiline-consistency");
    p.write("a.txt", "foo\nbar\n");
    p.build_index();
    let index_out = p.index_output(["-U", r"foo\nbar"]);
    let walk_out = p.walk_output(["-U", r"foo\nbar"]);
    assert_success(&index_out);
    assert_success(&walk_out);
    assert_eq!(
        normalize_stdout(&index_out),
        normalize_stdout(&walk_out),
        "index and walk multiline results differ"
    );
}

#[test]
fn threads_consistent_index_and_walk() {
    let p = TestProject::new("threads-consistency");
    p.write("a.txt", "hello\n");
    p.build_index();
    let index_out = p.index_output(["-j", "1", "hello"]);
    let walk_out = p.walk_output(["-j", "1", "hello"]);
    assert_success(&index_out);
    assert_success(&walk_out);
    assert_eq!(
        normalize_stdout(&index_out),
        normalize_stdout(&walk_out),
        "index and walk results differ with -j 1"
    );
}
