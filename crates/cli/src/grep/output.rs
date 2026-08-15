use crate::format::output::style::{ColorSpecs, HyperlinkFormat, OutputBuffering};
use crate::format::{
    ColorChoice, ColumnLimit, ColumnOverflow, FilenameMode, LineStyleFlags, OutputEmission,
    PassthruMode, PrintFormat, PrintLineStyle, PrintRecordStyle, PrintSeparators, PrintSpec, Quiet,
    RecordTerminator,
};
use clap::{ArgAction, Args};
use sift_core::SearchMode;
use sift_core::search::Stats;
use std::path::PathBuf;
use std::process::Command;

/// Describes the filename display context for deciding whether to show paths.
#[derive(Clone, Copy)]
pub enum FilenameContext {
    PathMode,
    DirectoryCorpus,
    SingleStream,
}

use super::argv::Argv;

/// Output-related flags resolved from clap declarations.
#[derive(Clone)]
pub struct OutputDecl {
    pub column: ColumnDecl,
    pub columns: ColumnsDecl,
    pub extra: ExtraOutputDecl,
    pub replace_trim: bool,
    pub path_separator: Option<String>,
    pub colors: Vec<String>,
    pub hyperlink_format: Option<String>,
    pub hostname_bin: Option<String>,
    pub line_number: bool,
    pub separators: SeparatorDecl,
    pub search_paths: Vec<PathBuf>,
    pub null_data: bool,
}

impl OutputDecl {
    #[must_use]
    pub fn separators(&self) -> PrintSeparators {
        let context_separator = if self.separators.suppress_context_sep {
            None
        } else if let Some(ref s) = self.separators.context_sep {
            Some(unescape_separator(s))
        } else {
            Some(b"--".to_vec())
        };
        let field_match_separator = self
            .separators
            .field_match
            .as_ref()
            .map_or_else(|| b":".to_vec(), |s| unescape_separator(s));
        let field_context_separator = self
            .separators
            .field_context
            .as_ref()
            .map_or_else(|| b"-".to_vec(), |s| unescape_separator(s));
        PrintSeparators {
            context_separator,
            field_match_separator,
            field_context_separator,
        }
    }
}

// ── Clap declarations (output flags) ──

#[derive(Args, Clone)]
pub struct SeparatorDecl {
    #[arg(
        long = "context-separator",
        value_name = "SEPARATOR",
        allow_hyphen_values = true
    )]
    pub context_sep: Option<String>,
    #[arg(long = "no-context-separator")]
    pub suppress_context_sep: bool,
    #[arg(long = "field-match-separator", value_name = "SEPARATOR")]
    pub field_match: Option<String>,
    #[arg(long = "field-context-separator", value_name = "SEPARATOR")]
    pub field_context: Option<String>,
}

#[derive(Args)]
pub struct LineNumberDecl {
    #[arg(short = 'n', long = "line-number")]
    pub line_number: bool,
    #[arg(short = 'N', long = "no-line-number")]
    pub no_line_number: bool,
}

#[derive(Args)]
pub struct FilenameDecl {
    #[arg(short = 'I', long = "no-filename")]
    pub no_filename: bool,
    #[arg(short = 'H', long = "with-filename")]
    pub with_filename: bool,
}

#[derive(Args)]
pub struct HeadingDecl {
    #[arg(long = "heading")]
    pub heading: bool,
    #[arg(long = "no-heading")]
    pub no_heading: bool,
}

#[derive(Args, Clone)]
pub struct ColumnDecl {
    #[arg(long = "column")]
    pub column: bool,
    #[arg(long = "vimgrep")]
    pub vimgrep: bool,
    #[arg(short = 'p', long = "pretty")]
    pub pretty: bool,
}

#[derive(Args, Clone)]
pub struct ColumnsDecl {
    #[arg(short = 'M', long = "max-columns", value_name = "NUM")]
    pub max_columns: Option<u64>,
    #[arg(long = "max-columns-preview")]
    pub max_columns_preview: bool,
}

