use std::path::{Path, PathBuf};

use crate::corpus::walk::LinkTraversal;
use crate::corpus::walk::{FileWalk, WalkFile};
use crate::index::contract::IndexWrite;
use crate::index::snapshot::ArtifactData;
use crate::index::{CorpusKind, FileId, IndexConfig, IndexDestination, IndexSource};

use super::build::{FingerprintCollector, IndexTables, PostingTables};
use super::files::FileTable;
use super::gram::GramWidth;
use super::index::{Index, IndexedFiles, IndexedFilesLocation, NGramIndexError, Storage};
use super::storage::grams::{GramSet, GramSets};
use super::storage::lexicon::Lexicon;
use super::storage::postings::Postings;

impl Index {
    /// Build an N-gram index from an explicit path list, or from a full walk when `paths` is empty.
    ///
    /// # Errors
    ///
    /// Returns an error if corpus walking, extraction, or encoding fails.
    pub fn build(
        &self,
        config: &IndexConfig<'_>,
        output_dir: &Path,
        paths: &[PathBuf],
    ) -> crate::Result<Self> {
        self.build_into(config, IndexDestination::Directory(output_dir), paths)
    }

    /// Build an N-gram index into a directory or snapshot destination.
    pub(crate) fn build_into(
        &self,
        config: &IndexConfig<'_>,
        dest: IndexDestination<'_>,
        paths: &[PathBuf],
    ) -> crate::Result<Self> {
        let tables = IndexTables::build(self.width, self.norm, config, paths)?;
        let root = config.corpus.root.canonicalize()?;
        self.persist_tables(&tables, &root, config.corpus.kind, dest)
    }

