use regex_syntax::ast::parse::Parser as AstParser;
use regex_syntax::hir::literal::{ExtractKind, Extractor};
use regex_syntax::hir::{self, Hir};

use crate::search::{Case, SearchQuery};

use super::gram::{GramWidth, LiteralNarrowing};
use super::index::Index;

impl Index {
    /// Extract literal byte arms from a query spec.
    /// Returns `None` if no usable literals for this N-gram width can be extracted.
    ///
    /// Case-insensitive queries extract case-preserving arms; [`GramMatch`] at
    /// lookup folds ASCII letter case. Non-ASCII case-insensitive literals
    /// decline narrowing so candidates stay conservative.
    ///
    /// [`GramMatch`]: super::gram::GramMatch
    pub(crate) fn extract_literal_arms(&self, query: &SearchQuery) -> Option<Vec<Vec<u8>>> {
        if query.invert_match() {
            return None;
        }
        let width = self.gram_width();
        let case = query.case();
        let mut literal_arms: Vec<Vec<u8>> = Vec::new();
        for p in query.patterns() {
            let arms = if query.fixed_strings() {
                vec![p.as_bytes().to_vec()]
            } else {
                // Keep HIR case-sensitive so arms stay long; matching policy is
                // chosen by the caller via GramMatch.
                Self::plan_pattern(
                    p.as_str(),
                    false,
                    query.word_regexp(),
                    query.line_regexp(),
                    width,
                )?
            };
            for lit in arms {
                if matches!(
                    width.literal_narrowing(lit.len()),
                    LiteralNarrowing::TooShort
                ) {
                    return None;
                }
                if matches!(case, Case::Insensitive) && !lit.is_ascii() {
                    return None;
                }
                literal_arms.push(lit);
            }
        }
        if literal_arms.is_empty() {
            return None;
        }
        if query.bom_sniffing() {
            Self::expand_bom_sniffing_arms(&mut literal_arms, width)?;
        }
        Some(literal_arms)
    }

    /// Under default BOM sniffing, UTF-16 files are decoded at search time.
    /// Expand ASCII arms with UTF-16LE/BE encodings so those files stay reachable
    /// while byte narrowing remains enabled for the UTF-8 majority.
    fn expand_bom_sniffing_arms(arms: &mut Vec<Vec<u8>>, width: GramWidth) -> Option<()> {
        if arms.iter().any(|arm| !arm.is_ascii()) {
            return None;
        }
        let min_len = width.get();
        let ascii_arms = arms.clone();
        for arm in ascii_arms {
            let le = Self::utf16_le(&arm);
            let be = Self::utf16_be(&arm);
            if le.len() >= min_len {
                arms.push(le);
            }
            if be.len() >= min_len {
                arms.push(be);
            }
        }
        Some(())
    }

    fn utf16_le(ascii: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(ascii.len() * 2);
        for &b in ascii {
            out.push(b);
            out.push(0);
        }
        out
    }

    fn utf16_be(ascii: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(ascii.len() * 2);
        for &b in ascii {
            out.push(0);
            out.push(b);
        }
        out
    }

    fn plan_pattern(
        pattern: &str,
        case_insensitive: bool,
        word_regexp: bool,
        line_regexp: bool,
        width: GramWidth,
    ) -> Option<Vec<Vec<u8>>> {
        let hir = Self::build_configured_hir(pattern, case_insensitive)?;
        let shaped = Self::shape_hir(hir, word_regexp, line_regexp);
        let lits = Self::extract_literals(&shaped, width);
        if lits.is_empty() { None } else { Some(lits) }
    }

    fn build_configured_hir(pattern: &str, case_insensitive: bool) -> Option<Hir> {
        let ast = AstParser::new().parse(pattern).ok()?;
        let mut builder = regex_syntax::hir::translate::TranslatorBuilder::new();
        builder.unicode(true);
        if case_insensitive {
            builder.case_insensitive(true);
        }
        let mut translator = builder.build();
        let hir = translator.translate(pattern, &ast).ok()?;
        Some(hir)
    }

    fn shape_hir(hir: Hir, word_regexp: bool, line_regexp: bool) -> Hir {
        if line_regexp {
            Self::wrap_line(hir)
        } else if word_regexp {
            Self::wrap_word(hir)
        } else {
            hir
        }
    }

    fn wrap_word(hir: Hir) -> Hir {
        Hir::concat(vec![
            Hir::look(hir::Look::WordStartHalfUnicode),
            hir,
            Hir::look(hir::Look::WordEndHalfUnicode),
        ])
    }

    fn wrap_line(hir: Hir) -> Hir {
        Hir::concat(vec![
            Hir::look(hir::Look::StartLF),
            hir,
            Hir::look(hir::Look::EndLF),
        ])
    }

    fn extract_literals(hir: &Hir, width: GramWidth) -> Vec<Vec<u8>> {
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

        Self::pick_better_lits(lits_prefix, lits_suffix, width)
    }

    fn pick_better_lits(
        lits_a: Option<&[regex_syntax::hir::literal::Literal]>,
        lits_b: Option<&[regex_syntax::hir::literal::Literal]>,
        width: GramWidth,
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
                if a_total >= b_total { lits_a } else { lits_b }
            }
        };

        let lits = match lits {
            Some(l) if !l.is_empty() => l,
            _ => return Vec::new(),
        };

        let mut out = Vec::new();
        for lit in lits {
            let bytes = lit.as_bytes();
            if !matches!(
                width.literal_narrowing(bytes.len()),
                LiteralNarrowing::TooShort
            ) {
                out.push(bytes.to_vec());
            }
        }
        out
    }
}
