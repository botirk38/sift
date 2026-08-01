use std::time::Instant;

use sift_core::search::{Events, Report, SearchInputs, SearchMode, Searcher, StatsMode};

use crate::format::collection::PrintExtras;
use crate::format::event::EventRenderer;
use crate::format::output::PrintSpec;
use crate::format::output::mode::OutputEmission;
use crate::format::output::style::PrintSeparators;

pub struct SearchPrinter;

impl SearchPrinter {
    /// Execute search and write formatted output to stdout.
    ///
    /// Quiet/summary discard streamed events; normal text/JSON match listing
    /// streams begin/match/end through the printer sink.
    ///
    /// # Errors
    ///
    /// Returns an error if search or output formatting fails.
    pub fn print(
        searcher: &Searcher,
        inputs: SearchInputs<'_>,
        mode: SearchMode,
        stats: StatsMode,
        print_spec: PrintSpec,
        separators: &PrintSeparators,
        extras: PrintExtras,
    ) -> sift_core::Result<Report> {
        match print_spec.emission {
            OutputEmission::Quiet => searcher.execute(inputs, stats, mode, Events::Discard),
            OutputEmission::Summary | OutputEmission::Normal => {
                let started = Instant::now();
                let context_requested =
                    searcher.options().before_context > 0 || searcher.options().after_context > 0;
                let binary_mode = searcher.options().binary_mode;
                let stream_events = matches!(print_spec.emission, OutputEmission::Normal);
                let mut renderer = EventRenderer::new(
                    print_spec,
                    separators,
                    extras,
                    started,
                    binary_mode,
                    context_requested,
                );
                let mut report = if stream_events {
                    searcher.execute(inputs, stats, mode, Events::Emit(&mut renderer))?
                } else {
                    searcher.execute(inputs, stats, mode, Events::Discard)?
                };
                renderer.finish(&mut report)?;
                Ok(report)
            }
        }
    }
}
