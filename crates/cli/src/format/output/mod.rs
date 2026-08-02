pub mod format;
pub mod mode;
pub mod passthru;
pub mod style;

use mode::OutputEmission;
use passthru::PassthruMode;
use sift_core::SearchMode;
use style::{PrintLineStyle, PrintRecordStyle};

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
