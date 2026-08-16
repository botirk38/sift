use std::io::IsTerminal;

use crate::format::output::format::ColumnLimit;
use crate::format::output::hyperlink::Hyperlink;

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
    pub hyperlink: Hyperlink,
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

/// ANSI SGR style applied to a printer field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnsiStyle {
    fg: Option<AnsiColor>,
    bg: Option<AnsiColor>,
    bold: bool,
}

impl AnsiStyle {
    #[must_use]
    pub fn sequence(self) -> Option<Vec<u8>> {
        if self.fg.is_none() && self.bg.is_none() && !self.bold {
            return None;
        }
        let mut out = b"\x1b[0m\x1b[".to_vec();
        let mut first = true;
        if self.bold {
            Self::push_code(&mut out, &mut first, b"1");
        }
        if let Some(fg) = self.fg {
            fg.write(&mut out, &mut first, true);
        }
        if let Some(bg) = self.bg {
            bg.write(&mut out, &mut first, false);
        }
        out.push(b'm');
        Some(out)
    }

    fn push_code(out: &mut Vec<u8>, first: &mut bool, code: &[u8]) {
        if !*first {
            out.push(b';');
        }
        *first = false;
        out.extend(code);
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set_fg(&mut self, name: &str) -> Result<(), String> {
        self.fg = Some(AnsiColor::parse(name)?);
        Ok(())
    }

    fn set_bg(&mut self, name: &str) -> Result<(), String> {
        self.bg = Some(AnsiColor::parse(name)?);
        Ok(())
    }

    fn apply_style(&mut self, name: &str) -> Result<(), String> {
        match StyleName::parse(name)? {
            StyleName::Bold => self.bold = true,
            StyleName::NoBold => self.bold = false,
            StyleName::Ignored => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsiColor {
    Named(u8),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl AnsiColor {
    fn parse(name: &str) -> Result<Self, String> {
        Ok(if name.eq_ignore_ascii_case("black") {
            Self::Named(0)
        } else if name.eq_ignore_ascii_case("blue") {
            Self::Named(4)
        } else if name.eq_ignore_ascii_case("green") {
            Self::Named(2)
        } else if name.eq_ignore_ascii_case("red") {
            Self::Named(1)
        } else if name.eq_ignore_ascii_case("cyan") {
            Self::Named(6)
        } else if name.eq_ignore_ascii_case("magenta") {
            Self::Named(5)
        } else if name.eq_ignore_ascii_case("yellow") {
            Self::Named(3)
        } else if name.eq_ignore_ascii_case("white") {
            Self::Named(7)
        } else {
            Self::extended(name)?
        })
    }

    fn number(s: &str) -> Option<u8> {
        s.strip_prefix("0x")
            .or_else(|| s.strip_prefix("0X"))
            .map_or_else(|| s.parse().ok(), |hex| u8::from_str_radix(hex, 16).ok())
    }

    fn extended(name: &str) -> Result<Self, String> {
        let parts: Vec<&str> = name.split(',').collect();
        match parts.as_slice() {
            [one] => {
                let n = Self::number(one).ok_or_else(|| {
                    if one.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        format!(
                            "unrecognized ansi256 color number, \
                             should be '[0-255]' (or a hex number), but is '{name}'"
                        )
                    } else {
                        format!(
                            "unrecognized color name '{name}'. Choose from: \
                             black, blue, green, red, cyan, magenta, yellow, white."
                        )
                    }
                })?;
                Ok(Self::Indexed(n))
            }
            [r, g, b] => {
                let err = || {
                    format!(
                        "unrecognized RGB color triple, \
                         should be '[0-255],[0-255],[0-255]' (or a hex \
                         triple), but is '{name}'"
                    )
                };
                Ok(Self::Rgb(
                    Self::number(r).ok_or_else(err)?,
                    Self::number(g).ok_or_else(err)?,
                    Self::number(b).ok_or_else(err)?,
                ))
            }
            _ if name.contains(',') => Err(format!(
                "unrecognized RGB color triple, \
                 should be '[0-255],[0-255],[0-255]' (or a hex \
                 triple), but is '{name}'"
            )),
            _ => Err(format!(
                "unrecognized color name '{name}'. Choose from: \
                 black, blue, green, red, cyan, magenta, yellow, white."
            )),
        }
    }

    fn write(self, out: &mut Vec<u8>, first: &mut bool, fg: bool) {
        match self {
            Self::Named(n) => {
                let code = if fg { 30 + n } else { 40 + n };
                AnsiStyle::push_code(out, first, code.to_string().as_bytes());
            }
            Self::Indexed(n) => {
                AnsiStyle::push_code(out, first, if fg { b"38;5" } else { b"48;5" });
                out.push(b';');
                out.extend(n.to_string().as_bytes());
            }
            Self::Rgb(r, g, b) => {
                AnsiStyle::push_code(out, first, if fg { b"38;2" } else { b"48;2" });
                out.push(b';');
                out.extend(r.to_string().as_bytes());
                out.push(b';');
                out.extend(g.to_string().as_bytes());
                out.push(b';');
                out.extend(b.to_string().as_bytes());
            }
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
    line: AnsiStyle,
    column: AnsiStyle,
    matched: AnsiStyle,
    highlight: AnsiStyle,
}

impl Default for ColorSpecs {
    fn default() -> Self {
        Self {
            path: AnsiStyle {
                fg: Some(if cfg!(windows) {
                    AnsiColor::Named(6)
                } else {
                    AnsiColor::Named(5)
                }),
                bg: None,
                bold: false,
            },
            line: AnsiStyle {
                fg: Some(AnsiColor::Named(2)),
                bg: None,
                bold: false,
            },
            column: AnsiStyle::default(),
            matched: AnsiStyle {
                fg: Some(AnsiColor::Named(1)),
                bg: None,
                bold: true,
            },
            highlight: AnsiStyle::default(),
        }
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
    pub const fn line(&self) -> AnsiStyle {
        self.line
    }

    #[must_use]
    pub const fn column(&self) -> AnsiStyle {
        self.column
    }

    #[must_use]
    pub const fn matched(&self) -> AnsiStyle {
        self.matched
    }

    #[must_use]
    pub const fn highlight(&self) -> AnsiStyle {
        self.highlight
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
        let style = match OutputKind::parse(kind)? {
            OutputKind::Path => &mut self.path,
            OutputKind::Line => &mut self.line,
            OutputKind::Column => &mut self.column,
            OutputKind::Match => &mut self.matched,
            OutputKind::Highlight => &mut self.highlight,
        };
        if attr.eq_ignore_ascii_case("none") {
            style.clear();
            return Ok(());
        }
        let Some(value) = value else {
            return Err(Self::invalid_format(spec));
        };
        if attr.eq_ignore_ascii_case("fg") {
            style.set_fg(value)?;
        } else if attr.eq_ignore_ascii_case("bg") {
            style.set_bg(value)?;
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

impl StyleName {
    fn parse(name: &str) -> Result<Self, String> {
        Ok(if name.eq_ignore_ascii_case("bold") {
            Self::Bold
        } else if name.eq_ignore_ascii_case("nobold") {
            Self::NoBold
        } else if name.eq_ignore_ascii_case("intense")
            || name.eq_ignore_ascii_case("nointense")
            || name.eq_ignore_ascii_case("underline")
            || name.eq_ignore_ascii_case("nounderline")
            || name.eq_ignore_ascii_case("italic")
            || name.eq_ignore_ascii_case("noitalic")
        {
            Self::Ignored
        } else {
            return Err(format!(
                "unrecognized style attribute '{name}'. Choose from: \
                 nobold, bold, nointense, intense, nounderline, \
                 underline, noitalic, italic."
            ));
        })
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
    fn colors_emit_256_and_truecolor() {
        let indexed =
            ColorSpecs::from_specs(&["match:fg:32".into(), "match:style:nobold".into()]).unwrap();
        assert_eq!(
            indexed.matched().sequence(),
            Some(b"\x1b[0m\x1b[38;5;32m".to_vec())
        );
        let rgb =
            ColorSpecs::from_specs(&["match:fg:255,128,0".into(), "match:style:nobold".into()])
                .unwrap();
        assert_eq!(
            rgb.matched().sequence(),
            Some(b"\x1b[0m\x1b[38;2;255;128;0m".to_vec())
        );
    }

    #[test]
    fn colors_line_column_highlight_apply() {
        let colors = ColorSpecs::from_specs(&[
            "line:fg:blue".into(),
            "column:fg:yellow".into(),
            "highlight:bg:blue".into(),
        ])
        .unwrap();
        assert_eq!(colors.line().sequence(), Some(b"\x1b[0m\x1b[34m".to_vec()));
        assert_eq!(
            colors.column().sequence(),
            Some(b"\x1b[0m\x1b[33m".to_vec())
        );
        assert_eq!(
            colors.highlight().sequence(),
            Some(b"\x1b[0m\x1b[44m".to_vec())
        );
    }

    #[test]
    fn colors_reject_unknown_output_type() {
        let err = ColorSpecs::from_specs(&["bogus:fg:red".into()]).unwrap_err();
        assert!(err.contains("unrecognized output type 'bogus'"));
    }
}
