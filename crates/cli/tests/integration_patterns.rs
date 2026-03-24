mod common;

use std::fs;

use common::{assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn pattern_file_roundtrip() {
    let root = fresh_dir("patterns-file-roundtrip");
    fs::write(root.join("t.txt"), "alpha beta\n").unwrap();
    let pat = root.join("patterns.txt");
    fs::write(&pat, "# comment\nbeta\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-f")
        .arg(&pat)
        .arg("--index")
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
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-e")
        .arg("alpha")
        .arg("-e")
        .arg("beta")
        .arg("--index")
        .arg(&idx)
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("a.txt:alpha"));
    assert!(stdout.contains("b.txt:beta"));
}

#[test]
fn pattern_file_and_positional_pattern_are_combined() {
    let root = fresh_dir("patterns-file-plus-positional");
    fs::write(root.join("a.txt"), "alpha\n").unwrap();
    fs::write(root.join("b.txt"), "beta\n").unwrap();
    let pat = root.join("patterns.txt");
    fs::write(&pat, "alpha\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("-f")
        .arg(&pat)
        .arg("--index")
        .arg(&idx)
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let stdout = normalized_stdout(&out);
    assert!(stdout.contains("a.txt:alpha"));
    assert!(stdout.contains("b.txt:beta"));
}
