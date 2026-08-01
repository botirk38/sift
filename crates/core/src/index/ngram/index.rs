use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::corpus::Candidate;
use crate::corpus::walk::LinkTraversal;
use crate::corpus::walk::{FileWalk, WalkFile};
use crate::index::snapshot::ArtifactData;
use crate::index::{CorpusKind, FileId, IndexConfig, IndexDestination, IndexedCorpus};
use crate::search::{Case, Query};

use super::build::{FingerprintCollector, IndexTables, PostingTables};
use super::files::{FileFingerprint, FileTable};
use super::gram::{GramMatch, GramNorm, GramWidth};
use super::storage::grams::{GramSet, GramSets};
use super::storage::lexicon::Lexicon;
use super::storage::postings::Postings;

/// Errors specific to opening or persisting an N-gram index.
#[derive(Debug, thiserror::Error)]
pub enum NGramIndexError {
    #[error("index component missing: {0}")]
    MissingComponent(PathBuf),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Opened runtime-width N-gram index.
#[derive(Debug)]
pub struct Index {
    pub(crate) width: GramWidth,
    pub(crate) norm: GramNorm,
    pub(crate) storage: Storage,
}

#[derive(Debug)]
pub struct Storage {
    pub(crate) root: PathBuf,
    pub(crate) files: IndexedFiles,
    pub(crate) gram_sets: GramSets,
    pub(crate) lexicon: Lexicon,
    pub(crate) postings: Postings,
    pub(crate) corpus_kind: CorpusKind,
}

#[derive(Debug)]
pub struct IndexedFiles {
    table: FileTable,
    fingerprints: OnceLock<Vec<FileFingerprint>>,
    coverage: OnceLock<IndexedCorpus>,
}

/// Whether [`IndexedFiles`] is loaded from disk or already in memory.
pub enum IndexedFilesLocation {
    /// `files.bin` on disk; fingerprints decode on first use.
    Disk(FileTable),
    /// Fingerprints already in memory (just-built index).
    Memory {
        table: FileTable,
        fingerprints: Vec<FileFingerprint>,
    },
}

impl IndexedFiles {
    pub(crate) fn new(location: IndexedFilesLocation) -> std::io::Result<Self> {
        match location {
            IndexedFilesLocation::Disk(table) => {
                table.validate_paths()?;
                Ok(Self {
                    table,
                    fingerprints: OnceLock::new(),
                    coverage: OnceLock::new(),
                })
            }
            IndexedFilesLocation::Memory {
                table,
                fingerprints,
            } => {
                let decoded = OnceLock::new();
                let _ = decoded.set(fingerprints);
                Ok(Self {
                    table,
                    fingerprints: decoded,
                    coverage: OnceLock::new(),
                })
            }
        }
    }

    fn fingerprints(&self) -> &[FileFingerprint] {
        self.fingerprints.get_or_init(|| {
            self.table
                .to_fingerprints()
                .expect("paths validated at open")
        })
    }

    pub(crate) fn as_slice(&self) -> &[FileFingerprint] {
        self.fingerprints()
    }

    pub(crate) fn get(&self, id: FileId) -> Option<&FileFingerprint> {
        self.fingerprints().get(id.get())
    }

    /// Borrow a single file row without decoding the full fingerprint table.
    pub(crate) fn row(&self, id: FileId) -> Option<super::files::FileRow<'_>> {
        self.table.row(id.get()).ok()
    }

    pub(crate) const fn len(&self) -> usize {
        self.table.len()
    }

    pub(crate) fn coverage(&self) -> IndexedCorpus {
        self.coverage
            .get_or_init(|| {
                IndexedCorpus::new(self.fingerprints().iter().map(|fp| fp.path.clone()))
            })
            .clone()
    }
}

impl Storage {
    pub(crate) const fn new(
        root: PathBuf,
        files: IndexedFiles,
        gram_sets: GramSets,
        lexicon: Lexicon,
        postings: Postings,
        corpus_kind: CorpusKind,
    ) -> Self {
        Self {
            root,
            files,
            gram_sets,
            lexicon,
            postings,
            corpus_kind,
        }
    }
}

impl Index {
    const fn from_parts(width: GramWidth, norm: GramNorm, storage: Storage) -> Self {
        Self {
            width,
            norm,
            storage,
        }
    }

    #[must_use]
    pub const fn gram_width(&self) -> GramWidth {
        self.width
    }

    #[must_use]
    pub const fn gram_norm(&self) -> GramNorm {
        self.norm
    }

    #[must_use]
    pub fn file_path(&self, id: FileId) -> Option<&Path> {
        self.storage.files.get(id).map(|fp| fp.path.as_path())
    }

