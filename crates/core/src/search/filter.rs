//! Search-time filtering over indexed file paths.
//!
//! All user-visible inclusion/exclusion rules are applied at search time,
//! not at index build time. The index stores a structural list of files;
//! this module applies visibility, scope, and glob rules to produce the
//! candidate set for a given search.

use std::path::{Path, PathBuf};

use ignore::gitignore::Gitignore;
use ignore::overrides::{Override, OverrideBuilder};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct IgnoreSources: u8 {
        const DOT     = 1 << 0;
        const VCS     = 1 << 1;
        const GLOBAL  = 1 << 2;
        const EXCLUDE = 1 << 3;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HiddenMode {
    #[default]
    Respect,
    Include,
}

#[derive(Debug, Clone, Default)]
pub struct IgnoreConfig {
    pub sources: IgnoreSources,
    pub custom_files: Vec<PathBuf>,
    pub require_git: bool,
}

#[derive(Debug, Clone, Default)]
pub struct VisibilityConfig {
    pub hidden: HiddenMode,
    pub ignore: IgnoreConfig,
}

#[derive(Debug, Clone, Default)]
pub struct GlobConfig {
    pub patterns: Vec<String>,
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilterConfig {
    pub scopes: Vec<PathBuf>,
    pub glob: GlobConfig,
    pub visibility: VisibilityConfig,
}

/// Pre-computed candidate for efficient batch filtering and search.
#[derive(Debug, Clone)]
pub struct CandidateInfo {
    /// File ID from the index.
    pub id: usize,
    /// Relative path as stored in the index.
    pub rel_path: PathBuf,
    /// Normalized relative path string (forward slashes, for gitignore/glob).
    pub rel_str: String,
    /// Absolute path on disk (`index.root.join(&rel_path)`).
    pub abs_path: PathBuf,
}

#[derive(Debug)]
pub struct SearchFilter {
    scopes: Vec<PathBuf>,
    hidden: HiddenMode,
    gitignore: Option<Gitignore>,
    glob: Option<Override>,
    glob_case_insensitive: bool,
}

impl SearchFilter {
    /// Build a search filter from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if glob patterns are invalid.
    #[allow(clippy::missing_panics_doc)]
    pub fn new(config: &SearchFilterConfig, index_root: &Path) -> crate::Result<Self> {
        let scopes = if config.scopes.is_empty() {
            vec![PathBuf::from("")]
        } else {
            config.scopes.clone()
        };

        let gitignore = Self::build_gitignore_matcher(index_root, &config.visibility.ignore)?;

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
                    crate::Error::RegexBuild(format!("invalid glob pattern '{g}': {e}"))
                })?;
            }
            Some(
                builder
                    .build()
                    .map_err(|e| crate::Error::RegexBuild(e.to_string()))?,
            )
        };

        Ok(Self {
            scopes,
            hidden: config.visibility.hidden,
            gitignore,
            glob,
            glob_case_insensitive,
        })
    }

    fn build_gitignore_matcher(
        root: &Path,
        ignore_config: &IgnoreConfig,
    ) -> crate::Result<Option<Gitignore>> {
        if ignore_config.sources.is_empty() && ignore_config.custom_files.is_empty() {
            return Ok(None);
        }

        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);

        if ignore_config.sources.contains(IgnoreSources::DOT) {
            let _ = builder.add(root.join(".ignore"));
            let _ = builder.add(root.join(".rgignore"));
        }

        if ignore_config.sources.contains(IgnoreSources::VCS) {
            let gitignore_path = root.join(".gitignore");
            if gitignore_path.is_file()
                && (!ignore_config.require_git || root.join(".git").is_dir())
            {
                let _ = builder.add(&gitignore_path);
            }
        }

        if ignore_config.sources.contains(IgnoreSources::EXCLUDE) {
            let exclude_path = root.join(".git/info/exclude");
            if exclude_path.is_file() {
                let _ = builder.add(&exclude_path);
            }
        }

        for custom in &ignore_config.custom_files {
            let path = root.join(custom);
            let _ = builder.add(&path);
        }

        let matcher = builder.build().map_err(crate::Error::Ignore)?;
        Ok(Some(matcher))
    }

    /// Check if a relative path passes all filters.
    #[must_use]
    pub fn is_candidate(&self, rel_path: &Path) -> bool {
        if !self.in_scope(rel_path) {
            return false;
        }
        self.matches_file(rel_path)
    }

    /// Check if a pre-computed `CandidateInfo` passes all filters.
    /// More efficient than `is_candidate` when the candidate has already been prepared.
    #[must_use]
    pub fn is_candidate_info(&self, info: &CandidateInfo) -> bool {
        if !self.in_scope_info(info) {
            return false;
        }
        self.matches_file_info(info)
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

    fn in_scope_info(&self, info: &CandidateInfo) -> bool {
        for scope in &self.scopes {
            if scope.as_os_str().is_empty() {
                return true;
            }
            if info.rel_path.starts_with(scope) {
                return true;
            }
        }
        false
    }

    fn matches_file(&self, rel_path: &Path) -> bool {
        // Fast hidden check - no allocation using as_encoded_bytes()
        if self.hidden == HiddenMode::Respect {
            if let Some(name) = rel_path.file_name() {
                let bytes = name.as_encoded_bytes();
                if bytes.starts_with(b".") && bytes.len() > 1 {
                    return false;
                }
            }
        }

        // Both gitignore and glob require path stringification - do once
        let needs_str = self.gitignore.is_some() || self.glob.is_some();
        if needs_str {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            return self.matches_file_str(Path::new(&rel_str));
        }

        true
    }

    fn matches_file_info(&self, info: &CandidateInfo) -> bool {
        // Fast hidden check - no allocation
        if self.hidden == HiddenMode::Respect {
            if let Some(name) = info.rel_path.file_name() {
                let bytes = name.as_encoded_bytes();
                if bytes.starts_with(b".") && bytes.len() > 1 {
                    return false;
                }
            }
        }

        // rel_str already computed
        self.matches_file_str(Path::new(&info.rel_str))
    }

    fn matches_file_str(&self, rel_path: &Path) -> bool {
        if let Some(ref matcher) = self.gitignore {
            if matcher.matched(rel_path, false).is_ignore() {
                return false;
            }
        }

        if let Some(ref glob) = self.glob {
            // ASCII fast-path for case-insensitive glob
            if self.glob_case_insensitive {
                let rel_str = rel_path.to_string_lossy();
                if rel_str.is_ascii() {
                    let rel_lower = rel_str.to_ascii_lowercase();
                    if glob.matched(Path::new(&rel_lower), false).is_ignore() {
                        return false;
                    }
                    return true;
                }
            }
            if glob.matched(rel_path, false).is_ignore() {
                return false;
            }
        }

        true
    }
}
