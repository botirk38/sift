use pcre2::bytes::{MatchData, Regex, RegexBuilder};

use crate::SearchError;
use crate::search::event::Span;
use crate::search::options::Case;
use crate::search::query::{PatternBound, Query};

use super::Scratch;
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

    pub(super) fn scratch(&self) -> Scratch {
        Scratch::Pcre2 {
            data: self.regex.create_match_data(),
        }
    }

    pub(super) fn matched(
        &self,
        haystack: &[u8],
        data: &mut MatchData,
    ) -> Result<bool, SearchError> {
        self.regex
            .is_match_with(data, haystack)
            .map_err(|err| SearchError::Match(err.to_string()))
    }

    pub(super) fn spans(
        &self,
        haystack: &[u8],
        template: Option<&[u8]>,
        data: &mut MatchData,
    ) -> Result<Vec<Span>, SearchError> {
        let mut spans = Vec::new();
        let mut last_end = 0;
        let mut last_match = None;
        while last_end <= haystack.len() {
            let found = self
                .regex
                .find_at_with(data, haystack, last_end)
                .map_err(|err| SearchError::Match(err.to_string()))?;
            let Some(m) = found else {
                break;
            };
            if m.start() == m.end() {
                last_end = m.end() + 1;
                if Some(m.end()) == last_match {
                    continue;
                }
            } else {
                last_end = m.end();
            }
            last_match = Some(m.end());
            spans.push(self.span(haystack, data, template));
        }
        Ok(spans)
    }

    fn span(&self, haystack: &[u8], data: &MatchData, template: Option<&[u8]>) -> Span {
        let range = data.get(0).map_or(0..0, |(start, end)| start..end);
        let replacement = template.map(|template| {
            let mut text = Vec::new();
            Template(template).expand(&self.groups(haystack, data), &mut text);
            text
        });
        Span { range, replacement }
    }

    fn groups<'a>(&'a self, haystack: &'a [u8], data: &'a MatchData) -> Groups<'a> {
        Groups {
            slots: (0..self.regex.captures_len())
                .map(|i| {
                    data.get(i)
                        .and_then(|(start, end)| haystack.get(start..end))
                })
                .collect(),
            names: self.regex.capture_names(),
        }
    }
}
