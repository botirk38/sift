/// How search results reach stdout.
///
/// - [`Normal`](Self::Normal) — pull begin/match/end events through the printer.
/// - [`Summary`](Self::Summary) — print counts/paths from the report.
/// - [`Quiet`](Self::Quiet) — write nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputEmission {
    #[default]
    Normal,
    Summary,
    Quiet,
}