#[derive(Args, Clone)]
pub struct ReplaceDecl {
    #[arg(short = 'r', long = "replace", value_name = "REPLACEMENT")]
    pub replace: Option<String>,
    #[arg(long = "trim")]
    pub trim: bool,
    #[arg(long = "passthru", visible_alias = "passthrough")]
    pub passthru: bool,
}

#[derive(Args, Clone)]
pub struct ExtraOutputDecl {
    #[arg(long = "include-zero")]
    pub include_zero: bool,
    #[arg(short = 'b', long = "byte-offset")]
    pub byte_offset: bool,
}

/// `-0` / `--null` and `--color` for clap; effective null/color use argv resolvers.
#[derive(Args)]
pub struct NullColorDecl {
    #[arg(short = '0', long = "null", action = ArgAction::SetTrue)]
    pub null_flag: bool,
    #[arg(long = "color", value_name = "WHEN")]
    pub color_flag: Option<String>,
}

/// Declares `--json` / `--no-json` for clap.
#[derive(Args)]
pub struct JsonDecl {
    #[arg(long = "json", action = ArgAction::SetTrue)]
    pub json_flag: bool,
    #[arg(long = "no-json", action = ArgAction::SetTrue)]
    pub no_json_flag: bool,
}

/// Declares `--stats` for clap.
#[derive(Args)]
pub struct StatsDecl {
    #[arg(long = "stats", action = ArgAction::SetTrue)]
    pub stats_flag: bool,
}

// ── Argv-order resolution ──

/// Output-related flags resolved from raw argv (ripgrep last-wins).
#[derive(Clone, Copy, Default)]
pub struct OutputModeFlags {
    pub stats: bool,
    pub json: bool,
    pub heading: bool,
}

#[derive(Clone, Copy, Default)]
pub struct OutputPathFlags {
    pub glob_case_insensitive: bool,
    pub nul_terminated: bool,
}

#[derive(Clone)]
pub struct OutputArgv {
    pub mode: OutputModeFlags,
    pub path: OutputPathFlags,
    pub color: ColorChoice,
    pub buffering: OutputBuffering,
    pub line_number: Option<bool>,
    pub with_filename: Option<bool>,
}

impl OutputArgv {
    #[must_use]
    pub fn resolve(argv: &Argv<'_>) -> Self {
        let tokens = argv.as_slice();
        Self {
            mode: OutputModeFlags {
                stats: Self::stats(tokens),
                json: Self::json(tokens),
                heading: Self::heading(tokens),
            },
            path: OutputPathFlags {
                glob_case_insensitive: Self::glob_case_insensitive(tokens),
                nul_terminated: Self::nul_terminated_paths(tokens),
            },
            color: Self::color(tokens),
            buffering: Self::buffering(tokens),
            line_number: Self::line_number(tokens),
            with_filename: Self::with_filename(tokens),
        }
    }

    fn glob_case_insensitive(args: &[String]) -> bool {
        let mut last_idx = 0usize;
        let mut result = false;
        for (i, arg) in args.iter().enumerate() {
            let bytes = arg.as_bytes();
            let is_long = bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-';
            if is_long {
                let suffix = &bytes[2..];
                let flag = if suffix == b"glob-case-insensitive" {
                    Some((i, true))
                } else if suffix == b"no-glob-case-insensitive" {
                    Some((i, false))
                } else {
                    None
                };
                if let Some((idx, val)) = flag
                    && idx > last_idx
                {
                    last_idx = idx;
                    result = val;
                }
            }
        }
        result
    }

    fn nul_terminated_paths(args: &[String]) -> bool {
        let mut result = false;
        for arg in args {
            match arg.as_str() {
                "-0" | "--null" => result = true,
                _ => {}
            }
        }
        result
    }

