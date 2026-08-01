pub mod config;
pub mod error;
pub mod ignore;
pub mod type_filter;

use std::path::{Path, PathBuf};

use error::FilterError;

use crate::search::Error as SearchError;

use ::ignore::gitignore::Gitignore;
use ::ignore::overrides::{Override, OverrideBuilder};

pub use config::{
    FileFilterConfig, GlobConfig, HiddenMode, IgnoreConfig, IgnoreSources, TypeFilterRule,
    VisibilityConfig,
};

/// Which filter rules apply when admitting a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterAdmission {
    /// Walked or untrusted files: apply ignore, hidden, glob, type, and scope.
    Full,
    /// Indexed under matching visibility: ignore/hidden already applied at build.
    /// Still apply scope, exclude, glob, type, depth, and filesize.
    Indexed,
}

#[derive(Debug)]
pub struct FileFilter {
    root: PathBuf,
    scopes: Vec<PathBuf>,
    exclude_paths: Vec<PathBuf>,
    visibility: VisibilityConfig,
    gitignore: Option<Gitignore>,
    glob: Option<Override>,
    glob_case_insensitive: bool,
    follow_links: bool,
    max_depth: Option<usize>,
    max_filesize: Option<u64>,
    type_glob: Option<Override>,
    one_file_system: bool,
}

