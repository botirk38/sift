//! Regex → trigram lookup arms (literals only; otherwise full scan).

use regex_syntax::hir::{Hir, HirKind};

use crate::index::trigram::extract_trigrams_bytes;
use crate::search::SearchOptions;

/// One OR branch: every trigram here must appear in a candidate file (intersection).
pub type Arm = Vec<[u8; 3]>;

/// Trigram-based narrowing plan, or fall back to scanning the whole corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrigramPlan {
    /// Union across arms (each arm is an intersection of posting lists).
    Narrow {
        arms: Vec<Arm>,
    },
    FullScan,
}

impl TrigramPlan {
    /// Build a plan from user patterns (OR across `-e` patterns). Multiple patterns are unioned
    /// at candidate resolution time.
    #[must_use]
    pub fn for_patterns(patterns: &[String], opts: &SearchOptions) -> Self {
        if opts.precludes_trigram_index() {
            return Self::FullScan;
        }
        let mut trigram_arms: Vec<Arm> = Vec::new();
        for p in patterns {
            let literal_branches: Vec<Vec<u8>> = if opts.fixed_strings() {
                vec![p.as_bytes().to_vec()]
            } else if let Some(arms) = plan_regex_pattern(p) {
                arms
            } else {
                return Self::FullScan;
            };
            for lit in literal_branches {
                if lit.len() < 3 {
                    return Self::FullScan;
                }
                trigram_arms.push(extract_trigrams_bytes(&lit));
            }
        }
        if trigram_arms.is_empty() {
            return Self::FullScan;
        }
        Self::Narrow { arms: trigram_arms }
    }
}

/// Hir that is only literal bytes (possibly empty) concatenation.
fn hir_concat_literals(hir: &Hir) -> Option<Vec<u8>> {
    match hir.kind() {
        HirKind::Empty => Some(Vec::new()),
        HirKind::Literal(l) => Some(l.0.to_vec()),
        HirKind::Concat(children) => {
            let mut out = Vec::new();
            for c in children {
                out.extend(hir_concat_literals(c)?);
            }
            Some(out)
        }
        _ => None,
    }
}

/// One regex pattern string → literal arms (OR of literals).
fn plan_regex_pattern(pattern: &str) -> Option<Vec<Vec<u8>>> {
    let mut parser = regex_syntax::Parser::new();
    let hir = parser.parse(pattern).ok()?;
    match hir.kind() {
        HirKind::Alternation(children) => {
            let mut arms = Vec::with_capacity(children.len());
            for c in children {
                arms.push(hir_concat_literals(c)?);
            }
            Some(arms)
        }
        _ => Some(vec![hir_concat_literals(&hir)?]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchMatchFlags;

    #[test]
    fn literal_regex_narrows() {
        let opts = SearchOptions::default();
        let p = TrigramPlan::for_patterns(&["hello".to_string()], &opts);
        assert!(matches!(p, TrigramPlan::Narrow { .. }));
    }

    #[test]
    fn dot_star_full_scan() {
        let opts = SearchOptions::default();
        let p = TrigramPlan::for_patterns(&[".*".to_string()], &opts);
        assert_eq!(p, TrigramPlan::FullScan);
    }

    #[test]
    fn alternation_two_arms() {
        let opts = SearchOptions::default();
        let p = TrigramPlan::for_patterns(&[r"foo|bar".to_string()], &opts);
        match p {
            TrigramPlan::Narrow { arms } => assert_eq!(arms.len(), 2),
            TrigramPlan::FullScan => panic!("expected narrow"),
        }
    }

    #[test]
    fn case_insensitive_full_scan() {
        let opts = SearchOptions {
            flags: SearchMatchFlags::CASE_INSENSITIVE,
            max_results: None,
        };
        let p = TrigramPlan::for_patterns(&["hello".to_string()], &opts);
        assert_eq!(p, TrigramPlan::FullScan);
    }

    #[test]
    fn short_literal_full_scan() {
        let opts = SearchOptions::default();
        let p = TrigramPlan::for_patterns(&["ab".to_string()], &opts);
        assert_eq!(p, TrigramPlan::FullScan);
    }
}
