mod common;

use common::{TestProject, assert_stdout_eq, assert_success, rel_match};

fn json_lines(output: &std::process::Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn json_emits_begin_match_end_and_summary() {
    let p = TestProject::new("json-basic");
    p.write("a.txt", "hello world\n");
    p.build_index();

    let output = p.index_output(["hello", "--json"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<serde_json::Value> = json_lines(&output);
    assert!(
        lines.len() >= 4,
        "expected begin, match, end, summary lines; got {}: {stdout:?}",
        lines.len()
    );
    assert_eq!(lines[0]["type"], "begin");
    assert_eq!(lines[1]["type"], "match");
    assert_eq!(lines[2]["type"], "end");
    assert_eq!(lines[lines.len() - 1]["type"], "summary");
    assert_eq!(lines[1]["data"]["lines"]["text"], "hello world\n");
    assert_eq!(lines[1]["data"]["line_number"], 1);
    assert_eq!(lines[1]["data"]["absolute_offset"], 0);
    assert_eq!(lines[1]["data"]["submatches"][0]["match"]["text"], "hello");
    assert_eq!(lines[1]["data"]["submatches"][0]["start"], 0);
    assert_eq!(lines[1]["data"]["submatches"][0]["end"], 5);
    assert!(lines[2]["data"]["stats"]["matches"].as_u64().unwrap() >= 1);
    let summary = &lines[lines.len() - 1];
    assert_eq!(summary["type"], "summary");
    assert!(
        summary["data"]["stats"]["matches"].as_u64().unwrap() >= 1,
        "JSON summary must carry match stats without --stats: {summary}"
    );
    assert!(
        summary["data"]["stats"]["bytes_searched"].as_u64().unwrap() > 0,
        "JSON summary must carry bytes_searched without --stats: {summary}"
    );
}

#[test]
fn json_emits_context_events_with_empty_submatches() {
    let p = TestProject::new("json-context");
    p.write("a.txt", "before\nhello world\nafter\n");
    p.build_index();

    let output = p.index_output(["hello", "--json", "-C", "1"]);

    assert_success(&output);
    let lines = json_lines(&output);
    assert_eq!(lines[0]["type"], "begin");
    assert_eq!(lines[1]["type"], "context");
    assert_eq!(lines[1]["data"]["lines"]["text"], "before\n");
    assert_eq!(lines[1]["data"]["line_number"], 1);
    assert_eq!(lines[1]["data"]["submatches"].as_array().unwrap().len(), 0);
    assert_eq!(lines[2]["type"], "match");
    assert_eq!(lines[3]["type"], "context");
    assert_eq!(lines[3]["data"]["lines"]["text"], "after\n");
}

#[test]
fn json_does_not_imply_human_stats_on_stderr() {
    let p = TestProject::new("json-stats-stderr");
    p.write("a.txt", "x\n");
    p.build_index();

    let output = p.index_output(["x", "--json"]);
    assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "expected quiet stderr without --stats, got: {stderr:?}"
    );
}

#[test]
fn json_with_stats_prints_human_stats_on_stderr() {
    let p = TestProject::new("json-with-stats-stderr");
    p.write("a.txt", "x\n");
    p.build_index();

    let output = p.index_output(["x", "--json", "--stats"]);
    assert_success(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("matches") && stderr.contains("bytes searched"),
        "expected human --stats lines on stderr with --json --stats, got: {stderr:?}"
    );
}

#[test]
fn json_with_count_uses_count_output() {
    let p = TestProject::new("json-bad-count");
    p.write("a.txt", "a\n");
    p.build_index();

    let output = p.index_output(["a", "--json", "--count"]);

    assert_success(&output);
    assert_stdout_eq(&output, &rel_match("a.txt", "1\n"));
}

#[test]
fn json_with_path_modes_uses_path_output() {
    let p = TestProject::new("json-path-mode");
    p.write("a.txt", "a\n");
    p.build_index();

    let output = p.index_output(["a", "--json", "--files-with-matches"]);

    assert_success(&output);
    assert_stdout_eq(&output, "a.txt\n");
}

#[test]
fn no_json_disables_json_output() {
    let p = TestProject::new("json-no-json");
    p.write("a.txt", "a\n");
    p.build_index();

    let output = p.index_output(["a", "--json", "--no-json"]);

    assert_success(&output);
    assert_stdout_eq(&output, &rel_match("a.txt", "a\n"));
}

#[test]
fn no_json_with_count_uses_count_output() {
    let p = TestProject::new("json-count-no-json");
    p.write("a.txt", "a\n");
    p.build_index();

    let output = p.index_output(["a", "--json", "--no-json", "--count"]);

    assert_success(&output);
    assert_stdout_eq(&output, &rel_match("a.txt", "1\n"));
}
