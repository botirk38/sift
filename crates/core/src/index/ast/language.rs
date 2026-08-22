//! Sift-owned language registry for the AST kind.
//!
//! Implements ast-grep's [`Language`]/[`LanguageExt`] contracts directly over
//! the pinned tree-sitter grammar crates, so sift does not depend on
//! `ast-grep-language` (which cannot be scoped to this language set). The
//! per-language settings (extension map, expando char, pattern preprocessing)
//! are vendored from `ast-grep-language` 0.45.0 and must track it on upgrades.

use std::borrow::Cow;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use ast_grep_core::language::Language;
use ast_grep_core::matcher::{Pattern, PatternBuilder, PatternError};
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};

/// A language the AST kind can parse. Numeric codes are persisted in
/// `kinds.bin` and must never be renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AstLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    /// TSX is a distinct grammar with its own node-kind id space; treating
    /// `.tsx` as TypeScript would key kind postings against the wrong grammar.
    Tsx,
    Go,
    Java,
}

impl AstLanguage {
    /// Every supported language, in persisted-code order.
    pub const ALL: [Self; 7] = [
        Self::Rust,
        Self::Python,
        Self::JavaScript,
        Self::TypeScript,
        Self::Tsx,
        Self::Go,
        Self::Java,
    ];

    /// Stable numeric code persisted in artifacts (`0` is reserved).
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Rust => 1,
            Self::Python => 2,
            Self::JavaScript => 3,
            Self::TypeScript => 4,
            Self::Tsx => 5,
            Self::Go => 6,
            Self::Java => 7,
        }
    }

    /// Language for a persisted code, if known to this build.
    #[must_use]
    pub const fn from_code(code: u16) -> Option<Self> {
        match code {
            1 => Some(Self::Rust),
            2 => Some(Self::Python),
            3 => Some(Self::JavaScript),
            4 => Some(Self::TypeScript),
            5 => Some(Self::Tsx),
            6 => Some(Self::Go),
            7 => Some(Self::Java),
            _ => None,
        }
    }

    /// CLI-facing name (`--lang` value).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::Java => "java",
        }
    }

    /// File extensions mapped to this language (ast-grep 0.45.0 parity).
    #[must_use]
    pub const fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Python => &["py", "py3", "pyi", "bzl", "bazel"],
            Self::JavaScript => &["cjs", "js", "mjs", "jsx"],
            Self::TypeScript => &["ts", "cts", "mts"],
            Self::Tsx => &["tsx"],
            Self::Go => &["go"],
            Self::Java => &["java"],
        }
    }

    /// Language for a file path, decided by extension.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        Self::ALL
            .into_iter()
            .find(|lang| lang.extensions().contains(&ext))
    }

    /// The compiled tree-sitter grammar for this language.
    #[must_use]
    pub fn ts_language(self) -> TSLanguage {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
        }
    }

    /// Metavariable stand-in where `$` cannot start an identifier
    /// (ast-grep-language 0.45.0 parity).
    #[must_use]
    pub const fn expando(self) -> char {
        match self {
            Self::Rust | Self::Python | Self::Go => 'µ',
            Self::JavaScript | Self::TypeScript | Self::Tsx | Self::Java => '$',
        }
    }

    /// Fingerprint of the compiled grammar: node kinds, named flags, fields,
    /// and the tree-sitter ABI. Detects grammar bumps without trusting crate
    /// version strings — persisted kind ids are only meaningful for one
    /// grammar build.
    #[must_use]
    pub fn grammar_fingerprint(self) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut hash = FNV_OFFSET;
        let mut mix = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(FNV_PRIME);
            }
        };

        let lang = self.ts_language();
        mix(&(lang.abi_version() as u64).to_le_bytes());
        let kind_count = lang.node_kind_count();
        mix(&(kind_count as u64).to_le_bytes());
        for id in 0..kind_count {
            let id = u16::try_from(id).unwrap_or(u16::MAX);
            mix(lang.node_kind_for_id(id).unwrap_or("").as_bytes());
            mix(&[0, u8::from(lang.node_kind_is_named(id))]);
        }
        let field_count = lang.field_count();
        mix(&(field_count as u64).to_le_bytes());
        // Field ids are 1-based; `field_count` is the last valid id.
        for id in 1..=field_count {
            let id = u16::try_from(id).unwrap_or(u16::MAX);
            mix(lang.field_name_for_id(id).unwrap_or("").as_bytes());
            mix(&[0]);
        }
        hash
    }
}

