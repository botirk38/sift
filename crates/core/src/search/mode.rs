/// What one listed hit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hit {
    #[default]
    Line,
    Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Print(Hit),
    Count {
        hit: Hit,
        zeros: ZeroCounts,
    },
    FilesWithMatches,
    FilesWithoutMatch,
    /// List every discovered origin without scanning file contents.
    Paths,
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Print(Hit::Line)
    }
}

impl SearchMode {
    /// File coverage required for this search mode.
    #[must_use]
    pub const fn coverage(self) -> crate::candidates::Coverage {
        use crate::candidates::Coverage;
        match self {
            Self::FilesWithoutMatch
            | Self::Paths
            | Self::Count {
                zeros: ZeroCounts::Include,
                ..
            } => Coverage::Complete,
            Self::Print(_)
            | Self::Count {
                zeros: ZeroCounts::Omit,
                ..
            }
            | Self::FilesWithMatches => Coverage::PotentialMatches,
        }
    }

    /// Whether this file receives a row in the mode-shaped [`crate::search::Listing`].
    pub(crate) const fn admits(self, matched: bool) -> bool {
        match self {
            Self::FilesWithoutMatch => !matched,
            Self::Paths
            | Self::Count {
                zeros: ZeroCounts::Include,
                ..
            } => true,
            Self::Print(_)
            | Self::Count {
                zeros: ZeroCounts::Omit,
                ..
            }
            | Self::FilesWithMatches => matched,
        }
    }

    /// Whether `FirstMatch` / quiet should stop after this file.
    ///
    /// Independent of Include-zero listing admission.
    pub(crate) const fn settles(self, matched: bool) -> bool {
        match self {
            Self::FilesWithoutMatch => !matched,
            Self::Paths => true,
            Self::Print(_) | Self::Count { .. } | Self::FilesWithMatches => matched,
        }
    }

    /// Summary modes render from the report and do not need streamed match events.
    #[must_use]
    pub const fn is_summary(self) -> bool {
        !matches!(self, Self::Print(_))
    }

    /// Path-only listing modes (`-l`, `--files-without-match`, `--files`).
    #[must_use]
    pub const fn is_path_mode(self) -> bool {
        matches!(
            self,
            Self::FilesWithMatches | Self::FilesWithoutMatch | Self::Paths
        )
    }

    /// Listing unit for content modes.
    #[must_use]
    pub const fn hit(self) -> Option<Hit> {
        match self {
            Self::Print(hit) | Self::Count { hit, .. } => Some(hit),
            Self::FilesWithMatches | Self::FilesWithoutMatch | Self::Paths => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZeroCounts {
    #[default]
    Omit,
    Include,
}
