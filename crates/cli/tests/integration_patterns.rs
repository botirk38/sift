mod common;

use std::fs;

use common::{abs_match, assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn pattern_file_roundtrip() {
    let root = fresh_dir("patterns-file-roundtrip");
    fs::write(root.join("t.txt"), "alpha beta\n").unwrap();
    let pat = root.join("patterns.txt");
    fs::write(&pat, "# comment\nbeta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-f")
        .arg(&pat)
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
}

#[test]
fn repeated_e_patterns_are_or_combined() {
    let root = fresh_dir("patterns-repeated-e");
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    fs::write(root.join("b.txt"), "beta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-e")
        .arg("alpha")
        .arg("-e")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "a.txt", "alpha")));
    assert!(stdout.contains(&abs_match(&root, "b.txt", "beta")));
}

#[test]
fn pattern_file_and_positional_pattern_are_combined() {
    let root = fresh_dir("patterns-file-plus-positional");
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    fs::write(root.join("b.txt"), "beta\n").unwrap();
    let pat = root.join("patterns.txt");
    fs::write(&pat, "alpha\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-f")
        .arg(&pat)
        .arg("--sift-dir")
        .arg(&idx)
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains(&abs_match(&root, "a.txt", "alpha")));
    assert!(stdout.contains(&abs_match(&root, "b.txt", "beta")));
}

#[test]
fn smart_case_lowercase_matches_casei() {
    let root = fresh_dir("patterns-smart-case-lower");
    fs::write(root.join("t.txt"), "alpha beta BETA Beta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-S")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(stdout.contains("BETA"));
    assert!(stdout.contains("Beta"));
}

#[test]
fn smart_case_uppercase_matches_case_sensitive() {
    let root = fresh_dir("patterns-smart-case-upper");
    fs::write(root.join("t.txt"), "alpha beta Beta BETA\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-S")
        .arg("Beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("Beta"));
    assert!(!stdout.contains("beta"));
    assert!(!stdout.contains("BETA"));
}

#[test]
fn case_sensitive_flag_overrides_ignore_case() {
    let root = fresh_dir("patterns-case-sensitive-override");
    fs::write(root.join("t.txt"), "alpha beta Beta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-i")
        .arg("-s")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(!stdout.contains("Beta"));
}

#[test]
fn smart_case_flag_overrides_ignore_case() {
    let root = fresh_dir("patterns-smart-case-override");
    fs::write(root.join("t.txt"), "alpha beta BETA\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-i")
        .arg("-S")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(stdout.contains("BETA"));
}

#[test]
fn case_flag_precedence_last_wins_sensitive_over_smart() {
    let root = fresh_dir("case-precedence-s-over-S");
    fs::write(root.join("t.txt"), "alpha beta Beta\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-S")
        .arg("-s")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(!stdout.contains("Beta"));
}

#[test]
fn case_flag_precedence_smart_over_sensitive() {
    let root = fresh_dir("case-precedence-S-over-s");
    fs::write(root.join("t.txt"), "alpha beta Beta BETA\n").unwrap();
    let idx = root.join(".sift");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-o")
        .arg("-s")
        .arg("-S")
        .arg("beta")
        .arg("--sift-dir")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("beta"));
    assert!(stdout.contains("Beta"));
    assert!(stdout.contains("BETA"));
}