    #[must_use]
    pub fn file_abs_path(&self, id: FileId) -> Option<PathBuf> {
        self.storage
            .files
            .get(id)
            .map(|fp| self.storage.root.join(&fp.path))
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.storage.root
    }

    #[must_use]
    pub const fn corpus_kind(&self) -> CorpusKind {
        self.storage.corpus_kind
    }

    /// Resolve candidate file ids for the query. Falls back to every indexed
    /// file when the query cannot be narrowed.
    ///
    /// `AsciiLower` indexes only narrow case-insensitive queries (Exact on
    /// folded grams). Case-sensitive queries cannot be proven from a folded
    /// lexicon, so they return every covered id.
    #[must_use]
    pub(crate) fn query_file_ids(&self, query: &Query) -> Vec<FileId> {
        let all_ids = || {
            (0..self.storage.files.len())
                .map(FileId::new)
                .collect::<Vec<_>>()
        };
        let Some(arms) = Self::extract_literal_arms(self.width, query) else {
            return all_ids();
        };
        let (arms, gram_match) = match (self.norm, query.case()) {
            (GramNorm::Identity, Case::Insensitive) => (arms, GramMatch::AsciiCase),
            (GramNorm::Identity, Case::Sensitive) => (arms, GramMatch::Exact),
            (GramNorm::AsciiLower, Case::Sensitive) => return all_ids(),
            (GramNorm::AsciiLower, Case::Insensitive) => {
                let folded = arms
                    .into_iter()
                    .map(|arm| GramNorm::AsciiLower.normalize_literal(&arm))
                    .collect();
                (folded, GramMatch::Exact)
            }
        };
        let ids = self.candidate_file_ids(&arms, gram_match);
        ids.into_iter()
            .filter_map(|id| usize::try_from(id).ok().map(FileId::new))
            .collect()
    }

    /// Returns an explanation of how a query would be handled.
    #[must_use]
    pub fn explain(&self, query: &Query) -> crate::index::QueryPlanOutput {
        let mode = match Self::extract_literal_arms(self.width, query) {
            Some(_) => crate::index::PlanMode::IndexedCandidates,
            None => crate::index::PlanMode::FullScan,
        };
        crate::index::QueryPlanOutput {
            pattern: query.patterns().join("|"),
            mode,
        }
    }

    #[must_use]
    pub(crate) fn all_file_ids(&self) -> Vec<FileId> {
        (0..self.storage.files.len()).map(FileId::new).collect()
    }

    #[must_use]
    pub fn candidate(&self, id: FileId) -> Option<Candidate> {
        let row = self.storage.files.row(id)?;
        let rel = PathBuf::from(row.path);
        let abs = self.storage.root.join(&rel);
        Some(Candidate::with_metadata(rel, abs, Some(row.size), None))
    }

    #[must_use]
    pub(crate) fn coverage(&self) -> IndexedCorpus {
        self.storage.files.coverage()
    }

    pub(crate) fn validate_lexicon_postings(
        lexicon: &Lexicon,
        postings: &Postings,
    ) -> Result<(), NGramIndexError> {
        let payload_len = postings.payload_len();
        for entry in lexicon {
            let start = usize::try_from(entry.offset).map_err(|_| {
                NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} offset {} exceeds usize",
                        entry.gram, entry.offset
                    ),
                ))
            })?;
            let end = lexicon.posting_byte_end(entry.offset, payload_len);
            if start > end || end > payload_len {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} posting range [{start},{end}) exceeds payload_len {payload_len}",
                        entry.gram,
                    ),
                )));
            }
            let slice = postings.slice(start, end.saturating_sub(start));
            let decoded_count = Postings::decode_sorted(slice)
                .map_err(|e| {
                    NGramIndexError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("posting list for gram {:?}: {e}", entry.gram),
                    ))
                })?
                .len();
            if decoded_count != entry.len as usize {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "lexicon entry {:?} claims len {} but posting list has {decoded_count} entries",
                        entry.gram, entry.len,
                    ),
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn validate_file_paths(
        fingerprints: &[FileFingerprint],
    ) -> Result<(), NGramIndexError> {
        for fp in fingerprints {
            if fp.path.as_os_str().is_empty()
                || fp.path.is_absolute()
                || fp
                    .path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(NGramIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("invalid file path in index: {}", fp.path.display()),
                )));
            }
        }
        Ok(())
    }
}
impl Index {
    /// Build N-gram artifacts into `dest` from a full corpus scan under `config`.
    ///
    /// # Errors
    ///
    /// Returns an error if corpus walking, extraction, or encoding fails.
    pub fn build(
        width: GramWidth,
        norm: GramNorm,
        dest: IndexDestination<'_>,
        config: &IndexConfig<'_>,
    ) -> crate::Result<()> {
        let tables = IndexTables::build(width, norm, config, &[])?;
        let root = config.corpus.root.canonicalize()?;
        let _ = Self::persist_tables(width, norm, &tables, &root, config.corpus.kind, dest)?;
        Ok(())
    }

