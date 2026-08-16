use std::time::Instant;

pub mod format;
pub mod hyperlink;
pub mod mode;
pub mod passthru;
pub mod style;

use mode::OutputEmission;
use passthru::PassthruMode;
use sift_core::search::{Hit, Inputs, SearchMode, SearchReport, Searcher};
use style::{PrintLineStyle, PrintRecordStyle};

use crate::format::event::EventRenderer;
use crate::format::output::style::PrintSeparators;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintSpec {
    pub format: PrintFormat,
    pub mode: SearchMode,
    pub emission: OutputEmission,
    pub lines: PrintLineStyle,
    pub records: PrintRecordStyle,
    pub passthru: PassthruMode,
}

impl Default for PrintSpec {
    fn default() -> Self {
        Self {
            format: PrintFormat::Text,
            mode: SearchMode::Print(Hit::Line),
            emission: OutputEmission::Normal,
            lines: PrintLineStyle::default(),
            records: PrintRecordStyle::default(),
            passthru: PassthruMode::Disabled,
        }
    }
}

impl PrintSpec {
    /// Execute search and write formatted output to stdout.
    ///
    /// Quiet/summary materialize a report; normal listing pulls `Events`.
    ///
    /// # Errors
    ///
    /// Returns an error if search or output formatting fails.
    pub fn print(
        self,
        searcher: &Searcher,
        inputs: Inputs<'_>,
        mode: SearchMode,
        separators: &PrintSeparators,
    ) -> sift_core::Result<SearchReport> {
        match self.emission {
            OutputEmission::Quiet => searcher.execute(inputs, mode),
            OutputEmission::Summary => {
                let started = Instant::now();
                let context_requested =
                    searcher.options().before_context > 0 || searcher.options().after_context > 0;
                let binary_mode = searcher.options().binary_mode;
                let mut renderer =
                    EventRenderer::new(self, separators, started, binary_mode, context_requested);
                let mut report = searcher.execute(inputs, mode)?;
                renderer.finish(&mut report)?;
                Ok(report)
            }
            OutputEmission::Normal => {
                let started = Instant::now();
                let context_requested =
                    searcher.options().before_context > 0 || searcher.options().after_context > 0;
                let binary_mode = searcher.options().binary_mode;
                let mut renderer =
                    EventRenderer::new(self, separators, started, binary_mode, context_requested);
                let mut events = searcher.stream(inputs, mode);
                for event in events.by_ref() {
                    renderer.event(event?)?;
                }
                let mut report = events.into_report();
                renderer.finish(&mut report)?;
                Ok(report)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_output_defaults() {
        let output = PrintSpec::default();
        assert_eq!(output.format, PrintFormat::Text);
        assert_eq!(output.mode, SearchMode::Print(Hit::Line));
        assert_eq!(output.emission, OutputEmission::Normal);
        assert!(matches!(output.passthru, PassthruMode::Disabled));
    }
}
