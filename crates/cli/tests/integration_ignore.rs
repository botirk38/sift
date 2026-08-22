mod common;

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use common::{TestProject, assert_success, normalize_stdout, rel_match};

// ─── existing ignore tests ───────────────────────────────────────────────────

#[test]
fn gitignore_excludes_when_git_present() {
    let p = TestProject::new("ignore-with-git");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "needle\n");
    p.write("skip.log", "needle\n");
    let status = Command::new("git")
        .args(["init"])
        .current_dir(p.root())
        .status()
        .expect("git binary for ignore test");
    assert!(status.success(), "git init failed");

    p.build_index_at(p.root());

    let out = p.index_output(["needle"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(
        stdout.contains(&rel_match("keep.txt", "needle")),
        "expected keep.txt: {stdout}"
    );
    assert!(
        !stdout.contains("skip.log"),
        "gitignored file should be skipped: {stdout}"
    );
}

#[test]
fn no_require_git_loads_gitignore_without_dot_git() {
    let p = TestProject::new("ignore-no-git-repo");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "needle\n");
    p.write("skip.log", "needle\n");
    assert!(!p.root().join(".git").exists());

    p.build_index_at(p.root());

    let out = p.index_output(["--no-require-git", "needle"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("keep.txt", "needle")));
    assert!(!stdout.contains("skip.log"), "unexpected: {stdout}");
}

#[test]
fn gitignore_applies_at_build_without_git_dir() {
    let p = TestProject::new("ignore-no-git-repo-build");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "needle\n");
    p.write("skip.log", "needle\n");
    assert!(!p.root().join(".git").exists());

    p.build_index_at(p.root());

    let out = p.index_output(["needle"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("keep.txt", "needle")));
    assert!(
        !stdout.contains("skip.log"),
        "gitignored files should be excluded at build time: {stdout}"
    );
}

#[test]
fn no_ignore_disables_gitignore() {
    let p = TestProject::new("ignore-no-ignore-flag");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "needle\n");
    p.write("skip.log", "needle\n");
    let status = Command::new("git")
        .args(["init"])
        .current_dir(p.root())
        .status()
        .expect("git");
    assert!(status.success());

    p.build_index_with(p.root(), false, &["--no-ignore"], &[]);

    let out = p.index_output(["--no-ignore", "needle"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains(&rel_match("keep.txt", "needle")));
    assert!(
        stdout.contains(&rel_match("skip.log", "needle")),
        "--no-ignore should search ignored paths: {stdout}"
    );
}

// ─── --no-ignore-vcs ─────────────────────────────────────────────────────────

