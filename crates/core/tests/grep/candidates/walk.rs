use sift_core::{
    CandidateFilter, CandidateFilterConfig, HiddenMode, IgnoreConfig, IgnoreSources,
    VisibilityConfig,
};
use tempfile::TempDir;

use crate::common::make_filter_corpus;

#[test]
fn filter_respects_gitignore_on_fixture() {
    let tmp = TempDir::new().expect("tempdir");
    make_filter_corpus(tmp.path());

    let config = CandidateFilterConfig {
        visibility: VisibilityConfig {
            hidden: HiddenMode::Respect,
            ignore: IgnoreConfig {
                sources: IgnoreSources::defaults(),
                require_git: false,
                ..IgnoreConfig::default()
            },
        },
        ..CandidateFilterConfig::default()
    };
    let filter = CandidateFilter::new(&config, tmp.path()).expect("filter");
    assert!(!filter.matches_path(std::path::Path::new("skip/ignored.txt")));
    assert!(filter.matches_path(std::path::Path::new("keep.txt")));
}
