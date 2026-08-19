use pcre2::bytes::{Captures, Regex, RegexBuilder};

use crate::SearchError;
use crate::search::event::Span;
use crate::search::options::Case;
use crate::search::query::{PatternBound, Query};

use super::template::{Groups, Template};

#[derive(Debug, Clone)]
pub(super) struct Pcre2 {
    regex: Regex,
}

impl Pcre2 {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        let opts = &query.options;
        let mut joined = String::new();
        for (i, pattern) in query.patterns.iter().enumerate() {
            if i > 0 {
                joined.push('|');
            }
            joined.push_str("(?:");
            if opts.fixed_strings() {
                joined.push_str(&pcre2::escape(pattern));
            } else {
                joined.push_str(pattern);
            }
            joined.push(')');
        }
        let pattern = match query.bound() {
            Some(PatternBound::Line) => format!("^(?:{joined})$"),
            Some(PatternBound::Word) => format!(r"(?<!\w)(?:{joined})(?!\w)"),
            None => joined,
        };

        let mut builder = RegexBuilder::new();
        builder.multi_line(true);
        builder.caseless(query.case() == Case::Insensitive);
        builder.utf(opts.unicode);
        builder.ucp(opts.unicode);
        builder.jit_if_available(true);
        if opts.crlf() {
            builder.crlf(true);
        }
        if opts.multiline() && opts.multiline_dotall() {
            builder.dotall(true);
        }
        let regex = builder
            .build(&pattern)
            .map_err(|err| SearchError::RegexBuild(err.to_string()))?;
        Ok(Self { regex })
    }

    pub(super) fn matched(&self, haystack: &[u8]) -> Result<bool, SearchError> {
        self.regex
            .is_match(haystack)
            .map_err(|err| SearchError::Match(err.to_string()))
    }

    pub(super) fn spans(
        &self,
        haystack: &[u8],
        template: Option<&[u8]>,
    ) -> Result<Vec<Span>, SearchError> {
        let mut spans = Vec::new();
        for caps in self.regex.captures_iter(haystack) {
            let caps = caps.map_err(|err| SearchError::Match(err.to_string()))?;
            spans.push(self.span(&caps, template));
        }
        Ok(spans)
    }

    fn span(&self, caps: &Captures<'_>, template: Option<&[u8]>) -> Span {
        let range = caps.get(0).map_or(0..0, |m| m.start()..m.end());
        let replacement = template.map(|template| {
            let mut text = Vec::new();
            Template(template).expand(&self.groups(caps), &mut text);
            text
        });
        Span { range, replacement }
    }

    fn groups<'a>(&'a self, caps: &'a Captures<'a>) -> Groups<'a> {
        Groups {
            slots: (0..caps.len())
                .map(|i| caps.get(i).map(|m| m.as_bytes()))
                .collect(),
            names: self.regex.capture_names(),
        }
    }
}
