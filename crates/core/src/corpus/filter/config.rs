use std::path::PathBuf;

use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct IgnoreSources: u8 {
        const DOT     = 1 << 0;
        const VCS     = 1 << 1;
        const GLOBAL  = 1 << 2;
        const EXCLUDE = 1 << 3;
        const PARENT  = 1 << 4;
    }
}

impl IgnoreSources {
    /// Default ignore sources used by `sift build` and search.
    #[must_use]
    pub const fn defaults() -> Self {
        Self::DOT
            .union(Self::VCS)
            .union(Self::EXCLUDE)
            .union(Self::GLOBAL)
            .union(Self::PARENT)
    }
}

impl IgnoreConfig {
    /// Standard ignore rules for corpus walks (`.gitignore`, global, etc.), without requiring a git repo.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            sources: IgnoreSources::defaults(),
            require_git: false,
            ..Self::default()
        }
    }

    /// Ignore rules disabled — every path is eligible unless filtered elsewhere.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            sources: IgnoreSources::empty(),
            require_git: false,
            ..Self::default()
        }
    }
}

impl Default for VisibilityConfig {
    fn default() -> Self {
        Self {
            hidden: HiddenMode::Respect,
            ignore: IgnoreConfig::standard(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HiddenMode {
    #[default]
    Respect,
    Include,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IgnoreConfig {
    pub sources: IgnoreSources,
    pub custom_files: Vec<PathBuf>,
    pub require_git: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisibilityConfig {
    pub hidden: HiddenMode,
    pub ignore: IgnoreConfig,
}

#[derive(Debug, Clone, Default)]
pub struct GlobConfig {
    pub patterns: Vec<String>,
    pub case_insensitive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeFilterRule {
    Include { name: String, globs: Vec<String> },
    Exclude { name: String, globs: Vec<String> },
}

#[derive(Debug, Clone, Default)]
pub struct FileFilterConfig {
    pub scopes: Vec<PathBuf>,
    pub exclude_paths: Vec<PathBuf>,
    pub glob: GlobConfig,
    pub visibility: VisibilityConfig,
    pub follow_links: bool,
    pub max_depth: Option<usize>,
    pub max_filesize: Option<u64>,
    pub type_filters: Vec<TypeFilterRule>,
    pub one_file_system: bool,
}