    /// Encode and store tables at the given destination, returning a live index.
    pub(crate) fn persist_tables(
        &self,
        tables: &IndexTables,
        root: &Path,
        corpus_kind: CorpusKind,
        dest: IndexDestination<'_>,
    ) -> crate::Result<Self> {
        match dest {
            IndexDestination::Directory(dir) => self.create_in_dir(tables, root, corpus_kind, dir),
            IndexDestination::Snapshot { writer, namespace } => {
                let ((fr, lr), (pr, gr)) = rayon::join(
                    || {
                        rayon::join(
                            || FileTable::encode(&tables.fingerprints),
                            || Lexicon::encode(self.width, &tables.lexicon),
                        )
                    },
                    || {
                        rayon::join(
                            || Postings::encode(&tables.postings),
                            || GramSets::encode(self.width, &tables.file_grams),
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
                    self.width,
                )?;
                let postings =
                    Postings::from_artifact(ArtifactData::Memory(postings_bytes.clone().into()))?;
                let gram_sets = GramSets::from_artifact(
                    ArtifactData::Memory(gram_sets_bytes.clone().into()),
                    self.width,
                )?;

                writer.put_artifact(namespace, crate::FILES_BIN, files_bytes)?;
                writer.put_artifact(namespace, crate::LEXICON_BIN, lexicon_bytes)?;
                writer.put_artifact(namespace, crate::POSTINGS_BIN, postings_bytes)?;
                writer.put_artifact(namespace, crate::GRAMS_BIN, gram_sets_bytes)?;

                Self::validate_file_paths(&tables.fingerprints)?;
                Self::validate_lexicon_postings(&lexicon, &postings)?;

                Ok(self.with_storage(Storage::new(
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
                )))
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
        index_dir: &Path,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Self> {
        Self::new()
            .width(width)
            .open_from(IndexSource::Directory(index_dir), root, corpus_kind)
    }

    pub(crate) fn open_from(
        &self,
        source: IndexSource<'_>,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Self> {
        self.open_tables(source, root, corpus_kind)
    }

    /// Open index tables from a storage source (directory or snapshot).
    pub(crate) fn open_tables(
        &self,
        source: IndexSource<'_>,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Self> {
        match source {
            IndexSource::Directory(dir) => {
                let files_path = dir.join(crate::FILES_BIN);
                let lexicon_path = dir.join(crate::LEXICON_BIN);
                let postings_path = dir.join(crate::POSTINGS_BIN);
                let grams_path = dir.join(crate::GRAMS_BIN);

                for p in [&files_path, &lexicon_path, &postings_path, &grams_path] {
                    if !p.is_file() {
                        return Err(NGramIndexError::MissingComponent(p.clone()).into());
                    }
                }

                let files = FileTable::open(&files_path).map_err(NGramIndexError::Io)?;
                let indexed_files = IndexedFiles::new(IndexedFilesLocation::Disk(files))
                    .map_err(NGramIndexError::Io)?;

                let lexicon =
                    Lexicon::open(&lexicon_path, self.width).map_err(NGramIndexError::Io)?;
                let postings = Postings::open(&postings_path).map_err(NGramIndexError::Io)?;
                let gram_sets =
                    GramSets::open(&grams_path, self.width).map_err(NGramIndexError::Io)?;

                Ok(self.with_storage(Storage::new(
                    root.to_path_buf(),
                    indexed_files,
                    gram_sets,
                    lexicon,
                    postings,
                    corpus_kind,
                )))
            }
            IndexSource::Snapshot { reader, namespace } => {
                let files_data = reader.artifact(namespace, crate::FILES_BIN)?;
                let files = FileTable::from_artifact(files_data).map_err(NGramIndexError::Io)?;
                let indexed_files = IndexedFiles::new(IndexedFilesLocation::Disk(files))
                    .map_err(NGramIndexError::Io)?;

                let lexicon_data = reader.artifact(namespace, crate::LEXICON_BIN)?;
                let lexicon = Lexicon::from_artifact(lexicon_data, self.width)
                    .map_err(NGramIndexError::Io)?;

                let postings_data = reader.artifact(namespace, crate::POSTINGS_BIN)?;
                let postings =
                    Postings::from_artifact(postings_data).map_err(NGramIndexError::Io)?;

                let gram_sets_data = reader.artifact(namespace, crate::GRAMS_BIN)?;
                let gram_sets = GramSets::from_artifact(gram_sets_data, self.width)
                    .map_err(NGramIndexError::Io)?;

                Ok(self.with_storage(Storage::new(
                    root.to_path_buf(),
                    indexed_files,
                    gram_sets,
                    lexicon,
                    postings,
                    corpus_kind,
                )))
            }
        }
    }

    /// Write tables to `dir` as persistence files and return an mmap-backed index.
    fn create_in_dir(
        &self,
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
                    || Lexicon::create(&lexicon_path, self.width, &tables.lexicon),
                )
            },
            || {
                rayon::join(
                    || Postings::create(&postings_path, &tables.postings),
                    || GramSets::create(&grams_path, self.width, &tables.file_grams),
                )
            },
        );

        let files = fr.map_err(crate::Error::Io)?;
        let lexicon = lr.map_err(crate::Error::Io)?;
        let postings = pr.map_err(crate::Error::Io)?;
        let gram_sets = gr.map_err(crate::Error::Io)?;

        Self::validate_file_paths(&tables.fingerprints)?;
        Self::validate_lexicon_postings(&lexicon, &postings)?;

        Ok(self.with_storage(Storage::new(
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
        )))
    }

    /// Update the index from the current corpus, writing artifact files into `output_dir`.
    ///
    /// Returns `Ok(Some(index))` if a new index was written, or `Ok(None)` if no files changed.
    ///
    /// # Errors
    ///
    /// Returns an error if corpus walking, extraction, or encoding fails.
    pub fn update(
        &self,
        config: &IndexConfig<'_>,
        output_dir: &Path,
        paths: &[PathBuf],
    ) -> crate::Result<Option<Self>> {
        self.rebuild(config, IndexDestination::Directory(output_dir), paths)
    }

    /// Rebuild index tables for changed files and persist to `dest`.
    pub(crate) fn rebuild(
        &self,
        config: &IndexConfig<'_>,
        dest: IndexDestination<'_>,
        paths: &[PathBuf],
    ) -> crate::Result<Option<Self>> {
        use rayon::prelude::*;
        use std::collections::HashMap;

        let Some(storage) = self.storage() else {
            return self.build_into(config, dest, paths).map(Some);
        };

        let fingerprints = if paths.is_empty() {
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
            FingerprintCollector::new(config.corpus.root, &corpus_paths).collect()?
        } else {
            Self::merge_partial_fingerprints(storage.files.as_slice(), config.corpus.root, paths)?
        };

        if fingerprints == storage.files.as_slice() {
            return Ok(None);
        }

        let prev_id_by_fp: HashMap<(&Path, i64, u64), usize> = storage
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
                    return storage.gram_sets.get(prev_id).map_err(crate::Error::Io);
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
        self.persist_tables(&tables, &root, config.corpus.kind, dest)
            .map(Some)
    }
}

impl crate::index::contract::Index for Index {
    fn kind(&self) -> &'static str {
        Self::kind(self)
    }

    fn params(&self) -> serde_json::Value {
        Self::params(self)
    }

    fn name(&self) -> String {
        Self::name(self)
    }

    fn build(&self, write: IndexWrite<'_>) -> crate::Result<()> {
        self.build_into(write.config, write.dest, write.paths)
            .map(|_| ())
    }

    fn open(
        &self,
        source: IndexSource<'_>,
        root: &Path,
        corpus_kind: CorpusKind,
    ) -> crate::Result<Box<dyn crate::index::contract::Index>> {
        Ok(Box::new(self.open_from(source, root, corpus_kind)?))
    }

    fn query(&self, query: &crate::search::SearchQuery) -> Vec<FileId> {
        Self::query_file_ids(self, query)
    }

    fn coverage(&self) -> crate::index::IndexedCorpus {
        Self::coverage(self)
    }

    fn candidate(&self, id: FileId) -> Option<crate::corpus::Candidate> {
        Self::candidate(self, id)
    }

    fn all_file_ids(&self) -> Vec<FileId> {
        Self::all_file_ids(self)
    }

    fn update(&self, write: IndexWrite<'_>) -> crate::Result<bool> {
        self.rebuild(write.config, write.dest, write.paths)
            .map(|updated| updated.is_some())
    }
}