impl FileFilter {
    fn path_is_hidden(rel_path: &Path) -> bool {
        // Scan the encoded path bytes once and split on separators instead of
        // materializing a `Components` iterator per candidate. A segment is
        // hidden when it starts with '.' and is longer than a single byte
        // (so a lone "." current-dir segment is not treated as hidden).
        rel_path
            .as_os_str()
            .as_encoded_bytes()
            .split(|&b| std::path::is_separator(b as char))
            .any(|segment| matches!(segment, [b'.', _, ..]))
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn scopes(&self) -> &[PathBuf] {
        &self.scopes
    }

    #[must_use]
    pub fn exclude_paths(&self) -> &[PathBuf] {
        &self.exclude_paths
    }

    #[must_use]
    pub(crate) const fn needs_search_globs(&self) -> bool {
        self.glob.is_some() || self.type_glob.is_some()
    }

    #[must_use]
    pub(crate) const fn needs_rel_str_for_matching(&self) -> bool {
        self.gitignore.is_some() || self.needs_search_globs()
    }

    #[must_use]
    pub const fn follow_links(&self) -> bool {
        self.follow_links
    }

    #[must_use]
    pub const fn max_depth(&self) -> Option<usize> {
        self.max_depth
    }

    #[must_use]
    pub const fn max_filesize(&self) -> Option<u64> {
        self.max_filesize
    }

    #[must_use]
    pub const fn one_file_system(&self) -> bool {
        self.one_file_system
    }

    #[must_use]
    pub const fn visibility(&self) -> &VisibilityConfig {
        &self.visibility
    }

    /// Creates a new candidate filter from configuration.
    ///
    /// # Errors
    ///
    /// Returns `SearchError` if glob patterns are invalid or type definitions are unknown.
    pub fn new(config: &FileFilterConfig, index_root: &Path) -> Result<Self, SearchError> {
        let scopes = if config.scopes.is_empty() {
            vec![PathBuf::from("")]
        } else {
            config.scopes.clone()
        };

        let gitignore = config.visibility.ignore.matcher(index_root)?;

        let glob_case_insensitive = config.glob.case_insensitive;
        let glob = if config.glob.patterns.is_empty() {
            None
        } else {
            let mut builder = OverrideBuilder::new(index_root);
            if config.glob.case_insensitive {
                let _ = builder.case_insensitive(true);
            }
            for g in &config.glob.patterns {
                builder.add(g).map_err(|e| {
                    SearchError::RegexBuild(format!("invalid glob pattern '{g}': {e}"))
                })?;
            }
            Some(
                builder
                    .build()
                    .map_err(|e| FilterError::RegexBuild(e.to_string()))?,
            )
        };

        let type_glob = config.type_override(index_root)?;

        Ok(Self {
            root: index_root.to_path_buf(),
            scopes,
            exclude_paths: config.exclude_paths.clone(),
            visibility: config.visibility.clone(),
            gitignore,
            glob,
            glob_case_insensitive,
            follow_links: config.follow_links,
            max_depth: config.max_depth,
            max_filesize: config.max_filesize,
            type_glob,
            one_file_system: config.one_file_system,
        })
    }

    #[must_use]
    pub(crate) fn retain(
        &self,
        candidates: Vec<crate::File>,
        admission: FilterAdmission,
    ) -> Vec<crate::File> {
        use rayon::prelude::*;
        candidates
            .into_par_iter()
            .filter(|candidate| candidate.matches(self, admission))
            .collect()
    }

    /// Check whether a relative path passes all configured rules.
    #[must_use]
    pub fn matches_path(&self, rel_path: &Path) -> bool {
        if !self.in_scope(rel_path) {
            return false;
        }
        if self.is_excluded(rel_path) {
            return false;
        }
        self.matches_rel_path(rel_path)
    }

    /// Check path-based rules for a corpus file under the given admission policy.
    /// Depth and filesize are checked by [`crate::File::matches`].
    #[must_use]
    pub(crate) fn matches_file(&self, file: &crate::File, admission: FilterAdmission) -> bool {
        if !self.in_scope(file.rel_path()) {
            return false;
        }
        if self.is_excluded(file.rel_path()) {
            return false;
        }
        match admission {
            FilterAdmission::Full => {
                if self.visibility.hidden == crate::corpus::filter::config::HiddenMode::Respect
                    && Self::path_is_hidden(file.rel_path())
                {
                    return false;
                }
                if !self.needs_rel_str_for_matching() {
                    return true;
                }
                self.matches_file_str(Path::new(file.rel_str()))
            }
            FilterAdmission::Indexed => {
                if !self.needs_search_globs() {
                    return true;
                }
                self.matches_search_globs(Path::new(file.rel_str()))
            }
        }
    }

    fn is_excluded(&self, rel_path: &Path) -> bool {
        self.exclude_paths
            .iter()
            .any(|excluded| !excluded.as_os_str().is_empty() && rel_path.starts_with(excluded))
    }

    fn in_scope(&self, rel_path: &Path) -> bool {
        for scope in &self.scopes {
            if scope.as_os_str().is_empty() {
                return true;
            }
            if rel_path.starts_with(scope) {
                return true;
            }
        }
        false
    }

    fn matches_rel_path(&self, rel_path: &Path) -> bool {
        if self.visibility.hidden == crate::corpus::filter::config::HiddenMode::Respect
            && Self::path_is_hidden(rel_path)
        {
            return false;
        }

        if self.needs_rel_str_for_matching() {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            return self.matches_file_str(Path::new(&rel_str));
        }

        true
    }

    fn matches_file_str(&self, rel_path: &Path) -> bool {
        if self
            .gitignore
            .as_ref()
            .is_some_and(|m| m.matched(rel_path, false).is_ignore())
        {
            return false;
        }
        self.matches_search_globs(rel_path)
    }

    fn matches_search_globs(&self, rel_path: &Path) -> bool {
        if let Some(ref glob) = self.glob {
            if self.glob_case_insensitive {
                let rel_str = rel_path.to_string_lossy();
                if rel_str.is_ascii() {
                    let rel_lower = rel_str.to_ascii_lowercase();
                    if glob.matched(Path::new(&rel_lower), false).is_ignore() {
                        return false;
                    }
                    return self.matches_type_glob(rel_path);
                }
            }
            if glob.matched(rel_path, false).is_ignore() {
                return false;
            }
        }

        self.matches_type_glob(rel_path)
    }

    fn matches_type_glob(&self, rel_path: &Path) -> bool {
        if let Some(ref tg) = self.type_glob {
            return !tg.matched(rel_path, false).is_ignore();
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::filter::FilterAdmission;
    use crate::corpus::filter::config::*;

    fn make_filter(config: &FileFilterConfig) -> FileFilter {
        FileFilter::new(config, Path::new("/root")).expect("create filter")
    }

    #[test]
    fn empty_config_includes_normal_visible_files() {
        let config = FileFilterConfig::default();
        let filter = make_filter(&config);
        assert!(filter.matches_path(Path::new("src/lib.rs")));
    }

    #[test]
    fn hidden_paths_rejected_by_default() {
        let config = FileFilterConfig::default();
        let filter = make_filter(&config);
        assert!(!filter.matches_path(Path::new(".hidden/file.txt")));
        assert!(!filter.matches_path(Path::new("dir/.hidden")));
    }

    #[test]
    fn hidden_paths_accepted_with_include_mode() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(filter.matches_path(Path::new(".hidden/file.txt")));
    }

    #[test]
    fn empty_scopes_include_all_files() {
        let config = FileFilterConfig::default();
        let filter = make_filter(&config);
        assert!(filter.matches_path(Path::new("any/path/file.txt")));
    }

    #[test]
    fn specific_scopes_include_matching_prefixes_only() {
        let config = FileFilterConfig {
            scopes: vec![PathBuf::from("src")],
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(filter.matches_path(Path::new("src/lib.rs")));
        assert!(!filter.matches_path(Path::new("tests/test.rs")));
    }

    #[test]
    fn exclude_paths_reject_matching_prefixes() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            exclude_paths: vec![PathBuf::from("vendor")],
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(!filter.matches_path(Path::new("vendor/pkg/file.rs")));
        assert!(filter.matches_path(Path::new("src/lib.rs")));
    }

    #[test]
    fn path_and_candidate_agree() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        let rel_path = Path::new("src/lib.rs");
        let info = crate::File::new(rel_path.to_path_buf(), PathBuf::from("/root/src/lib.rs"));
        assert_eq!(
            filter.matches_path(rel_path),
            filter.matches_file(&info, FilterAdmission::Full)
        );
    }

    #[test]
    fn glob_excludes_reject_excluded_paths() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            glob: GlobConfig {
                patterns: vec!["!*.log".to_string()],
                case_insensitive: false,
            },
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(!filter.matches_path(Path::new("debug.log")));
        assert!(filter.matches_path(Path::new("src/lib.rs")));
    }