    fn color(args: &[String]) -> ColorChoice {
        let mut result = ColorChoice::Auto;
        let mut i = 0usize;
        while i < args.len() {
            if let Some(rest) = args[i].strip_prefix("--color=") {
                result = parse_color_when(rest);
                i += 1;
                continue;
            }
            if args[i] == "--color"
                && let Some(v) = args.get(i + 1)
            {
                result = parse_color_when(v);
                i += 2;
                continue;
            }
            i += 1;
        }
        result
    }

    fn buffering(args: &[String]) -> OutputBuffering {
        let mut last_idx = 0usize;
        let mut result = OutputBuffering::Auto;
        for (i, arg) in args.iter().enumerate() {
            let value = match arg.as_str() {
                "--line-buffered" => Some(OutputBuffering::Line),
                "--block-buffered" => Some(OutputBuffering::Block),
                _ => None,
            };
            if let Some(value) = value
                && i >= last_idx
            {
                last_idx = i;
                result = value;
            }
        }
        result
    }

    fn stats(args: &[String]) -> bool {
        let mut last_idx = 0usize;
        let mut result = false;
        for (i, arg) in args.iter().enumerate() {
            if arg == "--stats" && i >= last_idx {
                last_idx = i;
                result = true;
            }
        }
        result
    }

    fn json(args: &[String]) -> bool {
        let mut last_idx = 0usize;
        let mut result = false;
        for (i, arg) in args.iter().enumerate() {
            if arg == "--json" && i >= last_idx {
                last_idx = i;
                result = true;
            }
            if arg == "--no-json" && i >= last_idx {
                last_idx = i;
                result = false;
            }
        }
        result
    }

    fn heading(args: &[String]) -> bool {
        let mut last_idx = 0usize;
        let mut result = false;
        for (i, arg) in args.iter().enumerate() {
            let bytes = arg.as_bytes();
            if bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-' {
                let value = match &bytes[2..] {
                    b"heading" => Some(true),
                    b"no-heading" => Some(false),
                    _ => None,
                };
                if let Some(value) = value
                    && i > last_idx
                {
                    last_idx = i;
                    result = value;
                }
            }
        }
        result
    }

    fn line_number(args: &[String]) -> Option<bool> {
        let mut last_idx = 0usize;
        let mut result = None;
        for (i, arg) in args.iter().enumerate() {
            let bytes = arg.as_bytes();
            let value = if bytes.len() == 2 && bytes[0] == b'-' {
                match bytes.get(1) {
                    Some(&b'n') => Some(true),
                    Some(&b'N') => Some(false),
                    _ => None,
                }
            } else if bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-' {
                match &bytes[2..] {
                    b"line-number" => Some(true),
                    b"no-line-number" => Some(false),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(value) = value
                && i > last_idx
            {
                last_idx = i;
                result = Some(value);
            }
        }
        result
    }

    fn with_filename(args: &[String]) -> Option<bool> {
        let mut last_idx = 0usize;
        let mut result = None;
        for (i, arg) in args.iter().enumerate() {
            let bytes = arg.as_bytes();
            let value = if bytes.len() == 2 && bytes[0] == b'-' {
                match bytes[1] {
                    b'H' => Some(true),
                    b'I' => Some(false),
                    _ => None,
                }
            } else if bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-' {
                match &bytes[2..] {
                    b"with-filename" => Some(true),
                    b"no-filename" => Some(false),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(value) = value
                && i > last_idx
            {
                last_idx = i;
                result = Some(value);
            }
        }
        result
    }
}

fn parse_color_when(s: &str) -> ColorChoice {
    match s {
        "never" => ColorChoice::Never,
        "always" | "ansi" => ColorChoice::Always,
        _ => ColorChoice::Auto,
    }
}

fn unescape_separator(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push(b'\n'),
                Some('t') => out.push(b'\t'),
                Some('\\') | None => out.push(b'\\'),
                Some('0') => out.push(0),
                Some(other) => {
                    out.push(b'\\');
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                }
            }
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}

