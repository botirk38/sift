use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::{DirEntry, Error as IgnoreError, WalkBuilder, WalkState};

use crate::corpus::file::File;
use crate::corpus::filter::{FileFilter, HiddenMode, IgnoreSources, VisibilityConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkTraversal {
    #[default]
    DoNotFollow,
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WalkMetadata {
    #[default]
    Skip,
    Read,
}

/// Reusable filesystem discovery over one corpus root.
pub struct FileWalk<'a> {
    root: &'a Path,
    scopes: &'a [PathBuf],
    excludes: &'a [PathBuf],
    visibility: VisibilityConfig,
    links: LinkTraversal,
    one_file_system: bool,
    max_depth: Option<usize>,
    max_filesize: Option<u64>,
    metadata: WalkMetadata,
}

#[derive(Debug)]
pub struct WalkFile {
    root: Arc<PathBuf>,
    rel_path: PathBuf,
    depth: usize,
    metadata: Option<Metadata>,
}

impl<'a> FileWalk<'a> {
    #[must_use]
    pub fn new(root: &'a Path) -> Self {
        Self {
            root,
            scopes: &[],
            excludes: &[],
            visibility: VisibilityConfig::default(),
            links: LinkTraversal::DoNotFollow,
            one_file_system: false,
            max_depth: None,
            max_filesize: None,
            metadata: WalkMetadata::Skip,
        }
    }

    #[must_use]
    pub fn from_filter(filter: &'a FileFilter) -> Self {
        Self::new(filter.root())
            .scopes(filter.scopes())
            .excludes(filter.exclude_paths())
            .visibility(filter.visibility().clone())
            .links(if filter.follow_links() {
                LinkTraversal::Follow
            } else {
                LinkTraversal::DoNotFollow
            })
            .one_file_system(filter.one_file_system())
            .max_depth(filter.max_depth())
            .max_filesize(filter.max_filesize())
    }

    #[must_use]
    pub const fn scopes(mut self, scopes: &'a [PathBuf]) -> Self {
        self.scopes = scopes;
        self
    }

    #[must_use]
    pub const fn excludes(mut self, excludes: &'a [PathBuf]) -> Self {
        self.excludes = excludes;
        self
    }

    #[must_use]
    pub fn visibility(mut self, visibility: VisibilityConfig) -> Self {
        self.visibility = visibility;
        self
    }

    #[must_use]
    pub const fn links(mut self, links: LinkTraversal) -> Self {
        self.links = links;
        self
    }

    #[must_use]
    pub const fn one_file_system(mut self, enabled: bool) -> Self {
        self.one_file_system = enabled;
        self
    }

    #[must_use]
    pub const fn max_depth(mut self, depth: Option<usize>) -> Self {
        self.max_depth = depth;
        self
    }

    #[must_use]
    pub const fn max_filesize(mut self, size: Option<u64>) -> Self {
        self.max_filesize = size;
        self
    }

    #[must_use]
    pub const fn metadata(mut self, metadata: WalkMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Discover matching files.
    ///
    /// # Errors
    ///
    /// Returns an error if the root cannot be canonicalized or a walk fails.
    pub fn files(self) -> crate::Result<Vec<WalkFile>> {
        self.files_matching(AllFiles)
    }

    /// Discover matching files and convert them into search candidates.
    ///
    /// Reads filesystem metadata so each [`File`] carries size and depth.
    ///
    /// # Errors
    ///
    /// Returns an error if the root cannot be canonicalized or a walk fails.
    pub fn candidates(self) -> crate::Result<Vec<File>> {
        self.candidates_matching(AllFiles)
    }

    /// Discover selected files and convert them into search candidates.
    ///
    /// # Errors
    ///
    /// Returns an error if the root cannot be canonicalized or a walk fails.
    pub fn candidates_matching<S: WalkSelector>(self, selector: S) -> crate::Result<Vec<File>> {
        Ok(self
            .metadata(WalkMetadata::Read)
            .files_matching(selector)?
            .into_iter()
            .map(File::from)
            .collect())
    }

    /// Discover matching files whose relative path is accepted by `selector`.
    ///
    /// # Errors
    ///
    /// Returns an error if the root cannot be canonicalized or a walk fails.
    pub fn files_matching<S: WalkSelector>(self, selector: S) -> crate::Result<Vec<WalkFile>> {
        let filter_root = self.root.canonicalize()?;
        let filter_root = Arc::new(filter_root);
        let mut files = Vec::new();
        if self.scopes.is_empty() {
            self.collect_scope(&filter_root, Path::new(""), selector, &mut files)?;
        } else {
            for scope in self.scopes {
                self.collect_scope(&filter_root, scope, selector.clone(), &mut files)?;
            }
        }
        files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
        files.dedup_by(|a, b| a.rel_path == b.rel_path);
        Ok(files)
    }

    fn is_excluded(&self, rel_path: &Path) -> bool {
        self.excludes
            .iter()
            .any(|excluded| !excluded.as_os_str().is_empty() && rel_path.starts_with(excluded))
    }

    fn collect_scope<S: WalkSelector>(
        &self,
        filter_root: &Arc<PathBuf>,
        scope: &Path,
        selector: S,
        files: &mut Vec<WalkFile>,
    ) -> crate::Result<()> {
        let path = if scope.as_os_str().is_empty() {
            filter_root.as_ref().clone()
        } else {
            filter_root.join(scope)
        };
        if !path.exists() {
            return Ok(());
        }
        let path = path.canonicalize().unwrap_or(path);
        if path.is_file() {
            let rel_path = path
                .strip_prefix(filter_root.as_path())
                .unwrap_or(&path)
                .to_path_buf();
            if !self.is_excluded(&rel_path)
                && selector.includes(rel_path.as_path())
                && let Some(file) = WalkFile::from_scope_file(
                    Arc::clone(filter_root),
                    rel_path,
                    path.as_path(),
                    self.max_filesize,
                    self.metadata,
                )
            {
                files.push(file);
            }
        } else if path.is_dir() {
            let mut walk = FileWalkRun::new(&path, filter_root, self, selector);
            files.extend(walk.run()?);
        }
        Ok(())
    }
}

impl WalkFile {
    fn from_scope_file(
        root: Arc<PathBuf>,
        rel_path: PathBuf,
        abs_path: &Path,
        max_filesize: Option<u64>,
        metadata_mode: WalkMetadata,
    ) -> Option<Self> {
        let metadata = Self::read_metadata(abs_path, max_filesize, metadata_mode);
        if let Some(limit) = max_filesize
            && metadata.as_ref().is_some_and(|m| m.len() > limit)
        {
            return None;
        }
        let depth = rel_path.components().count().saturating_sub(1);
        Some(Self {
            root,
            rel_path,
            depth,
            metadata,
        })
    }

    fn from_dir_entry(
        entry: &DirEntry,
        filter_root: &Arc<PathBuf>,
        max_filesize: Option<u64>,
        metadata_mode: WalkMetadata,
    ) -> Option<Self> {
        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(filter_root.as_path())
            .unwrap_or(abs_path)
            .to_path_buf();
        let metadata = Self::read_metadata(abs_path, max_filesize, metadata_mode);
        if let Some(limit) = max_filesize
            && metadata.as_ref().is_some_and(|m| m.len() > limit)
        {
            return None;
        }
        Some(Self {
            root: Arc::clone(filter_root),
            rel_path,
            depth: entry.depth().saturating_sub(1),
            metadata,
        })
    }

    fn read_metadata(
        path: &Path,
        max_filesize: Option<u64>,
        metadata_mode: WalkMetadata,
    ) -> Option<Metadata> {
        if max_filesize.is_some() || matches!(metadata_mode, WalkMetadata::Read) {
            std::fs::metadata(path).ok()
        } else {
            None
        }
    }

    #[must_use]
    pub fn rel_path(&self) -> &Path {
        self.rel_path.as_path()
    }

    #[must_use]
    pub fn abs_path(&self) -> PathBuf {
        self.root.join(&self.rel_path)
    }

    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    #[must_use]
    pub const fn metadata(&self) -> Option<&Metadata> {
        self.metadata.as_ref()
    }

    #[must_use]
    pub fn size(&self) -> Option<u64> {
        self.metadata.as_ref().map(Metadata::len)
    }

    #[must_use]
    pub fn into_rel_path(self) -> PathBuf {
        self.rel_path
    }
}

impl From<WalkFile> for File {
    fn from(file: WalkFile) -> Self {
        let size = file.size();
        let abs_path = file.root.join(&file.rel_path);
        Self::with_metadata(file.rel_path, abs_path, size, Some(file.depth))
    }
}

/// Selects which discovered relative paths a [`FileWalk`] should keep.
pub trait WalkSelector: Clone + Send + Sync {
    fn includes(&self, rel_path: &Path) -> bool;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AllFiles;

impl WalkSelector for AllFiles {
    fn includes(&self, _rel_path: &Path) -> bool {
        true
    }
}

struct FileWalkRun<'a, S: WalkSelector> {
    root: PathBuf,
    filter_root: Arc<PathBuf>,
    config: &'a FileWalk<'a>,
    selector: S,
    walk_error: Arc<Mutex<Option<crate::Error>>>,
    consolidated: Arc<Mutex<Vec<WalkFile>>>,
}

impl<'a, S: WalkSelector> FileWalkRun<'a, S> {
    fn new(root: &Path, filter_root: &Arc<PathBuf>, config: &'a FileWalk<'a>, selector: S) -> Self {
        Self {
            root: root.to_path_buf(),
            filter_root: Arc::clone(filter_root),
            config,
            selector,
            walk_error: Arc::new(Mutex::new(None)),
            consolidated: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn walk_builder(&self) -> WalkBuilder {
        let sources = self.config.visibility.ignore.sources;
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .follow_links(matches!(self.config.links, LinkTraversal::Follow))
            .same_file_system(self.config.one_file_system)
            .hidden(matches!(self.config.visibility.hidden, HiddenMode::Respect))
            .parents(sources.contains(IgnoreSources::PARENT))
            .ignore(sources.contains(IgnoreSources::DOT))
            .git_ignore(sources.contains(IgnoreSources::VCS))
            .git_exclude(sources.contains(IgnoreSources::EXCLUDE))
            .git_global(sources.contains(IgnoreSources::GLOBAL))
            .require_git(self.config.visibility.ignore.require_git);
        if let Some(depth) = self.config.max_depth {
            builder.max_depth(Some(depth + 1));
        }
        builder
    }

    fn run(&mut self) -> crate::Result<Vec<WalkFile>> {
        self.walk_builder().build_parallel().visit(self);

        {
            let mut guard = self.walk_error.lock().expect("walk error lock");
            if let Some(err) = guard.take() {
                return Err(err);
            }
        }

        Ok(std::mem::take(
            &mut *self.consolidated.lock().expect("candidate lock"),
        ))
    }
}

impl<'a, S: WalkSelector + 'a> ignore::ParallelVisitorBuilder<'a> for FileWalkRun<'_, S> {
    fn build(&mut self) -> Box<dyn ignore::ParallelVisitor + 'a> {
        Box::new(FileCollector {
            filter_root: Arc::clone(&self.filter_root),
            excludes: self.config.excludes.to_vec(),
            max_filesize: self.config.max_filesize,
            metadata: self.config.metadata,
            selector: self.selector.clone(),
            thread_files: Vec::new(),
            walk_error: Arc::clone(&self.walk_error),
            consolidated: Arc::clone(&self.consolidated),
        })
    }
}

struct FileCollector<S: WalkSelector> {
    filter_root: Arc<PathBuf>,
    excludes: Vec<PathBuf>,
    max_filesize: Option<u64>,
    metadata: WalkMetadata,
    selector: S,
    thread_files: Vec<WalkFile>,
    walk_error: Arc<Mutex<Option<crate::Error>>>,
    consolidated: Arc<Mutex<Vec<WalkFile>>>,
}

impl<S: WalkSelector> Drop for FileCollector<S> {
    fn drop(&mut self) {
        if self.thread_files.is_empty() {
            return;
        }
        let mut guard = self.consolidated.lock().expect("candidate lock");
        guard.append(&mut self.thread_files);
    }
}

impl<S: WalkSelector> ignore::ParallelVisitor for FileCollector<S> {
    fn visit(&mut self, entry: Result<DirEntry, IgnoreError>) -> WalkState {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                let mut guard = self.walk_error.lock().expect("walk error lock");
                if guard.is_none() {
                    *guard = Some(crate::Error::Ignore(err));
                }
                drop(guard);
                return WalkState::Quit;
            }
        };

        let abs_path = entry.path();
        let rel_path = abs_path
            .strip_prefix(self.filter_root.as_path())
            .unwrap_or(abs_path)
            .to_path_buf();
        if self.is_excluded(&rel_path) {
            return if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                WalkState::Skip
            } else {
                WalkState::Continue
            };
        }
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            return WalkState::Continue;
        }
        if !self.selector.includes(&rel_path) {
            return WalkState::Continue;
        }

        if let Some(file) =
            WalkFile::from_dir_entry(&entry, &self.filter_root, self.max_filesize, self.metadata)
        {
            self.thread_files.push(file);
        }
        WalkState::Continue
    }
}

