use std::ops::Range;

use regex::bytes::{Regex, RegexBuilder};
use regex_syntax::ast::{self, Ast};

use crate::SearchError;
use crate::search::event::Replacement;
use crate::search::options::CaseMode;
use crate::search::query::Query;

#[derive(Debug, Clone)]
pub(super) struct Rust {
    regex: Regex,
}

impl Rust {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        let opts = &query.options;
        let mut joined = String::new();
        for (i, pattern) in query.patterns.iter().enumerate() {
            if i > 0 {
                joined.push('|');
            }
            joined.push_str("(?:");
            if opts.fixed_strings() {
                joined.push_str(&regex::escape(pattern));
            } else {
                joined.push_str(pattern);
            }
            joined.push(')');
        }
        let caseless = match opts.case_mode {
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
            CaseMode::Smart => Self::smart_caseless(&joined),
        };
        let pattern = if opts.line_regexp() {
            format!("^(?:{joined})$")
        } else if opts.word_regexp() {
            format!(r"\b{{start-half}}(?:{joined})\b{{end-half}}")
        } else {
            joined
        };

        let mut builder = RegexBuilder::new(&pattern);
        builder.multi_line(true);
        builder.unicode(opts.unicode);
        builder.case_insensitive(caseless);
        if opts.crlf() {
            builder.crlf(true);
        }
        if opts.multiline() {
            if opts.multiline_dotall() {
                builder.dot_matches_new_line(true);
            }
        } else {
            builder.line_terminator(opts.line_terminator());
        }
        if opts.regex_size_limit > 0 {
            builder.size_limit(opts.regex_size_limit);
        }
        if opts.dfa_size_limit > 0 {
            builder.dfa_size_limit(opts.dfa_size_limit);
        }
        let regex = builder
            .build()
            .map_err(|err| SearchError::RegexBuild(err.to_string()))?;
        Ok(Self { regex })
    }

    pub(super) fn is_match(&self, haystack: &[u8]) -> bool {
        self.regex.is_match(haystack)
    }

    pub(super) fn ranges(&self, haystack: &[u8]) -> Vec<Range<usize>> {
        self.regex.find_iter(haystack).map(|m| m.range()).collect()
    }

    pub(super) fn replace(&self, haystack: &[u8], template: &[u8]) -> Replacement {
        let mut text = Vec::new();
        let mut matches = Vec::new();
        let mut last = 0;
        for caps in self.regex.captures_iter(haystack) {
            let Some(m) = caps.get(0) else { continue };
            text.extend_from_slice(&haystack[last..m.start()]);
            let start = text.len();
            caps.expand(template, &mut text);
            matches.push(text[start..].to_vec());
            last = m.end();
        }
        text.extend_from_slice(&haystack[last..]);
        Replacement { text, matches }
    }

    fn smart_caseless(pattern: &str) -> bool {
        let Ok(ast) = regex_syntax::ast::parse::Parser::new().parse(pattern) else {
            return false;
        };
        let mut literal = false;
        let mut uppercase = false;
        Self::ast_case(&ast, &mut literal, &mut uppercase);
        literal && !uppercase
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
