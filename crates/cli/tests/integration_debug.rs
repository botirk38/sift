mod common;

use common::TestProject;

#[test]
fn debug_emits_stderr_diagnostics_and_keeps_matches() {
    let p = TestProject::new("debug-basic");
    p.write("a.txt", "hello world\n");
    p.write("b.txt", "goodbye\n");
    p.build_index();

    let output = p.index_output(["hello", "--debug"]);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "expected match on stdout, got: {stdout:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DEBUG sift-dir:"),
        "expected sift-dir debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG corpus-root:"),
        "expected corpus-root debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG index: loaded (queryable)"),
        "expected loaded index debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG search-mode:"),
        "expected search-mode debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG patterns:"),
        "expected patterns debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG scan-scope:"),
        "expected scan-scope debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG candidates:"),
        "expected candidates debug line, got: {stderr:?}"
    );
}

#[test]
fn debug_absent_index_notes_walk() {
    let p = TestProject::new("debug-walk");
    p.write("a.txt", "hello world\n");

    let output = p.walk_output(["hello", "--debug"]);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello"),
        "expected match on stdout, got: {stdout:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DEBUG index: absent"),
        "expected absent index debug line, got: {stderr:?}"
    );
    assert!(
        stderr.contains("DEBUG note: index absent"),
        "expected index-absent note, got: {stderr:?}"
    );
    assert!(
        !stdout.contains("DEBUG"),
        "debug lines must not appear on stdout, got: {stdout:?}"
    );
}
