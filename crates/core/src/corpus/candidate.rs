use std::path::PathBuf;
use std::sync::OnceLock;

/// How to display a candidate path in output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathDisplay {
    #[default]
    Relative,
    Absolute,
}

use crate::corpus::filter::CandidateFilter;

/// A candidate file that might match a query.
///
/// Created by index planning or filesystem walk, then processed through
/// the candidate pipeline for depth, filesize, and metadata constraints.
/// The normalized string path is computed lazily on first access.
///
/// Fields are accessible via accessor methods to guarantee that the lazy
/// `rel_str` cache remains consistent with `rel_path`.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Path relative to the index root or filter root.
    rel_path: PathBuf,
    /// Absolute filesystem path.
    abs_path: PathBuf,
    rel_str: OnceLock<String>,
    /// File size cached from the walk `DirEntry` or index `FileFingerprint`,
    /// avoiding a redundant `statx` syscall in `within_filesize` and
    /// `total_file_bytes`.
    cached_size: Option<u64>,
    /// Directory depth cached from the walk `DirEntry`, avoiding repeated
    /// `components().count()` iterations in `within_depth`.
    cached_depth: Option<usize>,
}

impl Candidate {
    #[must_use]
    pub const fn new(rel_path: PathBuf, abs_path: PathBuf) -> Self {
        Self {
            rel_path,
            abs_path,
            rel_str: OnceLock::new(),
            cached_size: None,
            cached_depth: None,
        }
    }

    #[must_use]
    pub const fn with_metadata(
        rel_path: PathBuf,
        abs_path: PathBuf,
        size: Option<u64>,
        depth: Option<usize>,
    ) -> Self {
        Self {
            rel_path,
            abs_path,
            rel_str: OnceLock::new(),
            cached_size: size,
            cached_depth: depth,
        }
    }

    #[must_use]
    pub fn rel_path(&self) -> &std::path::Path {
        &self.rel_path
    }

    #[must_use]
    pub fn abs_path(&self) -> &std::path::Path {
        &self.abs_path
    }

    /// Normalized relative path string (forward slashes), computed lazily.
    #[must_use]
    pub fn rel_str(&self) -> &str {
        self.rel_str
            .get_or_init(|| self.rel_path.to_string_lossy().replace('\\', "/"))
    }

    #[must_use]
    pub fn display_path(&self, display: PathDisplay, path_separator: Option<u8>) -> String {
        let raw = match display {
            PathDisplay::Absolute => self.abs_path().display().to_string(),
            PathDisplay::Relative => self.rel_path().display().to_string(),
        };
        if let Some(sep) = path_separator {
            let mut buf = [0u8; 4];
            let sep_str = (sep as char).encode_utf8(&mut buf);
            raw.replace(std::path::MAIN_SEPARATOR, sep_str)
        } else {
            raw
        }
    }

    /// Check depth constraint against a filter's max depth.
    #[must_use]
    pub fn within_depth(&self, max_depth: Option<usize>) -> bool {
        max_depth.is_none_or(|d| {
            self.cached_depth
                .unwrap_or_else(|| self.rel_path.components().count().saturating_sub(1))
                <= d
        })
    }

    /// Cached file size, if available from walk or index metadata.
    #[must_use]
    pub const fn cached_size(&self) -> Option<u64> {
        self.cached_size
    }

    /// Check filesize constraint against a filter's max filesize.
    #[must_use]
    pub fn within_filesize(&self, max_filesize: Option<u64>) -> bool {
        max_filesize.is_none_or(|limit| {
            self.cached_size.map_or_else(
                || std::fs::metadata(&self.abs_path).map_or(true, |m| m.len() <= limit),
                |size| size <= limit,
            )
        })
    }

    /// Sum on-disk byte sizes for all candidates (used for search stats).
    #[must_use]
    pub fn total_file_bytes(candidates: &[Self]) -> u64 {
        candidates.iter().fold(0u64, |acc, c| {
            acc + c
                .cached_size
                .unwrap_or_else(|| std::fs::metadata(c.abs_path()).map_or(0, |m| m.len()))
        })
    }

    /// Check configured filter rules under the given admission policy.
    #[must_use]
    pub fn matches(
        &self,
        filter: &CandidateFilter,
        admission: crate::corpus::filter::FilterAdmission,
    ) -> bool {
        self.within_depth(filter.max_depth())
            && self.within_filesize(filter.max_filesize())
            && filter.matches_candidate(self, admission)
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.rel_path == other.rel_path && self.abs_path == other.abs_path
    }
}

impl Eq for Candidate {}
