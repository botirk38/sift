mod pcre2;
mod rust;

use std::ops::Range;

use crate::SearchError;
use crate::search::event::Replacement;
use crate::search::options::RegexEngine;
use crate::search::query::Query;

#[derive(Debug, Clone)]
pub(super) struct Matcher {
    engine: Engine,
}

#[derive(Debug, Clone)]
enum Engine {
    Rust(rust::Rust),
    Pcre2(pcre2::Pcre2),
}

impl Matcher {
    pub(super) fn compile(query: &Query) -> Result<Self, SearchError> {
        let engine = match query.options.regex_engine {
            RegexEngine::Rust => Engine::Rust(rust::Rust::compile(query)?),
            RegexEngine::Pcre2 => Engine::Pcre2(pcre2::Pcre2::compile(query)?),
            RegexEngine::Auto => rust::Rust::compile(query)
                .map(Engine::Rust)
                .or_else(|_| pcre2::Pcre2::compile(query).map(Engine::Pcre2))?,
        };
        Ok(Self { engine })
    }

    pub(super) const fn resolved_engine(&self) -> RegexEngine {
        match &self.engine {
            Engine::Rust(_) => RegexEngine::Rust,
            Engine::Pcre2(_) => RegexEngine::Pcre2,
        }
    }

    pub(super) fn is_match(&self, haystack: &[u8]) -> Result<bool, SearchError> {
        match &self.engine {
            Engine::Rust(engine) => Ok(engine.is_match(haystack)),
            Engine::Pcre2(engine) => engine.is_match(haystack),
        }
    }

    pub(super) fn ranges(&self, haystack: &[u8]) -> Result<Vec<Range<usize>>, SearchError> {
        match &self.engine {
            Engine::Rust(engine) => Ok(engine.ranges(haystack)),
            Engine::Pcre2(engine) => engine.ranges(haystack),
        }
    }

    pub(super) fn replace(
        &self,
        haystack: &[u8],
        template: &[u8],
    ) -> Result<Replacement, SearchError> {
        match &self.engine {
            Engine::Rust(engine) => Ok(engine.replace(haystack, template)),
            Engine::Pcre2(engine) => engine.replace(haystack, template),
        }
    }
}
