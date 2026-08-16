use std::io::IsTerminal;

use crate::format::output::format::ColumnLimit;

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

/// ANSI SGR style applied to paths or match text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnsiStyle {
    fg: Option<u8>,
    bold: bool,
}

impl AnsiStyle {
    #[must_use]
    pub fn sequence(self) -> Option<Vec<u8>> {
        let mut codes = Vec::new();
        if self.bold {
            codes.push("1".to_string());
        }
        if let Some(fg) = self.fg {
            codes.push(fg.to_string());
        }
        if codes.is_empty() {
            return None;
        }
        Some(format!("\x1b[0m\x1b[{}m", codes.join(";")).into_bytes())
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set_fg(&mut self, name: &str) -> Result<(), String> {
        self.fg = NamedFg::parse(name)?.code();
        Ok(())
    }

    fn apply_style(&mut self, name: &str) -> Result<(), String> {
        match Self::style_name(name)? {
            StyleName::Bold => self.bold = true,
            StyleName::NoBold => self.bold = false,
            StyleName::Ignored => {}
        }
        Ok(())
    }

    fn style_name(name: &str) -> Result<StyleName, String> {
        Ok(if name.eq_ignore_ascii_case("bold") {
            StyleName::Bold
        } else if name.eq_ignore_ascii_case("nobold") {
            StyleName::NoBold
        } else if name.eq_ignore_ascii_case("intense")
            || name.eq_ignore_ascii_case("nointense")
            || name.eq_ignore_ascii_case("underline")
            || name.eq_ignore_ascii_case("nounderline")
            || name.eq_ignore_ascii_case("italic")
            || name.eq_ignore_ascii_case("noitalic")
        {
            StyleName::Ignored
        } else {
            return Err(format!(
                "unrecognized style attribute '{name}'. Choose from: \
                 nobold, bold, nointense, intense, nounderline, \
                 underline, noitalic, italic."
            ));
        })
    }
}

enum NamedFg {
    Black,
    Blue,
    Green,
    Red,
    Cyan,
    Magenta,
    Yellow,
    White,
    Extended,
}

impl NamedFg {
    fn parse(name: &str) -> Result<Self, String> {
        Ok(if name.eq_ignore_ascii_case("black") {
            Self::Black
        } else if name.eq_ignore_ascii_case("blue") {
            Self::Blue
        } else if name.eq_ignore_ascii_case("green") {
            Self::Green
        } else if name.eq_ignore_ascii_case("red") {
            Self::Red
        } else if name.eq_ignore_ascii_case("cyan") {
            Self::Cyan
        } else if name.eq_ignore_ascii_case("magenta") {
            Self::Magenta
        } else if name.eq_ignore_ascii_case("yellow") {
            Self::Yellow
        } else if name.eq_ignore_ascii_case("white") {
            Self::White
        } else if Self::extended(name) {
            Self::Extended
        } else {
            return Err(format!(
                "unrecognized color name '{name}'. Choose from: \
                 black, blue, green, red, cyan, magenta, yellow, white."
            ));
        })
    }

    fn extended(name: &str) -> bool {
        name.starts_with("0x")
            || name.contains(',')
            || name.bytes().all(|byte| byte.is_ascii_digit())
    }

