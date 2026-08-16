mod common;

use common::{TestProject, assert_exit_code, assert_success, normalize_stderr, rel_match};

const GZIP_HAYSTACK: &[u8] = &[
    31, 139, 8, 0, 0, 0, 0, 0, 2, 19, 75, 204, 41, 200, 72, 228, 202, 75, 77, 77, 201, 73, 229,
    202, 207, 77, 77, 79, 228, 2, 0, 194, 77, 84, 228, 19, 0, 0, 0,
];

#[test]
fn search_zip_reads_gzip_content() {
    let p = TestProject::new("search-zip-gzip");
    p.write("hay.txt.gz", GZIP_HAYSTACK);

    let out = p.walk_output(["--search-zip", "needle", "hay.txt.gz"]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("hay.txt.gz", "needle\n"));
}

#[test]
fn search_zip_missing_tool_passthrough() {
    let p = TestProject::new("search-zip-missing-tool");
    p.write("hay.txt.gz", GZIP_HAYSTACK);
    let empty = p.root().join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();

    let mut cmd = p.sift();
    cmd.arg("--sift-dir").arg(p.root().join(".sift-not-found"));
    cmd.env("PATH", &empty);
    cmd.args(["--search-zip", "needle", "hay.txt.gz"]);
    let out = cmd.output().unwrap();

    assert_exit_code(&out, 1);
    assert!(
        !normalize_stderr(&out).contains("gzip"),
        "missing decompressor should passthrough, got {:?}",
        normalize_stderr(&out)
    );
}

#[test]
fn indexed_search_zip_bypasses_raw_index_narrowing() {
    let p = TestProject::new("indexed-search-zip-gzip");
    p.write("hay.txt.gz", GZIP_HAYSTACK).build_index();

    let out = p.index_output(["--search-zip", "needle", "hay.txt.gz"]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("hay.txt.gz", "needle\n"));
}

#[cfg(unix)]
#[test]
fn preprocessor_stdout_is_searched() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("preprocessor-search");
    p.write("hay.txt", "alpha\nneedle\n");
    p.write(
        "upper.sh",
        "#!/bin/sh\ntr '[:lower:]' '[:upper:]' < \"$1\"\n",
    );
    let script = p.root().join("upper.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let out = p.walk_output(["--pre", script.to_str().unwrap(), "NEEDLE", "hay.txt"]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("hay.txt", "NEEDLE\n"));
}

#[cfg(unix)]
#[test]
fn preprocessor_path_with_space_is_not_split() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("preprocessor-path-with-space");
    p.write("hay.txt", "alpha\nneedle\n");
    p.write(
        "upper script.sh",
        "#!/bin/sh\ntr '[:lower:]' '[:upper:]' < \"$1\"\n",
    );
    let script = p.root().join("upper script.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let out = p.walk_output(["--pre", script.to_str().unwrap(), "NEEDLE", "hay.txt"]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("hay.txt", "NEEDLE\n"));
}

#[cfg(unix)]
#[test]
fn indexed_preprocessor_bypasses_raw_index_narrowing() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("indexed-preprocessor-search");
    p.write("hay.txt", "alpha\nneedle\n");
    p.write(
        "upper.sh",
        "#!/bin/sh\ntr '[:lower:]' '[:upper:]' < \"$1\"\n",
    );
    let script = p.root().join("upper.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();
    p.build_index();

    let out = p.index_output(["--pre", script.to_str().unwrap(), "NEEDLE", "hay.txt"]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("hay.txt", "NEEDLE\n"));
}

#[cfg(unix)]
#[test]
fn pre_glob_limits_preprocessor() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("preprocessor-glob");
    p.write("a.txt", "alpha\nneedle\n");
    p.write("b.md", "alpha\nneedle\n");
    p.write(
        "upper.sh",
        "#!/bin/sh\ntr '[:lower:]' '[:upper:]' < \"$1\"\n",
    );
    let script = p.root().join("upper.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let out = p.walk_output([
        "--pre",
        script.to_str().unwrap(),
        "--pre-glob",
        "*.txt",
        "NEEDLE",
    ]);

    assert_success(&out);
    common::assert_stdout_eq(&out, &rel_match("a.txt", "NEEDLE\n"));
}

#[cfg(unix)]
#[test]
fn failing_preprocessor_reports_error() {
    use std::os::unix::fs::PermissionsExt;

    let p = TestProject::new("preprocessor-failure");
    p.write("hay.txt", "needle\n");
    p.write("fail.sh", "#!/bin/sh\necho nope >&2\nexit 7\n");
    let script = p.root().join("fail.sh");
    let mut permissions = std::fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script, permissions).unwrap();

    let out = p.walk_output(["--pre", script.to_str().unwrap(), "needle", "hay.txt"]);

    assert_exit_code(&out, 2);
    assert!(normalize_stderr(&out).contains("preprocessor"));
}

#[test]
fn invalid_pre_glob_is_reported() {
    let p = TestProject::new("preprocessor-invalid-glob");
    p.write("hay.txt", "needle\n");

    let out = p.walk_output(["--pre", "cat", "--pre-glob", "[", "needle", "hay.txt"]);

    assert_exit_code(&out, 2);
    assert!(normalize_stderr(&out).contains("invalid --pre-glob"));
}
