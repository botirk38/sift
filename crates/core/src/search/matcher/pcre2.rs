use std::ops::Range;

use grep_matcher::{Captures, Matcher as GrepMatcher};
use grep_pcre2::{RegexMatcher as Pcre2Matcher, RegexMatcherBuilder as Pcre2MatcherBuilder};

use crate::SearchError;
use crate::search::event::Replacement;
use crate::search::options::CaseMode;
use crate::search::query::Query;

#[derive(Debug, Clone)]
pub(super) struct Pcre2 {
    regex: Pcre2Matcher,
}

impl Pcre2 {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        let opts = &query.options;
        let mut builder = Pcre2MatcherBuilder::new();
        builder.multi_line(true);
        match opts.case_mode {
            CaseMode::Sensitive => {}
            CaseMode::Insensitive => {
                builder.caseless(true);
            }
            CaseMode::Smart => {
                builder.case_smart(true);
            }
        }
        builder.utf(opts.unicode);
        builder.ucp(opts.unicode);
        builder.fixed_strings(opts.fixed_strings());
        if opts.word_regexp() {
            builder.word(true);
        }
        if opts.line_regexp() {
            builder.whole_line(true);
        }
        if opts.crlf() {
            builder.crlf(true);
        }
        if opts.multiline() && opts.multiline_dotall() {
            builder.dotall(true);
        }
        let regex = builder
            .build_many(&query.patterns)
            .map_err(|err| SearchError::RegexBuild(err.to_string()))?;
        Ok(Self { regex })
    }

    pub(super) fn matched(&self, haystack: &[u8]) -> Result<bool, SearchError> {
        self.regex
            .is_match(haystack)
            .map_err(|err| SearchError::Match(err.to_string()))
    }

    pub(super) fn ranges(&self, haystack: &[u8]) -> Result<Vec<Range<usize>>, SearchError> {
        let mut ranges = Vec::new();
        self.regex
            .find_iter(haystack, |m| {
                ranges.push(m.start()..m.end());
                true
            })
            .map_err(|err| SearchError::Match(err.to_string()))?;
        Ok(ranges)
    }

    pub(super) fn replace(
        &self,
        haystack: &[u8],
        template: &[u8],
    ) -> Result<Replacement, SearchError> {
        let mut caps = self
            .regex
            .new_captures()
            .map_err(|err| SearchError::Match(err.to_string()))?;
        let mut text = Vec::new();
        let mut matches = Vec::new();
        self.regex
            .replace_with_captures(haystack, &mut caps, &mut text, |captures, dst| {
                let start = dst.len();
                captures.interpolate(
                    |name| self.regex.capture_index(name),
                    haystack,
                    template,
                    dst,
                );
                matches.push(dst[start..].to_vec());
                true
            })
            .map_err(|err| SearchError::Match(err.to_string()))?;
        Ok(Replacement { text, matches })
    }
}
