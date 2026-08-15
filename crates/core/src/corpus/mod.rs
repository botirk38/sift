//! Shared corpus foundation: candidates, filters, and filesystem walk.

pub mod file;
pub mod filter;
pub mod order;
pub mod walk;

pub use file::{File, PathDisplay};
pub use filter::{
    FileFilter, FileFilterConfig, FilterAdmission, GlobConfig, HiddenMode, IgnoreConfig,
    IgnoreSources, TypeFilterRule, VisibilityConfig,
};
pub use order::{FileOrder, FileOrderDirection, FileOrderKey};
pub use walk::{FileWalk, WalkFile, WalkMetadata};
