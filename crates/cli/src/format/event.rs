use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::sync::Arc;
use std::time::Instant;

use sift_core::search::{
    BinaryEvent, BinaryMode, ContextEvent, ContextKind, FileEvent, Listing, MatchEvent, Origin,
    Report, SearchEvent, SearchSink,
};

use crate::format::output::format::ColumnOverflow;
use crate::format::output::mode::OutputEmission;
use crate::format::output::style::{
    ColorOutput, FilenameMode, LineStyleFlags, PrintSeparators, RecordTerminator,
};
use crate::format::output::{PrintFormat, PrintSpec};
use sift_core::SearchMode;

#[derive(Default)]
struct FileJsonStats {
    matches: u64,
    matched_lines: u64,
    bytes_searched: u64,
}

pub(super) struct EventRenderer<'a> {
    output: PrintSpec,
    separators: &'a PrintSeparators,
    started: Instant,
    binary_mode: BinaryMode,
    context_requested: bool,
    bytes: Vec<u8>,
    json_stats: HashMap<Arc<str>, FileJsonStats>,
    headings: HashSet<Arc<str>>,
    binary_paths: HashSet<Arc<str>>,
}

impl<'a> EventRenderer<'a> {
    pub(super) fn new(
        output: PrintSpec,
        separators: &'a PrintSeparators,
        started: Instant,
        binary_mode: BinaryMode,
        context_requested: bool,
    ) -> Self {
        Self {
            output,
            separators,
            started,
            binary_mode,
            context_requested,
            bytes: Vec::new(),
            json_stats: HashMap::new(),
            headings: HashSet::new(),
            binary_paths: HashSet::new(),
        }
    }

    pub(super) fn finish(&mut self, report: &mut Report) -> sift_core::Result<()> {
        if matches!(self.output.emission, OutputEmission::Quiet) {
            return Ok(());
        }
        if matches!(self.output.format, PrintFormat::Json) {
            self.write_json_summary(report)?;
        } else {
            self.render_summary_modes(report);
        }
        let bytes_printed = self.bytes.len() as u64;
        std::io::stdout().lock().write_all(&self.bytes)?;
        if let Some(stats) = report.stats.as_mut() {
            stats.bytes_printed = bytes_printed;
        }
        Ok(())
    }

    fn render_summary_modes(&mut self, report: &Report) {
        let path_display = self.output.lines.path_display;
        match (&self.output.mode, &report.listed) {
            (SearchMode::CountLines { .. }, Listing::LineCounts(counts)) => {
                for count in counts {
                    self.write_path_record(
                        count.file.display(path_display).as_ref(),
                        &count.lines.to_string(),
                    );
                }
            }
            (SearchMode::CountMatches { .. }, Listing::SpanCounts(counts)) => {
                for count in counts {
                    self.write_path_record(
                        count.file.display(path_display).as_ref(),
                        &count.spans.to_string(),
                    );
                }
            }
            (SearchMode::FilesWithMatches | SearchMode::Paths, Listing::MatchingPaths(files))
            | (SearchMode::FilesWithoutMatch, Listing::NonMatchingPaths(files)) => {
                for file in files {
                    self.write_path_only(file.display(path_display).as_ref());
                }
            }
            _ => {}
        }
    }

    fn write_text_match(&mut self, event: &MatchEvent) {
        if matches!(
            self.output.mode,
            SearchMode::CountLines { .. }
                | SearchMode::CountMatches { .. }
                | SearchMode::FilesWithMatches
                | SearchMode::FilesWithoutMatch
                | SearchMode::Paths
        ) {
            return;
        }
        if self.binary_paths.contains(event.origin.key().as_ref())
            && !matches!(self.binary_mode, BinaryMode::AsText)
        {
            return;
        }
        let path_display = self.output.lines.path_display;
        if self.output.lines.heading()
            && self.headings.insert(Arc::from(event.origin.key().as_ref()))
        {
            self.write_display_path(event.origin.display(path_display).as_ref());
            self.terminator();
        }
        if matches!(self.output.mode, SearchMode::Matches) {
            for (index, range) in event.ranges.iter().enumerate() {
                self.write_prefix(event, range.start);
                if let Some(replacement) = event.replacement_matches.get(index) {
                    self.bytes.extend(replacement);
                } else {
                    self.bytes.extend(&event.bytes[range.clone()]);
                }
                self.terminator();
            }
            return;
        }
        let line = event.replacement.as_deref().unwrap_or(&event.bytes);
        let line = if self.output.lines.trim() {
            Self::trim_ascii(line)
        } else {
            line
        };
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        self.write_prefix(event, event.ranges.first().map_or(0, |range| range.start));
        if self.can_color_matches(event, line) {
            self.write_colored_match_line(event, line);
        } else {
            self.write_line(line);
        }
        self.terminator();
    }

