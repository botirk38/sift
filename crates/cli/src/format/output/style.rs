use std::io::IsTerminal;

use crate::format::output::format::ColumnLimit;
use grep_printer::{HyperlinkConfig, HyperlinkEnvironment, UserColorSpec};
use termcolor::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilenameMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorChoice {
    #[default]
    Auto,
    Never,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputBuffering {
    #[default]
    Auto,
    Line,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorOutput {
    Ansi,
    Plain,
}

impl ColorOutput {
    #[must_use]
    pub fn buffer(self) -> Buffer {
        match self {
            Self::Ansi => Buffer::ansi(),
            Self::Plain => Buffer::no_color(),
        }
    }
}

pub use sift_core::PathDisplay;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct LineStyleFlags: u8 {
        const HEADING     = 1 << 0;
        const LINE_NUMBER = 1 << 1;
        const BYTE_OFFSET = 1 << 2;
        const TRIM        = 1 << 3;
        const COLUMN      = 1 << 4;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrintLineStyle {
    pub filename_mode: FilenameMode,
    pub flags: LineStyleFlags,
    pub path_display: PathDisplay,
    pub columns: Option<ColumnLimit>,
}

impl PrintLineStyle {
    #[must_use]
    pub const fn heading(self) -> bool {
        self.flags.contains(LineStyleFlags::HEADING)
    }

    #[must_use]
    pub const fn line_number(self) -> bool {
        self.flags.contains(LineStyleFlags::LINE_NUMBER)
    }

    #[must_use]
    pub const fn byte_offset(self) -> bool {
        self.flags.contains(LineStyleFlags::BYTE_OFFSET)
    }

    #[must_use]
    pub const fn trim(self) -> bool {
        self.flags.contains(LineStyleFlags::TRIM)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordTerminator {
    #[default]
    Newline,
    Nul,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrintRecordStyle {
    pub terminator: RecordTerminator,
    pub color: ColorChoice,
    pub path_separator: Option<u8>,
    pub colors: ColorSpecs,
    pub hyperlink: HyperlinkFormat,
    pub hyperlink_host: Option<String>,
    pub buffering: OutputBuffering,
}

impl PrintRecordStyle {
    #[must_use]
    pub fn color_output(&self) -> ColorOutput {
        match self.color {
            ColorChoice::Never => ColorOutput::Plain,
            ColorChoice::Always => ColorOutput::Ansi,
            ColorChoice::Auto => {
                if std::io::stdout().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none()
                    && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
                {
                    ColorOutput::Ansi
                } else {
                    ColorOutput::Plain
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpecs(grep_printer::ColorSpecs);

impl Default for ColorSpecs {
    fn default() -> Self {
        Self(grep_printer::ColorSpecs::default_with_color())
    }
}

impl ColorSpecs {
    /// # Errors
    ///
    /// Returns an error when a user color specification is not ripgrep-compatible.
    pub fn from_specs(specs: &[String]) -> Result<Self, String> {
        let mut user_specs = grep_printer::default_color_specs();
        for spec in specs {
            user_specs.push(spec.parse::<UserColorSpec>().map_err(|e| e.to_string())?);
        }
        Ok(Self(grep_printer::ColorSpecs::new(&user_specs)))
    }

    #[must_use]
    pub fn as_grep(&self) -> grep_printer::ColorSpecs {
        self.0.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HyperlinkFormat {
    inner: grep_printer::HyperlinkFormat,
}

impl HyperlinkFormat {
    /// # Errors
    ///
    /// Returns an error when the format has invalid braces or variables.
    pub fn parse(value: Option<&str>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        value
            .parse::<grep_printer::HyperlinkFormat>()
            .map(|inner| Self { inner })
            .map_err(|e| e.to_string())
    }

    #[must_use]
    pub fn config(&self, host: Option<String>) -> HyperlinkConfig {
        let mut env = HyperlinkEnvironment::new();
        env.host(host);
        self.inner.clone().into_config(env)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl RecordTerminator {
    pub fn write_to(&self, out: &mut Vec<u8>) {
        match self {
            Self::Nul => out.push(0),
            Self::Newline => out.push(b'\n'),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintSeparators {
    pub context_separator: Option<Vec<u8>>,
    pub field_match_separator: Vec<u8>,
    pub field_context_separator: Vec<u8>,
}

impl Default for PrintSeparators {
    fn default() -> Self {
        Self {
            context_separator: Some(b"--".to_vec()),
            field_match_separator: b":".to_vec(),
            field_context_separator: b"-".to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_line_style_defaults() {
        let style = PrintLineStyle::default();
        assert!(!style.heading());
        assert!(!style.line_number());
        assert!(!style.byte_offset());
        assert!(!style.trim());
    }

    #[test]
    fn search_record_style_defaults() {
        let style = PrintRecordStyle::default();
        assert!(matches!(style.terminator, RecordTerminator::Newline));
        assert_eq!(style.color, ColorChoice::Auto);
        assert!(style.path_separator.is_none());
    }

    #[test]
    fn search_separators_defaults() {
        let sep = PrintSeparators::default();
        assert_eq!(sep.context_separator, Some(b"--".to_vec()));
        assert_eq!(sep.field_match_separator, b":".to_vec());
        assert_eq!(sep.field_context_separator, b"-".to_vec());
    }
}