    #[test]
    fn case_insensitive_glob_matching_works() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            glob: GlobConfig {
                patterns: vec!["!*.LOG".to_string()],
                case_insensitive: true,
            },
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(!filter.matches_path(Path::new("debug.log")));
        assert!(!filter.matches_path(Path::new("debug.LOG")));
    }

    #[test]
    fn type_filter_include_accepts_matching_globs() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            type_filters: vec![TypeFilterRule::Include {
                name: "rust".to_string(),
                globs: vec!["*.rs".to_string()],
            }],
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(filter.matches_path(Path::new("src/lib.rs")));
        assert!(!filter.matches_path(Path::new("src/lib.txt")));
    }

    #[test]
    fn type_filter_exclude_rejects_matching_globs() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            type_filters: vec![TypeFilterRule::Exclude {
                name: "rust".to_string(),
                globs: vec!["*.rs".to_string()],
            }],
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(!filter.matches_path(Path::new("src/lib.rs")));
        assert!(filter.matches_path(Path::new("src/lib.txt")));
    }

    #[test]
    fn invalid_type_glob_returns_error() {
        let config = FileFilterConfig {
            type_filters: vec![TypeFilterRule::Include {
                name: "broken".to_string(),
                globs: vec!["[invalid".to_string()],
            }],
            ..FileFilterConfig::default()
        };
        let result = FileFilter::new(&config, Path::new("/root"));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_glob_returns_error() {
        let config = FileFilterConfig {
            glob: GlobConfig {
                patterns: vec!["[invalid".to_string()],
                case_insensitive: false,
            },
            ..FileFilterConfig::default()
        };
        let result = FileFilter::new(&config, Path::new("/root"));
        assert!(result.is_err());
    }

    #[test]
    fn filter_accessor_follow_links() {
        let config = FileFilterConfig {
            follow_links: true,
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(filter.follow_links());
    }

    #[test]
    fn filter_accessor_max_depth() {
        let config = FileFilterConfig {
            max_depth: Some(5),
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert_eq!(filter.max_depth(), Some(5));
    }

    #[test]
    fn filter_accessor_max_filesize() {
        let config = FileFilterConfig {
            max_filesize: Some(1024),
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert_eq!(filter.max_filesize(), Some(1024));
    }

    #[test]
    fn filter_accessor_one_file_system() {
        let config = FileFilterConfig {
            one_file_system: true,
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert!(filter.one_file_system());
    }

    #[test]
    fn filter_accessor_scopes() {
        let config = FileFilterConfig {
            scopes: vec![PathBuf::from("src")],
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        assert_eq!(filter.scopes(), &[PathBuf::from("src")]);
    }

    #[test]
    fn indexed_admission_skips_hidden_and_ignore_rules() {
        let config = FileFilterConfig::default();
        let filter = make_filter(&config);
        let hidden = crate::File::new(
            PathBuf::from(".hidden/file.txt"),
            PathBuf::from("/root/.hidden/file.txt"),
        );
        assert!(!hidden.matches(&filter, FilterAdmission::Full));
        assert!(hidden.matches(&filter, FilterAdmission::Indexed));
    }

    #[test]
    fn indexed_admission_still_applies_globs() {
        let config = FileFilterConfig {
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
            glob: GlobConfig {
                patterns: vec!["!*.log".to_string()],
                case_insensitive: false,
            },
            ..FileFilterConfig::default()
        };
        let filter = make_filter(&config);
        let log = crate::File::new(PathBuf::from("debug.log"), PathBuf::from("/root/debug.log"));
        assert!(!log.matches(&filter, FilterAdmission::Indexed));
    }
}
