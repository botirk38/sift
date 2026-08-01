#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Lines,
    Matches,
    CountLines {
        zeros: ZeroCounts,
    },
    CountMatches {
        zeros: ZeroCounts,
    },
    FilesWithMatches,
    FilesWithoutMatch,
}

impl SearchMode {
    /// Candidate coverage required for this search mode.
    #[must_use]
    pub const fn coverage(self) -> crate::candidates::Coverage {
        use crate::candidates::Coverage;
        match self {
            Self::FilesWithoutMatch => Coverage::Complete,
            Self::CountLines { zeros } | Self::CountMatches { zeros } => match zeros {
                ZeroCounts::Include => Coverage::Complete,
                ZeroCounts::Omit => Coverage::PotentialMatches,
            },
            Self::Lines | Self::Matches | Self::FilesWithMatches => Coverage::PotentialMatches,
        }
    }

    /// Whether this file receives a row in the mode-shaped [`crate::search::Listing`].
    pub(crate) const fn admits(self, matched: bool) -> bool {
        match self {
            Self::FilesWithoutMatch => !matched,
            Self::CountLines {
                zeros: ZeroCounts::Include,
            }
            | Self::CountMatches {
                zeros: ZeroCounts::Include,
            } => true,
            Self::Lines
            | Self::Matches
            | Self::CountLines {
                zeros: ZeroCounts::Omit,
            }
            | Self::CountMatches {
                zeros: ZeroCounts::Omit,
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
            Self::Lines
            | Self::Matches
            | Self::CountLines { .. }
            | Self::CountMatches { .. }
            | Self::FilesWithMatches => matched,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZeroCounts {
    #[default]
    Omit,
    Include,
}
