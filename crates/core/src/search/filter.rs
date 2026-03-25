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

#[derive(Debug)]
pub struct SearchFilter {
    scopes: Vec<PathBuf>,
    hidden: HiddenMode,
    gitignore: Option<Gitignore>,
    glob: Option<Override>,
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

    #[must_use]
    pub fn is_candidate(&self, rel_path: &Path) -> bool {
        if !self.in_scope(rel_path) {
            return false;
        }
        self.matches_file(rel_path)
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

    fn matches_file(&self, rel_path: &Path) -> bool {
        if self.hidden == HiddenMode::Respect {
            if let Some(name) = rel_path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') && name_str.len() > 1 {
                    return false;
                }
            }
        }

        if let Some(ref matcher) = self.gitignore {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            if matcher.matched(Path::new(&rel_str), false).is_ignore() {
                return false;
            }
        }

        if let Some(ref glob) = self.glob {
            let rel_str = rel_path.to_string_lossy().replace('\\', "/");
            if glob.matched(Path::new(&rel_str), false).is_ignore() {
                return false;
            }
        }

        true
    }
}
