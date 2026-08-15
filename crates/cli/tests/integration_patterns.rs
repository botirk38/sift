mod common;

use common::{
    TestProject, assert_stdout_contains, assert_stdout_not_contains, assert_success, rel_match,
};

#[test]
fn pattern_file_roundtrip() {
    let p = TestProject::new("patterns-file-roundtrip");
    p.write("t.txt", "alpha beta\n");
    p.write("patterns.txt", "# comment\nbeta\n");
    p.build_index();

    // With `-f`, leftover positionals are search paths (ripgrep), not extra patterns.
    let out = p.index_output(["-f", "patterns.txt"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
}

#[test]
fn repeated_e_patterns_are_or_combined() {
    let p = TestProject::new("patterns-repeated-e");
    p.write("a.txt", "alpha\n");
    p.write("b.txt", "beta\n");
    p.build_index();

    let out = p.index_output(["-e", "alpha", "-e", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("a.txt", "alpha"));
    assert_stdout_contains(&out, &rel_match("b.txt", "beta"));
}

#[test]
fn pattern_file_and_regexp_are_combined() {
    let p = TestProject::new("patterns-file-plus-regexp");
    p.write("a.txt", "alpha\n");
    p.write("b.txt", "beta\n");
    p.write("patterns.txt", "alpha\n");
    p.build_index();

    // Ripgrep: combine `-f` with another pattern via `-e`, not a free positional.
    let out = p.index_output(["-f", "patterns.txt", "-e", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, &rel_match("a.txt", "alpha"));
    assert_stdout_contains(&out, &rel_match("b.txt", "beta"));
}

#[test]
fn smart_case_lowercase_matches_casei() {
    let p = TestProject::new("patterns-smart-case-lower");
    p.write("t.txt", "alpha beta BETA Beta\n");
    p.build_index();

    let out = p.index_output(["-o", "-S", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
    assert_stdout_contains(&out, "BETA");
    assert_stdout_contains(&out, "Beta");
}

#[test]
fn smart_case_uppercase_matches_case_sensitive() {
    let p = TestProject::new("patterns-smart-case-upper");
    p.write("t.txt", "alpha beta Beta BETA\n");
    p.build_index();

    let out = p.index_output(["-o", "-S", "Beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "Beta");
    assert_stdout_not_contains(&out, "beta");
    assert_stdout_not_contains(&out, "BETA");
}

#[test]
fn case_sensitive_flag_overrides_ignore_case() {
    let p = TestProject::new("patterns-case-sensitive-override");
    p.write("t.txt", "alpha beta Beta\n");
    p.build_index();

    let out = p.index_output(["-o", "-i", "-s", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
    assert_stdout_not_contains(&out, "Beta");
}

#[test]
fn smart_case_flag_overrides_ignore_case() {
    let p = TestProject::new("patterns-smart-case-override");
    p.write("t.txt", "alpha beta BETA\n");
    p.build_index();

    let out = p.index_output(["-o", "-i", "-S", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
    assert_stdout_contains(&out, "BETA");
}

#[test]
fn case_flag_precedence_last_wins_sensitive_over_smart() {
    let p = TestProject::new("case-precedence-s-over-S");
    p.write("t.txt", "alpha beta Beta\n");
    p.build_index();

    let out = p.index_output(["-o", "-S", "-s", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
    assert_stdout_not_contains(&out, "Beta");
}

#[test]
fn case_flag_precedence_smart_over_sensitive() {
    let p = TestProject::new("case-precedence-S-over-s");
    p.write("t.txt", "alpha beta Beta BETA\n");
    p.build_index();

    let out = p.index_output(["-o", "-s", "-S", "beta"]);
    assert_success(&out);
    assert_stdout_contains(&out, "beta");
    assert_stdout_contains(&out, "Beta");
    assert_stdout_contains(&out, "BETA");
}
