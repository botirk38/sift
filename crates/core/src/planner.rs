//! Regex → trigram lookup arms using required-literal extraction.
//!
//! Models after ripgrep's `InnerLiterals` approach:
//!   - Build Unicode-aware, flag-configured HIR via translation
//!   - Shape HIR for `-w` and `-x`
//!   - Extract required literals via both prefix and suffix extraction
//!   - Pick the better usable result, fall back to `FullScan` if none found

use regex_syntax::ast::parse::Parser as AstParser;
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::{self, Hir};

use crate::index::trigram::extract_trigrams_from_bytes;
use crate::search::SearchOptions;

/// One OR branch: every trigram here must appear in a candidate file (intersection).
pub type Arm = Vec<[u8; 3]>;

/// Trigram-based narrowing plan, or fall back to scanning the whole corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrigramPlan {
    Narrow { arms: Vec<Arm> },
    FullScan,
}

impl TrigramPlan {
    /// Build a plan from user patterns (OR across `-e` patterns).
    #[must_use]
    pub fn for_patterns(patterns: &[String], opts: &SearchOptions) -> Self {
        if opts.invert_match() {
            return Self::FullScan;
        }
        let mut trigram_arms: Vec<Arm> = Vec::new();
        for p in patterns {
            let arms = if opts.fixed_strings() {
                fixed_string_literals(p.as_bytes(), opts.case_insensitive())
            } else {
                match plan_pattern(p.as_str(), opts) {
                    Some(a) => a,
                    None => return Self::FullScan,
                }
            };
            for lit in arms {
                if lit.len() < 3 {
                    return Self::FullScan;
                }
                trigram_arms.push(extract_trigrams_from_bytes(&lit));
            }
        }
        if trigram_arms.is_empty() {
            return Self::FullScan;
        }
        Self::Narrow { arms: trigram_arms }
    }
}

/// Build a configured, translated HIR from a pattern string, applying flag semantics.
fn build_configured_hir(pattern: &str, opts: &SearchOptions) -> Option<Hir> {
    let ast = AstParser::new().parse(pattern).ok()?;

    let mut builder = regex_syntax::hir::translate::TranslatorBuilder::new();
    builder.unicode(true);
    if opts.case_insensitive() {
        builder.case_insensitive(true);
    }
    let mut translator = builder.build();
    let hir = translator.translate(pattern, &ast).ok()?;
    Some(hir)
}

/// Wrap HIR in word boundary looks, mirroring ripgrep's `into_word`.
fn wrap_word(hir: Hir, unicode: bool) -> Hir {
    let start_half = if unicode {
        hir::Look::WordStartHalfUnicode
    } else {
        hir::Look::WordStartHalfAscii
    };
    let end_half = if unicode {
        hir::Look::WordEndHalfUnicode
    } else {
        hir::Look::WordEndHalfAscii
    };
    Hir::concat(vec![Hir::look(start_half), hir, Hir::look(end_half)])
}

/// Wrap HIR in line boundary looks, mirroring ripgrep's `into_whole_line`.
fn wrap_line(hir: Hir) -> Hir {
    Hir::concat(vec![
        Hir::look(hir::Look::StartLF),
        hir,
        Hir::look(hir::Look::EndLF),
    ])
}

/// Shape the HIR according to search options (word/line boundaries).
fn shape_hir(hir: Hir, opts: &SearchOptions) -> Hir {
    if opts.line_regexp() {
        wrap_line(hir)
    } else if opts.word_regexp() {
        wrap_word(hir, true)
    } else {
        hir
    }
}

/// Extract required literals from HIR using both prefix and suffix extraction,
/// returning the better usable result.
fn extract_literals(hir: &Hir) -> Vec<Vec<u8>> {
    let extractor_prefix = Extractor::new();
    let extractor_suffix = {
        let mut e = Extractor::new();
        e.kind(ExtractKind::Suffix);
        e
    };

    let seq_prefix = extractor_prefix.extract(hir);
    let seq_suffix = extractor_suffix.extract(hir);

    let lits_prefix = seq_prefix.literals();
    let lits_suffix = seq_suffix.literals();

    pick_better_lits(lits_prefix, lits_suffix)
}

