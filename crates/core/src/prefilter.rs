//! Necessary substring checks before `Regex::is_match` on a line (conservative; unsupported → no prefilter).

use regex_syntax::hir::{Hir, HirKind, Literal};

use crate::search::SearchOptions;
use crate::verify;

/// OR of ANDs: a match requires one branch where every needle appears as a UTF-8 substring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexPrefilter {
    All(Vec<String>),
    Any(Vec<Vec<String>>),
}

impl RegexPrefilter {
    #[must_use]
    pub(crate) fn may_match_line(&self, line: &str) -> bool {
        match self {
            Self::All(needles) => needles.iter().all(|n| line.contains(n.as_str())),
            Self::Any(branches) => branches
                .iter()
                .any(|b| b.iter().all(|n| line.contains(n.as_str()))),
        }
    }
}

/// When `None`, run the full regex (or use `-F` substring path when applicable).
#[must_use]
pub fn regex_prefilter_for_patterns(
    patterns: &[String],
    opts: &SearchOptions,
) -> Option<RegexPrefilter> {
    if patterns.is_empty() {
        return None;
    }
    if opts.fixed_strings() || opts.case_insensitive() || opts.invert_match() {
        return None;
    }

    let mut branches: Vec<Vec<String>> = Vec::with_capacity(patterns.len());
    for p in patterns {
        let branch = verify::pattern_branch(p, opts);
        let pf = prefilter_from_branch_hir(&branch)?;
        match pf {
            RegexPrefilter::All(v) => branches.push(v),
            RegexPrefilter::Any(v) => branches.extend(v),
        }
    }

    if branches.is_empty() {
        return None;
    }
    if branches.len() == 1 {
        Some(RegexPrefilter::All(branches.into_iter().next()?))
    } else {
        Some(RegexPrefilter::Any(branches))
    }
}

fn prefilter_from_branch_hir(branch_pattern: &str) -> Option<RegexPrefilter> {
    let mut parser = regex_syntax::Parser::new();
    let hir = parser.parse(branch_pattern).ok()?;
    if let HirKind::Alternation(children) = hir.kind() {
        let mut ors = Vec::with_capacity(children.len());
        for c in children {
            let v = mandatory_substrings_concat(c)?;
            if v.is_empty() {
                return None;
            }
            ors.push(v);
        }
        return Some(RegexPrefilter::Any(ors));
    }
    let v = mandatory_substrings_concat(&hir)?;
    if v.is_empty() {
        None
    } else {
        Some(RegexPrefilter::All(v))
    }
}

/// Mandatory UTF-8 substrings (AND) for expressions built only from literals, repetition of a
/// single literal, empty/lookaround (zero-width), and concatenation. Otherwise `None`.
fn mandatory_substrings_concat(hir: &Hir) -> Option<Vec<String>> {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => Some(Vec::new()),
        HirKind::Literal(Literal(bytes)) => {
            let s = std::str::from_utf8(bytes).ok()?.to_string();
            if s.is_empty() {
                Some(Vec::new())
            } else {
                Some(vec![s])
            }
        }
        HirKind::Class(_) | HirKind::Alternation(_) => None,
        HirKind::Repetition(rep) => {
            if rep.min == 0 {
                return Some(Vec::new());
            }
            let inner = mandatory_substrings_concat(&rep.sub)?;
            if inner.len() == 1 {
                let s = &inner[0];
                let count = usize::try_from(rep.min).ok()?;
                Some(vec![s.repeat(count)])
            } else if inner.is_empty() {
                Some(Vec::new())
            } else {
                None
            }
        }
        HirKind::Capture(c) => mandatory_substrings_concat(&c.sub),
        HirKind::Concat(children) => {
            let mut out = Vec::new();
            for c in children {
                out.extend(mandatory_substrings_concat(c)?);
            }
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{SearchMatchFlags, SearchOptions};

    fn opts() -> SearchOptions {
        SearchOptions::default()
    }

    #[test]
    fn literal_pattern() {
        let pf = regex_prefilter_for_patterns(&["hello".to_string()], &opts()).unwrap();
        assert!(matches!(pf, RegexPrefilter::All(ref v) if v == &["hello"]));
        assert!(pf.may_match_line("say hello there"));
        assert!(!pf.may_match_line("goodbye"));
    }

    #[test]
    fn alternation_or_prefilter() {
        let pf = regex_prefilter_for_patterns(&[r"foo|bar".to_string()], &opts()).unwrap();
        assert!(matches!(pf, RegexPrefilter::Any(ref b) if b.len() == 2));
        assert!(pf.may_match_line("foo"));
        assert!(pf.may_match_line("bar"));
        assert!(!pf.may_match_line("baz"));
    }

    #[test]
    fn concat_foo_dot_star_bar() {
        let pf = regex_prefilter_for_patterns(&[r"foo.*bar".to_string()], &opts()).unwrap();
        assert!(matches!(pf, RegexPrefilter::All(ref v) if v == &["foo", "bar"]));
        assert!(pf.may_match_line("fooxxxbar"));
        assert!(pf.may_match_line("foobar")); // .* can match empty between foo and bar
        assert!(!pf.may_match_line("foo"));
    }

    #[test]
    fn no_prefilter_case_insensitive() {
        let o = SearchOptions {
            flags: SearchMatchFlags::CASE_INSENSITIVE,
            max_results: None,
        };
        assert!(regex_prefilter_for_patterns(&["hello".to_string()], &o).is_none());
    }

    #[test]
    fn no_prefilter_fixed_strings() {
        let o = SearchOptions {
            flags: SearchMatchFlags::FIXED_STRINGS,
            max_results: None,
        };
        assert!(regex_prefilter_for_patterns(&["a.c".to_string()], &o).is_none());
    }

    #[test]
    fn no_prefilter_dot_star_only() {
        assert!(regex_prefilter_for_patterns(&[".*".to_string()], &opts()).is_none());
    }

    #[test]
    fn word_regexp_prefilter_is_substring_only() {
        let o = SearchOptions {
            flags: SearchMatchFlags::WORD_REGEXP,
            max_results: None,
        };
        let pf = regex_prefilter_for_patterns(&["cat".to_string()], &o).unwrap();
        assert!(pf.may_match_line("a cat here"));
        // Necessary substring "cat" appears inside "concat"; word boundaries are enforced by regex.
        assert!(pf.may_match_line("concat"));
    }
}
