//! Compiled ast-grep pattern for one language.
//!
//! Every ast-grep matcher type stays confined behind this wrapper so a future
//! in-house matcher is a single-module replacement.

use std::fmt;
use std::ops::Range;

use ast_grep_core::matcher::PatternNode;
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::{Matcher, Pattern};

use super::language::AstLanguage;

/// An ast-grep pattern compiled for one [`AstLanguage`].
#[derive(Clone)]
pub struct AstPattern {
    lang: AstLanguage,
    text: String,
    inner: Pattern,
}

impl fmt::Debug for AstPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AstPattern")
            .field("lang", &self.lang)
            .field("text", &self.text)
            .finish_non_exhaustive()
    }
}

impl AstPattern {
    /// Compile `text` as an ast-grep pattern for `lang`.
    ///
    /// # Errors
    ///
    /// Returns the pattern-compilation error rendered as text when `text` is
    /// not valid pattern code for `lang`.
    pub fn compile(text: &str, lang: AstLanguage) -> Result<Self, String> {
        let inner = Pattern::try_new(text, lang).map_err(|err| err.to_string())?;
        Ok(Self {
            lang,
            text: text.to_string(),
            inner,
        })
    }

    #[must_use]
    pub const fn lang(&self) -> AstLanguage {
        self.lang
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Node kinds a match can root at, per ast-grep's own advertisement.
    /// `None` means the pattern can root anywhere (no kind narrowing).
    #[must_use]
    pub fn kinds(&self) -> Option<Vec<u16>> {
        let kinds = self.inner.potential_kinds()?;
        // A kind id that does not fit u16 cannot come from tree-sitter; if it
        // ever does, narrowing must widen (None), never shrink.
        kinds
            .iter()
            .map(u16::try_from)
            .collect::<Result<Vec<u16>, _>>()
            .ok()
    }

    /// Terminal token texts a match must contain verbatim.
    ///
    /// Pattern text is preprocessed before parsing (`$` becomes the language's
    /// expando char for metavariable sigils), so terminals containing `$` or
    /// the expando char are skipped: their text never appears in target files.
    #[must_use]
    pub fn literals(&self) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        let expando = self.lang.expando();
        Self::collect_literals(&self.inner.node, expando, &mut out);
        out.sort_unstable();
        out.dedup();
        out
    }

    fn collect_literals(node: &PatternNode, expando: char, out: &mut Vec<Vec<u8>>) {
        match node {
            PatternNode::MetaVar { .. } => {}
            PatternNode::Terminal { text, .. } => {
                if !text.is_empty() && !text.contains('$') && !text.contains(expando) {
                    out.push(text.clone().into_bytes());
                }
            }
            PatternNode::Internal { children, .. } => {
                for child in children {
                    Self::collect_literals(child, expando, out);
                }
            }
        }
    }

    /// Byte spans of every match of this pattern in `src`.
    #[must_use]
    pub fn spans(&self, src: &str) -> Vec<Range<usize>> {
        let root = self.lang.ast_grep(src);
        root.root()
            .find_all(self.inner.clone())
            .map(|found| found.range())
            .collect()
    }
}

// `Pattern` holds only owned node data; assert the auto traits the searcher
// relies on (`Searcher` is cloned and shared across rayon workers).
const _: fn() = || {
    const fn assert_traits<T: Send + Sync + Clone>() {}
    assert_traits::<AstPattern>();
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_for_every_language() {
        let cases = [
            (AstLanguage::Rust, "fn $NAME() {}"),
            (AstLanguage::Python, "def $NAME(): $$$BODY"),
            (AstLanguage::JavaScript, "function $NAME() { $$$BODY }"),
            (AstLanguage::TypeScript, "function $NAME(): $RET { $$$B }"),
            (AstLanguage::Tsx, "<div>$CHILD</div>"),
            (AstLanguage::Go, "func $NAME() $RET { $$$BODY }"),
            (AstLanguage::Java, "class $NAME { $$$BODY }"),
        ];
        for (lang, text) in cases {
            let pattern = AstPattern::compile(text, lang);
            assert!(pattern.is_ok(), "{lang}: {text} -> {pattern:?}");
        }
    }

    #[test]
    fn rejects_unparseable_pattern() {
        assert!(AstPattern::compile("", AstLanguage::Rust).is_err());
    }

    #[test]
    fn kinds_are_advertised_for_shaped_patterns() {
        let pattern = AstPattern::compile("fn $NAME() {}", AstLanguage::Rust).expect("compile");
        let kinds = pattern.kinds().expect("shaped pattern has kinds");
        assert!(!kinds.is_empty());
        // A bare metavariable can root anywhere.
        let bare = AstPattern::compile("$A", AstLanguage::Rust).expect("compile");
        assert!(bare.kinds().is_none());
    }

    #[test]
    fn literals_skip_metavariables_and_expando_text() {
        let pattern = AstPattern::compile("foo($A, $$$REST)", AstLanguage::Rust).expect("compile");
        let literals = pattern.literals();
        assert!(literals.contains(&b"foo".to_vec()));
        for lit in &literals {
            let text = String::from_utf8(lit.clone()).expect("utf-8 literal");
            assert!(!text.contains('$'), "{text}");
            assert!(!text.contains('\u{b5}'), "{text}");
            // Every surviving literal must appear verbatim in the raw pattern.
            assert!("foo($A, $$$REST)".contains(&text), "{text}");
        }
    }

    #[test]
    fn spans_locate_matches_in_source() {
        let pattern = AstPattern::compile("$OBJ.unwrap()", AstLanguage::Rust).expect("compile");
        let src = "fn main() {\n    let v = maybe().unwrap();\n}\n";
        let spans = pattern.spans(src);
        assert_eq!(spans.len(), 1);
        assert_eq!(&src[spans[0].clone()], "maybe().unwrap()");
    }

    #[test]
    fn spans_are_empty_when_nothing_matches() {
        let pattern = AstPattern::compile("$OBJ.unwrap()", AstLanguage::Rust).expect("compile");
        assert!(pattern.spans("fn main() {}\n").is_empty());
    }
}
