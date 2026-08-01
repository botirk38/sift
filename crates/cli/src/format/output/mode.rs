/// How search results reach stdout.
///
/// - [`Normal`](Self::Normal) — stream begin/match/end events through the printer.
/// - [`Summary`](Self::Summary) — discard events; print counts/paths from the report.
/// - [`Quiet`](Self::Quiet) — discard events; write nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputEmission {
    #[default]
    Normal,
    Summary,
    Quiet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchEmissionMode {
    Lines,
    OnlyMatching,
}

/// Whether `-q` / `--quiet` was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quiet {
    #[default]
    Off,
    On,
}

/// Whether match polarity is inverted (`--invert-match`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InvertMatch {
    #[default]
    Off,
    On,
}
