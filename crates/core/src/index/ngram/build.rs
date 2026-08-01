//! Corpus walk, file stat, and posting assembly helpers.
//!
//! These are private helpers used by [`Index::build`] and [`Index::update`].

use std::path::{Path, PathBuf};

use super::files::FileFingerprint;
use super::gram::{Gram, GramNorm, GramWidth};
use super::storage::grams::GramSet;
use super::storage::lexicon::LexiconEntry;
use super::storage::postings::Postings;

use crate::corpus::walk::LinkTraversal;
use crate::corpus::walk::{FileWalk, WalkFile};
use crate::index::{CorpusKind, IndexConfig};

/// Collected index data ready for persistence.
pub struct IndexTables {
    pub fingerprints: Vec<FileFingerprint>,
    pub file_grams: Vec<GramSet>,
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Fingerprint collection
// ---------------------------------------------------------------------------

/// Stats each file to produce fingerprints with mtime and size.
pub struct FingerprintCollector<'a> {
    root: &'a Path,
    paths: &'a [PathBuf],
}

impl<'a> FingerprintCollector<'a> {
    pub const fn new(root: &'a Path, paths: &'a [PathBuf]) -> Self {
        Self { root, paths }
    }

    pub fn collect(&self) -> crate::Result<Vec<FileFingerprint>> {
        use rayon::prelude::*;
        self.paths
            .par_iter()
            .map(|rel| {
                let abs = self.root.join(rel);
                let meta = std::fs::metadata(&abs)?;
                let mtime_secs = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
                let size = meta.len();
                Ok(FileFingerprint {
                    path: rel.clone(),
                    mtime_secs,
                    size,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Posting assembly
// ---------------------------------------------------------------------------

/// Assembles gram -> file ID posting lists from per-file gram sets.
pub struct PostingTables {
    pub lexicon: Vec<LexiconEntry>,
    pub postings: Vec<u8>,
}

/// Packed `(gram ordinal, file id)` sort key. Sorting the packed integer in
/// ascending order is identical to sorting `(ordinal, file_id)`
/// lexicographically, so one integer comparison replaces a chained tuple
/// comparison. Narrow grams (width <= 4) pack losslessly into `u64`; wider gram
/// ordinals exceed 32 bits and need the `u128` key.
trait PostingKey: Copy + Ord + Send {
    fn pack(ordinal: u64, file_id: u32) -> Self;
    fn ordinal(self) -> u64;
    fn file_id(self) -> u32;
}

impl PostingKey for u64 {
    fn pack(ordinal: u64, file_id: u32) -> Self {
        (ordinal << 32) | Self::from(file_id)
    }
    fn ordinal(self) -> u64 {
        self >> 32
    }
    fn file_id(self) -> u32 {
        u32::try_from(self & Self::from(u32::MAX)).expect("masked file id fits in u32")
    }
}

impl PostingKey for u128 {
    fn pack(ordinal: u64, file_id: u32) -> Self {
        (Self::from(ordinal) << 32) | Self::from(file_id)
    }
    fn ordinal(self) -> u64 {
        u64::try_from(self >> 32).expect("gram ordinal fits u64")
    }
    fn file_id(self) -> u32 {
        u32::try_from(self & Self::from(u32::MAX)).expect("masked file id fits in u32")
    }
}

impl PostingTables {
    pub fn assemble(width: GramWidth, file_grams: &[GramSet]) -> crate::Result<Self> {
        // width <= 4 means the gram ordinal fits in 32 bits, so the packed
        // (ordinal, file_id) key fits a u64 and halves sort-key size and
        // comparison cost versus the u128 key wider grams require.
        if width.get() <= 4 {
            Self::assemble_packed::<u64>(width, file_grams)
        } else {
            Self::assemble_packed::<u128>(width, file_grams)
        }
    }

    fn assemble_packed<K: PostingKey>(
        width: GramWidth,
        file_grams: &[GramSet],
    ) -> crate::Result<Self> {
        let total: usize = file_grams.iter().map(|s| s.as_slice().len()).sum();
        if file_grams.len() > u32::MAX as usize {
            return Err(crate::Error::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "too many indexed files",
            )));
        }

        // The sort is parallelized because it otherwise dominates build/update
        // time while the Rayon pool sits idle.
        let mut pairs = Vec::with_capacity(total);
        for (fid, set) in file_grams.iter().enumerate() {
            let fid32 = u32::try_from(fid).expect("file count checked above");
            for gram in set.as_slice() {
                pairs.push(K::pack(gram.ordinal(), fid32));
            }
        }
        rayon::slice::ParallelSliceMut::par_sort_unstable(&mut pairs[..]);
        Self::encode_pairs(width, &pairs)
    }

    fn encode_pairs<K: PostingKey>(width: GramWidth, pairs: &[K]) -> crate::Result<Self> {
        let mut posting_bytes = Vec::with_capacity(pairs.len());
        let mut lexicon = Vec::new();
        let mut ids: Vec<u32> = Vec::new();
        let mut i = 0;
        while i < pairs.len() {
            let gram_key = pairs[i].ordinal();
            let start = i;
            while i < pairs.len() && pairs[i].ordinal() == gram_key {
                i += 1;
            }
            let gram = Gram::from_ordinal(width, gram_key)?;
            let offset: u64 = posting_bytes.len().try_into().map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "postings offset overflow",
                ))
            })?;
            let len = u32::try_from(i - start).map_err(|_| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "posting list too long",
                ))
            })?;
            ids.clear();
            ids.extend(pairs[start..i].iter().map(|p| p.file_id()));
            posting_bytes.extend_from_slice(&Postings::encode_list(&ids));
            lexicon.push(LexiconEntry { gram, offset, len });
        }
        Ok(Self {
            lexicon,
            postings: posting_bytes,
        })
    }
}

