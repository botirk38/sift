use std::path::{Path, PathBuf};

use crate::format::PathDisplay;
use clap::Args;
use sift_core::{Indexes, StoreMeta};

#[derive(Args)]
pub struct PathArgs {
    #[arg(short = 'm', long = "max-count", value_name = "NUM")]
    pub max_count: Option<usize>,
    #[arg(long, default_value = ".sift")]
    pub sift_dir: PathBuf,
    #[arg(short = 'L', long = "follow")]
    pub follow: bool,
}

impl PathArgs {
    #[must_use]
    pub fn daemon(&self) -> Option<crate::index::Daemon> {
        std::env::var_os("SIFT_NO_DAEMON")
            .is_none()
            .then(|| crate::index::Daemon::new(self.sift_dir.clone()))
    }
}

/// Resolved corpus root, search prefixes, and index exclusions for a search run.
pub struct CorpusScope {
    pub filter_root: PathBuf,
    pub prefixes: Vec<PathBuf>,
    pub exclude_paths: Vec<PathBuf>,
}

impl CorpusScope {
    /// Resolve search scope from index metadata or walk-from-cwd when no index exists.
    ///
    /// # Errors
    ///
    /// Returns an error if path resolution fails.
    pub fn resolve(
        indexes: Option<&Indexes>,
        meta: Option<&StoreMeta>,
        cwd: &Path,
        search_paths: &[PathBuf],
        sift_dir: &Path,
    ) -> anyhow::Result<Self> {
        match indexes.filter(|indexes| indexes.queryable()) {
            None => {
                let root = meta.map_or_else(
                    || cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf()),
                    |m| m.corpus.root.clone(),
                );
                Ok(Self {
                    filter_root: root.clone(),
                    prefixes: Self::walk_prefixes(&root, search_paths)?,
                    exclude_paths: Self::excluded_paths(&root, sift_dir),
                })
            }
            Some(indexes) => {
                let root = indexes.corpus_root();
                Ok(Self {
                    filter_root: root.to_path_buf(),
                    prefixes: Self::indexed_prefixes(root, cwd, search_paths)?,
                    exclude_paths: Self::excluded_paths(root, sift_dir),
                })
            }
        }
    }

    #[must_use]
    pub fn path_display(search_paths: &[PathBuf]) -> PathDisplay {
        for scope in search_paths {
            if scope.is_absolute() {
                return PathDisplay::Absolute;
            }
        }
        PathDisplay::Relative
    }

    /// Resolve search path prefixes against an indexed corpus root.
    ///
    /// # Errors
    ///
    /// # Panics
    ///
    /// Panics if a path is under the index root but stripping the prefix fails.
    pub fn indexed_prefixes(
        index_root: &Path,
        cwd: &Path,
        requested: &[PathBuf],
    ) -> anyhow::Result<Vec<PathBuf>> {
        if requested.is_empty() {
            return Ok(vec![PathBuf::from("")]);
        }
        let index_root = index_root
            .canonicalize()
            .unwrap_or_else(|_| index_root.to_path_buf());
        let mut out = Vec::with_capacity(requested.len());
        for rel in requested {
            let abs = if rel.is_absolute() {
                rel.clone()
            } else {
                cwd.join(rel)
            };
            let abs = abs.canonicalize().unwrap_or(abs);
            if !abs.starts_with(&index_root) {
                anyhow::bail!(
                    "path {} is not under indexed corpus root {}",
                    abs.display(),
                    index_root.display()
                );
            }
            out.push(
                abs.strip_prefix(&index_root)
                    .expect("prefix checked")
                    .to_path_buf(),
            );
        }
        Ok(out)
    }

    /// Resolve search path prefixes against the current directory.
    ///
    /// # Errors
    ///
    /// # Panics
    ///
    /// Panics if a path is under the current directory but stripping the prefix fails.
    pub fn walk_prefixes(cwd: &Path, requested: &[PathBuf]) -> anyhow::Result<Vec<PathBuf>> {
        if requested.is_empty() {
            return Ok(vec![PathBuf::from("")]);
        }
        let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        let mut out = Vec::with_capacity(requested.len());
        for rel in requested {
            let abs = if rel.is_absolute() {
                rel.clone()
            } else {
                cwd.join(rel)
            };
            let abs = abs.canonicalize().unwrap_or(abs);
            if !abs.starts_with(&cwd) {
                anyhow::bail!("path {} is not under {}", abs.display(), cwd.display());
            }
            out.push(
                abs.strip_prefix(&cwd)
                    .expect("prefix checked")
                    .to_path_buf(),
            );
        }
        Ok(out)
    }

    /// Paths under `search_root` to exclude from search (typically the sift index dir).
    ///
    /// # Panics
    ///
    /// Panics if the canonicalized sift dir is under `search_root` but `strip_prefix` fails.
    #[must_use]
    pub fn excluded_paths(search_root: &Path, sift_dir: &Path) -> Vec<PathBuf> {
        let abs = if sift_dir.is_absolute() {
            sift_dir.to_path_buf()
        } else {
            std::env::current_dir()
                .map_or_else(|_| sift_dir.to_path_buf(), |cwd| cwd.join(sift_dir))
        };
        let abs = abs.canonicalize().unwrap_or(abs);
        let root = search_root
            .canonicalize()
            .unwrap_or_else(|_| search_root.to_path_buf());
        if abs.starts_with(&root) {
            vec![
                abs.strip_prefix(&root)
                    .expect("prefix checked")
                    .to_path_buf(),
            ]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_display_relative_when_empty() {
        assert_eq!(CorpusScope::path_display(&[]), PathDisplay::Relative);
    }

    #[test]
    fn path_display_relative_when_relative() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("src")]),
            PathDisplay::Relative
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_display_absolute_when_absolute_unix() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("/home/user")]),
            PathDisplay::Absolute
        );
    }

    #[cfg(windows)]
    #[test]
    fn path_display_absolute_when_absolute_windows() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("C:\\Users")]),
            PathDisplay::Absolute
        );
    }

    #[cfg(unix)]
    #[test]
    fn path_display_absolute_when_first_is_absolute_unix() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("/root"), PathBuf::from("sub")]),
            PathDisplay::Absolute
        );
    }

    #[cfg(windows)]
    #[test]
    fn path_display_absolute_when_first_is_absolute_windows() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("D:\\projects"), PathBuf::from("sub")]),
            PathDisplay::Absolute
        );
    }

    #[test]
    fn path_display_relative_when_all_relative() {
        assert_eq!(
            CorpusScope::path_display(&[PathBuf::from("a"), PathBuf::from("b/c")]),
            PathDisplay::Relative
        );
    }
}
