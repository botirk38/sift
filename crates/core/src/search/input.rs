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
}

/// How an input entered the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mention {
    /// Named on argv (a file path or `-`).
    Explicit,
    /// Found by walking a search path.
    Discovered,
}

impl Mention {
    #[must_use]
    pub fn of(file: &File, explicit: &[PathBuf]) -> Self {
        if file.is_explicit(explicit) {
            Self::Explicit
        } else {
            Self::Discovered
        }
    }
}

pub enum Input<'a> {
    Path {
        origin: Origin,
        mention: Mention,
    },
    Bytes {
        origin: Origin,
        bytes: Cow<'a, [u8]>,
        mention: Mention,
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
    pub const fn mention(&self) -> Mention {
        match self {
            Self::Path { mention, .. } | Self::Bytes { mention, .. } => *mention,
        }
    }

    /// Open a corpus file as a path input for search.
    #[must_use]
    pub fn from_file(file: File, explicit: &[PathBuf]) -> Self {
        let mention = Mention::of(&file, explicit);
        Self::Path {
            origin: Origin::file(file),
            mention,
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

    #[must_use]
    pub fn empty() -> Self {
        Self::with_capacity(0)
    }

    pub fn push(&mut self, input: Input<'a>) {
        self.items.push(input);
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
    pub fn into_vec(self) -> Vec<Input<'a>> {
        self.items
    }
}

impl<'a> IntoIterator for Inputs<'a> {
    type Item = Input<'a>;
    type IntoIter = std::vec::IntoIter<Input<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a> FromIterator<Input<'a>> for Inputs<'a> {
    fn from_iter<I: IntoIterator<Item = Input<'a>>>(iter: I) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

impl<'a> Extend<Input<'a>> for Inputs<'a> {
    fn extend<T: IntoIterator<Item = Input<'a>>>(&mut self, iter: T) {
        self.items.extend(iter);
    }
}
