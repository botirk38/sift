//! Trigram index: build, load, search.

mod builder;
pub mod files;
pub mod trigram;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

pub use builder::build_index_tables;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    pub pattern: String,
    pub mode: &'static str,
}

/// In-memory trigram index backed by memory-mapped storage.
///
/// All data is accessed zero-copy from mapped files. Opening an index is cheap
/// — just memory-mapping the three index files, no deserialization.
#[derive(Debug)]
pub struct Index {
    pub root: PathBuf,
    files: files::MappedFilesView,
    file_paths: Vec<PathBuf>,
    lexicon: crate::storage::lexicon::MappedLexicon,
    postings: crate::storage::postings::MappedPostings,
    pub index_dir: Option<PathBuf>,
}

impl Index {
    /// Open an index directory produced by [`IndexBuilder::build`].
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::MissingMeta`] if `sift.meta` is absent,
    /// [`crate::Error::InvalidMeta`] if metadata is empty or malformed,
    /// [`crate::Error::MissingComponent`] if a trigram table file is missing,
    /// or [`crate::Error::Io`] on read/mmap failure.
    pub fn open(path: &Path) -> crate::Result<Self> {
        let sift_dir = path.to_path_buf();
        let index_dir = sift_dir.join(crate::INDEX_SUBDIR);
        let meta_path = sift_dir.join(crate::META_FILENAME);
        if !meta_path.is_file() {
            return Err(crate::Error::MissingMeta(meta_path));
        }
        let raw = std::fs::read_to_string(&meta_path)?;
        let line = raw
            .lines()
            .next()
            .ok_or_else(|| crate::Error::InvalidMeta(meta_path.clone()))?;
        let root = PathBuf::from(line);
        let paths = [
            index_dir.join(crate::FILES_BIN),
            index_dir.join(crate::LEXICON_BIN),
            index_dir.join(crate::POSTINGS_BIN),
        ];
        for p in &paths {
            if !p.is_file() {
                return Err(crate::Error::MissingComponent(p.clone()));
            }
        }

        let files = files::MappedFilesView::open(&paths[0]).map_err(crate::Error::Io)?;
        let file_paths = files.to_path_bufs().map_err(crate::Error::Io)?;
        let lexicon =
            crate::storage::lexicon::MappedLexicon::open(&paths[1]).map_err(crate::Error::Io)?;
        let postings =
            crate::storage::postings::MappedPostings::open(&paths[2]).map_err(crate::Error::Io)?;

        Ok(Self {
            root,
            files,
            file_paths,
            lexicon,
            postings,
            index_dir: Some(sift_dir),
        })
    }

    /// Persist the in-memory index to `dir`.
    ///
    /// # Errors
    ///
    /// Propagates IO errors from creating directories or writing files.
    pub fn save_to_dir(&self, dir: &Path) -> crate::Result<()> {
        std::fs::create_dir_all(dir)?;
        let meta_path = dir.join(crate::META_FILENAME);
        std::fs::write(&meta_path, format!("{}\n", self.root.display()))?;

        let index_dir = dir.join(crate::INDEX_SUBDIR);
        std::fs::create_dir_all(&index_dir)?;
        std::fs::write(index_dir.join(crate::FILES_BIN), self.files.backing_slice())
            .map_err(crate::Error::Io)?;
        std::fs::write(
            index_dir.join(crate::LEXICON_BIN),
            self.lexicon.backing_slice(),
        )
        .map_err(crate::Error::Io)?;
        std::fs::write(
            index_dir.join(crate::POSTINGS_BIN),
            self.postings.backing_slice(),
        )
        .map_err(crate::Error::Io)?;
        Ok(())
    }

    #[must_use]
    pub fn index_dir(&self) -> Option<&Path> {
        self.index_dir.as_deref()
    }

    #[must_use]
    pub fn explain(&self, pattern: &str) -> QueryPlan {
        let mode = match crate::planner::TrigramPlan::for_patterns(
            &[pattern.to_string()],
            &crate::SearchOptions::default(),
        ) {
            crate::planner::TrigramPlan::FullScan => "full_scan",
            crate::planner::TrigramPlan::Narrow { .. } => "indexed_candidates",
        };
        QueryPlan {
            pattern: pattern.to_string(),
            mode,
        }
    }

    #[must_use]
    pub fn posting_bytes_slice(&self, tri: [u8; 3]) -> &[u8] {
        let Some(entry) = self.lexicon.get(tri) else {
            return &[];
        };
        let start = usize::try_from(entry.offset).unwrap_or(usize::MAX);
        let n = usize::try_from(entry.len).unwrap_or(usize::MAX);
        let nbytes = n.saturating_mul(4);
        self.postings.slice(start, nbytes)
    }

    /// Get sorted file IDs for a trigram. Materializes from mapped bytes.
    ///
    /// # Panics
    ///
    /// Panics if postings data for this trigram is corrupted.
    #[must_use]
    pub fn posting_list_for_trigram(&self, tri: [u8; 3]) -> Vec<u32> {
        let slice = self.posting_bytes_slice(tri);
        if !slice.len().is_multiple_of(4) {
            return Vec::new();
        }
        slice
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }

    #[must_use]
    pub fn candidate_file_ids(&self, arms: &[crate::planner::Arm]) -> Vec<u32> {
        let mut id_lists: Vec<Vec<u32>> = Vec::with_capacity(arms.len());
        for arm in arms {
            if arm.is_empty() {
                continue;
            }
            let slices: Vec<&[u8]> = arm
                .iter()
                .map(|tri| self.posting_bytes_slice(*tri))
                .collect();
            if slices.iter().any(|s| s.is_empty()) {
                continue;
            }
            let ids = intersect_sorted_posting_byte_slices(&slices);
            if !ids.is_empty() {
                id_lists.push(ids);
            }
        }
        let refs: Vec<&[u32]> = id_lists.iter().map(Vec::as_slice).collect();
        union_sorted_runs(&refs)
    }

    #[must_use]
    pub fn candidate_paths(&self, arms: &[crate::planner::Arm]) -> Vec<PathBuf> {
        self.candidate_file_ids(arms)
            .iter()
            .filter_map(|&id| self.file_paths.get(id as usize).cloned())
            .collect()
    }

    #[must_use]
    pub fn file_path(&self, id: usize) -> Option<&Path> {
        self.file_paths.get(id).map(PathBuf::as_path)
    }

    #[must_use]
    pub const fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn iter_files(&self) -> impl Iterator<Item = &Path> {
        self.file_paths.iter().map(PathBuf::as_path)
    }
}

fn intersect_sorted_posting_byte_slices(slices: &[&[u8]]) -> Vec<u32> {
    if slices.is_empty() {
        return Vec::new();
    }
    if slices.len() == 1 {
        return u32_vec_from_le_bytes(slices[0]);
    }
    let mut cur = intersect_two_posting_bytes(slices[0], slices[1]);
    for s in &slices[2..] {
        cur = intersect_vec_with_posting_bytes(&cur, s);
        if cur.is_empty() {
            break;
        }
    }
    cur
}

fn intersect_two_posting_bytes(a: &[u8], b: &[u8]) -> Vec<u32> {
    if !a.len().is_multiple_of(4) || !b.len().is_multiple_of(4) {
        return Vec::new();
    }
    let an = a.len() / 4;
    let bn = b.len() / 4;
    let mut i = 0usize;
    let mut j = 0usize;
    let mut out = Vec::new();
    while i < an && j < bn {
        let ai = u32::from_le_bytes(a[i * 4..i * 4 + 4].try_into().unwrap());
        let bj = u32::from_le_bytes(b[j * 4..j * 4 + 4].try_into().unwrap());
        match ai.cmp(&bj) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                out.push(ai);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn intersect_vec_with_posting_bytes(cur: &[u32], b: &[u8]) -> Vec<u32> {
    if !b.len().is_multiple_of(4) {
        return Vec::new();
    }
    let bn = b.len() / 4;
    let mut i = 0usize;
    let mut j = 0usize;
    let mut out = Vec::new();
    while i < cur.len() && j < bn {
        let bj = u32::from_le_bytes(b[j * 4..j * 4 + 4].try_into().unwrap());
        match cur[i].cmp(&bj) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                out.push(cur[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

fn u32_vec_from_le_bytes(slice: &[u8]) -> Vec<u32> {
    if !slice.len().is_multiple_of(4) {
        return Vec::new();
    }
    slice
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn union_sorted_runs(lists: &[&[u32]]) -> Vec<u32> {
    if lists.is_empty() {
        return Vec::new();
    }
    let total: usize = lists.iter().map(|s| s.len()).sum();
    let mut all: Vec<u32> = Vec::with_capacity(total);
    for s in lists {
        all.extend_from_slice(s);
    }
    all.sort_unstable();
    all.dedup();
    all
}

pub struct IndexBuilder<'a> {
    root: &'a Path,
    dir: Option<PathBuf>,
}

impl<'a> IndexBuilder<'a> {
    #[must_use]
    pub const fn new(root: &'a Path) -> Self {
        Self { root, dir: None }
    }

    #[must_use]
    pub fn with_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.dir = Some(dir.into());
        self
    }

    /// Walk `root`, extract trigrams, and return an in-memory [`Index`].
    ///
    /// # Errors
    ///
    /// Propagates IO errors from walking, reading files, or writing persistence files
    /// (if `with_dir` was called).
    pub fn build(self) -> crate::Result<Index> {
        let root = self.root.canonicalize()?;
        let tables = build_index_tables(&root)?;

        let files = files::MappedFilesView::from_paths(&tables.files);
        let lexicon = crate::storage::lexicon::MappedLexicon::from_entries(&tables.lexicon);
        let postings = crate::storage::postings::MappedPostings::from_bytes(&tables.postings);

        let mut index = Index {
            root,
            files,
            file_paths: tables.files,
            lexicon,
            postings,
            index_dir: None,
        };

        if let Some(dir) = self.dir {
            index.index_dir = Some(dir.clone());
            index.save_to_dir(&dir)?;
        }
        Ok(index)
    }
}
