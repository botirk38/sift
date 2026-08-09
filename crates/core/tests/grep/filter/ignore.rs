use sift_core::{FileFilter, FileFilterConfig, IgnoreConfig, VisibilityConfig};
use tempfile::TempDir;

use crate::common::make_filter_corpus;

#[test]
fn matcher_excludes_gitignore_and_ignore_paths() {
    let tmp = TempDir::new().expect("tempdir");
    make_filter_corpus(tmp.path());

    let filter = FileFilter::new(
        &FileFilterConfig {
            visibility: VisibilityConfig {
                ignore: IgnoreConfig::standard(),
                ..VisibilityConfig::default()
            },
            ..FileFilterConfig::default()
        },
        tmp.path(),
    )
    .expect("filter");

    assert!(!filter.matches_path("skip/ignored.txt".as_ref()));
    assert!(!filter.matches_path("also_skip/omit.txt".as_ref()));
    assert!(filter.matches_path("keep.txt".as_ref()));
}