    /// Encode and store tables at the given destination, returning a live index.
    fn persist_tables(
        width: GramWidth,
        norm: GramNorm,
        tables: &IndexTables,
        root: &Path,
        corpus_kind: CorpusKind,
        dest: IndexDestination<'_>,
    ) -> crate::Result<Self> {
        match dest {
            IndexDestination::Directory(dir) => {
                Self::create_in_dir(width, norm, tables, root, corpus_kind, dir)
            }
            IndexDestination::Snapshot { writer, namespace } => {
                let ((fr, lr), (pr, gr)) = rayon::join(
                    || {
                        rayon::join(
                            || FileTable::encode(&tables.fingerprints),
                            || Lexicon::encode(width, &tables.lexicon),
                        )
                    },
                    || {
                        rayon::join(
                            || Postings::encode(&tables.postings),
                            || GramSets::encode(width, &tables.file_grams),
                        )
                    },
                );

                let files_bytes = fr.map_err(crate::Error::Io)?;
                let lexicon_bytes = lr.map_err(crate::Error::Io)?;
                let postings_bytes = pr.map_err(crate::Error::Io)?;
                let gram_sets_bytes = gr.map_err(crate::Error::Io)?;

                let files =
                    FileTable::from_artifact(ArtifactData::Memory(files_bytes.clone().into()))?;
                let lexicon = Lexicon::from_artifact(
                    ArtifactData::Memory(lexicon_bytes.clone().into()),
                    width,
                )?;
                let postings =
                    Postings::from_artifact(ArtifactData::Memory(postings_bytes.clone().into()))?;
                let gram_sets = GramSets::from_artifact(
                    ArtifactData::Memory(gram_sets_bytes.clone().into()),
                    width,
                )?;

                writer.put_artifact(namespace, crate::FILES_BIN, files_bytes)?;
                writer.put_artifact(namespace, crate::LEXICON_BIN, lexicon_bytes)?;
                writer.put_artifact(namespace, crate::POSTINGS_BIN, postings_bytes)?;
                writer.put_artifact(namespace, crate::GRAMS_BIN, gram_sets_bytes)?;

                Self::validate_file_paths(&tables.fingerprints)?;
                Self::validate_lexicon_postings(&lexicon, &postings)?;

                Ok(Self::from_parts(
                    width,
                    norm,
                    Storage::new(
                        root.to_path_buf(),
                        IndexedFiles::new(IndexedFilesLocation::Memory {
                            table: files,
                            fingerprints: tables.fingerprints.clone(),
                        })
                        .map_err(crate::Error::Io)?,
                        gram_sets,
                        lexicon,
                        postings,
                        corpus_kind,
                    ),
                ))
            }
        }
    }

