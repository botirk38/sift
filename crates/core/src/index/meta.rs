use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::corpus::filter::{FileFilter, VisibilityConfig};

use super::IndexError;
use super::record::IndexRecord;

const META_FILE: &str = "meta.json";
/// Store format version. Bump when `.sift` layout or meta shape breaks.
pub const STORE_VERSION: u32 = 2;

/// Persistent store manifest (`.sift/meta.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreMeta {
    pub version: u32,
    pub corpus: CorpusMeta,
    #[serde(default)]
    pub coverage: IndexCoverage,
    pub walk: WalkMeta,
    pub filters: FilterMeta,
    pub indexes: Vec<IndexRecord>,
}

/// Whether the store is expected to cover the whole configured corpus.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum IndexCoverage {
    /// The committed snapshots are complete read versions for the configured corpus.
    #[default]
    Complete,
    /// Snapshots may be partial; candidate planning must discover unindexed paths.
    Lazy,
}

/// Which corpus this store indexes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusMeta {
    pub root: PathBuf,
    #[serde(default)]
    pub include_paths: Vec<PathBuf>,
    #[serde(default)]
    pub exclude_paths: Vec<PathBuf>,
}

/// Filesystem walk behavior used when the index was built.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalkMeta {
    pub follow_links: bool,
    #[serde(default)]
    pub one_file_system: bool,
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_filesize: Option<u64>,
}

/// Ignore and visibility rules in effect when the index was built.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterMeta {
    pub visibility: VisibilityConfig,
}

impl StoreMeta {
    /// Create a new `StoreMeta` with the current store version.
    #[must_use]
    pub const fn new(
        corpus: CorpusMeta,
        coverage: IndexCoverage,
        walk: WalkMeta,
        filters: FilterMeta,
        indexes: Vec<IndexRecord>,
    ) -> Self {
        Self {
            version: STORE_VERSION,
            corpus,
            coverage,
            walk,
            filters,
            indexes,
        }
    }

    /// Path to `meta.json` within `dir`.
    #[must_use]
    pub fn path(dir: &Path) -> PathBuf {
        dir.join(META_FILE)
    }

    /// Read from `dir/meta.json`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is missing or malformed.
    pub fn read(dir: &Path) -> crate::Result<Self> {
        let meta_path = Self::path(dir);
        let raw = std::fs::read_to_string(&meta_path)?;
        let meta: Self = serde_json::from_str(&raw).map_err(|e| {
            crate::Error::Index(IndexError::InvalidManifest {
                path: meta_path,
                source: e,
            })
        })?;
        meta.validate_version()?;
        Ok(meta)
    }

    /// Reject unsupported store versions.
    ///
    /// # Errors
    ///
    /// Returns an error when `version` is not [`STORE_VERSION`].
    pub const fn validate_version(&self) -> crate::Result<()> {
        if self.version != STORE_VERSION {
            return Err(crate::Error::Index(IndexError::UnsupportedVersion {
                found: self.version,
                expected: STORE_VERSION,
            }));
        }
        Ok(())
    }

    /// Write to `dir/meta.json`.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or writing fails.
    pub fn write(&self, dir: &Path) -> crate::Result<()> {
        let meta_path = Self::path(dir);
        let json = serde_json::to_vec_pretty(self).map_err(|e| {
            crate::Error::Index(IndexError::InvalidManifest {
                path: meta_path.clone(),
                source: e,
            })
        })?;
        std::fs::write(&meta_path, json)?;
        Ok(())
    }

    /// Catalog of index kinds this store should build.
    #[must_use]
    pub fn catalog(&self) -> &[IndexRecord] {
        &self.indexes
    }

    /// Whether this index metadata covers the search-time candidate universe.
    #[must_use]
    pub fn covers_candidate_filter(&self, filter: &FileFilter) -> bool {
        self.walk.follow_links == filter.follow_links()
            && self.walk.one_file_system == filter.one_file_system()
            && self.walk.max_depth == filter.max_depth()
            && self.walk.max_filesize == filter.max_filesize()
            && self.filters.visibility == *filter.visibility()
            && self.covers_search_scopes(filter)
    }

    fn covers_search_scopes(&self, filter: &FileFilter) -> bool {
        if self.corpus.include_paths.is_empty() {
            return true;
        }

        filter.scopes().iter().all(|scope| {
            !scope.as_os_str().is_empty()
                && scope != Path::new(".")
                && self
                    .corpus
                    .include_paths
                    .iter()
                    .any(|include| scope.starts_with(include))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::filter::IgnoreConfig;
    use tempfile::TempDir;

    #[test]
    fn read_write_roundtrip() {
        let tmp = TempDir::new().expect("create temp dir");

        let meta = StoreMeta::new(
            CorpusMeta {
                root: PathBuf::from("/some/root"),
                include_paths: vec![PathBuf::from("only.rs")],
                exclude_paths: vec![PathBuf::from(".sift")],
            },
            IndexCoverage::Lazy,
            WalkMeta {
                follow_links: true,
                one_file_system: true,
                max_depth: Some(3),
                max_filesize: Some(1024),
            },
            FilterMeta {
                visibility: VisibilityConfig {
                    ignore: IgnoreConfig::standard(),
                    ..VisibilityConfig::default()
                },
            },
            IndexRecord::default_catalog(),
        );
        meta.write(tmp.path()).expect("write meta");

        let loaded = StoreMeta::read(tmp.path()).expect("read meta");
        assert_eq!(loaded.version, meta.version);
        assert_eq!(loaded.corpus.root, meta.corpus.root);
        assert_eq!(loaded.corpus.include_paths, meta.corpus.include_paths);
        assert_eq!(loaded.coverage, meta.coverage);
        assert_eq!(loaded.walk, meta.walk);
        assert_eq!(loaded.filters, meta.filters);
        assert_eq!(loaded.indexes, meta.indexes);
    }

    #[test]
    fn path_returns_meta_json() {
        let p = StoreMeta::path(Path::new("/foo/bar"));
        assert_eq!(p, PathBuf::from("/foo/bar/meta.json"));
    }
}
