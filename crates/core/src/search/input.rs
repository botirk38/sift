use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::corpus::Candidate;
use crate::corpus::candidate::PathDisplay;

/// How the corpus-relative hit path relates to the display path.
///
/// Relative path display reuses [`InputIdentity::display_path`] as the hit path
/// without a second allocation. Absolute display still owns a distinct relative
/// path for hit tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitPath {
    /// No corpus path (stdin / anonymous bytes).
    Absent,
    /// Hit path equals [`InputIdentity::display_path`].
    Display,
    /// Distinct owned path (corpus-relative when display is absolute).
    Owned(PathBuf),
}

#[derive(Debug, Clone)]
pub struct InputIdentity {
    pub display_path: PathBuf,
    /// Corpus hit path relative to how [`Self::display_path`] is shown.
    pub corpus_hit: HitPath,
    pub byte_len: Option<u64>,
}

pub enum Input<'a> {
    Path {
        path: Cow<'a, Path>,
        identity: InputIdentity,
        explicit: bool,
    },
    Bytes {
        path: Cow<'a, str>,
        bytes: Cow<'a, [u8]>,
        identity: InputIdentity,
        explicit: bool,
    },
}

impl InputIdentity {
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        Self {
            display_path: PathBuf::from(name),
            corpus_hit: HitPath::Absent,
            byte_len: None,
        }
    }

    /// Corpus-relative hit path for reporting, if any.
    #[must_use]
    pub fn hit_path(&self) -> Option<&Path> {
        match &self.corpus_hit {
            HitPath::Absent => None,
            HitPath::Display => Some(self.display_path.as_path()),
            HitPath::Owned(path) => Some(path.as_path()),
        }
    }
}

impl Input<'_> {
    /// Display path for matching and optional corpus rel-path for hit tracking.
    #[must_use]
    pub fn paths(&self) -> (PathBuf, Option<PathBuf>) {
        (
            self.display_path().to_path_buf(),
            self.hit_path().map(Path::to_path_buf),
        )
    }

    #[must_use]
    pub fn display_path(&self) -> &Path {
        match self {
            Self::Path { identity, .. } | Self::Bytes { identity, .. } => &identity.display_path,
        }
    }

    #[must_use]
    pub const fn identity(&self) -> &InputIdentity {
        match self {
            Self::Path { identity, .. } | Self::Bytes { identity, .. } => identity,
        }
    }

    #[must_use]
    pub fn hit_path(&self) -> Option<&Path> {
        self.identity().hit_path()
    }

    #[must_use]
    pub fn byte_len(&self) -> u64 {
        match self {
            Self::Path { path, identity, .. } => identity
                .byte_len
                .unwrap_or_else(|| std::fs::metadata(path).map_or(0, |m| m.len())),
            Self::Bytes { bytes, .. } => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        }
    }
}

pub struct Inputs<'a> {
    items: Vec<Input<'a>>,
}

impl<'a> Inputs<'a> {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            items: Vec::with_capacity(capacity),
        }
    }

    pub fn push_path(&mut self, path: Cow<'a, Path>, identity: InputIdentity, explicit: bool) {
        self.items.push(Input::Path {
            path,
            identity,
            explicit,
        });
    }

    pub fn push_bytes(
        &mut self,
        path: Cow<'a, str>,
        bytes: Cow<'a, [u8]>,
        identity: InputIdentity,
    ) {
        self.push_bytes_input(path, bytes, identity, false);
    }

    pub fn push_explicit_bytes(
        &mut self,
        path: Cow<'a, str>,
        bytes: Cow<'a, [u8]>,
        identity: InputIdentity,
    ) {
        self.push_bytes_input(path, bytes, identity, true);
    }

    fn push_bytes_input(
        &mut self,
        path: Cow<'a, str>,
        bytes: Cow<'a, [u8]>,
        identity: InputIdentity,
        explicit: bool,
    ) {
        self.items.push(Input::Bytes {
            path,
            bytes,
            identity,
            explicit,
        });
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn byte_count(&self) -> u64 {
        self.items.iter().map(Input::byte_len).sum()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Input<'_>] {
        &self.items
    }
}

/// Read transformed bytes that should be searched for one candidate.
pub trait CandidateTransform: Sync {
    /// # Errors
    ///
    /// Returns an error if transformed content cannot be read.
    fn read_candidate(&self, candidate: &Candidate) -> crate::Result<Vec<u8>>;
}

/// How a discovered file is presented as a search input.
pub struct InputConversion<'a> {
    explicit_paths: &'a [PathBuf],
    path_display: PathDisplay,
    transform: Option<&'a dyn CandidateTransform>,
}

impl<'a> InputConversion<'a> {
    #[must_use]
    pub const fn new(
        explicit_paths: &'a [PathBuf],
        path_display: PathDisplay,
        transform: Option<&'a dyn CandidateTransform>,
    ) -> Self {
        Self {
            explicit_paths,
            path_display,
            transform,
        }
    }

    /// Open a candidate as a searchable input.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured candidate transform cannot read input bytes.
    pub fn open<'c>(&self, candidate: &'c Candidate) -> crate::Result<Input<'c>> {
        let explicit = self.is_explicit(candidate);
        let identity = self.identity(candidate);
        if let Some(transform) = self.transform {
            let bytes = transform.read_candidate(candidate)?;
            return Ok(Input::Bytes {
                path: Cow::Owned(candidate.abs_path().display().to_string()),
                bytes: Cow::Owned(bytes),
                identity,
                explicit,
            });
        }
        Ok(Input::Path {
            path: Cow::Borrowed(candidate.abs_path()),
            identity,
            explicit,
        })
    }

    fn is_explicit(&self, candidate: &Candidate) -> bool {
        self.explicit_paths
            .iter()
            .any(|path| path == candidate.rel_path() || path == candidate.abs_path())
    }

    fn identity(&self, candidate: &Candidate) -> InputIdentity {
        match self.path_display {
            PathDisplay::Relative => InputIdentity {
                display_path: candidate.rel_path().to_path_buf(),
                corpus_hit: HitPath::Display,
                byte_len: candidate.cached_size(),
            },
            PathDisplay::Absolute => InputIdentity {
                display_path: candidate.abs_path().to_path_buf(),
                corpus_hit: HitPath::Owned(candidate.rel_path().to_path_buf()),
                byte_len: candidate.cached_size(),
            },
        }
    }
}

/// Inputs ready for [`crate::search::Searcher`] execution.
pub struct SearchInputs<'a> {
    pub candidates: crate::candidates::Candidates<'a>,
    pub streams: Inputs<'a>,
    pub conversion: InputConversion<'a>,
}

impl SearchInputs<'_> {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.streams.is_empty()
    }
}
