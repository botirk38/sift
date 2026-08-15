mod common;

use common::TestProject;

#[test]
fn stats_reports_matches_and_files_searched() {
    let p = TestProject::new("stats-basic");
    p.write("a.txt", "hello world\n");
    p.write("b.txt", "goodbye\n");
    p.build_index();

    let output = p.index_output(["hello", "--stats"]);
    common::assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("1 matches"),
        "expected match count in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("1 files contained matches"),
        "expected files-with-matches count in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("1 files searched"),
        "expected narrowed files searched in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("bytes printed"),
        "expected bytes printed line in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("bytes searched"),
        "expected bytes searched line in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("s elapsed"),
        "expected elapsed line in stderr, got: {stderr:?}"
    );
}

#[test]
fn stats_reports_in_walk_mode() {
    let p = TestProject::new("stats-walk");
    p.write("a.txt", "hello world\n");
    p.write("b.txt", "goodbye\n");

    let output = p.walk_output(["hello", "--stats"]);
    common::assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("1 matches"),
        "expected match count in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("2 files searched"),
        "expected both files searched in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("1 files contained matches"),
        "expected files-with-matches count in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("bytes printed"),
        "expected bytes printed line in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("bytes searched"),
        "expected bytes searched line in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("s elapsed"),
        "expected elapsed line in stderr, got: {stderr:?}"
    );
}

#[test]
fn stats_counts_matches_across_multiple_files() {
    let p = TestProject::new("stats-multi-file");
    p.write("a.txt", "hit\n");
    p.write("b.txt", "hit\n");
    p.write("c.txt", "miss\n");

    let output = p.walk_output(["hit", "--stats"]);
    common::assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("2 matches"),
        "expected two match count in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("2 files contained matches"),
        "expected two files with matches in stderr, got: {stderr:?}"
    );
    assert!(
        stderr.contains("3 files searched"),
        "expected all files searched in stderr, got: {stderr:?}"
    );
}

#[test]
fn count_mode_narrows_indexed_candidates() {
    let p = TestProject::new("stats-count-narrow");
    p.write("a.txt", "hello world\n");
    p.write("b.txt", "goodbye\n");
    p.build_index();

    // `-E none` keeps index narrowing enabled (default Auto may disable it).
    let output = p.index_output(["hello", "-c", "-E", "none", "--stats"]);
    common::assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("1 files searched"),
        "expected -c without --include-zero to narrow like line search, got: {stderr:?}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a.txt:1"),
        "expected count for matching file, got: {stdout:?}"
    );
    assert!(
        !stdout.contains("b.txt"),
        "expected non-matching file omitted without --include-zero, got: {stdout:?}"
    );
}

#[test]
fn stats_with_files_with_matches_reports_presence_tally() {
    let p = TestProject::new("stats-l-presence");
    // Multiple hits in one file: -l stops after the first, so the tally is
    // files-with-a-hit (2), not a full span count (3).
    p.write("a.txt", "hit\nhit\n");
    p.write("b.txt", "hit\n");
    p.write("c.txt", "miss\n");
    p.build_index();

    let output = p.index_output(["-l", "hit", "--stats"]);
    common::assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("a.txt") && stdout.contains("b.txt") && !stdout.contains("c.txt"),
        "expected matching paths only, got: {stdout:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("2 matches"),
        "expected presence tally equal to files with matches, got: {stderr:?}"
    );
    assert!(
        stderr.contains("2 files contained matches"),
        "expected files-with-matches count, got: {stderr:?}"
    );
}

#[test]
fn stats_with_files_without_match_reports_presence_tally() {
    let p = TestProject::new("stats-L-presence");
    p.write("a.txt", "hit\nhit\n");
    p.write("b.txt", "hit\n");
    p.write("c.txt", "miss\n");
    p.build_index();

    let output = p.index_output(["--files-without-match", "hit", "--stats"]);
    common::assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("c.txt") && !stdout.contains("a.txt") && !stdout.contains("b.txt"),
        "expected only non-matching path, got: {stdout:?}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("2 matches"),
        "expected presence tally of files that did match, got: {stderr:?}"
    );
    assert!(
        stderr.contains("2 files contained matches"),
        "expected files-with-matches count, got: {stderr:?}"
    );
}
