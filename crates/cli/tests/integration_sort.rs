mod common;

use std::thread;
use std::time::Duration;

use common::{TestProject, assert_exit_code, assert_success, normalize_stderr, normalize_stdout};

#[test]
fn sortr_path_reverses_search_output() {
    let p = TestProject::new("sort-reverse-path");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");
    p.write("c.txt", "needle\n");

    let out = p.walk_output(["--sortr", "path", "needle"]);

    assert_success(&out);
    assert_eq!(
        normalize_stdout(&out),
        "c.txt:needle\nb.txt:needle\na.txt:needle\n"
    );
}

#[test]
fn later_sort_overrides_sortr() {
    let p = TestProject::new("sort-overrides-sortr");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");

    let out = p.walk_output(["--sortr", "path", "--sort", "path", "needle"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "a.txt:needle\nb.txt:needle\n");
}

#[test]
fn later_sortr_overrides_sort() {
    let p = TestProject::new("sortr-overrides-earlier-sort");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");

    let out = p.walk_output(["--sort", "path", "--sortr", "path", "needle"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "b.txt:needle\na.txt:needle\n");
}

#[test]
fn sort_files_overrides_earlier_sortr() {
    let p = TestProject::new("sort-files-overrides-sortr");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");

    let out = p.walk_output(["--sortr", "path", "--sort-files", "needle"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "a.txt:needle\nb.txt:needle\n");
}

#[test]
fn later_sortr_overrides_sort_files() {
    let p = TestProject::new("sortr-overrides-sort-files");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");

    let out = p.walk_output(["--sort-files", "--sortr", "path", "needle"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "b.txt:needle\na.txt:needle\n");
}

#[test]
fn sortr_path_reverses_index_output() {
    let p = TestProject::new("sort-reverse-path-index");
    p.write("a.txt", "needle\n");
    p.write("b.txt", "needle\n");
    p.build_index();

    let out = p.index_output(["--sortr", "path", "needle"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "b.txt:needle\na.txt:needle\n");
}

#[test]
fn modified_sort_orders_by_file_timestamp() {
    let p = TestProject::new("sort-modified");
    p.write("newer.txt", "needle\n");
    thread::sleep(Duration::from_millis(40));
    p.write("older_path.txt", "needle\n");

    let out = p.walk_output(["--sortr", "modified", "needle"]);

    assert_success(&out);
    assert_eq!(
        normalize_stdout(&out),
        "older_path.txt:needle\nnewer.txt:needle\n"
    );
}

#[test]
fn sort_files_maps_to_path_sorting() {
    let p = TestProject::new("sort-files");
    p.write("b.txt", "b\n");
    p.write("a.txt", "a\n");

    let out = p.walk_output(["--files", "--sort-files"]);

    assert_success(&out);
    assert_eq!(normalize_stdout(&out), "a.txt\nb.txt\n");
}

#[test]
fn invalid_sort_key_is_reported() {
    let p = TestProject::new("sort-invalid-key");
    p.write("a.txt", "needle\n");

    let out = p.walk_output(["--sort", "size", "needle"]);

    assert_exit_code(&out, 2);
    assert!(
        normalize_stderr(&out).contains("unknown sort key 'size'"),
        "unexpected stderr: {}",
        normalize_stderr(&out)
    );
}
