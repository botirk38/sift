//! Regex compilation — Rust regex syntax (ERE-like), with grep-style `-F`/`-w`/`-x` shaping.

use regex::bytes::Regex;

use crate::search::SearchOptions;

pub fn pattern_branch(p: &str, opts: &SearchOptions) -> String {
    let mut s = if opts.fixed_strings() {
        regex::escape(p)
    } else {
        p.to_string()
    };
    if opts.line_regexp() {
        s = format!("^(?:{s})$");
    } else if opts.word_regexp() {
        s = format!(r"\b(?:{s})\b");
    }
    s
}

/// Build a combined `Regex` from one or more patterns.
///
/// # Errors
///
/// Returns [`regex::Error`] if the combined pattern is invalid.
pub fn compile_search_pattern(
    patterns: &[String],
    opts: &SearchOptions,
) -> Result<Regex, regex::Error> {
    debug_assert!(!patterns.is_empty());
    let branches: Vec<String> = patterns.iter().map(|p| pattern_branch(p, opts)).collect();
    let combined = if branches.len() == 1 {
        branches[0].clone()
    } else {
        branches
            .into_iter()
            .map(|b| format!("(?:{b})"))
            .collect::<Vec<_>>()
            .join("|")
    };
    regex::bytes::RegexBuilder::new(&combined)
        .case_insensitive(opts.case_insensitive())
        .multi_line(false)
        .dot_matches_new_line(false)
        .build()
}

/// Build a `Regex` for a single pattern.
///
/// # Errors
///
/// Returns [`regex::Error`] if `pattern` is invalid.
pub fn compile_pattern(pattern: &str, case_insensitive: bool) -> Result<Regex, regex::Error> {
    use crate::search::SearchMatchFlags;

    let mut flags = SearchMatchFlags::empty();
    if case_insensitive {
        flags |= SearchMatchFlags::CASE_INSENSITIVE;
    }
    let opts = SearchOptions {
        flags,
        max_results: None,
    };
    compile_search_pattern(&[pattern.to_string()], &opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{SearchMatchFlags, SearchOptions};

    fn opts(flags: SearchMatchFlags) -> SearchOptions {
        SearchOptions {
            flags,
            max_results: None,
        }
    }

    #[test]
    fn alternation_matches_either_pattern() {
        let flags = SearchMatchFlags::empty();
        let re =
            compile_search_pattern(&["foo".to_string(), "bar".to_string()], &opts(flags)).unwrap();
        assert!(re.is_match(b"foo"));
        assert!(re.is_match(b"bar"));
        assert!(!re.is_match(b"baz"));
    }

    #[test]
    fn fixed_strings_escape_metacharacters() {
        let flags = SearchMatchFlags::FIXED_STRINGS;
        let re = compile_search_pattern(&[r"a.c".to_string()], &opts(flags)).unwrap();
        assert!(re.is_match(b"a.c"));
        assert!(!re.is_match(b"abc"));
    }

    #[test]
    fn case_insensitive() {
        let flags = SearchMatchFlags::CASE_INSENSITIVE;
        let re = compile_search_pattern(&["Hello".to_string()], &opts(flags)).unwrap();
        assert!(re.is_match(b"hello"));
        assert!(re.is_match(b"HELLO"));
    }

    #[test]
    fn word_regexp() {
        let flags = SearchMatchFlags::WORD_REGEXP;
        let re = compile_search_pattern(&["cat".to_string()], &opts(flags)).unwrap();
        assert!(re.is_match(b"a cat here"));
        assert!(!re.is_match(b"concat"));
    }

    #[test]
    fn line_regexp() {
        let flags = SearchMatchFlags::LINE_REGEXP;
        let re = compile_search_pattern(&["yes".to_string()], &opts(flags)).unwrap();
        assert!(re.is_match(b"yes"));
        assert!(!re.is_match(b"oh yes sir"));
    }

    #[test]
    fn invalid_regex_returns_err() {
        let flags = SearchMatchFlags::empty();
        assert!(compile_search_pattern(&["(".to_string()], &opts(flags)).is_err());
    }
}
