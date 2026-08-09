mod pcre2;
mod rust;

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

pub(super) struct MatcherBuilder<'a> {
    query: &'a Query,
}

impl<'a> MatcherBuilder<'a> {
    pub(super) const fn new(query: &'a Query) -> Self {
        Self { query }
    }

    pub(super) fn build(self) -> Result<Matcher, SearchError> {
        match self.query.options.regex_engine {
            RegexEngine::Rust => rust::build(self.query).map(Matcher::Rust),
            RegexEngine::Pcre2 => pcre2::build(self.query).map(Matcher::Pcre2),
            RegexEngine::Auto => rust::build(self.query).map_or_else(
                |_| pcre2::build(self.query).map(Matcher::Pcre2),
                |matcher| Ok(Matcher::Rust(matcher)),
            ),
        }
    }
}

impl Matcher {
    pub(super) const fn resolved_engine(&self) -> RegexEngine {
        match self {
            Self::Rust(_) => RegexEngine::Rust,
            Self::Pcre2(_) => RegexEngine::Pcre2,
        }
    }
}
