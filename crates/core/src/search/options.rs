use std::str::FromStr;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SearchFlags: u16 {
        const INVERT_MATCH     = 1 << 0;
        const FIXED_STRINGS    = 1 << 1;
        const WORD_REGEXP      = 1 << 2;
        const LINE_REGEXP      = 1 << 3;
        const ONLY_MATCHING    = 1 << 4;
        const MULTILINE        = 1 << 5;
        const MULTILINE_DOTALL = 1 << 6;
        const CRLF             = 1 << 7;
        const NULL_DATA        = 1 << 8;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegexEngine {
    #[default]
    Rust,
    Pcre2,
    Auto,
}

impl RegexEngine {
    /// Linked PCRE2 library version (`major`, `minor`).
    #[must_use]
    pub fn pcre2_version() -> (u32, u32) {
        grep_pcre2::version()
    }
}

impl FromStr for RegexEngine {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "default" | "rust" => Ok(Self::Rust),
            "pcre2" => Ok(Self::Pcre2),
            "auto" | "auto-hybrid" => Ok(Self::Auto),
            other => Err(format!(
                "unknown regex engine '{other}': expected default, pcre2, or auto"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseMode {
    #[default]
    Sensitive,
    Insensitive,
    Smart,
}

/// Letter-case matching after [`CaseMode::Smart`] is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
}

/// Whether the search may use indexes to narrow candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Narrowing {
    /// Plan may query indexes for potential matches.
    #[default]
    Allowed,
    /// Plan must not use index narrowing (e.g. content transforms).
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinaryMode {
    #[default]
    Quit,
    Binary,
    AsText,
}

/// When search may stop before exhausting every input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchBound {
    /// Search every input.
    #[default]
    Exhaustive,
    /// Stop after the first matching input (quiet existence checks).
    FirstMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InputEncoding {
    #[default]
    Auto,
    Raw,
    Explicit(String),
}

impl InputEncoding {
    #[must_use]
    pub const fn bom_sniffing(&self) -> bool {
        matches!(self, Self::Auto | Self::Explicit(_))
    }

    #[must_use]
    pub fn label(&self) -> Option<&str> {
        match self {
            Self::Explicit(label) => Some(label),
            Self::Auto | Self::Raw => None,
        }
    }

    pub(super) fn searcher_encoding(&self) -> Option<grep_searcher::Encoding> {
        self.label()
            .and_then(|label| grep_searcher::Encoding::new(label).ok())
    }

    #[must_use]
    pub const fn uses_decoded_input(&self) -> bool {
        matches!(self, Self::Auto | Self::Explicit(_))
    }

    /// Whether search may transcode file bytes before matching (explicit `-E`).
    ///
    /// `Auto` only BOM-sniffs and usually still matches raw UTF-8/ASCII bytes, so
    /// it does **not** force decode for index-query decisions.
    #[must_use]
    pub const fn forces_decode(&self) -> bool {
        matches!(self, Self::Explicit(_))
    }
}

impl FromStr for InputEncoding {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.eq_ignore_ascii_case("auto") {
            return Ok(Self::Auto);
        }
        if value.eq_ignore_ascii_case("none") {
            return Ok(Self::Raw);
        }
        grep_searcher::Encoding::new(value)
            .map(|_| Self::Explicit(value.to_string()))
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub flags: SearchFlags,
    pub case_mode: CaseMode,
    pub max_results: Option<usize>,
    pub before_context: usize,
    pub after_context: usize,
    pub binary_mode: BinaryMode,
    pub search_bound: SearchBound,
    pub input_encoding: InputEncoding,
    pub replace: Option<String>,
    pub unicode: bool,
    pub regex_size_limit: usize,
    pub dfa_size_limit: usize,
    pub regex_engine: RegexEngine,
    pub narrowing: Narrowing,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            flags: SearchFlags::default(),
            case_mode: CaseMode::default(),
            max_results: None,
            before_context: 0,
            after_context: 0,
            binary_mode: BinaryMode::default(),
            search_bound: SearchBound::default(),
            input_encoding: InputEncoding::default(),
            replace: None,
            unicode: true,
            regex_size_limit: 0,
            dfa_size_limit: 0,
            regex_engine: RegexEngine::default(),
            narrowing: Narrowing::default(),
        }
    }
}

impl SearchOptions {
    #[must_use]
    pub const fn invert_match(&self) -> bool {
        self.flags.contains(SearchFlags::INVERT_MATCH)
    }

    #[must_use]
    pub const fn fixed_strings(&self) -> bool {
        self.flags.contains(SearchFlags::FIXED_STRINGS)
    }

    #[must_use]
    pub const fn word_regexp(&self) -> bool {
        self.flags.contains(SearchFlags::WORD_REGEXP)
    }

    #[must_use]
    pub const fn line_regexp(&self) -> bool {
        self.flags.contains(SearchFlags::LINE_REGEXP)
    }

    #[must_use]
    pub const fn only_matching(&self) -> bool {
        self.flags.contains(SearchFlags::ONLY_MATCHING)
    }

    #[must_use]
    pub const fn multiline(&self) -> bool {
        self.flags.contains(SearchFlags::MULTILINE)
    }

    #[must_use]
    pub const fn multiline_dotall(&self) -> bool {
        self.flags.contains(SearchFlags::MULTILINE_DOTALL)
    }

    #[must_use]
    pub const fn crlf(&self) -> bool {
        self.flags.contains(SearchFlags::CRLF)
    }

    #[must_use]
    pub const fn null_data(&self) -> bool {
        self.flags.contains(SearchFlags::NULL_DATA)
    }

    #[must_use]
    pub const fn line_terminator(&self) -> u8 {
        if self.null_data() { b'\0' } else { b'\n' }
    }
}