    fn write_text_context(&mut self, event: &ContextEvent) {
        if !matches!(self.output.mode, SearchMode::Lines | SearchMode::Matches) {
            return;
        }
        self.write_display_prefix(
            event
                .origin
                .display(self.output.lines.path_display)
                .as_ref(),
            &self.separators.field_context_separator,
        );
        if self.line_numbers()
            && let Some(line) = event.line_number
        {
            self.bytes.extend(line.to_string().as_bytes());
            self.bytes.extend(&self.separators.field_context_separator);
        }
        let bytes = if self.output.lines.trim() {
            Self::trim_ascii(&event.bytes)
        } else {
            &event.bytes
        };
        self.bytes
            .extend(bytes.strip_suffix(b"\n").unwrap_or(bytes));
        self.terminator();
    }

    fn write_prefix(&mut self, event: &MatchEvent, column_offset: usize) {
        self.write_display_prefix(
            event
                .origin
                .display(self.output.lines.path_display)
                .as_ref(),
            &self.separators.field_match_separator,
        );
        if self.line_numbers()
            && let Some(line) = event.line_number
        {
            self.bytes.extend(line.to_string().as_bytes());
            self.bytes.extend(&self.separators.field_match_separator);
        }
        if self.output.lines.byte_offset()
            && let Some(offset) = event.absolute_byte_offset
        {
            self.bytes.extend(offset.to_string().as_bytes());
            self.bytes.extend(&self.separators.field_match_separator);
        }
        if self.output.lines.flags.contains(LineStyleFlags::COLUMN) {
            self.bytes
                .extend((column_offset + 1).to_string().as_bytes());
            self.bytes.extend(&self.separators.field_match_separator);
        }
    }

    fn write_display_prefix(&mut self, path: &str, separator: &[u8]) {
        if self.output.lines.heading()
            || matches!(self.output.lines.filename_mode, FilenameMode::Never)
        {
            return;
        }
        self.write_display_path(path);
        self.bytes.extend(separator);
    }

    fn write_display_path(&mut self, path: &str) {
        let display =
            Self::with_path_separator(path.to_owned(), self.output.records.path_separator);
        self.write_hyperlink_start(display.as_str());
        if let Some(color) = self.ansi_for(self.output.records.colors.as_grep().path()) {
            self.bytes.extend(color);
            self.bytes.extend(display.as_bytes());
            self.bytes.extend(b"\x1b[0m");
        } else {
            self.bytes.extend(display.as_bytes());
        }
        self.write_hyperlink_end();
    }

    fn write_hyperlink_start(&mut self, display: &str) {
        if matches!(self.output.records.color_output(), ColorOutput::Ansi)
            && !self.output.records.hyperlink.is_empty()
        {
            self.bytes.extend(b"\x1b]8;;vscode://file");
            self.bytes.extend(display.as_bytes());
            self.bytes.extend(b"\x1b\\");
        }
    }

    fn write_hyperlink_end(&mut self) {
        if matches!(self.output.records.color_output(), ColorOutput::Ansi)
            && !self.output.records.hyperlink.is_empty()
        {
            self.bytes.extend(b"\x1b]8;;\x1b\\");
        }
    }

    fn can_color_matches(&self, event: &MatchEvent, line: &[u8]) -> bool {
        matches!(self.output.records.color_output(), ColorOutput::Ansi)
            && self.output.lines.columns.is_none()
            && event.replacement.is_none()
            && line == event.bytes.strip_suffix(b"\n").unwrap_or(&event.bytes)
    }

    fn write_colored_match_line(&mut self, event: &MatchEvent, line: &[u8]) {
        let Some(color) = self.ansi_for(self.output.records.colors.as_grep().matched()) else {
            self.bytes.extend(line);
            return;
        };
        let mut cursor = 0;
        for range in &event.ranges {
            let end = range.end.min(line.len());
            let start = range.start.min(end);
            self.bytes.extend(&line[cursor..start]);
            self.bytes.extend(&color);
            self.bytes.extend(&line[start..end]);
            self.bytes.extend(b"\x1b[0m");
            cursor = end;
        }
        self.bytes.extend(&line[cursor..]);
    }

    fn ansi_for(&self, spec: &termcolor::ColorSpec) -> Option<Vec<u8>> {
        if !matches!(self.output.records.color_output(), ColorOutput::Ansi) {
            return None;
        }
        let mut codes = Vec::new();
        if spec.bold() {
            codes.push("1".to_string());
        }
        if let Some(color) = spec.fg().copied().and_then(Self::ansi_fg) {
            codes.push(color.to_string());
        }
        if codes.is_empty() {
            return None;
        }
        Some(format!("\x1b[0m\x1b[{}m", codes.join(";")).into_bytes())
    }

