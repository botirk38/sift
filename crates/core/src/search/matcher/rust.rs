use regex::bytes::{Captures, Regex, RegexBuilder};

use crate::SearchError;
use crate::search::event::Span;
use crate::search::options::Case;
use crate::search::query::{PatternBound, Query};

use super::template::{Groups, Template};

#[derive(Debug, Clone)]
pub(super) struct Rust {
    regex: Regex,
    names: Vec<Option<String>>,
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
        let pattern = match query.bound() {
            Some(PatternBound::Line) => format!("^(?:{joined})$"),
            Some(PatternBound::Word) => {
                format!(r"\b{{start-half}}(?:{joined})\b{{end-half}}")
            }
            None => joined,
        };

        let mut builder = RegexBuilder::new(&pattern);
        builder.multi_line(true);
        builder.unicode(opts.unicode);
        builder.case_insensitive(query.case() == Case::Insensitive);
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
        let names = regex
            .capture_names()
            .map(|name| name.map(str::to_owned))
            .collect();
        Ok(Self { regex, names })
    }

    pub(super) fn matched(&self, haystack: &[u8]) -> bool {
        self.regex.is_match(haystack)
    }

    pub(super) fn spans(&self, haystack: &[u8], template: Option<&[u8]>) -> Vec<Span> {
        let mut spans = Vec::new();
        for caps in self.regex.captures_iter(haystack) {
            spans.push(Self::span(&caps, template, &self.names));
        }
        spans
    }

    fn span(caps: &Captures<'_>, template: Option<&[u8]>, names: &[Option<String>]) -> Span {
        let range = caps.get(0).map_or(0..0, |m| m.range());
        let replacement = template.map(|template| {
            let mut text = Vec::new();
            Template(template).expand(&Self::groups(caps, names), &mut text);
            text
        });
        Span { range, replacement }
    }

    fn groups<'a>(caps: &'a Captures<'a>, names: &'a [Option<String>]) -> Groups<'a> {
        Groups {
            slots: (0..caps.len())
                .map(|i| caps.get(i).map(|m| m.as_bytes()))
                .collect(),
            names,
        }
    }
}
