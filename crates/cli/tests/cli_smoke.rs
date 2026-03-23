//! Integration tests for the `sift` binary (index + search, exit codes).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn sift_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sift"))
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("sift-cli-{name}-{}", std::process::id()))
}

#[test]
fn index_then_search_finds_line() {
    let root = tmp("smoke");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "fn f() {\n  let y = 2;\n}\n").unwrap();
    let idx = root.join(".idx");

    let s = Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(&root)
        .status()
        .unwrap();
    assert!(s.success(), "build index failed");

    let out = Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg(r"let\s+y")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("src/lib.rs") && stdout.contains("let y"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn search_no_match_exits_1() {
    let root = tmp("no-match");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "nope\n").unwrap();
    let idx = root.join(".idx");
    Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(&root)
        .status()
        .unwrap();

    let s = Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg("ZZZ_NOT_THERE")
        .status()
        .unwrap();
    assert_eq!(s.code(), Some(1));
}

#[test]
fn quiet_exit_codes() {
    let root = tmp("quiet");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".idx");
    Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(&root)
        .status()
        .unwrap();

    let ok = Command::new(sift_exe())
        .arg("-q")
        .arg("--index")
        .arg(&idx)
        .arg("found")
        .status()
        .unwrap();
    assert_eq!(ok.code(), Some(0));

    let miss = Command::new(sift_exe())
        .arg("-q")
        .arg("--index")
        .arg(&idx)
        .arg("nopeeee")
        .status()
        .unwrap();
    assert_eq!(miss.code(), Some(1));
}

#[test]
fn pattern_file_roundtrip() {
    let root = tmp("patfile");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("t.txt"), "alpha beta\n").unwrap();
    let pat = root.join("patterns.txt");
    fs::write(&pat, "# comment\nbeta\n").unwrap();
    let idx = root.join(".idx");
    Command::new(sift_exe())
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(&root)
        .status()
        .unwrap();

    let out = Command::new(sift_exe())
        .arg("-f")
        .arg(&pat)
        .arg("--index")
        .arg(&idx)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("beta"));
}

#[test]
fn path_scope_limits_matches() {
    let root = tmp("path-scope");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/x.txt"), "ONLY_IN_A\n").unwrap();
    fs::write(root.join("b/y.txt"), "ONLY_IN_B\n").unwrap();
    let idx = root.join(".idx");

    let s = Command::new(sift_exe())
        .current_dir(&root)
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(".")
        .status()
        .unwrap();
    assert!(s.success(), "build failed");

    let out = Command::new(sift_exe())
        .current_dir(&root)
        .arg("--index")
        .arg(&idx)
        .arg("ONLY_IN_")
        .arg("a")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("a/x.txt") && stdout.contains("ONLY_IN_A"));
    assert!(!stdout.contains("b/y.txt"));

    let out_both = Command::new(sift_exe())
        .current_dir(&root)
        .arg("--index")
        .arg(&idx)
        .arg("ONLY_IN_")
        .arg("a")
        .arg("b")
        .output()
        .unwrap();
    assert!(out_both.status.success());
    let s2 = String::from_utf8_lossy(&out_both.stdout);
    assert!(s2.contains("a/x.txt") && s2.contains("b/y.txt"));
}

#[test]
fn search_literal_index_without_subcommand() {
    let root = tmp("literal-index");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("t.txt"), "word index here\n").unwrap();
    let idx = root.join(".idx");
    Command::new(sift_exe())
        .current_dir(&root)
        .arg("--index")
        .arg(&idx)
        .arg("build")
        .arg(".")
        .status()
        .unwrap();

    let out = Command::new(sift_exe())
        .current_dir(&root)
        .arg("--index")
        .arg(&idx)
        .arg("index")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("index"));
}