    fn write_line(&mut self, line: &[u8]) {
        let Some(limit) = self.output.lines.columns else {
            self.bytes.extend(line);
            return;
        };
        let Ok(max) = usize::try_from(limit.max) else {
            self.bytes.extend(line);
            return;
        };
        if line.len() <= max {
            self.bytes.extend(line);
            return;
        }
        match limit.overflow {
            ColumnOverflow::Omit => self.bytes.extend(b"[Omitted long matching line]"),
            ColumnOverflow::Preview => {
                self.bytes.extend(&line[..max]);
                self.bytes.extend(b" [... omitted end of long line]");
            }
        }
    }

    const fn line_numbers(&self) -> bool {
        self.output.lines.line_number() || self.context_requested
    }

    fn write_path_only(&mut self, path: &str) {
        self.write_display_path(path);
        self.terminator();
    }

    fn write_path_record(&mut self, path: &str, value: &str) {
        if matches!(self.output.lines.filename_mode, FilenameMode::Never) {
        } else {
            self.write_display_path(path);
            if matches!(self.output.records.terminator, RecordTerminator::Nul) {
                self.bytes.push(0);
            } else {
                self.bytes.extend(&self.separators.field_match_separator);
            }
        }
        self.bytes.extend(value.as_bytes());
        self.terminator();
    }

    fn write_binary(&mut self, event: &BinaryEvent) {
        self.binary_paths
            .insert(Arc::from(event.origin.key().as_ref()));
        if !matches!(self.output.mode, SearchMode::Lines | SearchMode::Matches) {
            return;
        }
        if matches!(self.output.emission, OutputEmission::Quiet) {
            return;
        }
        if matches!(self.binary_mode, BinaryMode::Quit) && !event.explicit {
            return;
        }
        if !matches!(self.output.lines.filename_mode, FilenameMode::Never) {
            self.write_display_path(
                event
                    .origin
                    .display(self.output.lines.path_display)
                    .as_ref(),
            );
            self.bytes.extend(b": ");
        }
        self.bytes.extend(
            format!(
                "binary file matches (found \"\\0\" byte around offset {})",
                event.absolute_byte_offset
            )
            .as_bytes(),
        );
        self.terminator();
    }

    fn write_context_break(&mut self) {
        if let Some(separator) = self.separators.context_separator.as_ref() {
            self.bytes.extend(separator);
            self.terminator();
        }
    }

    fn write_json_summary(&mut self, report: &Report) -> sift_core::Result<()> {
        let stats = report.stats.as_ref();
        let elapsed = self.started.elapsed();
        let value = serde_json::json!({
            "type": "summary",
            "data": {
                "elapsed_total": {
                    "secs": elapsed.as_secs(),
                    "nanos": elapsed.subsec_nanos(),
                    "human": format!("{:.6}s", elapsed.as_secs_f64()),
                },
                "stats": {
                    "matches": stats.map_or(0, |stats| match stats.matches {
                        sift_core::MatchTotals::None => 0,
                        sift_core::MatchTotals::Lines(n) | sift_core::MatchTotals::Spans(n) => n,
                    }),
                    "bytes_searched": stats.map_or(0, |stats| stats.bytes_searched),
                }
            }
        });
        self.bytes.extend(
            serde_json::to_string(&value)
                .map_err(sift_core::SearchError::from)?
                .as_bytes(),
        );
        self.bytes.push(b'\n');
        Ok(())
    }

    fn terminator(&mut self) {
        match self.output.records.terminator {
            RecordTerminator::Newline => self.bytes.push(b'\n'),
            RecordTerminator::Nul => self.bytes.push(0),
        }
    }
}

impl SearchSink for EventRenderer<'_> {
    fn event(&mut self, event: SearchEvent) -> sift_core::Result<()> {
        match event {
            SearchEvent::Begin(_) => {}
            SearchEvent::Match(event) => self.matched(&event)?,
            SearchEvent::Context(event) => self.context(&event)?,
            SearchEvent::ContextBreak => self.write_context_break(),
            SearchEvent::Binary(event) => self.write_binary(&event),
            SearchEvent::End(event) => self.end(&event)?,
        }
        Ok(())
    }
}

