use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::corpus::{File, PathDisplay};

/// Shared identity for inputs, listing rows, and search events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    File(Arc<File>),
    Stream { label: Arc<str> },
}

impl Origin {
    #[must_use]
    pub fn file(file: File) -> Self {
        Self::File(Arc::new(file))
    }

    #[must_use]
    pub fn stream(label: impl Into<Arc<str>>) -> Self {
        Self::Stream {
            label: label.into(),
        }
    }

    /// Corpus-relative path for daemon / lazy index enqueue, if any.
    #[must_use]
    pub fn corpus_path(&self) -> Option<&Path> {
        match self {
            Self::File(file) => Some(file.rel_path()),
            Self::Stream { .. } => None,
        }
    }

    /// Absolute filesystem path when this origin is a corpus file.
    #[must_use]
    pub fn abs_path(&self) -> Option<&Path> {
        match self {
            Self::File(file) => Some(file.abs_path()),
            Self::Stream { .. } => None,
        }
    }

    /// Stable key for printer sets (abs path or stream label).
    #[must_use]
    pub fn key(&self) -> Cow<'_, str> {
        match self {
            Self::File(file) => file.abs_path().to_string_lossy(),
            Self::Stream { label } => Cow::Borrowed(label),
        }
    }

    /// Text shown to the user for this origin.
    #[must_use]
    pub fn display(&self, mode: PathDisplay) -> Cow<'_, str> {
        match (self, mode) {
            (Self::File(file), PathDisplay::Relative) => file.rel_path().to_string_lossy(),
            (Self::File(file), PathDisplay::Absolute) => file.abs_path().to_string_lossy(),
            (Self::Stream { label }, _) => Cow::Borrowed(label),
        }
    }

    #[must_use]
    pub fn cached_size(&self) -> Option<u64> {
        match self {
            Self::File(file) => file.cached_size(),
            Self::Stream { .. } => None,
        }
    }

    #[must_use]
    pub fn is_explicit(&self, explicit: &[PathBuf]) -> bool {
        match self {
            Self::File(file) => file.is_explicit(explicit),
            Self::Stream { .. } => true,
        }
    }
}

pub enum Input<'a> {
    Path {
        origin: Origin,
        explicit: bool,
    },
    Bytes {
        origin: Origin,
        bytes: Cow<'a, [u8]>,
        explicit: bool,
    },
}

impl Input<'_> {
    #[must_use]
    pub const fn origin(&self) -> &Origin {
        match self {
            Self::Path { origin, .. } | Self::Bytes { origin, .. } => origin,
        }
    }

    #[must_use]
    pub fn byte_len(&self) -> u64 {
        match self {
            Self::Path { origin, .. } => origin.cached_size().unwrap_or_else(|| {
                origin
                    .abs_path()
                    .and_then(|path| std::fs::metadata(path).ok())
                    .map_or(0, |m| m.len())
            }),
            Self::Bytes { bytes, .. } => u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        }
    }
}

impl Input<'_> {
    /// Open a corpus file as a path input for search.
    #[must_use]
    pub fn from_file(file: File, explicit: &[PathBuf]) -> Self {
        let explicit = file.is_explicit(explicit);
        Self::Path {
            origin: Origin::file(file),
            explicit,
        }
    }
}

pub struct ByteInput<'a> {
    pub label: Cow<'a, str>,
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
        self.items.push(Input::Bytes {
            origin: Origin::stream(stream.label.as_ref()),
            bytes: stream.bytes,
            explicit: stream.explicit,
        });
        self
    }

    pub fn push_path(&mut self, origin: Origin, explicit: bool) {
        self.items.push(Input::Path { origin, explicit });
    }

    pub fn push_file_bytes(&mut self, file: File, bytes: Vec<u8>, explicit: bool) {
        self.items.push(Input::Bytes {
            origin: Origin::file(file),
            bytes: Cow::Owned(bytes),
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
