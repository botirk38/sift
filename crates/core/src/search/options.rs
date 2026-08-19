use std::str::FromStr;

use super::input::Mention;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SearchFlags: u16 {
        const INVERT_MATCH     = 1 << 0;
        const FIXED_STRINGS    = 1 << 1;
        const WORD_REGEXP      = 1 << 2;
        const LINE_REGEXP      = 1 << 3;
        const MULTILINE        = 1 << 4;
        const MULTILINE_DOTALL = 1 << 5;
        const CRLF             = 1 << 6;
        const NULL_DATA        = 1 << 7;
        const PASSTHRU         = 1 << 8;
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
        pcre2::version()
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

/// How NULs in a haystack are treated for one input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nul {
    /// Leave NULs in place and search through them.
    Keep,
    /// Replace NULs with the line terminator, then search.
    Convert,
    /// Stop the scan at the first NUL.
    Quit,
}

impl BinaryMode {
    pub(crate) const fn nul(self, mention: Mention, null_data: bool) -> Nul {
        if null_data {
            return Nul::Keep;
        }
        match (self, mention) {
            (Self::AsText, _) => Nul::Keep,
            (Self::Binary, _) | (Self::Quit, Mention::Explicit) => Nul::Convert,
            (Self::Quit, Mention::Discovered) => Nul::Quit,
        }
    }
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

/// How file bytes are read for search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Io {
    Sync,
    #[default]
    Mmap,
    Uring,
}

impl FromStr for Io {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "sync" => Ok(Self::Sync),
            "mmap" => Ok(Self::Mmap),
            "uring" if cfg!(target_os = "linux") => Ok(Self::Uring),
            "uring" => Err("io uring is only available on Linux".into()),
            other => Err(format!(
                "unknown io '{other}': expected sync, mmap, or uring"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputEncoding {
    #[default]
    Auto,
    Raw,
    Explicit(&'static encoding_rs::Encoding),
}

impl InputEncoding {
    #[must_use]
    pub const fn bom_sniffing(&self) -> bool {
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
        encoding_rs::Encoding::for_label_no_replacement(value.as_bytes())
            .map(Self::Explicit)
            .ok_or_else(|| format!("unknown encoding '{value}'"))
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
    pub io: Io,
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
            io: Io::default(),
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
    pub const fn passthru(&self) -> bool {
        self.flags.contains(SearchFlags::PASSTHRU)
    }

    #[must_use]
    pub const fn line_terminator(&self) -> u8 {
        if self.null_data() { b'\0' } else { b'\n' }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_auto_and_none() {
        assert_eq!(
            "auto".parse::<InputEncoding>().unwrap(),
            InputEncoding::Auto
        );
        assert_eq!("NONE".parse::<InputEncoding>().unwrap(), InputEncoding::Raw);
    }

    #[test]
    fn encoding_explicit_keeps_label() {
        assert_eq!(
            "utf-16le".parse::<InputEncoding>().unwrap(),
            InputEncoding::Explicit(encoding_rs::UTF_16LE)
        );
    }

    #[test]
    fn encoding_unknown_is_rejected() {
        assert!("not-an-encoding".parse::<InputEncoding>().is_err());
    }

    #[test]
    fn io_parse_sync_mmap() {
        assert_eq!("sync".parse::<Io>().unwrap(), Io::Sync);
        assert_eq!("mmap".parse::<Io>().unwrap(), Io::Mmap);
    }

    #[test]
    fn io_parse_unknown_is_rejected() {
        assert!("dma".parse::<Io>().is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn io_parse_uring_on_linux() {
        assert_eq!("uring".parse::<Io>().unwrap(), Io::Uring);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn io_parse_uring_off_linux_is_error() {
        let err = "uring".parse::<Io>().unwrap_err();
        assert!(err.contains("Linux"), "{err}");
    }
}
