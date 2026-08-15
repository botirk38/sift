mod common;

use std::process::Command;

use common::{
    TestProject, assert_exit_code, assert_stderr_empty, assert_success, normalize_stderr,
    normalize_stdout,
};

// ─── --max-depth ─────────────────────────────────────────────────────────────

fn setup_depth_tree(name: &str) -> TestProject {
    let p = TestProject::new(name);
    p.mkdir("a/b/c");
    p.write("top.txt", "hello\n");
    p.write("a/mid.txt", "hello\n");
    p.write("a/b/deep.txt", "hello\n");
    p.write("a/b/c/deeper.txt", "hello\n");
    p
}

#[test]
fn max_depth_limits_walk_search() {
    let p = setup_depth_tree("max-depth-walk");

    let out = p.walk_output(["--max-depth", "1", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("top.txt"), "should find top.txt");
    assert!(
        stdout.contains("a/mid.txt"),
        "should find a/mid.txt at depth 1"
    );
    assert!(
        !stdout.contains("deep.txt"),
        "should NOT find deep.txt at depth > 1"
    );
    assert!(
        !stdout.contains("deeper.txt"),
        "should NOT find deeper.txt at depth > 1"
    );
}

#[test]
fn max_depth_limits_index_search() {
    let p = setup_depth_tree("max-depth-index");
    p.build_index();

    let out = p.index_output(["--max-depth", "1", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("top.txt"), "should find top.txt");
    assert!(
        stdout.contains("a/mid.txt"),
        "should find a/mid.txt at depth 1"
    );
    assert!(
        !stdout.contains("deep.txt"),
        "should NOT find deep.txt at depth > 1"
    );
}

#[test]
fn max_depth_zero_finds_only_root_files() {
    let p = setup_depth_tree("max-depth-zero");

    let out = p.walk_output(["--max-depth", "0", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("top.txt"), "should find top.txt at depth 0");
    assert!(
        !stdout.contains("mid.txt"),
        "should NOT find mid.txt at depth 0"
    );
}

// ─── --max-filesize ──────────────────────────────────────────────────────────

fn setup_filesize_tree(name: &str) -> TestProject {
    let p = TestProject::new(name);
    p.write("small.txt", "hello\n");
    p.write("big.txt", "hello\n".repeat(1000));
    p
}

#[test]
fn max_filesize_skips_large_files_walk() {
    let p = setup_filesize_tree("max-filesize-walk");

    let out = p.walk_output(["--max-filesize", "100", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("small.txt"), "should find small.txt");
    assert!(
        !stdout.contains("big.txt"),
        "should skip big.txt above 100 bytes"
    );
}

#[test]
fn max_filesize_skips_large_files_index() {
    let p = setup_filesize_tree("max-filesize-index");
    p.build_index();

    let out = p.index_output(["--max-filesize", "100", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("small.txt"), "should find small.txt");
    assert!(
        !stdout.contains("big.txt"),
        "should skip big.txt above 100 bytes"
    );
}

#[test]
fn max_filesize_suffix_k_walk() {
    let p = setup_filesize_tree("max-filesize-k-walk");

    let out = p.walk_output(["--max-filesize", "1K", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("small.txt"),
        "should find small.txt under 1K"
    );
    assert!(!stdout.contains("big.txt"), "should skip big.txt above 1K");
}

// ─── --iglob ─────────────────────────────────────────────────────────────────

#[test]
fn iglob_case_insensitive_filter_walk() {
    let p = TestProject::new("iglob-walk");
    p.write("file.TXT", "hello\n");
    p.write("file.rs", "hello\n");

    let out = p.walk_output(["--iglob", "*.txt", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("file.TXT"),
        "iglob *.txt should match file.TXT case-insensitively"
    );
    assert!(
        !stdout.contains("file.rs"),
        "iglob *.txt should not match file.rs"
    );
}

#[test]
fn iglob_case_insensitive_filter_index() {
    let p = TestProject::new("iglob-index");
    p.write("file.TXT", "hello\n");
    p.write("file.rs", "hello\n");
    p.build_index();

    let out = p.index_output(["--iglob", "*.txt", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("file.TXT"),
        "iglob *.txt should match file.TXT case-insensitively"
    );
    assert!(
        !stdout.contains("file.rs"),
        "iglob *.txt should not match file.rs"
    );
}

// ─── --ignore-file ───────────────────────────────────────────────────────────

#[test]
fn ignore_file_custom_walk() {
    let p = TestProject::new("ignore-file-walk");
    p.write("a.txt", "hello\n");
    p.write("b.log", "hello\n");
    p.write("myignore", "*.log\n");

    let out = p.walk_output(["--ignore-file", "myignore", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("a.txt"), "should find a.txt");
    assert!(
        !stdout.contains("b.log"),
        "should skip b.log via custom ignore file"
    );
}

#[test]
fn ignore_file_custom_index() {
    let p = TestProject::new("ignore-file-index");
    p.write("a.txt", "hello\n");
    p.write("b.log", "hello\n");
    p.write("myignore", "*.log\n");
    p.build_index();

    let out = p.index_output(["--ignore-file", "myignore", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("a.txt"), "should find a.txt");
    assert!(
        !stdout.contains("b.log"),
        "should skip b.log via custom ignore file"
    );
}

// ─── --files ─────────────────────────────────────────────────────────────────

#[test]
fn files_lists_matching_paths_walk() {
    let p = TestProject::new("files-walk");
    p.write("a.txt", "content\n");
    p.write("b.rs", "content\n");

    let out = p.walk_output(["--files"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("a.txt"), "should list a.txt");
    assert!(stdout.contains("b.rs"), "should list b.rs");
}

#[test]
fn files_respects_glob_filter() {
    let p = TestProject::new("files-glob");
    p.write("a.txt", "content\n");
    p.write("b.rs", "content\n");

    let out = p.walk_output(["--files", "-g", "*.txt"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("a.txt"), "should list a.txt matching *.txt");
    assert!(!stdout.contains("b.rs"), "should not list b.rs");
}

#[test]
fn files_path_scopes_listing_indexed() {
    let p = TestProject::new("files-path-scope-index");
    p.mkdir("a");
    p.mkdir("b");
    p.write("a/x.txt", "content\n");
    p.write("b/y.txt", "content\n");
    p.build_index();

    let out = p.index_output(["--files", "a"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert_eq!(stdout, "a/x.txt\n", "unexpected stdout: {stdout}");
}

#[test]
fn files_path_scopes_listing_walk() {
    let p = TestProject::new("files-path-scope-walk");
    p.mkdir("a");
    p.mkdir("b");
    p.write("a/x.txt", "content\n");
    p.write("b/y.txt", "content\n");

    let out = p.walk_output(["--files", "a"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert_eq!(stdout, "a/x.txt\n", "unexpected stdout: {stdout}");
}

#[test]
fn files_multiple_paths_union() {
    let p = TestProject::new("files-multi-path");
    p.mkdir("a");
    p.mkdir("b");
    p.mkdir("c");
    p.write("a/x.txt", "content\n");
    p.write("b/y.txt", "content\n");
    p.write("c/z.txt", "content\n");
    p.build_index();

    let out = p.index_output(["--files", "a", "b"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("a/x.txt") && stdout.contains("b/y.txt"));
    assert!(!stdout.contains("c/z.txt"), "unexpected stdout: {stdout}");
}

// ─── --type / --type-not / --type-list / --type-add / --type-clear ───────────

#[test]
fn type_filter_includes_rust_files_walk() {
    let p = TestProject::new("type-include-walk");
    p.write("lib.rs", "hello\n");
    p.write("script.py", "hello\n");
    p.write("notes.txt", "hello\n");

    let out = p.walk_output(["-t", "rust", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("lib.rs"), "should find lib.rs with -t rust");
    assert!(
        !stdout.contains("script.py"),
        "should skip script.py with -t rust"
    );
    assert!(
        !stdout.contains("notes.txt"),
        "should skip notes.txt with -t rust"
    );
}

#[test]
fn type_filter_includes_rust_files_index() {
    let p = TestProject::new("type-include-index");
    p.write("lib.rs", "hello\n");
    p.write("script.py", "hello\n");
    p.build_index();

    let out = p.index_output(["-t", "rust", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("lib.rs"), "should find lib.rs with -t rust");
    assert!(
        !stdout.contains("script.py"),
        "should skip script.py with -t rust"
    );
}

#[test]
fn type_not_excludes_python_walk() {
    let p = TestProject::new("type-not-walk");
    p.write("lib.rs", "hello\n");
    p.write("script.py", "hello\n");

    let out = p.walk_output(["-T", "py", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("lib.rs"), "should find lib.rs with -T py");
    assert!(
        !stdout.contains("script.py"),
        "should skip script.py with -T py"
    );
}

#[test]
fn type_not_excludes_python_index() {
    let p = TestProject::new("type-not-index");
    p.write("lib.rs", "hello\n");
    p.write("script.py", "hello\n");
    p.build_index();

    let out = p.index_output(["-T", "py", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("lib.rs"), "should find lib.rs with -T py");
    assert!(
        !stdout.contains("script.py"),
        "should skip script.py with -T py"
    );
}

#[test]
fn type_list_output() {
    let out = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("--type-list")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("rust: *.rs"), "should list rust type");
    assert!(stdout.contains("py: *.py"), "should list py type");
    assert!(stdout.contains("js:"), "should list js type");
    assert!(stdout.contains("*.js"), "js type should include *.js");
}

#[test]
fn type_add_creates_custom_type() {
    let p = TestProject::new("type-add");
    p.write("a.xyz", "hello\n");
    p.write("b.txt", "hello\n");

    let out = p.walk_output(["--type-add", "xyz:*.xyz", "-t", "xyz", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("a.xyz"),
        "custom type xyz should match a.xyz"
    );
    assert!(
        !stdout.contains("b.txt"),
        "custom type xyz should not match b.txt"
    );
}

#[test]
fn type_clear_removes_builtin() {
    let out = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("--type-clear")
        .arg("rust")
        .arg("--type-list")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        !stdout.contains("rust:"),
        "--type-clear rust should remove rust from type list"
    );
    assert!(
        stdout.contains("py:"),
        "other types should still exist after clearing rust"
    );
}

#[test]
fn type_clear_then_type_add_redefines_type() {
    let out = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("--type-clear")
        .arg("rust")
        .arg("--type-add")
        .arg("rust:*.rsx")
        .arg("--type-list")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("rust: *.rsx"),
        "later --type-add should redefine cleared rust type, got: {stdout}"
    );
    assert!(
        !stdout.contains("rust: *.rs\n"),
        "cleared built-in rust glob should not survive redefinition: {stdout}"
    );
}

#[test]
fn type_add_then_type_clear_removes_custom_type() {
    let out = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("--type-add")
        .arg("custom:*.custom")
        .arg("--type-clear")
        .arg("custom")
        .arg("--type-list")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        !stdout.contains("custom:"),
        "later --type-clear should remove custom type, got: {stdout}"
    );
}

#[test]
fn type_add_include_unknown_type_errors() {
    let out = Command::new(env!("CARGO_BIN_EXE_sift"))
        .arg("--type-add")
        .arg("combo:include:rust,notatype")
        .arg("--type-list")
        .output()
        .unwrap();
    assert_exit_code(&out, 2);
    let stderr = normalize_stderr(&out);
    assert!(
        stderr.contains("invalid type definition 'combo:include:rust,notatype'"),
        "expected invalid include diagnostic, got: {stderr}"
    );
}

#[test]
fn type_all_includes_known_file_types() {
    let p = TestProject::new("type-all-include");
    p.write("src.rs", "hello\n");
    p.write("script.py", "hello\n");
    p.write("data.unknownext", "hello\n");

    p.build_index();
    let indexed = p.index_output(["-t", "all", "hello"]);
    assert_success(&indexed);
    let indexed_stdout = normalize_stdout(&indexed);
    assert!(
        indexed_stdout.contains("src.rs"),
        "rust file should match -t all with index"
    );
    assert!(
        indexed_stdout.contains("script.py"),
        "python file should match -t all with index"
    );
    assert!(
        !indexed_stdout.contains("data.unknownext"),
        "unknown type should not match -t all with index"
    );

    let out = p.walk_output(["-t", "all", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("src.rs"), "rust file should match -t all");
    assert!(
        stdout.contains("script.py"),
        "python file should match -t all"
    );
    assert!(
        !stdout.contains("data.unknownext"),
        "unknown type should not match -t all"
    );
}

#[test]
fn type_not_all_excludes_known_file_types() {
    let p = TestProject::new("type-all-exclude");
    p.write("src.rs", "hello\n");
    p.write("script.py", "hello\n");
    p.write("data.unknownext", "hello\n");

    p.build_index();
    let indexed = p.index_output(["-T", "all", "hello"]);
    assert_success(&indexed);
    let indexed_stdout = normalize_stdout(&indexed);
    assert!(
        !indexed_stdout.contains("src.rs"),
        "rust file should be excluded by -T all with index"
    );
    assert!(
        !indexed_stdout.contains("script.py"),
        "python file should be excluded by -T all with index"
    );
    assert!(
        indexed_stdout.contains("data.unknownext"),
        "unknown type should remain searchable with -T all with index"
    );

    let out = p.walk_output(["-T", "all", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        !stdout.contains("src.rs"),
        "rust file should be excluded by -T all"
    );
    assert!(
        !stdout.contains("script.py"),
        "python file should be excluded by -T all"
    );
    assert!(
        stdout.contains("data.unknownext"),
        "unknown type should remain searchable with -T all"
    );
}

#[test]
fn later_type_include_overrides_type_exclude() {
    let p = TestProject::new("type-order-include");
    p.write("a.rs", "hello\n");
    p.write("b.py", "hello\n");

    p.build_index();
    let indexed = p.index_output(["-T", "rust", "-t", "rust", "hello"]);
    assert_success(&indexed);
    let indexed_stdout = normalize_stdout(&indexed);
    assert!(indexed_stdout.contains("a.rs"));
    assert!(!indexed_stdout.contains("b.py"));

    let walked = p.walk_output(["-T", "rust", "-t", "rust", "hello"]);
    assert_success(&walked);
    let walked_stdout = normalize_stdout(&walked);
    assert!(walked_stdout.contains("a.rs"));
    assert!(!walked_stdout.contains("b.py"));
}

#[test]
fn later_type_exclude_overrides_type_include() {
    let p = TestProject::new("type-order-exclude");
    p.write("a.rs", "hello\n");
    p.write("b.py", "hello\n");

    p.build_index();
    let indexed = p.index_output(["-t", "rust", "-T", "rust", "hello"]);
    assert_exit_code(&indexed, 1);
    assert_stderr_empty(&indexed);

    let walked = p.walk_output(["-t", "rust", "-T", "rust", "hello"]);
    assert_exit_code(&walked, 1);
    assert_stderr_empty(&walked);
}

// ─── --sort ──────────────────────────────────────────────────────────────────

#[test]
fn files_output_is_sorted() {
    let p = TestProject::new("files-sorted");
    p.write("c.txt", "content\n");
    p.write("a.txt", "content\n");
    p.write("b.txt", "content\n");

    let out = p.walk_output(["--files"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    let files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    let mut sorted = files.clone();
    sorted.sort_unstable();
    assert_eq!(files, sorted, "--files output should be sorted");
}

// ─── compound: --max-depth + --max-filesize ──────────────────────────────────

#[test]
fn compound_max_depth_and_max_filesize_walk() {
    let p = TestProject::new("compound-depth-size-walk");
    p.mkdir("sub");
    p.write("small.txt", "hello\n");
    p.write("big.txt", "hello\n".repeat(500));
    p.write("sub/deep.txt", "hello\n");
    p.write("sub/deep_big.txt", "hello\n".repeat(500));

    let out = p.walk_output(["--max-depth", "0", "--max-filesize", "100", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("small.txt"),
        "should find small.txt (depth 0, small)"
    );
    assert!(
        !stdout.contains("big.txt"),
        "should skip big.txt (too large)"
    );
    assert!(
        !stdout.contains("deep.txt"),
        "should skip deep.txt (too deep)"
    );
    assert!(
        !stdout.contains("deep_big.txt"),
        "should skip deep_big.txt (too deep + too large)"
    );
}

// ─── compound: -t + --type-add ───────────────────────────────────────────────

#[test]
fn type_add_then_include_walk() {
    let p = TestProject::new("type-add-include-walk");
    p.write("a.myext", "hello\n");
    p.write("b.txt", "hello\n");

    let out = p.walk_output(["--type-add", "custom:*.myext", "-t", "custom", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains("a.myext"),
        "custom type should match a.myext"
    );
    assert!(
        !stdout.contains("b.txt"),
        "custom type should not match b.txt"
    );
}

// ─── indexed and walk filtering ──────────────────────────────────────────────

fn filter_fixture(name: &str) -> TestProject {
    let p = TestProject::new(name);
    p.mkdir("sub");
    p.write("a.rs", "hello world\n");
    p.write("b.py", "hello world\n");
    p.write("sub/c.rs", "hello world\n");
    p
}

fn sorted_stdout_lines(out: &std::process::Output) -> Vec<String> {
    let mut lines: Vec<String> = normalize_stdout(out).lines().map(str::to_string).collect();
    lines.sort();
    lines
}

#[test]
fn type_include_consistent_index_and_walk() {
    let p = filter_fixture("type-consistent");
    p.build_index();
    let args = ["-t", "rust", "hello"];
    let expected = ["a.rs:hello world", "sub/c.rs:hello world"];

    let index = p.index_output(args);
    assert_success(&index);
    assert_stderr_empty(&index);
    assert_eq!(sorted_stdout_lines(&index), expected);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stderr_empty(&walk);
    assert_eq!(sorted_stdout_lines(&walk), expected);
}

#[test]
fn max_depth_consistent_index_and_walk() {
    let p = filter_fixture("depth-consistent");
    p.build_index();
    let args = ["--max-depth", "0", "hello"];
    let expected = ["a.rs:hello world", "b.py:hello world"];

    let index = p.index_output(args);
    assert_success(&index);
    assert_stderr_empty(&index);
    assert_eq!(sorted_stdout_lines(&index), expected);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stderr_empty(&walk);
    assert_eq!(sorted_stdout_lines(&walk), expected);
}

#[test]
fn max_filesize_consistent_index_and_walk() {
    let p = filter_fixture("filesize-consistent");
    p.build_index();
    let args = ["--max-filesize", "10K", "hello"];
    let expected = [
        "a.rs:hello world",
        "b.py:hello world",
        "sub/c.rs:hello world",
    ];

    let index = p.index_output(args);
    assert_success(&index);
    assert_stderr_empty(&index);
    assert_eq!(sorted_stdout_lines(&index), expected);

    let walk = p.walk_output(args);
    assert_success(&walk);
    assert_stderr_empty(&walk);
    assert_eq!(sorted_stdout_lines(&walk), expected);
}
