use std::time::Duration;

/// Search execution statistics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Stats {
    /// Listed hit total. `None` when contents were not searched.
    /// Files-with/without-match modes store a presence tally (`files_with_matches`).
    pub matches: Option<usize>,
    pub files_with_matches: usize,
    pub files_searched: usize,
    pub bytes_printed: u64,
    pub bytes_searched: u64,
    pub elapsed: Duration,
}
