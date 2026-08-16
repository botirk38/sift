mod common;

use common::TestProject;

#[test]
fn null_terminates_paths_with_files_with_matches() {
    let p = TestProject::new("integration-null-l");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "other\n");
    p.build_index();

    let output = p.index_output(["-l", "--null", "needle"]);
    common::assert_success(&output);

    assert!(
        output.stdout.contains(&b'\0'),
        "expected NUL between path records, got {:?}",
        output.stdout
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains('\n'),
        "with --null, path list should not use newlines"
    );
}

#[test]
fn null_data_treats_nul_as_line_terminator() {
    let p = TestProject::new("integration-null-data");
    p.write("a.txt", b"alpha\0needle\0omega\0");
    p.build_index();

    let output = p.index_output(["--null-data", "needle"]);
    common::assert_success(&output);

    assert!(
        output.stdout.ends_with(b"\0"),
        "expected NUL-terminated match record, got {:?}",
        output.stdout
    );
    assert!(
        !output.stdout.contains(&b'\n'),
        "--null-data output should use NUL record terminators"
    );
}

#[test]
fn null_data_terminates_count_records_with_nul() {
    let p = TestProject::new("integration-null-data-count");
    p.write("a.txt", b"alpha\0needle\0omega\0");
    p.build_index();

    let output = p.index_output(["--null-data", "--count", "needle"]);
    common::assert_success(&output);

    assert_eq!(output.stdout, b"a.txt\0\x31\0");
}

#[test]
fn null_data_terminates_files_with_matches_with_nul() {
    let p = TestProject::new("integration-null-data-files-with-matches");
    p.write("a.txt", b"alpha\0needle\0");
    p.write("b.txt", b"alpha\0omega\0");
    p.build_index();

    let output = p.index_output(["--null-data", "--files-with-matches", "needle"]);
    common::assert_success(&output);

    assert_eq!(output.stdout, b"a.txt\0");
}

#[test]
fn null_data_terminates_files_without_match_with_nul() {
    let p = TestProject::new("integration-null-data-files-without-match");
    p.write("a.txt", b"alpha\0needle\0");
    p.write("b.txt", b"alpha\0omega\0");
    p.build_index();

    let output = p.index_output(["--null-data", "--files-without-match", "needle"]);
    common::assert_success(&output);

    assert_eq!(output.stdout, b"b.txt\0");
}

#[test]
fn null_data_terminates_context_separator_with_nul() {
    let p = TestProject::new("integration-null-data-context-separator");
    p.write("a.txt", b"before\0needle\0gap\0gap\0gap\0needle\0after\0");
    p.build_index();

    let output = p.index_output(["--null-data", "--context", "1", "needle"]);
    common::assert_success(&output);

    assert!(
        output.stdout.windows(3).any(|window| window == b"--\0"),
        "expected NUL-terminated context separator, got {:?}",
        output.stdout
    );
    assert!(
        !output.stdout.contains(&b'\n'),
        "--null-data context output should not contain newline terminators"
    );
}

#[test]
fn null_data_is_separate_from_null_path_terminator() {
    let p = TestProject::new("integration-null-data-separate");
    p.write("a.txt", b"alpha\0needle\0");
    p.build_index();

    let output = p.index_output(["--null", "needle"]);
    common::assert_exit_code(&output, 1);

    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("needle"),
        "--null alone should not switch input records to NUL data"
    );
}

#[test]
fn color_always_emits_ansi_on_stdout() {
    let p = TestProject::new("integration-color");
    p.write("t.txt", "needle\n");
    p.build_index();

    let output = p.index_output(["--color=always", "needle", "t.txt"]);
    common::assert_success(&output);

    let s = common::normalize_stdout(&output);
    assert!(
        s.contains('\x1b'),
        "expected ANSI escapes with --color=always, got {s:?}"
    );
}

#[test]
fn color_ansi_emits_ansi_on_captured_stdout() {
    let p = TestProject::new("integration-color-ansi");
    p.write("t.txt", "needle\n");

    let output = p.walk_output(["--color=ansi", "needle", "t.txt"]);
    common::assert_success(&output);

    let s = common::normalize_stdout(&output);
    assert!(
        s.contains('\x1b'),
        "expected ANSI escapes with --color=ansi, got {s:?}"
    );
}