/// Build in-memory index tables from a corpus configuration.
///
/// When `paths` is empty, discovers files via [`FileWalk`]. Otherwise indexes
/// exactly the given corpus-relative paths.
impl IndexTables {
    pub fn build(
        width: GramWidth,
        norm: GramNorm,
        config: &IndexConfig<'_>,
        paths: &[PathBuf],
    ) -> crate::Result<Self> {
        use rayon::prelude::*;

        let (paths, root) = match config.corpus.kind {
            CorpusKind::SingleFile => {
                if config.corpus.include_paths.is_empty() {
                    return Err(crate::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "SingleFile corpus must specify the file in include_paths",
                    )));
                }
                let paths = config.corpus.include_paths.to_vec();
                (paths, config.corpus.root)
            }
            CorpusKind::Directory => {
                let paths = if paths.is_empty() {
                    FileWalk::new(config.corpus.root)
                        .scopes(config.corpus.include_paths)
                        .excludes(config.corpus.exclude_paths)
                        .visibility(config.visibility.clone())
                        .links(if config.corpus.follow_links {
                            LinkTraversal::Follow
                        } else {
                            LinkTraversal::DoNotFollow
                        })
                        .one_file_system(config.walk.one_file_system)
                        .max_depth(config.walk.max_depth)
                        .max_filesize(config.walk.max_filesize)
                        .files()?
                        .into_iter()
                        .map(WalkFile::into_rel_path)
                        .collect()
                } else {
                    paths.to_vec()
                };
                (paths, config.corpus.root)
            }
        };

        let fingerprints = FingerprintCollector::new(root, &paths).collect()?;

        let file_grams: Vec<GramSet> = fingerprints
            .par_iter()
            .map(|fp| {
                let abs = root.join(&fp.path);
                std::fs::read(&abs).map(|bytes| GramSet::collect(width, &bytes, norm))
            })
            .collect::<std::io::Result<_>>()
            .map_err(crate::Error::Io)?;

        let tables = PostingTables::assemble(width, &file_grams)?;

        Ok(Self {
            fingerprints,
            file_grams,
            lexicon: tables.lexicon,
            postings: tables.postings,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::filter::IgnoreConfig;
    use crate::corpus::walk::LinkTraversal;
    use crate::corpus::walk::{FileWalk, WalkFile};
    use crate::corpus::{FileFilter, FileFilterConfig, VisibilityConfig};
    use crate::index::config::IndexWalkConfig;
    use crate::index::ngram::gram::GramWidth;
    use crate::index::ngram::storage::postings::Postings;
    use crate::index::{CorpusKind, CorpusSpec, IndexConfig};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct TablesFixture;

    impl TablesFixture {
        fn no_ignore_config(root: &Path) -> IndexConfig<'_> {
            IndexConfig {
                corpus: CorpusSpec {
                    root,
                    kind: CorpusKind::Directory,
                    follow_links: false,
                    include_paths: &[],
                    exclude_paths: &[],
                },
                walk: IndexWalkConfig::new(false),
                visibility: VisibilityConfig {
                    ignore: IgnoreConfig::disabled(),
                    ..Default::default()
                },
            }
        }

        fn build(root: &Path) -> IndexTables {
            let config = Self::no_ignore_config(root);
            IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
                .expect("build tables")
        }
    }

    struct PostingCounts;

    impl PostingCounts {
        fn file_occurrences(postings: &[u8], lexicon: &[LexiconEntry], file_id: u32) -> usize {
            lexicon
                .iter()
                .map(|entry| {
                    let start = usize::try_from(entry.offset).unwrap_or(usize::MAX);
                    let end = lexicon
                        .iter()
                        .find(|e| e.offset > entry.offset)
                        .map_or(postings.len(), |e| {
                            usize::try_from(e.offset).unwrap_or(usize::MAX)
                        });
                    let slice = &postings[start..end];
                    Postings::decode_sorted(slice)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|&id| id == file_id)
                        .count()
                })
                .sum()
        }
    }

    struct FilterCorpus;

    impl FilterCorpus {
        fn write(root: &Path) {
            fs::create_dir_all(root.join("skip")).expect("mkdir skip");
            fs::create_dir_all(root.join("also_skip")).expect("mkdir also_skip");
            fs::write(root.join("keep.txt"), "beta\n").expect("write keep");
            fs::write(root.join("skip/ignored.txt"), "beta\n").expect("write skip");
            fs::write(root.join("also_skip/omit.txt"), "beta\n").expect("write omit");
            fs::write(root.join(".gitignore"), "skip/**\n").expect("write gitignore");
            fs::write(root.join(".ignore"), "also_skip/**\n").expect("write ignore");
        }
    }

    struct FilterParity;

    impl FilterParity {
        fn filter_config(build: &IndexConfig<'_>) -> FileFilterConfig {
            FileFilterConfig {
                exclude_paths: build.corpus.exclude_paths.to_vec(),
                visibility: build.visibility.clone(),
                follow_links: build.corpus.follow_links,
                ..FileFilterConfig::default()
            }
        }

        fn all_corpus_files(root: &Path) -> Vec<PathBuf> {
            let mut files = Vec::new();
            Self::collect_files_recursive(root, root, &mut files);
            files.sort_unstable();
            files
        }

        fn collect_files_recursive(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
            let entries = fs::read_dir(dir).expect("read dir");
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::collect_files_recursive(root, &path, out);
                } else if path.is_file() {
                    let rel = path.strip_prefix(root).expect("under root").to_path_buf();
                    out.push(rel);
                }
            }
        }
    }

    #[test]
    fn build_index_tables_sorts_file_paths_deterministically() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join("z.txt"), "hello\n").expect("write z");
        fs::write(tmp.path().join("a.txt"), "world\n").expect("write a");
        fs::write(tmp.path().join("m.txt"), "test\n").expect("write m");

        let tables = TablesFixture::build(tmp.path());
        let paths: Vec<PathBuf> = tables
            .fingerprints
            .iter()
            .map(|fp| fp.path.clone())
            .collect();
        let expected = vec![
            PathBuf::from("a.txt"),
            PathBuf::from("m.txt"),
            PathBuf::from("z.txt"),
        ];
        assert_eq!(paths, expected);
    }

    #[test]
    fn build_index_tables_exclude_paths_prevents_indexing() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join("keep.txt"), "hello\n").expect("write keep");
        let excluded_dir = tmp.path().join("excluded");
        fs::create_dir_all(&excluded_dir).expect("create excluded dir");
        fs::write(excluded_dir.join("skip.txt"), "world\n").expect("write skip");

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[PathBuf::from("excluded")],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig {
                ignore: IgnoreConfig::disabled(),
                ..Default::default()
            },
        };
        let tables = IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
            .expect("build tables");
        assert_eq!(tables.fingerprints.len(), 1);
        assert_eq!(tables.fingerprints[0].path, PathBuf::from("keep.txt"));
    }

    #[test]
    fn build_index_tables_duplicate_trigrams_deduplicated_in_postings() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join("a.txt"), "ababab\n").expect("write file");
        fs::write(tmp.path().join("b.txt"), "ababab\n").expect("write file");

        let tables = TablesFixture::build(tmp.path());
        assert_eq!(tables.fingerprints.len(), 2);

        let unique_trigrams = 3;
        assert_eq!(
            PostingCounts::file_occurrences(&tables.postings, &tables.lexicon, 0),
            unique_trigrams
        );
        assert_eq!(
            PostingCounts::file_occurrences(&tables.postings, &tables.lexicon, 1),
            unique_trigrams
        );
    }

    #[test]
    fn corpus_walk_excludes_gitignored_paths_without_git_repo() {
        let tmp = TempDir::new().expect("create temp dir");
        FilterCorpus::write(tmp.path());

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        let paths: Vec<_> = FileWalk::new(config.corpus.root)
            .visibility(config.visibility.clone())
            .links(LinkTraversal::DoNotFollow)
            .files()
            .expect("walk corpus")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();
        assert!(paths.iter().any(|p| p == Path::new("keep.txt")));
        assert!(!paths.iter().any(|p| p.starts_with("skip")));
        assert!(!paths.iter().any(|p| p.starts_with("also_skip")));
    }

    #[test]
    fn corpus_walk_with_empty_ignore_sources_includes_gitignored() {
        let tmp = TempDir::new().expect("create temp dir");
        FilterCorpus::write(tmp.path());

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig {
                ignore: IgnoreConfig::disabled(),
                ..Default::default()
            },
        };
        let paths: Vec<_> = FileWalk::new(config.corpus.root)
            .visibility(config.visibility.clone())
            .links(LinkTraversal::DoNotFollow)
            .files()
            .expect("walk corpus")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();
        assert!(paths.iter().any(|p| p.starts_with("skip")));
        assert!(paths.iter().any(|p| p.starts_with("also_skip")));
    }

    #[test]
    fn corpus_walk_agrees_with_candidate_filter() {
        let tmp = TempDir::new().expect("create temp dir");
        FilterCorpus::write(tmp.path());

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        let indexed: Vec<_> = FileWalk::new(config.corpus.root)
            .visibility(config.visibility.clone())
            .links(LinkTraversal::DoNotFollow)
            .files()
            .expect("walk corpus")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();
        let filter =
            FileFilter::new(&FilterParity::filter_config(&config), tmp.path()).expect("filter");

        for rel in FilterParity::all_corpus_files(tmp.path()) {
            let should_index = filter.matches_path(&rel);
            let is_indexed = indexed.iter().any(|p| p == &rel);
            assert_eq!(
                is_indexed, should_index,
                "path {rel:?}: indexed={is_indexed} filter={should_index}"
            );
        }
    }

    #[test]
    fn build_index_tables_include_paths_filters_to_single_file() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join("keep.txt"), "hello\n").expect("write keep");
        fs::write(tmp.path().join("skip.txt"), "world\n").expect("write skip");

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[PathBuf::from("keep.txt")],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig {
                ignore: IgnoreConfig::disabled(),
                ..Default::default()
            },
        };
        let tables = IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
            .expect("build tables");
        assert_eq!(tables.fingerprints.len(), 1);
        assert_eq!(tables.fingerprints[0].path, PathBuf::from("keep.txt"));
    }

    #[test]
    fn build_single_file_corpus_ignores_siblings() {
        let tmp = TempDir::new().expect("create temp dir");
        let file = tmp.path().join("only.txt");
        fs::write(&file, "needle\n").expect("write file");
        fs::write(tmp.path().join("other.txt"), "haystack\n").expect("write other");

        let only_txt = PathBuf::from("only.txt");
        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::SingleFile,
                follow_links: false,
                include_paths: &[only_txt],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        let tables = IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
            .expect("build tables");
        assert_eq!(tables.fingerprints.len(), 1);
        assert_eq!(
            tables.fingerprints[0].path,
            PathBuf::from("only.txt"),
            "should only index the specified file, not siblings"
        );
    }

    #[test]
    fn fingerprints_have_nonzero_size() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join("a.txt"), "hello world\n").expect("write file");

        let tables = TablesFixture::build(tmp.path());
        assert_eq!(tables.fingerprints.len(), 1);
        assert!(
            tables.fingerprints[0].size > 0,
            "fingerprint should capture file size"
        );
    }

    #[test]
    fn build_respects_gitignore_by_default() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::write(tmp.path().join(".gitignore"), "*.ignored\n").expect("write gitignore");
        fs::write(tmp.path().join("keep.txt"), "hello\n").expect("write keep");
        fs::write(tmp.path().join("skip.ignored"), "secret\n").expect("write ignored");

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        let tables = IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
            .expect("build tables");
        let paths: Vec<_> = tables.fingerprints.iter().map(|f| f.path.clone()).collect();
        assert!(
            !paths.iter().any(|p| p == "skip.ignored"),
            "gitignored file must not be indexed, got {paths:?}"
        );
        assert!(
            paths.iter().any(|p| p == "keep.txt"),
            "keep.txt must be indexed, got {paths:?}"
        );
    }

    #[test]
    fn build_prunes_gitignored_directory() {
        let tmp = TempDir::new().expect("create temp dir");
        fs::create_dir_all(tmp.path().join("src")).expect("mkdir src");
        fs::create_dir_all(tmp.path().join("target")).expect("mkdir target");
        fs::write(tmp.path().join(".gitignore"), "/target\n").expect("write gitignore");
        fs::write(tmp.path().join("src/keep.txt"), "needle\n").expect("write keep");
        fs::write(tmp.path().join("target/ignored.txt"), "secret\n").expect("write ignored");

        let config = IndexConfig {
            corpus: CorpusSpec {
                root: tmp.path(),
                kind: CorpusKind::Directory,
                follow_links: false,
                include_paths: &[],
                exclude_paths: &[],
            },
            walk: IndexWalkConfig::new(false),
            visibility: VisibilityConfig::default(),
        };
        let tables = IndexTables::build(GramWidth::TRIGRAM, GramNorm::Identity, &config, &[])
            .expect("build tables");
        let paths: Vec<_> = tables.fingerprints.iter().map(|f| f.path.clone()).collect();
        assert!(
            paths.iter().any(|p| p == Path::new("src/keep.txt")),
            "keep must be indexed, got {paths:?}"
        );
        assert!(
            !paths.iter().any(|p| p.starts_with("target")),
            "target/ must not be indexed, got {paths:?}"
        );
    }
}
