//! Trigram index: build, load, search.

mod builder;
pub mod files;
pub mod trigram;

use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::{Path, PathBuf};

pub use builder::build_index_tables;
use serde::{Deserialize, Serialize};

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
    pub corpus_kind: CorpusKind,
    files: files::MappedFilesView,
    file_paths: Vec<PathBuf>,
    lexicon: crate::storage::lexicon::MappedLexicon,
    postings: crate::storage::postings::MappedPostings,
    pub index_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CorpusKind {
    Directory,
    File { entries: Vec<PathBuf> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexMeta {
    pub root: PathBuf,
    #[serde(flatten)]
    pub kind: CorpusKind,
}

impl IndexMeta {
    const fn new(root: PathBuf, kind: CorpusKind) -> Self {
        Self { root, kind }
    }

    fn validate(self, meta_path: &Path) -> crate::Result<Self> {
        if !self.root.is_absolute() {
            return Err(crate::Error::InvalidMeta(meta_path.to_path_buf()));
        }
        match &self.kind {
            CorpusKind::Directory => {}
            CorpusKind::File { entries } => {
                if entries.len() != 1
                    || entries.iter().any(|entry| {
                        entry.as_os_str().is_empty()
                            || entry.is_absolute()
                            || entry.components().count() != 1
                    })
                {
                    return Err(crate::Error::InvalidMeta(meta_path.to_path_buf()));
                }
            }
        }
        Ok(self)
    }
}

fn validate_file_paths(
    kind: &CorpusKind,
    file_paths: &[PathBuf],
    meta_path: &Path,
) -> crate::Result<()> {
    match kind {
        CorpusKind::Directory => {
            if file_paths
                .iter()
                .any(|path| path.as_os_str().is_empty() || path.is_absolute())
            {
                return Err(crate::Error::InvalidMeta(meta_path.to_path_buf()));
            }
        }
        CorpusKind::File { entries } => {
            if file_paths != entries {
                return Err(crate::Error::InvalidMeta(meta_path.to_path_buf()));
            }
        }
    }
    Ok(())
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
        let meta = serde_json::from_str::<IndexMeta>(&raw)
            .map_err(|_| crate::Error::InvalidMeta(meta_path.clone()))?
            .validate(&meta_path)?;
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
        validate_file_paths(&meta.kind, &file_paths, &meta_path)?;
        let lexicon =
            crate::storage::lexicon::MappedLexicon::open(&paths[1]).map_err(crate::Error::Io)?;
        let postings =
            crate::storage::postings::MappedPostings::open(&paths[2]).map_err(crate::Error::Io)?;

        Ok(Self {
            root: meta.root,
            corpus_kind: meta.kind,
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
        let meta = IndexMeta::new(self.root.clone(), self.corpus_kind.clone());
        std::fs::write(
            &meta_path,
            serde_json::to_vec_pretty(&meta)
                .map_err(|_| crate::Error::InvalidMeta(meta_path.clone()))?,
        )?;

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
            let mut slices: Vec<&[u8]> = arm
                .iter()
                .map(|tri| self.posting_bytes_slice(*tri))
                .collect();
            if slices.iter().any(|s| s.is_empty()) {
                continue;
            }
            slices.sort_unstable_by_key(|slice| slice.len());
            let ids = intersect_sorted_posting_byte_slices(&slices);
            if !ids.is_empty() {
                id_lists.push(ids);
            }
        }
        merge_sorted_runs(id_lists)
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

fn merge_sorted_runs(lists: Vec<Vec<u32>>) -> Vec<u32> {
    if lists.is_empty() {
        return Vec::new();
    }
    if lists.len() == 1 {
        return lists.into_iter().next().unwrap_or_default();
    }

    let total: usize = lists.iter().map(Vec::len).sum();
    let mut heap: BinaryHeap<Reverse<(u32, usize)>> = BinaryHeap::with_capacity(lists.len());
    let mut positions = vec![0usize; lists.len()];

    for (list_idx, list) in lists.iter().enumerate() {
        if let Some(&first) = list.first() {
            heap.push(Reverse((first, list_idx)));
        }
    }

    let mut out = Vec::with_capacity(total);
    let mut last = None;
    while let Some(Reverse((value, list_idx))) = heap.pop() {
        if last != Some(value) {
            out.push(value);
            last = Some(value);
        }

        positions[list_idx] += 1;
        if let Some(&next) = lists[list_idx].get(positions[list_idx]) {
            heap.push(Reverse((next, list_idx)));
        }
    }
    out
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
        let canonical = self.root.canonicalize()?;
        let (root, build_root) = if canonical.is_file() {
            let parent = canonical.parent().ok_or_else(|| {
                crate::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "single-file corpus must have a parent directory",
                ))
            })?;
            (parent.to_path_buf(), canonical)
        } else {
            (canonical.clone(), canonical)
        };
        let (corpus_kind, tables) = build_index_tables(&build_root)?;

        let files = files::MappedFilesView::from_paths(&tables.files);
        let lexicon = crate::storage::lexicon::MappedLexicon::from_entries(&tables.lexicon);
        let postings = crate::storage::postings::MappedPostings::from_bytes(&tables.postings);

        let mut index = Index {
            root,
            corpus_kind,
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

#[cfg(test)]
mod tests {
    use super::{intersect_sorted_posting_byte_slices, merge_sorted_runs};

    fn bytes(ids: &[u32]) -> Vec<u8> {
        ids.iter().flat_map(|id| id.to_le_bytes()).collect()
    }

    #[test]
    fn merge_sorted_runs_preserves_order_and_uniqueness() {
        let merged = merge_sorted_runs(vec![vec![1, 3, 7], vec![1, 2, 7, 9], vec![4, 7, 8]]);
        assert_eq!(merged, vec![1, 2, 3, 4, 7, 8, 9]);
    }

    #[test]
    fn intersect_sorted_posting_byte_slices_handles_smallest_first_order() {
        let a = bytes(&[1, 3, 5, 7, 9]);
        let b = bytes(&[3, 7]);
        let c = bytes(&[0, 3, 4, 7, 8]);
        let slices = vec![a.as_slice(), b.as_slice(), c.as_slice()];
        let ids = intersect_sorted_posting_byte_slices(&slices);
        assert_eq!(ids, vec![3, 7]);
    }
}