fn resolve_hostname(hostname_bin: Option<&str>) -> Result<Option<String>, String> {
    let Some(command) = hostname_bin else {
        return Ok(None);
    };
    let output = Command::new(command)
        .output()
        .map_err(|e| format!("failed to run --hostname-bin '{command}': {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "--hostname-bin '{command}' exited with status {}",
            output.status
        ));
    }
    let host = String::from_utf8(output.stdout)
        .map_err(|e| format!("--hostname-bin '{command}' emitted invalid UTF-8: {e}"))?;
    Ok(Some(host.trim_end_matches(['\r', '\n']).to_string()))
}

impl OutputDecl {
    /// Build the core grep output from resolved argv and CLI configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when color specs are invalid, hyperlink formats are invalid,
    /// or the hostname command fails.
    pub fn print_spec(
        &self,
        output_argv: &OutputArgv,
        mode: SearchMode,
        quiet: Quiet,
        line_number_override: Option<bool>,
        filename_ctx: FilenameContext,
    ) -> Result<PrintSpec, String> {
        use super::paths::CorpusScope;

        let pretty = self.column.pretty;
        let vimgrep = self.column.vimgrep;
        let output_format = Self::format(output_argv, mode);
        let effective_color = if pretty && output_argv.color == ColorChoice::Auto {
            ColorChoice::Always
        } else {
            output_argv.color
        };
        let colors = ColorSpecs::from_specs(&self.colors)?;
        let hyperlink = HyperlinkFormat::parse(self.hyperlink_format.as_deref())?;
        let hyperlink_host = resolve_hostname(self.hostname_bin.as_deref())?;

        Ok(PrintSpec {
            format: output_format,
            mode,
            emission: match quiet {
                Quiet::On => OutputEmission::Quiet,
                Quiet::Off if mode.is_summary() => OutputEmission::Summary,
                Quiet::Off => OutputEmission::Normal,
            },
            lines: PrintLineStyle {
                filename_mode: Self::filename_mode(output_argv.with_filename, filename_ctx),
                flags: self.line_style_flags(
                    output_argv.mode.heading || pretty,
                    self.effective_line_number(line_number_override, output_format),
                    self.column.column || vimgrep,
                ),
                path_display: if matches!(mode, SearchMode::Paths) {
                    // Match prior `--files` listing: absolute paths under the filter root.
                    sift_core::PathDisplay::Absolute
                } else {
                    CorpusScope::path_display(&self.search_paths)
                },
                columns: self.columns.max_columns.map(|max| ColumnLimit {
                    max,
                    overflow: if self.columns.max_columns_preview {
                        ColumnOverflow::Preview
                    } else {
                        ColumnOverflow::Omit
                    },
                }),
            },
            records: PrintRecordStyle {
                terminator: if output_argv.path.nul_terminated || self.null_data {
                    RecordTerminator::Nul
                } else {
                    RecordTerminator::Newline
                },
                color: effective_color,
                path_separator: self
                    .path_separator
                    .as_ref()
                    .and_then(|s| s.as_bytes().first().copied()),
                colors,
                hyperlink,
                hyperlink_host,
                buffering: output_argv.buffering,
            },
            passthru: PassthruMode::Disabled,
        })
    }

    #[must_use]
    pub const fn format(output_argv: &OutputArgv, mode: SearchMode) -> PrintFormat {
        if output_argv.mode.json && matches!(mode, SearchMode::Lines | SearchMode::Matches) {
            PrintFormat::Json
        } else {
            PrintFormat::Text
        }
    }

    #[must_use]
    pub const fn print_stats(output_argv: &OutputArgv, mode: SearchMode) -> bool {
        output_argv.mode.stats || matches!(Self::format(output_argv, mode), PrintFormat::Json)
    }

