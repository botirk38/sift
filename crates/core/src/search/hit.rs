use std::path::PathBuf;

use crate::search::input::Origin;

/// One match hit (line text or span text); path lives on the enclosing origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Count {
    pub origin: Origin,
    pub hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedFile {
    pub origin: Origin,
    pub matches: Vec<Match>,
}

/// Mode-shaped search results — one arm per listing shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    MatchingPaths(Vec<Origin>),
    NonMatchingPaths(Vec<Origin>),
    Counts(Vec<Count>),
    Matches(Vec<MatchedFile>),
}

impl Listing {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        match self {
            Self::MatchingPaths(v) | Self::NonMatchingPaths(v) => v.is_empty(),
            Self::Counts(v) => v.is_empty(),
            Self::Matches(v) => v.is_empty(),
        }
    }

    #[must_use]
    pub(crate) const fn empty(mode: crate::search::SearchMode) -> Self {
        use crate::search::SearchMode;
        match mode {
            SearchMode::FilesWithMatches | SearchMode::Paths => Self::MatchingPaths(Vec::new()),
            SearchMode::FilesWithoutMatch => Self::NonMatchingPaths(Vec::new()),
            SearchMode::Count { .. } => Self::Counts(Vec::new()),
            SearchMode::Print(_) => Self::Matches(Vec::new()),
        }
    }

    /// Corpus-relative paths for files that matched (lazy index enqueue).
    #[must_use]
    pub fn corpus_hit_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        match self {
            Self::MatchingPaths(origins) => {
                for origin in origins {
                    if let Some(path) = origin.corpus_path() {
                        paths.push(path.to_path_buf());
                    }
                }
            }
            Self::NonMatchingPaths(_) => {}
            Self::Counts(counts) => {
                for count in counts {
                    if count.hits > 0
                        && let Some(path) = count.origin.corpus_path()
                    {
                        paths.push(path.to_path_buf());
                    }
                }
            }
            Self::Matches(files) => {
                for file in files {
                    if let Some(path) = file.origin.corpus_path() {
                        paths.push(path.to_path_buf());
                    }
                }
            }
        }
        paths
    }
}
