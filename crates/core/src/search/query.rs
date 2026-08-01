use crate::grep::Error;
use crate::search::options::{CaseMode, InputEncoding, RegexEngine, SearchOptions};

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

    /// Disable index narrowing (e.g. content transform searches raw-indexed files).
    #[must_use]
    pub const fn without_index_narrowing(mut self) -> Self {
        self.options.allow_index_narrowing = false;
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

    /// Whether index gram matching should treat letters case-insensitively.
    ///
    /// Resolves [`CaseMode::Smart`] from patterns: all-ASCII-lowercase → insensitive
    /// (matcher `case_smart`); otherwise sensitive.
    #[must_use]
    pub fn case_insensitive_for_index(&self) -> bool {
        match self.options.case_mode {
            CaseMode::Sensitive => false,
            CaseMode::Insensitive => true,
            CaseMode::Smart => self
                .patterns
                .iter()
                .all(|p| !p.chars().any(|c| c.is_ascii_uppercase())),
        }
    }

    /// Whether the index may narrow candidates for this query.
    #[must_use]
    pub const fn narrowing_allowed(&self) -> bool {
        self.options.allow_index_narrowing
            && !self.options.invert_match()
            && !self.options.input_encoding.forces_decode()
            && !matches!(self.options.regex_engine, RegexEngine::Pcre2)
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
    fn smart_case_lowercase_is_insensitive_for_index() {
        let query = SearchQuery::new(
            vec!["err_sys".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert!(query.case_insensitive_for_index());
        assert!(query.narrowing_allowed());
    }

    #[test]
    fn smart_case_uppercase_is_sensitive_for_index() {
        let query = SearchQuery::new(
            vec!["ERR_SYS".into()],
            SearchOptions {
                case_mode: CaseMode::Smart,
                ..SearchOptions::default()
            },
        )
        .expect("query");
        assert!(!query.case_insensitive_for_index());
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
        assert!(!query.narrowing_allowed());
    }
}