#[test]
fn no_ignore_vcs_disables_gitignore() {
    let p = TestProject::new("no-ignore-vcs-walk");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "findme in txt\n");
    p.write("skip.log", "findme in log\n");
    let status = Command::new("git")
        .args(["init"])
        .current_dir(p.root())
        .status()
        .expect("git");
    assert!(status.success());

    let out = p.walk_output(["findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(!stdout.contains("skip.log"), "gitignore should hide .log");

    let out = p.walk_output(["--no-ignore-vcs", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(
        stdout.contains("skip.log"),
        "--no-ignore-vcs should search gitignored paths: {stdout}"
    );
}

#[test]
fn no_ignore_vcs_index() {
    let p = TestProject::new("no-ignore-vcs-index");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "findme in txt\n");
    p.write("skip.log", "findme in log\n");
    p.build_index_with(p.root(), false, &["--no-ignore-vcs"], &[]);

    let out = p.index_output(["--no-ignore-vcs", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(
        stdout.contains("skip.log"),
        "--no-ignore-vcs should search gitignored paths in index mode: {stdout}"
    );
}

// ─── -u / --unrestricted ─────────────────────────────────────────────────────

#[test]
fn unrestricted_disables_gitignore_walk() {
    let p = TestProject::new("unrestricted-walk");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "findme in txt\n");
    p.write("skip.log", "findme in log\n");
    let status = Command::new("git")
        .args(["init"])
        .current_dir(p.root())
        .status()
        .expect("git");
    assert!(status.success());

    let out = p.walk_output(["findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(!stdout.contains("skip.log"), "gitignore should hide .log");

    let out = p.walk_output(["-u", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(
        stdout.contains("skip.log"),
        "-u should search gitignored paths: {stdout}"
    );
}

#[test]
fn unrestricted_disables_gitignore_index() {
    let p = TestProject::new("unrestricted-index");
    p.write(".gitignore", "*.log\n");
    p.write("keep.txt", "findme in txt\n");
    p.write("skip.log", "findme in log\n");
    p.build_index_with(p.root(), false, &["-u"], &[]);

    let out = p.index_output(["-u", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("keep.txt"), "should find txt");
    assert!(
        stdout.contains("skip.log"),
        "-u should search gitignored paths in index mode: {stdout}"
    );
}

// ─── --no-ignore-dot ─────────────────────────────────────────────────────────

#[test]
fn no_ignore_dot_disables_dot_ignore_files() {
    let p = TestProject::new("no-ignore-dot-walk");
    p.write(".ignore", "secret.txt\n");
    p.write("secret.txt", "findme secret\n");
    p.write("public.txt", "findme public\n");

    let out = p.walk_output(["findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        !stdout.contains("secret.txt"),
        ".ignore should hide secret.txt"
    );

    let out = p.walk_output(["--no-ignore-dot", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        stdout.contains("secret.txt"),
        "--no-ignore-dot should search .ignore targets: {stdout}"
    );
}

#[test]
fn no_ignore_dot_index() {
    let p = TestProject::new("no-ignore-dot-index");
    p.write(".ignore", "secret.txt\n");
    p.write("secret.txt", "findme secret\n");
    p.write("public.txt", "findme public\n");
    p.build_index_with(p.root(), false, &["--no-ignore-dot"], &[]);

    let out = p.index_output(["--no-ignore-dot", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        stdout.contains("secret.txt"),
        "--no-ignore-dot should search .ignore targets in index mode: {stdout}"
    );
}

// ─── --no-ignore-parent ──────────────────────────────────────────────────────

#[test]
fn no_ignore_parent_walk() {
    let p = TestProject::new("no-ignore-parent-walk");
    p.write(".gitignore", "*.log\n");
    p.mkdir("project/.git");
    p.write("project/data.log", "findme in log\n");
    p.write("project/data.txt", "findme in txt\n");
    let missing = p.root().join(".sift-missing");

    // Without --no-ignore-parent: parent .gitignore applies, data.log excluded
    let out = p
        .sift()
        .current_dir(p.root().join("project"))
        .arg("--sift-dir")
        .arg(&missing)
        .arg("findme")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("data.txt"), "should find txt");

    // With --no-ignore-parent: parent .gitignore NOT applied, data.log found
    let out = p
        .sift()
        .current_dir(p.root().join("project"))
        .arg("--sift-dir")
        .arg(&missing)
        .arg("--no-ignore-parent")
        .arg("findme")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("data.txt"), "should find txt");
    assert!(
        stdout.contains("data.log"),
        "with --no-ignore-parent, should find log file"
    );
}

// ─── --no-ignore-exclude ─────────────────────────────────────────────────────

#[test]
fn no_ignore_exclude_walk() {
    let p = TestProject::new("no-ignore-exclude-walk");
    p.mkdir(".git/info");
    p.write(".git/info/exclude", "*.bak\n");
    p.write("file.bak", "findme in bak\n");
    p.write("file.txt", "findme in txt\n");

    // With exclude: .bak is ignored
    let out = p.walk_output(["findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("file.txt"), "should find txt");
    assert!(!stdout.contains("file.bak"), "exclude should hide .bak");

    // With --no-ignore-exclude: .bak is found
    let out = p.walk_output(["--no-ignore-exclude", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("file.txt"), "should find txt");
    assert!(
        stdout.contains("file.bak"),
        "with --no-ignore-exclude, should find .bak"
    );
}

#[test]
fn no_ignore_exclude_index() {
    let p = TestProject::new("no-ignore-exclude-index");
    p.mkdir(".git/info");
    p.write(".git/info/exclude", "*.bak\n");
    p.write("file.bak", "findme in bak\n");
    p.write("file.txt", "findme in txt\n");
    p.build_index_with(Path::new("."), false, &["--no-ignore-exclude"], &[]);

    let out = p.index_output(["--no-ignore-exclude", "findme"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("file.txt"), "should find txt");
    assert!(
        stdout.contains("file.bak"),
        "with --no-ignore-exclude, should find .bak in index mode"
    );
}

// ─── --ignore-file / --no-ignore-files ───────────────────────────────────────

#[test]
fn ignore_file_walk() {
    let p = TestProject::new("ignore-file-walk");
    p.write("secret.txt", "findme secret\n");
    p.write("public.txt", "findme public\n");
    p.write("custom.ignore", "secret.txt\n");
    let ignore_arg: OsString = p.root().join("custom.ignore").into();

    let out = p.walk_output(vec![
        OsString::from("--ignore-file"),
        ignore_arg,
        OsString::from("findme"),
    ]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        !stdout.contains("secret.txt"),
        "ignore-file should hide secret"
    );
}

#[test]
fn ignore_file_index() {
    let p = TestProject::new("ignore-file-index");
    p.write("secret.txt", "findme secret\n");
    p.write("public.txt", "findme public\n");
    p.write("custom.ignore", "secret.txt\n");
    p.build_index();
    let ignore_arg: OsString = p.root().join("custom.ignore").into();

    let out = p.index_output(vec![
        OsString::from("--ignore-file"),
        ignore_arg,
        OsString::from("findme"),
    ]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        !stdout.contains("secret.txt"),
        "ignore-file should hide secret in index mode"
    );
}

#[test]
fn no_ignore_files_overrides_ignore_file() {
    let p = TestProject::new("no-ignore-files-walk");
    p.write("secret.txt", "findme secret\n");
    p.write("public.txt", "findme public\n");
    p.write("custom.ignore", "secret.txt\n");
    let ignore_arg: OsString = p.root().join("custom.ignore").into();

    let out = p.walk_output(vec![
        OsString::from("--no-ignore-files"),
        OsString::from("--ignore-file"),
        ignore_arg,
        OsString::from("findme"),
    ]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("public.txt"), "should find public");
    assert!(
        stdout.contains("secret.txt"),
        "--no-ignore-files should override --ignore-file"
    );
}

#[test]
fn ignore_file_consistent_index_and_walk() {
    let p = TestProject::new("ignore-file-consistent");
    p.write("a.txt", "findme alpha\n");
    p.write("b.txt", "findme beta\n");
    p.write("custom.ignore", "b.txt\n");
    p.build_index();
    let ignore_arg: OsString = p.root().join("custom.ignore").into();
    let args = vec![
        OsString::from("--ignore-file"),
        ignore_arg,
        OsString::from("findme"),
    ];

    let index_out = p.index_output(args.clone());
    assert_success(&index_out);

    let walk_out = p.walk_output(args);
    assert_success(&walk_out);

    let mut index_lines: Vec<String> = normalize_stdout(&index_out)
        .lines()
        .map(str::to_string)
        .collect();
    let mut walk_lines: Vec<String> = normalize_stdout(&walk_out)
        .lines()
        .map(str::to_string)
        .collect();
    index_lines.sort_unstable();
    walk_lines.sort_unstable();
    assert_eq!(
        index_lines, walk_lines,
        "ignore-file should produce same results in both modes"
    );
}

// ─── --no-messages ───────────────────────────────────────────────────────────

#[test]
fn no_messages_suppresses_error_output() {
    let p = TestProject::new("no-messages");
    p.write("file.txt", "hello\n");

    let out = p.walk_output(["--no-messages", "hello"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("error: unexpected argument"),
        "--no-messages should be accepted as a valid flag"
    );
}

// ─── --no-ignore-messages ────────────────────────────────────────────────────

#[test]
fn no_ignore_messages_accepted() {
    let p = TestProject::new("no-ignore-messages");
    p.write("file.txt", "hello\n");

    let out = p.walk_output(["--no-ignore-messages", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("hello"), "should find match");
}

// ─── --no-ignore-global ──────────────────────────────────────────────────────

#[test]
fn no_ignore_global_accepted() {
    let p = TestProject::new("no-ignore-global");
    p.write("file.txt", "hello\n");

    let out = p.walk_output(["--no-ignore-global", "hello"]);
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("hello"), "should find match");
}

// ─── toggling: --no-ignore-parent then --ignore-parent ───────────────────────

#[test]
fn ignore_parent_toggle_last_wins() {
    let p = TestProject::new("ignore-parent-toggle");
    p.write(".gitignore", "*.log\n");
    p.mkdir("project/.git");
    p.write("project/data.log", "findme in log\n");
    p.write("project/data.txt", "findme in txt\n");
    let missing = p.root().join(".sift-missing");

    let out = p
        .sift()
        .current_dir(p.root().join("project"))
        .arg("--sift-dir")
        .arg(&missing)
        .arg("--no-ignore-parent")
        .arg("--ignore-parent")
        .arg("findme")
        .output()
        .unwrap();
    assert_success(&out);
    let stdout = normalize_stdout(&out);
    assert!(stdout.contains("data.txt"), "should find txt");
    assert!(
        !stdout.contains("data.log"),
        "--ignore-parent should re-apply parent rules"
    );
}