impl EventRenderer<'_> {
    fn matched(&mut self, event: &MatchEvent) -> sift_core::Result<()> {
        if matches!(self.output.format, PrintFormat::Json) {
            self.json_begin(&event.origin)?;
            let stats = self
                .json_stats
                .entry(Arc::from(event.origin.key().as_ref()))
                .or_default();
            stats.matches += event.ranges.len() as u64;
            stats.matched_lines += 1;
            let submatches: Vec<_> = event
                .ranges
                .iter()
                .map(|range| {
                    serde_json::json!({
                        "match": { "text": String::from_utf8_lossy(&event.bytes[range.clone()]).to_string() },
                        "start": range.start,
                        "end": range.end,
                    })
                })
                .collect();
            let path = event.origin.display(self.output.lines.path_display);
            let value = serde_json::json!({
                "type": "match",
                "data": {
                    "path": { "text": Self::display_text(path) },
                    "lines": { "text": String::from_utf8_lossy(&event.bytes).to_string() },
                    "line_number": event.line_number,
                    "absolute_offset": event.absolute_byte_offset,
                    "submatches": submatches,
                }
            });
            self.write_json(&value)?;
        } else {
            self.write_text_match(event);
        }
        Ok(())
    }

    fn context(&mut self, event: &ContextEvent) -> sift_core::Result<()> {
        if matches!(self.output.format, PrintFormat::Json) {
            self.json_begin(&event.origin)?;
            let kind = match event.kind {
                ContextKind::Before => "before",
                ContextKind::After => "after",
                ContextKind::Other => "context",
            };
            let path = event.origin.display(self.output.lines.path_display);
            let value = serde_json::json!({
                "type": "context",
                "data": {
                    "path": { "text": Self::display_text(path) },
                    "lines": { "text": String::from_utf8_lossy(&event.bytes).to_string() },
                    "line_number": event.line_number,
                    "absolute_offset": event.absolute_byte_offset,
                    "submatches": [],
                    "context_kind": kind,
                }
            });
            self.write_json(&value)?;
        } else {
            self.write_text_context(event);
        }
        Ok(())
    }

    fn end(&mut self, event: &FileEvent) -> sift_core::Result<()> {
        if matches!(self.output.format, PrintFormat::Json) {
            let Some(stats) = self.json_stats.remove(event.origin.key().as_ref()) else {
                return Ok(());
            };
            let path = event.origin.display(self.output.lines.path_display);
            let value = serde_json::json!({
                "type": "end",
                "data": {
                    "path": { "text": Self::display_text(path) },
                    "stats": {
                        "matches": stats.matches,
                        "matched_lines": stats.matched_lines,
                        "bytes_searched": stats.bytes_searched,
                    }
                }
            });
            self.write_json(&value)?;
        }
        Ok(())
    }

    fn json_begin(&mut self, origin: &Origin) -> sift_core::Result<()> {
        let key: Arc<str> = Arc::from(origin.key().as_ref());
        if self.json_stats.contains_key(key.as_ref()) {
            return Ok(());
        }
        let path = origin.display(self.output.lines.path_display);
        let value = serde_json::json!({
            "type": "begin",
            "data": { "path": { "text": Self::display_text(path) } }
        });
        self.write_json(&value)?;
        self.json_stats.insert(key, FileJsonStats::default());
        Ok(())
    }

    fn write_json(&mut self, value: &serde_json::Value) -> sift_core::Result<()> {
        self.bytes.extend(
            serde_json::to_string(&value)
                .map_err(sift_core::SearchError::from)?
                .as_bytes(),
        );
        self.bytes.push(b'\n');
        Ok(())
    }

    fn display_text(path: impl AsRef<str>) -> String {
        path.as_ref().to_owned()
    }

    fn trim_ascii(bytes: &[u8]) -> &[u8] {
        let start = bytes
            .iter()
            .position(|byte| !byte.is_ascii_whitespace() || *byte == b'\n')
            .unwrap_or(bytes.len());
        &bytes[start..]
    }

    fn with_path_separator(path: String, separator: Option<u8>) -> String {
        let Some(separator) = separator else {
            return path;
        };
        let mut buf = [0u8; 4];
        let separator = (separator as char).encode_utf8(&mut buf);
        path.replace(std::path::MAIN_SEPARATOR, separator)
    }

    const fn ansi_fg(color: termcolor::Color) -> Option<u8> {
        match color {
            termcolor::Color::Black => Some(30),
            termcolor::Color::Blue => Some(34),
            termcolor::Color::Green => Some(32),
            termcolor::Color::Red => Some(31),
            termcolor::Color::Cyan => Some(36),
            termcolor::Color::Magenta => Some(35),
            termcolor::Color::Yellow => Some(33),
            termcolor::Color::White => Some(37),
            _ => None,
        }
    }
}
