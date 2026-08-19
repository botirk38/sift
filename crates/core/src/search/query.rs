use regex_syntax::ast::{self, Ast};

use crate::search::error::Error;
use crate::search::options::{
    Case, CaseMode, InputEncoding, Narrowing, RegexEngine, SearchOptions,
};

/// How a pattern is bounded in the haystack (`-x` / `-w`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PatternBound {
    Line,
    Word,
}

/// Patterns and options for search and index narrowing.
#[derive(Debug, Clone)]
pub struct Query {
    pub(crate) patterns: Vec<String>,
    pub(crate) options: SearchOptions,
}

impl Query {
    /// Build a query from patterns and options.
    ///
    /// # Errors
    ///
    /// Returns `Error::EmptyPatterns` if `patterns` is empty.
    pub fn new(patterns: Vec<String>, options: SearchOptions) -> Result<Self, Error> {
        if patterns.is_empty() {
            return Err(Error::EmptyPatterns);
        }
        Ok(Self { patterns, options })
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    #[must_use]
    pub const fn options(&self) -> &SearchOptions {
        &self.options
    }

    /// Record the engine that will actually run (after Auto resolution).
    #[must_use]
    pub const fn with_engine(mut self, engine: RegexEngine) -> Self {
        self.options.regex_engine = engine;
        self
    }

    #[must_use]
    pub const fn with_narrowing(mut self, narrowing: Narrowing) -> Self {
        self.options.narrowing = narrowing;
        self
    }

    #[must_use]
    pub const fn fixed_strings(&self) -> bool {
        self.options.fixed_strings()
    }

    #[must_use]
    pub const fn word_regexp(&self) -> bool {
        self.options.word_regexp()
    }

    #[must_use]
    pub const fn line_regexp(&self) -> bool {
        self.options.line_regexp()
    }

    #[must_use]
    pub const fn invert_match(&self) -> bool {
        self.options.invert_match()
    }

    #[must_use]
    pub const fn bom_sniffing(&self) -> bool {
        matches!(self.options.input_encoding, InputEncoding::Auto)
    }

    pub(crate) const fn bound(&self) -> Option<PatternBound> {
        if self.line_regexp() {
            Some(PatternBound::Line)
        } else if self.word_regexp() {
            Some(PatternBound::Word)
        } else {
            None
        }
    }

    /// Letter-case matching after resolving [`CaseMode::Smart`].
    ///
    /// Index grams and both regex engines use this same decision.
    #[must_use]
    pub fn case(&self) -> Case {
        match self.options.case_mode {
            CaseMode::Sensitive => Case::Sensitive,
            CaseMode::Insensitive => Case::Insensitive,
            CaseMode::Smart => self.smart_case(),
        }
    }

    /// Effective narrowing after engine, invert, and encoding constraints.
    #[must_use]
    pub const fn narrowing(&self) -> Narrowing {
        if matches!(self.options.narrowing, Narrowing::Disabled)
            || self.options.invert_match()
            || self.options.input_encoding.forces_decode()
            || matches!(self.options.regex_engine, RegexEngine::Pcre2)
        {
            Narrowing::Disabled
        } else {
            Narrowing::Allowed
        }
    }

    fn smart_case(&self) -> Case {
        if self.fixed_strings() {
            let uppercase = self
                .patterns
                .iter()
                .any(|p| p.chars().any(char::is_uppercase));
            return if uppercase {
                Case::Sensitive
            } else {
                Case::Insensitive
            };
        }
        let mut joined = String::new();
        for (i, pattern) in self.patterns.iter().enumerate() {
            if i > 0 {
                joined.push('|');
            }
            joined.push_str("(?:");
            joined.push_str(pattern);
            joined.push(')');
        }
        match regex_syntax::ast::parse::Parser::new().parse(&joined) {
            Ok(ast) => {
                let mut literal = false;
                let mut uppercase = false;
                Self::ast_case(&ast, &mut literal, &mut uppercase);
                if literal && !uppercase {
                    Case::Insensitive
                } else {
                    Case::Sensitive
                }
            }
            Err(_) if self.patterns.iter().any(|p| Self::uppercase_literal(p)) => Case::Sensitive,
            Err(_) => Case::Insensitive,
        }
    }

    fn uppercase_literal(pattern: &str) -> bool {
        let mut chars = pattern.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                chars.next();
            } else if c.is_uppercase() {
                return true;
            }
        }
        false
    }

    fn ast_case(ast: &Ast, literal: &mut bool, uppercase: &mut bool) {
        if *literal && *uppercase {
            return;
        }
        match ast {
            Ast::Empty(_)
            | Ast::Flags(_)
            | Ast::Dot(_)
            | Ast::Assertion(_)
            | Ast::ClassUnicode(_)
            | Ast::ClassPerl(_) => {}
            Ast::Literal(lit) => {
                *literal = true;
                *uppercase |= lit.c.is_uppercase();
            }
            Ast::ClassBracketed(class) => Self::class_set_case(&class.kind, literal, uppercase),
            Ast::Repetition(rep) => Self::ast_case(&rep.ast, literal, uppercase),
            Ast::Group(group) => Self::ast_case(&group.ast, literal, uppercase),
            Ast::Alternation(alt) => {
                for child in &alt.asts {
                    Self::ast_case(child, literal, uppercase);
                }
            }
            Ast::Concat(concat) => {
                for child in &concat.asts {
                    Self::ast_case(child, literal, uppercase);
                }
            }
        }
    }

    fn class_set_case(set: &ast::ClassSet, literal: &mut bool, uppercase: &mut bool) {
        if *literal && *uppercase {
            return;
        }
        match set {
            ast::ClassSet::Item(item) => Self::class_item_case(item, literal, uppercase),
            ast::ClassSet::BinaryOp(op) => {
                Self::class_set_case(&op.lhs, literal, uppercase);
                Self::class_set_case(&op.rhs, literal, uppercase);
            }
        }
    }

    fn class_item_case(item: &ast::ClassSetItem, literal: &mut bool, uppercase: &mut bool) {
        if *literal && *uppercase {
            return;
        }
        match item {
            ast::ClassSetItem::Empty(_)
            | ast::ClassSetItem::Ascii(_)
            | ast::ClassSetItem::Unicode(_)
            | ast::ClassSetItem::Perl(_) => {}
            ast::ClassSetItem::Literal(lit) => {
                *literal = true;
                *uppercase |= lit.c.is_uppercase();
            }
            ast::ClassSetItem::Range(range) => {
                *literal = true;
                *uppercase |= range.start.c.is_uppercase() || range.end.c.is_uppercase();
            }
            ast::ClassSetItem::Bracketed(class) => {
                Self::class_set_case(&class.kind, literal, uppercase);
            }
            ast::ClassSetItem::Union(union) => {
                for child in &union.items {
                    Self::class_item_case(child, literal, uppercase);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::options::{CaseMode, RegexEngine, SearchOptions};

    #[test]
    fn smart_case_lowercase_is_insensitive() {
        let query = Query::new(
            vec!["err_sys".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Insensitive);
        assert_eq!(query.narrowing(), Narrowing::Allowed);
    }

    #[test]
    fn smart_case_uppercase_is_sensitive() {
        let query = Query::new(
            vec!["ERR_SYS".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Sensitive);
    }

    #[test]
    fn smart_case_meta_only_is_sensitive() {
        let query = Query::new(
            vec![r"\w".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Sensitive);
    }

    #[test]
    fn smart_case_unparsed_lookaround_without_upper_is_insensitive() {
        let query = Query::new(
            vec![r"(?<=ba)r".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Insensitive);
    }

    #[test]
    fn pcre2_engine_disables_narrowing() {
        let query = Query::new(
            vec!["foo".into()],
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.narrowing(), Narrowing::Disabled);
    }
}
