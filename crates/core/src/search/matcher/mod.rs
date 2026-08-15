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
}
