mod event;
pub mod output;
pub mod printer;
pub mod sink;

pub use output::format::{ColumnLimit, ColumnOverflow};
pub use output::mode::{MatchEmissionMode, OutputEmission, PrintMode, ZeroCountMode};
pub use output::passthru::PassthruMode;
pub use output::style::{
    ColorChoice, FilenameMode, LineStyleFlags, PathDisplay, PrintLineStyle, PrintRecordStyle,
    PrintSeparators, RecordTerminator,
};
pub use output::{PrintFormat, PrintSpec};
pub use printer::SearchPrinter;
