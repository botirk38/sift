mod pcre2;
mod rust;

use std::ops::Range;

use grep_matcher::{Captures, Match, Matcher as GrepMatcher};
use grep_pcre2::RegexMatcher as Pcre2Matcher;
use grep_regex::RegexMatcher;

use crate::SearchError;
use crate::search::options::RegexEngine;
use crate::search::query::Query;

#[derive(Debug, Clone)]
pub(super) enum Matcher {
    Rust(RegexMatcher),
    Pcre2(Pcre2Matcher),
}

impl Matcher {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        match query.options.regex_engine {
            RegexEngine::Rust => rust::build(query).map(Self::Rust),
            RegexEngine::Pcre2 => pcre2::build(query).map(Self::Pcre2),
            RegexEngine::Auto => rust::build(query).map_or_else(
                |_| pcre2::build(query).map(Self::Pcre2),
                |matcher| Ok(Self::Rust(matcher)),
            ),
        }
    }

    pub(super) const fn resolved_engine(&self) -> RegexEngine {
        match self {
            Self::Rust(_) => RegexEngine::Rust,
            Self::Pcre2(_) => RegexEngine::Pcre2,
        }
    }

    pub(super) fn is_match(&self, haystack: &[u8]) -> Result<bool, SearchError> {
        match self {
            Self::Rust(matcher) => matcher
                .is_match(haystack)
                .map_err(|err| SearchError::Match(err.to_string())),
            Self::Pcre2(matcher) => matcher
                .is_match(haystack)
                .map_err(|err| SearchError::Match(err.to_string())),
        }
    }

    pub(super) fn find(&self, haystack: &[u8]) -> Result<Option<Match>, SearchError> {
        match self {
            Self::Rust(matcher) => matcher
                .find(haystack)
                .map_err(|err| SearchError::Match(err.to_string())),
            Self::Pcre2(matcher) => matcher
                .find(haystack)
                .map_err(|err| SearchError::Match(err.to_string())),
        }
    }

    pub(super) fn ranges(&self, bytes: &[u8]) -> Result<Vec<Range<usize>>, SearchError> {
        match self {
            Self::Rust(matcher) => Self::collect_ranges(matcher, bytes),
            Self::Pcre2(matcher) => Self::collect_ranges(matcher, bytes),
        }
    }

    pub(super) fn expand(
        &self,
        bytes: &[u8],
        replacement: &[u8],
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), SearchError> {
        match self {
            Self::Rust(matcher) => Self::expand_with(matcher, bytes, replacement),
            Self::Pcre2(matcher) => Self::expand_with(matcher, bytes, replacement),
        }
    }

    fn collect_ranges<M: GrepMatcher>(
        matcher: &M,
        bytes: &[u8],
    ) -> Result<Vec<Range<usize>>, SearchError> {
        let mut ranges = Vec::new();
        matcher
            .find_iter(bytes, |m| {
                ranges.push(m.start()..m.end());
                true
            })
            .map_err(|err| SearchError::Match(err.to_string()))?;
        Ok(ranges)
    }

    fn expand_with<M: GrepMatcher>(
        matcher: &M,
        bytes: &[u8],
        replacement: &[u8],
    ) -> Result<(Vec<u8>, Vec<Vec<u8>>), SearchError> {
        let mut caps = matcher
            .new_captures()
            .map_err(|err| SearchError::Match(err.to_string()))?;
        let mut line = Vec::new();
        let mut spans = Vec::new();
        matcher
            .replace_with_captures(bytes, &mut caps, &mut line, |captures, dst| {
                let start = dst.len();
                captures.interpolate(|name| matcher.capture_index(name), bytes, replacement, dst);
                spans.push(dst[start..].to_vec());
                true
            })
            .map_err(|err| SearchError::Match(err.to_string()))?;
        Ok((line, spans))
    }
}
