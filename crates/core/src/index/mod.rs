mod disk;
pub mod error;
pub mod files;
mod indexes;
pub mod kinds;
pub mod meta;
pub mod mmap;
pub mod ngram;
pub(crate) mod postings;
pub mod record;

pub use disk::SnapshotId;
pub use error::IndexError;
pub use files::Files;
pub use indexes::Indexes;
pub use kinds::FileId;
pub use meta::{CorpusMeta, FilterMeta, IndexCoverage, StoreMeta, WalkMeta};
pub use record::IndexRecord;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn empty_meta(root: std::path::PathBuf) -> StoreMeta {
        StoreMeta::new(
            CorpusMeta {
                root,
                include_paths: Vec::new(),
                exclude_paths: Vec::new(),
            },
            IndexCoverage::Complete,
            WalkMeta {
                follow_links: false,
                one_file_system: false,
                max_depth: None,
                max_filesize: None,
            },
            FilterMeta {
                visibility: crate::corpus::filter::VisibilityConfig::default(),
            },
            IndexRecord::default_catalog(),
        )
    }

    #[test]
    fn file_id_new_and_get() {
        let id = FileId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn indexes_open_empty_when_no_current_file() {
        let tmp = TempDir::new().expect("create temp dir");
        let sift_dir = tmp.path().join(".sift");
        fs::create_dir_all(&sift_dir).expect("create sift dir");
        let meta = empty_meta(tmp.path().to_path_buf());
        let indexes = Indexes::open(&sift_dir, &meta).expect("open indexes");
        assert!(!indexes.queryable());
    }

    #[test]
    fn indexes_load_does_not_create_store() {
        let tmp = TempDir::new().expect("create temp dir");
        let sift_dir = tmp.path().join(".sift");
        let indexes = Indexes::load(&sift_dir).expect("load indexes");
        assert!(indexes.is_none());
        assert!(!StoreMeta::path(&sift_dir).exists());
        assert!(!sift_dir.exists());
    }

    #[test]
    fn indexes_load_errors_on_dangling_current() {
        let tmp = TempDir::new().expect("create temp dir");
        let sift_dir = tmp.path().join(".sift");
        fs::create_dir_all(&sift_dir).expect("create sift dir");
        fs::write(sift_dir.join("CURRENT"), "missing-snapshot\n").expect("write CURRENT");
        assert!(Indexes::load(&sift_dir).is_err());
    }
}