#[test]
fn colors_customize_match_style() {
    let p = TestProject::new("integration-colors-match");
    p.write("t.txt", "needle\n");

    let output = p.walk_output([
        "--color=always",
        "--colors",
        "match:fg:blue",
        "--colors",
        "match:style:nobold",
        "needle",
        "t.txt",
    ]);
    common::assert_success(&output);

    let s = common::normalize_stdout(&output);
    assert!(
        s.contains("\x1b[0m\x1b[34mneedle\x1b[0m"),
        "expected custom blue non-bold match style, got {s:?}"
    );
}

#[test]
fn colors_customize_path_style() {
    let p = TestProject::new("integration-colors-path");
    p.write("t.txt", "needle\n");

    let output = p.walk_output([
        "--color=always",
        "--colors",
        "path:fg:blue",
        "--with-filename",
        "needle",
        "t.txt",
    ]);
    common::assert_success(&output);

    let s = common::normalize_stdout(&output);
    assert!(
        s.contains("\x1b[0m\x1b[34mt.txt\x1b[0m:"),
        "expected custom blue path style, got {s:?}"
    );
}

#[test]
fn invalid_colors_spec_exits_with_error() {
    let p = TestProject::new("integration-colors-invalid");
    p.write("t.txt", "needle\n");

    let output = p.walk_output(["--colors", "bogus:fg:red", "needle", "t.txt"]);

    common::assert_exit_code(&output, 2);
    let stderr = common::normalize_stderr(&output);
    assert!(
        stderr.contains("unrecognized output type 'bogus'"),
        "expected invalid color spec error, got {stderr:?}"
    );
}

#[test]
fn hyperlink_format_vscode_wraps_path() {
    let p = TestProject::new("integration-hyperlink-vscode");
    p.write("t.txt", "needle\n");

    let output = p.walk_output([
        "--color=always",
        "--hyperlink-format=vscode",
        "--with-filename",
        "--line-number",
        "--column",
        "needle",
        "t.txt",
    ]);
    common::assert_success(&output);

    let s = String::from_utf8_lossy(&output.stdout);
    assert!(
        s.contains("\x1b]8;;vscode://file")
            && s.contains("t.txt")
            && s.contains(":1:1")
            && s.contains("\x1b]8;;\x1b\\:"),
        "expected OSC-8 vscode hyperlink with path:line:column, got {s:?}"
    );
}

#[test]
fn colors_256_and_line_number() {
    let p = TestProject::new("integration-colors-256-line");
    p.write("t.txt", "needle\n");

    let output = p.walk_output([
        "--color=always",
        "--colors",
        "match:fg:32",
        "--colors",
        "match:style:nobold",
        "--line-number",
        "needle",
        "t.txt",
    ]);
    common::assert_success(&output);

    let s = common::normalize_stdout(&output);
    assert!(
        s.contains("\x1b[0m\x1b[32m1\x1b[0m:") && s.contains("\x1b[0m\x1b[38;5;32mneedle\x1b[0m"),
        "expected green line number and 256-color match, got {s:?}"
    );
}

#[cfg(unix)]
#[test]
fn hyperlink_hostname_bin_interpolates_host() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("integration-hyperlink-host");
    p.write("t.txt", "needle\n");
    p.write("host.sh", "#!/bin/sh\necho testhost\n");
    let script = p.root().join("host.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let output = p.walk_output([
        "--color=always",
        "--hyperlink-format=file://{host}{path}",
        "--hostname-bin",
        script.to_str().unwrap(),
        "--with-filename",
        "needle",
        "t.txt",
    ]);
    common::assert_success(&output);

    let s = String::from_utf8_lossy(&output.stdout);
    assert!(
        s.contains("\x1b]8;;file://testhost/") && s.contains("t.txt"),
        "expected file://testhost hyperlink, got {s:?}"
    );
}