    /// Open a previously persisted N-gram index from `index_dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if persistence files are missing or malformed.
    pub fn open(
        width: GramWidth,
        norm: GramNorm,
        index_dir: &Path,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Self> {
        let files_path = index_dir.join(crate::FILES_BIN);
        let lexicon_path = index_dir.join(crate::LEXICON_BIN);
        let postings_path = index_dir.join(crate::POSTINGS_BIN);
        let grams_path = index_dir.join(crate::GRAMS_BIN);

        for p in [&files_path, &lexicon_path, &postings_path, &grams_path] {
            if !p.is_file() {
                return Err(NGramIndexError::MissingComponent(p.clone()).into());
            }
        }

        let files = FileTable::open(&files_path).map_err(NGramIndexError::Io)?;
        let indexed_files =
            IndexedFiles::new(IndexedFilesLocation::Disk(files)).map_err(NGramIndexError::Io)?;

        let lexicon = Lexicon::open(&lexicon_path, width).map_err(NGramIndexError::Io)?;
        let postings = Postings::open(&postings_path).map_err(NGramIndexError::Io)?;
        let gram_sets = GramSets::open(&grams_path, width).map_err(NGramIndexError::Io)?;

        Ok(Self::from_parts(
            width,
            norm,
            Storage::new(
                root.to_path_buf(),
                indexed_files,
                gram_sets,
                lexicon,
                postings,
                corpus_kind,
            ),
        ))
    }

    /// Write tables to `dir` as persistence files and return an mmap-backed index.
    fn create_in_dir(
        width: GramWidth,
        norm: GramNorm,
        tables: &IndexTables,
        root: &Path,
        corpus_kind: CorpusKind,
        dir: &Path,
    ) -> crate::Result<Self> {
        std::fs::create_dir_all(dir)?;

        let files_path = dir.join(crate::FILES_BIN);
        let lexicon_path = dir.join(crate::LEXICON_BIN);
        let postings_path = dir.join(crate::POSTINGS_BIN);
        let grams_path = dir.join(crate::GRAMS_BIN);

        let ((fr, lr), (pr, gr)) = rayon::join(
            || {
                rayon::join(
                    || FileTable::create(&files_path, &tables.fingerprints),
                    || Lexicon::create(&lexicon_path, width, &tables.lexicon),
                )
            },
            || {
                rayon::join(
                    || Postings::create(&postings_path, &tables.postings),
                    || GramSets::create(&grams_path, width, &tables.file_grams),
                )
            },
        );

        let files = fr.map_err(crate::Error::Io)?;
        let lexicon = lr.map_err(crate::Error::Io)?;
        let postings = pr.map_err(crate::Error::Io)?;
        let gram_sets = gr.map_err(crate::Error::Io)?;

        Self::validate_file_paths(&tables.fingerprints)?;
        Self::validate_lexicon_postings(&lexicon, &postings)?;

        Ok(Self::from_parts(
            width,
            norm,
            Storage::new(
                root.to_path_buf(),
                IndexedFiles::new(IndexedFilesLocation::Memory {
                    table: files,
                    fingerprints: tables.fingerprints.clone(),
                })
                .map_err(crate::Error::Io)?,
                gram_sets,
                lexicon,
                postings,
                corpus_kind,
            ),
        ))
    }

    /// Rebuild index tables for changed files and persist to `dest`.
    ///
    /// Returns `Ok(true)` if artifacts were written, or `Ok(false)` if unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if corpus walking, extraction, or encoding fails.
    pub fn update(
        &self,
        dest: IndexDestination<'_>,
        config: &IndexConfig<'_>,
    ) -> crate::Result<bool> {
        self.rebuild(config, dest).map(|updated| updated.is_some())
    }

    fn rebuild(
        &self,
        config: &IndexConfig<'_>,
        dest: IndexDestination<'_>,
    ) -> crate::Result<Option<Self>> {
        use rayon::prelude::*;
        use std::collections::HashMap;

        let corpus_paths: Vec<PathBuf> = FileWalk::new(config.corpus.root)
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
            .collect();
        let fingerprints =
            FingerprintCollector::new(config.corpus.root, &corpus_paths).collect()?;

        if fingerprints == self.storage.files.as_slice() {
            return Ok(None);
        }

        let prev_id_by_fp: HashMap<(&Path, i64, u64), usize> = self
            .storage
            .files
            .as_slice()
            .iter()
            .enumerate()
            .map(|(id, fp)| ((fp.path.as_path(), fp.mtime_secs, fp.size), id))
            .collect();

        let file_grams: Vec<GramSet> = fingerprints
            .par_iter()
            .map(|fp| {
                if let Some(&prev_id) =
                    prev_id_by_fp.get(&(fp.path.as_path(), fp.mtime_secs, fp.size))
                {
                    return self
                        .storage
                        .gram_sets
                        .get(prev_id)
                        .map_err(crate::Error::Io);
                }
                let abs = config.corpus.root.join(&fp.path);
                std::fs::read(&abs)
                    .map(|bytes| GramSet::collect(self.width, &bytes, self.norm))
                    .map_err(crate::Error::Io)
            })
            .collect::<crate::Result<_>>()?;

        let postings = PostingTables::assemble(self.width, &file_grams)?;

        let tables = IndexTables {
            fingerprints,
            file_grams,
            lexicon: postings.lexicon,
            postings: postings.postings,
        };

        let root = config.corpus.root.canonicalize()?;
        Self::persist_tables(
            self.width,
            self.norm,
            &tables,
            &root,
            config.corpus.kind,
            dest,
        )
        .map(Some)
    }
}

impl crate::index::Index for Index {
    fn query(&self, query: &Query) -> Vec<FileId> {
        self.query_file_ids(query)
    }

    fn coverage(&self) -> IndexedCorpus {
        Self::coverage(self)
    }

    fn all_file_ids(&self) -> Vec<FileId> {
        Self::all_file_ids(self)
    }

    fn update(&self, dest: IndexDestination<'_>, config: &IndexConfig<'_>) -> crate::Result<bool> {
        Self::update(self, dest, config)
    }
}
