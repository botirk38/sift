use crate::grep::Error;
use crate::search::options::{
    Case, CaseMode, InputEncoding, Narrowing, RegexEngine, SearchOptions,
};

/// Patterns and options for search and index narrowing.
#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub(crate) patterns: Vec<String>,
    pub(crate) options: SearchOptions,
}

pub struct SearchQueryBuilder {
    patterns: Vec<String>,
    options: SearchOptions,
}

impl SearchQuery {
    /// Build a query from patterns and options.
    ///
    /// # Errors
    ///
    /// Returns `Error::EmptyPatterns` if `patterns` is empty.
    pub fn new(patterns: Vec<String>, options: SearchOptions) -> Result<Self, Error> {
        if patterns.is_empty() {
            return Err(Error::EmptyPatterns);
        }
        Ok(Self { patterns, options })
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    #[must_use]
    pub const fn options(&self) -> &SearchOptions {
        &self.options
    }

    /// Record the engine that will actually run (after Auto resolution).
    #[must_use]
    pub const fn with_engine(mut self, engine: RegexEngine) -> Self {
        self.options.regex_engine = engine;
        self
    }

    #[must_use]
    pub const fn with_narrowing(mut self, narrowing: Narrowing) -> Self {
        self.options.narrowing = narrowing;
        self
    }

    #[must_use]
    pub const fn fixed_strings(&self) -> bool {
        self.options.fixed_strings()
    }

    #[must_use]
    pub const fn word_regexp(&self) -> bool {
        self.options.word_regexp()
    }

    #[must_use]
    pub const fn line_regexp(&self) -> bool {
        self.options.line_regexp()
    }

    #[must_use]
    pub const fn invert_match(&self) -> bool {
        self.options.invert_match()
    }

    #[must_use]
    pub const fn bom_sniffing(&self) -> bool {
        matches!(self.options.input_encoding, InputEncoding::Auto)
    }

    /// Letter-case matching for index grams after resolving [`CaseMode::Smart`].
    #[must_use]
    pub fn case(&self) -> Case {
        match self.options.case_mode {
            CaseMode::Sensitive => Case::Sensitive,
            CaseMode::Insensitive => Case::Insensitive,
            CaseMode::Smart => {
                if self
                    .patterns
                    .iter()
                    .all(|p| !p.chars().any(|c| c.is_ascii_uppercase()))
                {
                    Case::Insensitive
                } else {
                    Case::Sensitive
                }
            }
        }
    }

    /// Effective narrowing after engine, invert, and encoding constraints.
    #[must_use]
    pub const fn narrowing(&self) -> Narrowing {
        if matches!(self.options.narrowing, Narrowing::Disabled)
            || self.options.invert_match()
            || self.options.input_encoding.forces_decode()
            || matches!(self.options.regex_engine, RegexEngine::Pcre2)
        {
            Narrowing::Disabled
        } else {
            Narrowing::Allowed
        }
    }
}

impl SearchQueryBuilder {
    #[must_use]
    pub fn new(patterns: Vec<String>) -> Self {
        Self {
            patterns,
            options: SearchOptions::default(),
        }
    }

    #[must_use]
    pub fn options(mut self, options: SearchOptions) -> Self {
        self.options = options;
        self
    }

    /// Build inert search query data.
    ///
    /// # Errors
    ///
    /// Returns `Error::EmptyPatterns` if no patterns were provided.
    pub fn build(self) -> Result<SearchQuery, Error> {
        SearchQuery::new(self.patterns, self.options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::options::{CaseMode, RegexEngine, SearchOptions};

    #[test]
    fn smart_case_lowercase_is_insensitive() {
        let query = SearchQuery::new(
            vec!["err_sys".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Insensitive);
        assert_eq!(query.narrowing(), Narrowing::Allowed);
    }

    #[test]
    fn smart_case_uppercase_is_sensitive() {
        let query = SearchQuery::new(
            vec!["ERR_SYS".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.case(), Case::Sensitive);
    }

    #[test]
    fn pcre2_engine_disables_narrowing() {
        let query = SearchQuery::new(
            vec!["foo".into()],
            SearchOptions {
                regex_engine: RegexEngine::Pcre2,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert_eq!(query.narrowing(), Narrowing::Disabled);
    }
}