/// Pick the better literal set for narrowing: prefer finite, longer common literals.
fn pick_better_lits(
    lits_a: Option<&[regex_syntax::hir::literal::Literal]>,
    lits_b: Option<&[regex_syntax::hir::literal::Literal]>,
) -> Vec<Vec<u8>> {
    fn total_bytes(lits: Option<&[regex_syntax::hir::literal::Literal]>) -> usize {
        lits.map_or(0, |l| l.iter().map(|lit| lit.as_bytes().len()).sum())
    }

    let a_count = lits_a.map_or(0, <[regex_syntax::hir::literal::Literal]>::len);
    let b_count = lits_b.map_or(0, <[regex_syntax::hir::literal::Literal]>::len);
    let a_has = a_count > 0;
    let b_has = b_count > 0;

    let lits = match (a_has, b_has) {
        (true, false) => lits_a,
        (false, true) => lits_b,
        (false, false) => return Vec::new(),
        (true, true) => {
            let a_total = total_bytes(lits_a);
            let b_total = total_bytes(lits_b);
            if a_total >= b_total {
                lits_a
            } else {
                lits_b
            }
        }
    };

    let lits = match lits {
        Some(l) if !l.is_empty() => l,
        _ => return Vec::new(),
    };

    let mut out = Vec::new();
    for lit in lits {
        let bytes = lit.as_bytes();
        if bytes.len() >= 3 {
            out.push(bytes.to_vec());
        }
    }
    out
}

/// Extract literals from a fixed string, lowercasing for case-insensitive matching.
fn fixed_string_literals(lit: &[u8], case_insensitive: bool) -> Vec<Vec<u8>> {
    if case_insensitive {
        vec![lit.to_ascii_lowercase()]
    } else {
        vec![lit.to_vec()]
    }
}

/// Plan a single pattern string into literal arms.
fn plan_pattern(pattern: &str, opts: &SearchOptions) -> Option<Vec<Vec<u8>>> {
    let hir = build_configured_hir(pattern, opts)?;
    let shaped = shape_hir(hir, opts);
    let lits = extract_literals(&shaped);
    if lits.is_empty() {
        None
    } else {
        Some(lits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{CaseMode, SearchMatchFlags};

    fn narrow(patterns: &[String], opts: &SearchOptions) -> bool {
        matches!(
            TrigramPlan::for_patterns(patterns, opts),
            TrigramPlan::Narrow { .. }
        )
    }

    fn full_scan(patterns: &[String], opts: &SearchOptions) -> bool {
        matches!(
            TrigramPlan::for_patterns(patterns, opts),
            TrigramPlan::FullScan
        )
    }

    #[test]
    fn literal_narrows() {
        assert!(narrow(&["beta".to_string()], &SearchOptions::default()));
    }

    #[test]
    fn dot_star_full_scan() {
        assert!(full_scan(&[".*".to_string()], &SearchOptions::default()));
    }

    #[test]
    fn alternation_narrows() {
        assert!(narrow(&[r"foo|bar".to_string()], &SearchOptions::default()));
    }

    #[test]
    fn word_literal_narrows() {
        let opts = SearchOptions {
            flags: SearchMatchFlags::WORD_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        };
        assert!(narrow(&["beta".to_string()], &opts));
    }

    #[test]
    fn line_regexp_narrows() {
        let opts = SearchOptions {
            flags: SearchMatchFlags::LINE_REGEXP,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        };
        assert!(narrow(&["beta".to_string()], &opts));
    }

    #[test]
    fn case_insensitive_narrows() {
        let opts = SearchOptions {
            flags: SearchMatchFlags::empty(),
            case_mode: CaseMode::Insensitive,
            max_results: None,
        };
        assert!(narrow(&["beta".to_string()], &opts));
    }

    #[test]
    fn required_literal_inside_regex_narrows() {
        assert!(narrow(
            &["[A-Z]+_RESUME".to_string()],
            &SearchOptions::default()
        ));
    }

    #[test]
    fn unicode_class_full_scan() {
        assert!(full_scan(
            &[r"\p{Greek}".to_string()],
            &SearchOptions::default()
        ));
    }

    #[test]
    fn no_literal_full_scan() {
        assert!(full_scan(
            &[r"\w{5}\s+\w{5}".to_string()],
            &SearchOptions::default()
        ));
    }

    #[test]
    fn short_literal_full_scan() {
        assert!(full_scan(&["ab".to_string()], &SearchOptions::default()));
    }

    #[test]
    fn fixed_string_narrows() {
        let opts = SearchOptions {
            flags: SearchMatchFlags::FIXED_STRINGS,
            case_mode: CaseMode::Sensitive,
            max_results: None,
        };
        assert!(narrow(&["beta.gamma".to_string()], &opts));
    }
}