impl<S: WalkSelector> FileCollector<S> {
    fn is_excluded(&self, rel_path: &Path) -> bool {
        self.excludes
            .iter()
            .any(|excluded| !excluded.as_os_str().is_empty() && rel_path.starts_with(excluded))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::TempDir;

    use super::*;
    use crate::corpus::filter::{HiddenMode, IgnoreConfig};

    fn raw_visibility() -> VisibilityConfig {
        VisibilityConfig {
            hidden: HiddenMode::Include,
            ignore: IgnoreConfig::disabled(),
        }
    }

    #[test]
    fn empty_scopes_walk_root() {
        let tmp = TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("a.txt"), "a").expect("write file");

        let paths: Vec<_> = FileWalk::new(tmp.path())
            .visibility(raw_visibility())
            .files()
            .expect("walk")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();

        assert_eq!(paths, vec![PathBuf::from("a.txt")]);
    }

    #[test]
    fn scopes_limit_walk() {
        let tmp = TempDir::new().expect("tempdir");
        std::fs::create_dir(tmp.path().join("src")).expect("mkdir");
        std::fs::write(tmp.path().join("src/lib.rs"), "lib").expect("write file");
        std::fs::write(tmp.path().join("README.md"), "readme").expect("write file");
        let scopes = [PathBuf::from("src")];

        let paths: Vec<_> = FileWalk::new(tmp.path())
            .scopes(&scopes)
            .visibility(raw_visibility())
            .files()
            .expect("walk")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();

        assert_eq!(paths, vec![PathBuf::from("src/lib.rs")]);
    }

