use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::corpus::Candidate;

/// How a searchable input identifies its file for listing and events.
#[derive(Debug, Clone)]
pub enum InputIdentity {
    /// Corpus file from candidate resolution.
    Candidate(Candidate),
    /// Stdin or anonymous byte stream.
    Stream {
        name: PathBuf,
        byte_len: Option<u64>,
    },
}

/// Shared path identity for listing rows and search events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileIdentity {
    Candidate(Arc<Candidate>),
    Stream { name: Arc<Path> },
}

impl InputIdentity {
    #[must_use]
    pub fn stream(name: &str) -> Self {
        Self::Stream {
            name: PathBuf::from(name),
            byte_len: None,
        }
    }

    #[must_use]
    pub fn file_identity(&self) -> Arc<FileIdentity> {
        match self {
            Self::Candidate(candidate) => {
                Arc::new(FileIdentity::Candidate(Arc::new(candidate.clone())))
            }
            Self::Stream { name, .. } => Arc::new(FileIdentity::Stream {
                name: Arc::from(name.as_path()),
            }),
        }
    }

    #[must_use]
    pub const fn byte_len(&self) -> Option<u64> {
        match self {
            Self::Candidate(candidate) => candidate.cached_size(),
            Self::Stream { byte_len, .. } => *byte_len,
        }
    }
}

impl FileIdentity {
    /// Corpus-relative path for daemon / lazy index enqueue, if any.
    #[must_use]
    pub fn corpus_path(&self) -> Option<&Path> {
        match self {
            Self::Candidate(candidate) => Some(candidate.rel_path()),
            Self::Stream { .. } => None,
        }
    }

    /// Absolute filesystem path when this identity is a candidate.
    #[must_use]
    pub fn abs_path(&self) -> Option<&Path> {
        match self {
            Self::Candidate(candidate) => Some(candidate.abs_path()),
            Self::Stream { .. } => None,
        }
    }

    /// Stable key for printer sets (abs path or stream name).
    #[must_use]
    pub fn key_path(&self) -> &Path {
        match self {
            Self::Candidate(candidate) => candidate.abs_path(),
            Self::Stream { name } => name.as_ref(),
        }
    }

    /// Path shown to the user for this identity.
    #[must_use]
    pub fn display_path(&self, mode: crate::corpus::candidate::PathDisplay) -> &Path {
        use crate::corpus::candidate::PathDisplay;
        match (self, mode) {
            (Self::Candidate(c), PathDisplay::Relative) => c.rel_path(),
            (Self::Candidate(c), PathDisplay::Absolute) => c.abs_path(),
            (Self::Stream { name }, _) => name.as_ref(),
        }
    }
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

impl Input<'_> {
    #[must_use]
    pub const fn identity(&self) -> &InputIdentity {
        match self {
            Self::Path { identity, .. } | Self::Bytes { identity, .. } => identity,
        }
    }

    #[must_use]
    pub fn byte_len(&self) -> u64 {
        match self {
            Self::Path { path, identity, .. } => identity
                .byte_len()
                .unwrap_or_else(|| std::fs::metadata(path).map_or(0, |m| m.len())),
            Self::Bytes { bytes, .. } => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        }
    }
}

impl<'c> Input<'c> {
    /// Open a corpus candidate as a path input for search.
    #[must_use]
    pub fn from_candidate(candidate: &'c Candidate, explicit: &[PathBuf]) -> Self {
        let is_explicit = explicit
            .iter()
            .any(|path| path == candidate.rel_path() || path == candidate.abs_path());
        Self::Path {
            path: Cow::Borrowed(candidate.abs_path()),
            identity: InputIdentity::Candidate(candidate.clone()),
            explicit: is_explicit,
        }
    }
}

pub struct ByteInput<'a> {
    pub path: Cow<'a, str>,
    pub bytes: Cow<'a, [u8]>,
    pub explicit: bool,
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

    #[must_use]
    pub fn empty() -> Self {
        Self::with_capacity(0)
    }

    #[must_use]
    pub fn with_stream(mut self, stream: ByteInput<'a>) -> Self {
        let name = stream.path.as_ref().to_string();
        let identity = InputIdentity::stream(&name);
        if stream.explicit {
            self.push_explicit_bytes(stream.path, stream.bytes, identity);
        } else {
            self.push_bytes(stream.path, stream.bytes, identity);
        }
        self
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

    pub fn push_candidate_bytes(&mut self, candidate: Candidate, bytes: Vec<u8>, explicit: bool) {
        let path = Cow::Owned(candidate.abs_path().display().to_string());
        let identity = InputIdentity::Candidate(candidate);
        self.push_bytes_input(path, Cow::Owned(bytes), identity, explicit);
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

/// Inputs ready for [`crate::search::Searcher`] execution.
pub struct SearchInputs<'a> {
    pub candidates: crate::candidates::Candidates<'a>,
    pub streams: Inputs<'a>,
    pub explicit: &'a [PathBuf],
}

impl SearchInputs<'_> {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.streams.is_empty()
    }
}