    const fn code(self) -> Option<u8> {
        match self {
            Self::Black => Some(30),
            Self::Blue => Some(34),
            Self::Green => Some(32),
            Self::Red => Some(31),
            Self::Cyan => Some(36),
            Self::Magenta => Some(35),
            Self::Yellow => Some(33),
            Self::White => Some(37),
            Self::Extended => None,
        }
    }
}

enum OutputKind {
    Path,
    Line,
    Column,
    Match,
    Highlight,
}

impl OutputKind {
    fn parse(name: &str) -> Result<Self, String> {
        Ok(if name.eq_ignore_ascii_case("path") {
            Self::Path
        } else if name.eq_ignore_ascii_case("line") {
            Self::Line
        } else if name.eq_ignore_ascii_case("column") {
            Self::Column
        } else if name.eq_ignore_ascii_case("match") {
            Self::Match
        } else if name.eq_ignore_ascii_case("highlight") {
            Self::Highlight
        } else {
            return Err(format!(
                "unrecognized output type '{name}'. Choose from: \
                 path, line, column, match, highlight."
            ));
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColorSpecs {
    path: AnsiStyle,
    matched: AnsiStyle,
}

impl Default for ColorSpecs {
    fn default() -> Self {
        let mut specs = Self {
            path: AnsiStyle::default(),
            matched: AnsiStyle::default(),
        };
        #[cfg(unix)]
        {
            specs.path.fg = NamedFg::Magenta.code();
        }
        #[cfg(windows)]
        {
            specs.path.fg = NamedFg::Cyan.code();
        }
        specs.matched.fg = NamedFg::Red.code();
        specs.matched.bold = true;
        specs
    }
}

impl ColorSpecs {
    /// # Errors
    ///
    /// Returns an error when a user color specification is not ripgrep-compatible.
    pub fn from_specs(specs: &[String]) -> Result<Self, String> {
        let mut colors = Self::default();
        for spec in specs {
            colors.apply(spec)?;
        }
        Ok(colors)
    }

    #[must_use]
    pub const fn path(&self) -> AnsiStyle {
        self.path
    }

    #[must_use]
    pub const fn matched(&self) -> AnsiStyle {
        self.matched
    }

    fn apply(&mut self, spec: &str) -> Result<(), String> {
        let mut parts = spec.split(':');
        let Some(kind) = parts.next() else {
            return Err(Self::invalid_format(spec));
        };
        let Some(attr) = parts.next() else {
            return Err(Self::invalid_format(spec));
        };
        let value = parts.next();
        if parts.next().is_some() {
            return Err(Self::invalid_format(spec));
        }
        let kind = OutputKind::parse(kind)?;
        let target = match kind {
            OutputKind::Path => Some(&mut self.path),
            OutputKind::Match => Some(&mut self.matched),
            OutputKind::Line | OutputKind::Column | OutputKind::Highlight => None,
        };
        if attr.eq_ignore_ascii_case("none") {
            if let Some(style) = target {
                style.clear();
            }
            return Ok(());
        }
        let Some(value) = value else {
            return Err(Self::invalid_format(spec));
        };
        let Some(style) = target else {
            Self::validate_attr(attr, value)?;
            return Ok(());
        };
        if attr.eq_ignore_ascii_case("fg") {
            style.set_fg(value)?;
        } else if attr.eq_ignore_ascii_case("bg") {
            NamedFg::parse(value)?;
        } else if attr.eq_ignore_ascii_case("style") {
            style.apply_style(value)?;
        } else {
            return Err(format!(
                "unrecognized spec type '{attr}'. Choose from: \
                 fg, bg, style, none."
            ));
        }
        Ok(())
    }

    fn validate_attr(attr: &str, value: &str) -> Result<(), String> {
        if attr.eq_ignore_ascii_case("fg") || attr.eq_ignore_ascii_case("bg") {
            NamedFg::parse(value).map(|_| ())
        } else if attr.eq_ignore_ascii_case("style") {
            AnsiStyle::style_name(value).map(|_| ())
        } else {
            Err(format!(
                "unrecognized spec type '{attr}'. Choose from: \
                 fg, bg, style, none."
            ))
        }
    }

    fn invalid_format(spec: &str) -> String {
        format!(
            "invalid color spec format: '{spec}'. Valid format is \
             '(path|line|column|match|highlight):(fg|bg|style):(value)'."
        )
    }
}

enum StyleName {
    Bold,
    NoBold,
    Ignored,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HyperlinkFormat {
    enabled: bool,
}

impl HyperlinkFormat {
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        Self {
            enabled: match value {
                None | Some("") => false,
                Some(format) if format.eq_ignore_ascii_case("none") => false,
                Some(_) => true,
            },
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.enabled
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

    #[test]
    fn colors_parse_match_blue_nobold() {
        let colors =
            ColorSpecs::from_specs(&["match:fg:blue".into(), "match:style:nobold".into()]).unwrap();
        assert_eq!(
            colors.matched().sequence(),
            Some(b"\x1b[0m\x1b[34m".to_vec())
        );
    }

    #[test]
    fn colors_reject_unknown_output_type() {
        let err = ColorSpecs::from_specs(&["bogus:fg:red".into()]).unwrap_err();
        assert!(err.contains("unrecognized output type 'bogus'"));
    }

    #[test]
    fn hyperlink_none_is_empty() {
        assert!(HyperlinkFormat::parse(None).is_empty());
        assert!(HyperlinkFormat::parse(Some("none")).is_empty());
        assert!(!HyperlinkFormat::parse(Some("vscode")).is_empty());
    }
}