    #[test]
    fn excludes_prune_subtrees() {
        let tmp = TempDir::new().expect("tempdir");
        std::fs::create_dir(tmp.path().join("target")).expect("mkdir");
        std::fs::write(tmp.path().join("target/generated.rs"), "generated").expect("write file");
        std::fs::write(tmp.path().join("src.rs"), "src").expect("write file");
        let excludes = [PathBuf::from("target")];

        let paths: Vec<_> = FileWalk::new(tmp.path())
            .excludes(&excludes)
            .visibility(raw_visibility())
            .files()
            .expect("walk")
            .into_iter()
            .map(WalkFile::into_rel_path)
            .collect();

        assert_eq!(paths, vec![PathBuf::from("src.rs")]);
    }

    #[test]
    fn can_serialize_walk_files_into_custom_records() {
        struct WalkSummary {
            path: PathBuf,
            size: Option<u64>,
        }

        let tmp = TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("a.txt"), "alpha").expect("write file");

        let records: Vec<_> = FileWalk::new(tmp.path())
            .visibility(raw_visibility())
            .metadata(WalkMetadata::Read)
            .files()
            .expect("walk")
            .into_iter()
            .map(|file| WalkSummary {
                path: file.rel_path().to_path_buf(),
                size: file.size(),
            })
            .collect();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, PathBuf::from("a.txt"));
        assert_eq!(records[0].size, Some(5));
    }
}
