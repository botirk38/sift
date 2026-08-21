use regex_automata::meta::Regex;
use regex_automata::util::captures::Captures;
use regex_automata::util::iter::Searcher;
use regex_automata::util::syntax;
use regex_automata::{Input, MatchKind, PatternID};

use crate::SearchError;
use crate::search::event::Span;
use crate::search::options::Case;
use crate::search::query::{PatternBound, Query};

use super::Scratch;
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

        let mut syntaxc = syntax::Config::new()
            .utf8(false)
            .multi_line(true)
            .unicode(opts.unicode)
            .case_insensitive(query.case() == Case::Insensitive);
        if opts.crlf() {
            syntaxc = syntaxc.crlf(true);
        }
        if opts.multiline() && opts.multiline_dotall() {
            syntaxc = syntaxc.dot_matches_new_line(true);
        }

        let mut metac = Regex::config()
            .match_kind(MatchKind::LeftmostFirst)
            .utf8_empty(false)
            .pool(false);
        if !opts.multiline() {
            let term = opts.line_terminator();
            metac = metac.line_terminator(term);
            syntaxc = syntaxc.line_terminator(term);
        }
        if opts.regex_size_limit > 0 {
            metac = metac.nfa_size_limit(Some(opts.regex_size_limit));
        }
        if opts.dfa_size_limit > 0 {
            metac = metac.hybrid_cache_capacity(opts.dfa_size_limit);
        }

        let regex = Regex::builder()
            .configure(metac)
            .syntax(syntaxc)
            .build(&pattern)
            .map_err(|err| SearchError::RegexBuild(err.to_string()))?;
        let names = regex
            .group_info()
            .pattern_names(PatternID::ZERO)
            .map(|name| name.map(str::to_owned))
            .collect();
        Ok(Self { regex, names })
    }

    pub(super) fn scratch(&self) -> Scratch {
        Scratch::Rust {
            cache: Box::new(self.regex.create_cache()),
            caps: self.regex.create_captures(),
        }
    }

    pub(super) fn matched(&self, haystack: &[u8], cache: &mut regex_automata::meta::Cache) -> bool {
        self.regex.is_match_with(cache, Input::new(haystack))
    }

    pub(super) fn spans(
        &self,
        haystack: &[u8],
        template: Option<&[u8]>,
        cache: &mut regex_automata::meta::Cache,
        caps: &mut Captures,
    ) -> Vec<Span> {
        let mut spans = Vec::new();
        let mut it = Searcher::new(Input::new(haystack));
        loop {
            let _ = it.advance(|input| {
                self.regex.search_captures_with(cache, input, caps);
                Ok(caps.get_match())
            });
            if !caps.is_match() {
                break;
            }
            spans.push(Self::span(caps, haystack, template, &self.names));
        }
        spans
    }

    fn span(
        caps: &Captures,
        haystack: &[u8],
        template: Option<&[u8]>,
        names: &[Option<String>],
    ) -> Span {
        let range = caps.get_group(0).map_or(0..0, |span| span.range());
        let replacement = template.map(|template| {
            let mut text = Vec::new();
            Template(template).expand(&Self::groups(caps, haystack, names), &mut text);
            text
        });
        Span { range, replacement }
    }

    fn groups<'a>(
        caps: &'a Captures,
        haystack: &'a [u8],
        names: &'a [Option<String>],
    ) -> Groups<'a> {
        Groups {
            slots: (0..caps.group_len())
                .map(|i| {
                    caps.get_group(i)
                        .and_then(|span| haystack.get(span.range()))
                })
                .collect(),
            names,
        }
    }
}
