use std::time::Instant;

pub mod format;
pub mod hyperlink;
pub mod mode;
pub mod passthru;
pub mod style;

use mode::OutputEmission;
use passthru::PassthruMode;
use sift_core::search::{Events, Report, SearchInputs, SearchMode, Searcher, StatsMode};
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
            mode: SearchMode::Lines,
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
    /// Quiet/summary discard streamed events; normal text/JSON match listing
    /// streams begin/match/end through the printer sink.
    ///
    /// # Errors
    ///
    /// Returns an error if search or output formatting fails.
    pub fn print(
        self,
        searcher: &Searcher,
        inputs: SearchInputs<'_>,
        mode: SearchMode,
        stats: StatsMode,
        separators: &PrintSeparators,
    ) -> sift_core::Result<Report> {
        match self.emission {
            OutputEmission::Quiet => searcher.execute(inputs, stats, mode, Events::Discard),
            OutputEmission::Summary | OutputEmission::Normal => {
                let started = Instant::now();
                let context_requested =
                    searcher.options().before_context > 0 || searcher.options().after_context > 0;
                let binary_mode = searcher.options().binary_mode;
                let emission = self.emission;
                let mut renderer =
                    EventRenderer::new(self, separators, started, binary_mode, context_requested);
                let events = match emission {
                    OutputEmission::Normal => Events::Emit(&mut renderer),
                    OutputEmission::Summary => Events::Discard,
                    OutputEmission::Quiet => unreachable!("quiet handled above"),
                };
                let mut report = searcher.execute(inputs, stats, mode, events)?;
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
        assert_eq!(output.mode, SearchMode::Lines);
        assert_eq!(output.emission, OutputEmission::Normal);
        assert!(matches!(output.passthru, PassthruMode::Disabled));
    }
}