    pub fn write_stats(stats: &Stats) {
        let matches = match stats.matches {
            sift_core::MatchTotals::None => 0,
            sift_core::MatchTotals::Lines(n) | sift_core::MatchTotals::Spans(n) => n,
        };
        eprintln!("{matches} matches");
        eprintln!("{} files contained matches", stats.files_with_matches);
        eprintln!("{} files searched", stats.files_searched);
        eprintln!("{} bytes printed", stats.bytes_printed);
        eprintln!("{} bytes searched", stats.bytes_searched);
        eprintln!("{:.6}s elapsed", stats.elapsed.as_secs_f64());
    }

    #[must_use]
    fn line_style_flags(&self, heading: bool, line_number: bool, column: bool) -> LineStyleFlags {
        let mut f = LineStyleFlags::empty();
        if heading {
            f |= LineStyleFlags::HEADING;
        }
        if line_number {
            f |= LineStyleFlags::LINE_NUMBER;
        }
        if column {
            f |= LineStyleFlags::COLUMN;
        }
        if self.extra.byte_offset {
            f |= LineStyleFlags::BYTE_OFFSET;
        }
        if self.replace_trim {
            f |= LineStyleFlags::TRIM;
        }
        f
    }

    #[must_use]
    const fn filename_mode(with_filename: Option<bool>, context: FilenameContext) -> FilenameMode {
        if matches!(context, FilenameContext::PathMode) || matches!(with_filename, Some(true)) {
            FilenameMode::Always
        } else if matches!(with_filename, Some(false))
            || matches!(context, FilenameContext::SingleStream)
        {
            FilenameMode::Never
        } else {
            FilenameMode::Always
        }
    }

    #[must_use]
    const fn effective_line_number(
        &self,
        line_number_override: Option<bool>,
        output_format: PrintFormat,
    ) -> bool {
        if matches!(output_format, PrintFormat::Json) {
            return true;
        }
        match line_number_override {
            Some(val) => val,
            None => self.line_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grep::pattern::PatternArgv;
    use clap::Parser;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    fn out_argv(items: &[&str]) -> OutputArgv {
        OutputArgv::resolve(&Argv::new(&args(items)))
    }

    #[test]
    fn output_argv_null_short() {
        assert!(out_argv(&["sift", "-0", "pat"]).path.nul_terminated);
    }

    #[test]
    fn output_argv_color_last_wins() {
        assert!(matches!(
            out_argv(&["sift", "--color=never", "--color=always", "pat"]).color,
            ColorChoice::Always
        ));
    }

    #[test]
    fn output_argv_json_toggle() {
        assert!(!out_argv(&["sift", "--json", "--no-json", "pat"]).mode.json);
    }

    #[test]
    fn output_argv_line_number_last_wins() {
        assert_eq!(
            out_argv(&["sift", "-n", "-N", "pat"]).line_number,
            Some(false)
        );
    }

    #[test]
    fn context_lines_after_short() {
        assert_eq!(
            PatternArgv::context(&Argv::new(&args(&["sift", "-A", "5", "pat"]))),
            (0, 5)
        );
    }

    #[test]
    fn filename_mode_single_stream_defaults_to_never() {
        assert!(matches!(
            OutputDecl::filename_mode(None, FilenameContext::SingleStream),
            FilenameMode::Never
        ));
    }

    #[test]
    fn effective_line_number_json() {
        let config = OutputDecl {
            line_number: false,
            ..output_config(&["sift", "pat"])
        };
        assert!(config.effective_line_number(None, PrintFormat::Json));
    }

    fn output_config(flag_args: &[&str]) -> OutputDecl {
        let flag_vec = args(flag_args);
        let argv = Argv::new(&flag_vec);
        crate::cli::Cli::try_parse_from(flag_args)
            .unwrap()
            .run_config(&argv)
            .unwrap()
            .output
    }

    #[test]
    fn resolve_separators_default_context_sep() {
        let sep = output_config(&["sift", "pat"]).separators();
        assert_eq!(sep.context_separator, Some(b"--".to_vec()));
    }
}
