use std::path::{Path, PathBuf};

use crate::corpus::PathDisplay;
use crate::search::input::Origin;

/// One match hit (line text or span text); path lives on the enclosing listed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub text: String,
}

/// Listed search result row identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFile {
    pub origin: Origin,
    pub binary_byte_offset: Option<u64>,
}

impl ListedFile {
    /// Corpus-relative path for daemon / lazy index enqueue, if any.
    #[must_use]
    pub fn corpus_path(&self) -> Option<&Path> {
        self.origin.corpus_path()
    }

    /// Text shown to the user for this origin.
    #[must_use]
    pub fn display(&self, mode: PathDisplay) -> std::borrow::Cow<'_, str> {
        self.origin.display(mode)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineCount {
    pub file: ListedFile,
    pub lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanCount {
    pub file: ListedFile,
    pub spans: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedFile {
    pub file: ListedFile,
    /// Empty under Collect (match text was emitted as events).
    pub matches: Vec<Match>,
}

/// Private row produced for one file.
#[derive(Debug)]
pub enum ListedRow {
    MatchingPath(ListedFile),
    NonMatchingPath(ListedFile),
    LineCount(LineCount),
    SpanCount(SpanCount),
    Lines(MatchedFile),
    Spans(MatchedFile),
}

/// Mode-shaped search results — one arm per [`crate::search::SearchMode`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    MatchingPaths(Vec<ListedFile>),
    NonMatchingPaths(Vec<ListedFile>),
    LineCounts(Vec<LineCount>),
    SpanCounts(Vec<SpanCount>),
    Lines(Vec<MatchedFile>),
    Spans(Vec<MatchedFile>),
}

impl Listing {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        match self {
            Self::MatchingPaths(v) | Self::NonMatchingPaths(v) => v.is_empty(),
            Self::LineCounts(v) => v.is_empty(),
            Self::SpanCounts(v) => v.is_empty(),
            Self::Lines(v) | Self::Spans(v) => v.is_empty(),
        }
    }

    #[must_use]
    pub(crate) const fn empty(mode: crate::search::SearchMode) -> Self {
        use crate::search::SearchMode;
        match mode {
            SearchMode::FilesWithMatches | SearchMode::Paths => Self::MatchingPaths(Vec::new()),
            SearchMode::FilesWithoutMatch => Self::NonMatchingPaths(Vec::new()),
            SearchMode::CountLines { .. } => Self::LineCounts(Vec::new()),
            SearchMode::CountMatches { .. } => Self::SpanCounts(Vec::new()),
            SearchMode::Lines => Self::Lines(Vec::new()),
            SearchMode::Matches => Self::Spans(Vec::new()),
        }
    }

    pub(crate) fn push_row(&mut self, row: ListedRow) {
        match (self, row) {
            (Self::MatchingPaths(v), ListedRow::MatchingPath(f))
            | (Self::NonMatchingPaths(v), ListedRow::NonMatchingPath(f)) => v.push(f),
            (Self::LineCounts(v), ListedRow::LineCount(c)) => v.push(c),
            (Self::SpanCounts(v), ListedRow::SpanCount(c)) => v.push(c),
            (Self::Lines(v), ListedRow::Lines(f)) | (Self::Spans(v), ListedRow::Spans(f)) => {
                v.push(f);
            }
            _ => unreachable!("ListedRow arm must match Listing / SearchMode"),
        }
    }

    /// Corpus-relative paths for files that matched (lazy index enqueue).
    #[must_use]
    pub fn corpus_hit_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        match self {
            Self::MatchingPaths(files) => {
                for file in files {
                    if let Some(path) = file.corpus_path() {
                        paths.push(path.to_path_buf());
                    }
                }
            }
            Self::NonMatchingPaths(_) => {}
            Self::LineCounts(counts) => {
                for count in counts {
                    if count.lines > 0
                        && let Some(path) = count.file.corpus_path()
                    {
                        paths.push(path.to_path_buf());
                    }
                }
            }
            Self::SpanCounts(counts) => {
                for count in counts {
                    if count.spans > 0
                        && let Some(path) = count.file.corpus_path()
                    {
                        paths.push(path.to_path_buf());
                    }
                }
            }
            Self::Lines(files) | Self::Spans(files) => {
                for file in files {
                    if let Some(path) = file.file.corpus_path() {
                        paths.push(path.to_path_buf());
                    }
                }
            }
        }
        paths
    }
}
