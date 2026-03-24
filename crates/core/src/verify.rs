//! Regex compilation — Rust regex syntax (ERE-like), with grep-style `-F`/`-w`/`-x` shaping.

use regex_automata::meta::Regex;
use regex_syntax::escape;

use crate::search::SearchOptions;

pub fn pattern_branch(p: &str, opts: &SearchOptions) -> String {
    let mut s = if opts.fixed_strings() {
        escape(p)
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
/// Returns [`regex_automata::meta::BuildError`] if the combined pattern is invalid.
pub fn compile_search_pattern(
    patterns: &[String],
    opts: &SearchOptions,
) -> Result<Regex, Box<regex_automata::meta::BuildError>> {
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
    let mut builder = Regex::builder();
    if opts.case_insensitive() {
        builder.syntax(regex_automata::util::syntax::Config::new().case_insensitive(true));
    }
    builder.build(&combined).map_err(Box::new)
}

/// Build a `Regex` for a single pattern.
///
/// # Errors
///
/// Returns [`regex_automata::meta::BuildError`] if `pattern` is invalid.
pub fn compile_pattern(
    pattern: &str,
    case_insensitive: bool,
) -> Result<Regex, Box<regex_automata::meta::BuildError>> {
    use crate::search::SearchMatchFlags;

    let flags = if case_insensitive {
        SearchMatchFlags::CASE_INSENSITIVE
    } else {
        SearchMatchFlags::empty()
    };
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
        let mut cache = regex_automata::meta::Cache::new(&re);
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"foo"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"bar"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"baz"))
            .is_none());
    }

    #[test]
    fn fixed_strings_escape_metacharacters() {
        let flags = SearchMatchFlags::FIXED_STRINGS;
        let re = compile_search_pattern(&[r"a.c".to_string()], &opts(flags)).unwrap();
        let mut cache = regex_automata::meta::Cache::new(&re);
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"a.c"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"abc"))
            .is_none());
    }

    #[test]
    fn case_insensitive() {
        let flags = SearchMatchFlags::CASE_INSENSITIVE;
        let re = compile_search_pattern(&["Hello".to_string()], &opts(flags)).unwrap();
        let mut cache = regex_automata::meta::Cache::new(&re);
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"hello"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"HELLO"))
            .is_some());
    }

    #[test]
    fn word_regexp() {
        let flags = SearchMatchFlags::WORD_REGEXP;
        let re = compile_search_pattern(&["cat".to_string()], &opts(flags)).unwrap();
        let mut cache = regex_automata::meta::Cache::new(&re);
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"a cat here"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"concat"))
            .is_none());
    }

    #[test]
    fn line_regexp() {
        let flags = SearchMatchFlags::LINE_REGEXP;
        let re = compile_search_pattern(&["yes".to_string()], &opts(flags)).unwrap();
        let mut cache = regex_automata::meta::Cache::new(&re);
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"yes"))
            .is_some());
        assert!(re
            .search_with(&mut cache, &regex_automata::Input::new(b"oh yes sir"))
            .is_none());
    }

    #[test]
    fn invalid_regex_returns_err() {
        let flags = SearchMatchFlags::empty();
        assert!(compile_search_pattern(&["(".to_string()], &opts(flags)).is_err());
    }
}