impl fmt::Display for AstLanguage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for AstLanguage {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|lang| lang.name() == value)
            .ok_or_else(|| format!("unknown language '{value}' (expected one of rust, python, javascript, typescript, tsx, go, java)"))
    }
}

impl Language for AstLanguage {
    fn kind_to_id(&self, kind: &str) -> u16 {
        self.ts_language().id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.ts_language()
            .field_id_for_name(field)
            .map(std::num::NonZeroU16::get)
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|src| StrDoc::try_new(src, *self))
    }

    fn expando_char(&self) -> char {
        self.expando()
    }

    // Vendored verbatim from ast-grep-language 0.45.0 `pre_process_pattern`:
    // metavariable-shaped `$` sigils are rewritten to the expando char before
    // the pattern text is parsed.
    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
        if self.expando() == '$' {
            return Cow::Borrowed(query);
        }
        let expando = self.expando();
        let mut ret = Vec::with_capacity(query.len());
        let mut dollar_count = 0;
        for c in query.chars() {
            if c == '$' {
                dollar_count += 1;
                continue;
            }
            let need_replace = matches!(c, 'A'..='Z' | '_') || dollar_count == 3;
            let sigil = if need_replace { expando } else { '$' };
            ret.extend(std::iter::repeat_n(sigil, dollar_count));
            dollar_count = 0;
            ret.push(c);
        }
        let sigil = if dollar_count == 3 { expando } else { '$' };
        ret.extend(std::iter::repeat_n(sigil, dollar_count));
        Cow::Owned(ret.into_iter().collect())
    }
}

impl LanguageExt for AstLanguage {
    fn get_ts_language(&self) -> TSLanguage {
        self.ts_language()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_zero_is_reserved() {
        for lang in AstLanguage::ALL {
            assert_eq!(AstLanguage::from_code(lang.code()), Some(lang));
            assert!(lang.code() > 0);
        }
        assert_eq!(AstLanguage::from_code(0), None);
        assert_eq!(AstLanguage::from_code(8), None);
    }

    #[test]
    fn names_round_trip() {
        for lang in AstLanguage::ALL {
            assert_eq!(lang.name().parse::<AstLanguage>(), Ok(lang));
        }
        assert!("klingon".parse::<AstLanguage>().is_err());
    }

    #[test]
    fn extensions_map_to_languages() {
        let cases = [
            ("src/main.rs", Some(AstLanguage::Rust)),
            ("app.py", Some(AstLanguage::Python)),
            ("BUILD.bazel", Some(AstLanguage::Python)),
            ("index.js", Some(AstLanguage::JavaScript)),
            ("view.jsx", Some(AstLanguage::JavaScript)),
            ("lib.ts", Some(AstLanguage::TypeScript)),
            ("lib.mts", Some(AstLanguage::TypeScript)),
            ("App.tsx", Some(AstLanguage::Tsx)),
            ("main.go", Some(AstLanguage::Go)),
            ("Main.java", Some(AstLanguage::Java)),
            ("README.md", None),
            ("Makefile", None),
        ];
        for (path, expected) in cases {
            assert_eq!(AstLanguage::from_path(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn fingerprints_are_stable_and_distinct() {
        for lang in AstLanguage::ALL {
            assert_eq!(lang.grammar_fingerprint(), lang.grammar_fingerprint());
        }
        // TypeScript and TSX are different grammars and must not share a
        // fingerprint even though they ship in one crate.
        assert_ne!(
            AstLanguage::TypeScript.grammar_fingerprint(),
            AstLanguage::Tsx.grammar_fingerprint()
        );
    }

    #[test]
    fn expando_rewrites_metavariables_only() {
        let lang = AstLanguage::Python;
        assert_eq!(lang.pre_process_pattern("$A + $_B"), "µA + µ_B");
        assert_eq!(lang.pre_process_pattern("f($$$ARGS)"), "f(µµµARGS)");
        // A dollar that does not start a metavariable stays a dollar.
        assert_eq!(lang.pre_process_pattern("cost = $5"), "cost = $5");
        // `$`-native languages leave the pattern untouched.
        assert!(matches!(
            AstLanguage::JavaScript.pre_process_pattern("$A"),
            Cow::Borrowed("$A")
        ));
    }
}
