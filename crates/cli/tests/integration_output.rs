mod common;

use std::fs;

use common::{assert_success, build_index, command, fresh_dir, normalized_stdout};

#[test]
fn quiet_exit_codes() {
    let root = fresh_dir("output-quiet");
    fs::write(root.join("a.txt"), "found\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let ok = command(None)
        .arg("-q")
        .arg("--index")
        .arg(&idx)
        .arg("found")
        .status()
        .unwrap();
    assert_eq!(ok.code(), Some(0));

    let miss = command(None)
        .arg("-q")
        .arg("--index")
        .arg(&idx)
        .arg("nopeeee")
        .status()
        .unwrap();
    assert_eq!(miss.code(), Some(1));
}

#[test]
fn files_with_matches_print_each_path_once() {
    let root = fresh_dir("output-files-with-matches");
    fs::write(root.join("a.txt"), "match\nmatch again\n").unwrap();
    fs::write(root.join("b.txt"), "match\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("-l")
        .arg("match")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, ["a.txt", "b.txt"]);
}

#[test]
fn files_without_match_print_only_non_matching_paths() {
    let root = fresh_dir("output-files-without-match");
    fs::write(root.join("a.txt"), "hit\n").unwrap();
    fs::write(root.join("b.txt"), "miss\n").unwrap();
    fs::write(root.join("c.txt"), "hit too\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("-L")
        .arg("hit")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, ["b.txt"]);
}

#[test]
fn count_prints_match_totals_per_file() {
    let root = fresh_dir("output-count");
    fs::write(root.join("a.txt"), "hit\nhit\n").unwrap();
    fs::write(root.join("b.txt"), "miss\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("-c")
        .arg("hit")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, ["a.txt:2", "b.txt:0"]);
}

#[test]
fn line_number_and_no_filename_format_output() {
    let root = fresh_dir("output-line-number-no-filename");
    fs::write(root.join("t.txt"), "alpha\nbeta\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("-n")
        .arg("--no-filename")
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, ["2:beta"]);
}

#[test]
fn only_matching_prints_each_match_span() {
    let root = fresh_dir("output-only-matching");
    fs::write(root.join("t.txt"), "alpha beta beta\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("-o")
        .arg("--no-filename")
        .arg("beta")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines, ["beta", "beta"]);
}

#[test]
fn max_count_limits_total_matches() {
    let root = fresh_dir("output-max-count");
    fs::write(root.join("a.txt"), "match one\nmatch two\n").unwrap();
    fs::write(root.join("b.txt"), "match three\n").unwrap();
    let idx = root.join(".idx");

    build_index(None, &idx, &root);

    let out = command(None)
        .arg("--index")
        .arg(&idx)
        .arg("--max-count")
        .arg("1")
        .arg("--no-filename")
        .arg("match")
        .output()
        .unwrap();
    assert_success(&out);

    let lines: Vec<_> = normalized_stdout(&out)
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 1, "unexpected lines: {lines:?}");
}
