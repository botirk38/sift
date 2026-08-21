mod pcre2;
mod rust;
mod template;

use crate::SearchError;
use crate::search::event::Span;
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

/// Per-file matcher scratch. Owned by [`crate::search::scan::FileScan`].
pub(super) enum Scratch {
    Rust {
        cache: Box<regex_automata::meta::Cache>,
        caps: regex_automata::util::captures::Captures,
    },
    Pcre2 {
        data: ::pcre2::bytes::MatchData,
    },
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

    pub(super) fn scratch(&self) -> Scratch {
        match &self.engine {
            Engine::Rust(engine) => engine.scratch(),
            Engine::Pcre2(engine) => engine.scratch(),
        }
    }

    pub(super) fn matched(
        &self,
        haystack: &[u8],
        scratch: &mut Scratch,
    ) -> Result<bool, SearchError> {
        match (&self.engine, scratch) {
            (Engine::Rust(engine), Scratch::Rust { cache, .. }) => {
                Ok(engine.matched(haystack, cache))
            }
            (Engine::Pcre2(engine), Scratch::Pcre2 { data }) => engine.matched(haystack, data),
            _ => unreachable!("scratch does not match compiled engine"),
        }
    }

    pub(super) fn spans(
        &self,
        haystack: &[u8],
        template: Option<&[u8]>,
        scratch: &mut Scratch,
    ) -> Result<Vec<Span>, SearchError> {
        match (&self.engine, scratch) {
            (Engine::Rust(engine), Scratch::Rust { cache, caps }) => {
                Ok(engine.spans(haystack, template, cache, caps))
            }
            (Engine::Pcre2(engine), Scratch::Pcre2 { data }) => {
                engine.spans(haystack, template, data)
            }
            _ => unreachable!("scratch does not match compiled engine"),
        }
    }
}
