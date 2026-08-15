mod event;
pub mod output;
pub mod sink;

pub use output::format::{ColumnLimit, ColumnOverflow};
pub use output::mode::{Debug, InvertMatch, MatchEmissionMode, OutputEmission, Quiet};
pub use output::passthru::PassthruMode;
pub use output::style::{
    ColorChoice, FilenameMode, LineStyleFlags, PathDisplay, PrintLineStyle, PrintRecordStyle,
    PrintSeparators, RecordTerminator,
};
pub use output::{PrintFormat, PrintSpec};
