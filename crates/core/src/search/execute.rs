use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkMatch};
use rayon::prelude::*;

use crate::planner::TrigramPlan;
use crate::Index;

use super::{CompiledSearch, FilenameMode, OutputEmission, SearchFilter, SearchMode, SearchOutput};

#[cfg(test)]
use super::{GlobConfig, HiddenMode, IgnoreConfig, Match, SearchFilterConfig, VisibilityConfig};

impl CompiledSearch {
    #[must_use]
    pub fn candidate_file_ids(
        &self,
        index: &Index,
        filter: &SearchFilter,
        exhaustive: bool,
    ) -> Vec<usize> {
        let ids: Vec<usize> = if exhaustive {
            (0..index.file_count()).collect()
        } else {
            match &self.plan {
                TrigramPlan::FullScan => (0..index.file_count()).collect(),
                TrigramPlan::Narrow { arms } => index
                    .candidate_file_ids(arms.as_slice())
                    .into_iter()
                    .map(|id| id as usize)
                    .collect(),
            }
        };

        ids.into_iter()
            .filter(|&id| {
                let Some(rel) = index.file_path(id) else {
                    return false;
                };
                filter.is_candidate(rel)
            })
            .collect()
    }

    /// Execute a search over an opened index and print results to stdout.
    ///
    /// # Errors
    ///
    /// Returns an error if the matcher cannot be built or stdout cannot be written.
    pub fn run_index(
        &self,
        index: &Index,
        filter: &SearchFilter,
        output: SearchOutput,
    ) -> crate::Result<bool> {
        if self.opts.max_results == Some(0) {
            return Err(crate::Error::InvalidMaxCount);
        }
        let candidate_ids =
            self.candidate_file_ids(index, filter, Self::uses_exhaustive_candidates(output.mode));
        if candidate_ids.is_empty() {
            return Ok(false);
        }

        let matcher = self.build_matcher()?;
        let parallel = candidate_ids.len() >= parallel_candidate_min_files();
        match output.mode {
            SearchMode::Standard | SearchMode::OnlyMatching => {
                self.run_standard(index, &candidate_ids, &matcher, output, parallel)
            }
            SearchMode::Count
            | SearchMode::CountMatches
            | SearchMode::FilesWithMatches
            | SearchMode::FilesWithoutMatch => {
                self.run_summary(index, &candidate_ids, &matcher, output, parallel)
            }
        }
    }

    fn run_standard(
        &self,
        index: &Index,
        candidate_ids: &[usize],
        matcher: &RegexMatcher,
        output: SearchOutput,
        parallel: bool,
    ) -> crate::Result<bool> {
        if parallel {
            let stop = AtomicBool::new(false);
            let mut files = candidate_ids
                .par_iter()
                .enumerate()
                .map_init(
                    || StandardWorker::new(self, matcher.clone(), output),
                    |worker, (result_index, &id)| {
                        worker.search_candidate(index, result_index, id, &stop)
                    },
                )
                .collect::<Vec<_>>();
            files.sort_by_key(|file| file.index);
            return flush_chunk_output(files.into_iter().map(|file| file.output));
        }

        self.run_standard_capped(index, candidate_ids, matcher, output)
    }

    fn run_summary(
        &self,
        index: &Index,
        candidate_ids: &[usize],
        matcher: &RegexMatcher,
        output: SearchOutput,
        parallel: bool,
    ) -> crate::Result<bool> {
        if parallel {
            let stop = AtomicBool::new(false);
            let mut files = candidate_ids
                .par_iter()
                .enumerate()
                .map_init(
                    || {
                        SummaryWorker::new(
                            self,
                            matcher.clone(),
                            self.opts.max_results,
                            output.mode,
                        )
                    },
                    |worker, (result_index, &id)| {
                        worker.search_candidate(index, result_index, id, output, &stop)
                    },
                )
                .collect::<Vec<_>>();
            files.sort_by_key(|file| file.index);
            return flush_chunk_output(files.into_iter().map(|file| file.output));
        }

        self.run_summary_capped(index, candidate_ids, matcher, output)
    }

    fn run_standard_capped(
        &self,
        index: &Index,
        candidate_ids: &[usize],
        matcher: &RegexMatcher,
        output: SearchOutput,
    ) -> crate::Result<bool> {
        let mut any_match = false;
        let mut out = Vec::new();
        let mut actual = index.root.clone();
        for &id in candidate_ids {
            let mut searcher = self.build_searcher(output.line_number, self.opts.max_results);
            let Some(candidate) = index.file_path(id) else {
                continue;
            };
            actual.push(candidate);
            let depth = candidate.components().count();
            let mut sink = StandardSink::new(matcher, output, &actual, &mut out);
            let _ = searcher.search_path(matcher, &actual, &mut sink);
            any_match |= sink.matched;
            for _ in 0..depth {
                actual.pop();
            }
            if output.emission == OutputEmission::Quiet && any_match {
                break;
            }
        }

        flush_chunk_output(std::iter::once(ChunkOutput {
            bytes: out,
            matched: any_match,
        }))
    }

    fn run_summary_capped(
        &self,
        index: &Index,
        candidate_ids: &[usize],
        matcher: &RegexMatcher,
        output: SearchOutput,
    ) -> crate::Result<bool> {
        let mut any_match = false;
        let mut out = Vec::new();
        let mut worker =
            SummaryWorker::new(self, matcher.clone(), self.opts.max_results, output.mode);
        let mut actual = index.root.clone();
        for &id in candidate_ids {
            let Some(candidate) = index.file_path(id) else {
                continue;
            };
            actual.push(candidate);
            let depth = candidate.components().count();
            let result = worker.search_file(&actual);
            any_match |= mode_is_success(output.mode, result);
            write_summary_record(&mut out, output, &actual, result)?;
            for _ in 0..depth {
                actual.pop();
            }
            if output.emission == OutputEmission::Quiet && mode_is_success(output.mode, result) {
                break;
            }
        }

        flush_chunk_output(std::iter::once(ChunkOutput {
            bytes: out,
            matched: any_match,
        }))
    }

    #[cfg(test)]
    pub(crate) fn collect_index_matches(&self, index: &Index) -> crate::Result<Vec<Match>> {
        let config = SearchFilterConfig {
            scopes: vec![],
            glob: GlobConfig::default(),
            visibility: VisibilityConfig {
                hidden: HiddenMode::Include,
                ignore: IgnoreConfig::default(),
            },
        };
        let filter = SearchFilter::new(&config, &index.root)?;
        let candidate_ids = self.candidate_file_ids(index, &filter, false);
        self.collect_index_candidates(index, &candidate_ids)
    }

    #[cfg(test)]
    pub(crate) fn collect_walk_matches(&self, root: &Path) -> crate::Result<Vec<Match>> {
        let root = root.canonicalize()?;
        let mut candidates = Vec::new();
        let walker = ignore::WalkBuilder::new(&root)
            .follow_links(false)
            .hidden(false)
            .parents(false)
            .ignore(false)
            .git_global(false)
            .git_ignore(false)
            .git_exclude(false)
            .require_git(false)
            .build();
        for entry in walker {
            let entry = entry.map_err(crate::Error::Ignore)?;
            if entry.file_type().is_some_and(|ft| ft.is_file()) {
                let path = entry.path();
                if path.components().any(|c| c.as_os_str() == ".sift") {
                    continue;
                }
                candidates.push(path.to_path_buf());
            }
        }
        self.collect_walk_candidates(&candidates)
    }

    #[cfg(test)]
    fn collect_index_candidates(
        &self,
        index: &Index,
        candidate_ids: &[usize],
    ) -> crate::Result<Vec<Match>> {
        let matcher = self.build_matcher()?;
        let mut searcher = self.build_searcher(true, None);
        let mut out = Vec::new();
        let mut actual = index.root.clone();
        for &id in candidate_ids {
            let Some(candidate) = index.file_path(id) else {
                continue;
            };
            actual.push(candidate);
            let depth = candidate.components().count();
            let mut sink =
                CollectSink::new(actual.clone(), self.opts.only_matching(), matcher.clone());
            let _ = searcher.search_path(&matcher, &actual, &mut sink);
            for _ in 0..depth {
                actual.pop();
            }
            out.extend(sink.into_matches());
        }
        Ok(out)
    }

    #[cfg(test)]
    fn collect_walk_candidates(&self, candidates: &[PathBuf]) -> crate::Result<Vec<Match>> {
        let matcher = self.build_matcher()?;
        let mut searcher = self.build_searcher(true, None);
        let mut out = Vec::new();
        for candidate in candidates {
            let mut sink = CollectSink::new(
                candidate.clone(),
                self.opts.only_matching(),
                matcher.clone(),
            );
            let _ = searcher.search_path(&matcher, candidate, &mut sink);
            out.extend(sink.into_matches());
        }
        Ok(out)
    }
}

struct StandardWorker<'a> {
    search: &'a CompiledSearch,
    matcher: RegexMatcher,
    output: SearchOutput,
    bytes: Vec<u8>,
}

impl<'a> StandardWorker<'a> {
    const fn new(search: &'a CompiledSearch, matcher: RegexMatcher, output: SearchOutput) -> Self {
        Self {
            search,
            matcher,
            output,
            bytes: Vec::new(),
        }
    }

    fn search_candidate(
        &mut self,
        index: &Index,
        result_index: usize,
        id: usize,
        stop: &AtomicBool,
    ) -> FileResult {
        self.bytes.clear();
        if stop.load(Ordering::SeqCst) {
            return FileResult {
                index: result_index,
                output: ChunkOutput {
                    bytes: Vec::new(),
                    matched: false,
                },
            };
        }

        let Some(candidate) = index.file_path(id) else {
            return FileResult {
                index: result_index,
                output: ChunkOutput {
                    bytes: Vec::new(),
                    matched: false,
                },
            };
        };
        let actual = index.root.join(candidate);
        let matched = {
            let mut searcher = self
                .search
                .build_searcher(self.output.line_number, self.search.opts.max_results);
            let mut sink = StandardSink::new(&self.matcher, self.output, &actual, &mut self.bytes);
            let _ = searcher.search_path(&self.matcher, &actual, &mut sink);
            sink.matched
        };

        if self.output.emission == OutputEmission::Quiet && matched {
            stop.store(true, Ordering::SeqCst);
        }

        FileResult {
            index: result_index,
            output: ChunkOutput {
                bytes: self.bytes.clone(),
                matched,
            },
        }
    }
}

struct StandardSink<'a> {
    matcher: &'a RegexMatcher,
    output: SearchOutput,
    path: &'a Path,
    bytes: &'a mut Vec<u8>,
    matched: bool,
    match_count: usize,
}

impl<'a> StandardSink<'a> {
    const fn new(
        matcher: &'a RegexMatcher,
        output: SearchOutput,
        path: &'a Path,
        bytes: &'a mut Vec<u8>,
    ) -> Self {
        Self {
            matcher,
            output,
            path,
            bytes,
            matched: false,
            match_count: 0,
        }
    }
}

impl Sink for StandardSink<'_> {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.matched = true;
        self.match_count += 1;

        if self.output.emission == OutputEmission::Quiet {
            return Ok(true);
        }

        if matches!(self.output.mode, SearchMode::OnlyMatching) {
            let line_number = mat.line_number();
            let line = mat.bytes();
            let _ = self.matcher.find_iter(line, |m: grep_matcher::Match| {
                let _ = write_standard_prefix(self.bytes, self.output, self.path, line_number);
                let _ = self.bytes.write_all(&line[m.start()..m.end()]);
                let _ = self.bytes.write_all(b"\n");
                true
            });
            return Ok(true);
        }

        write_standard_prefix(self.bytes, self.output, self.path, mat.line_number())?;
        self.bytes.write_all(mat.bytes())?;
        if !mat.bytes().ends_with(b"\n") {
            self.bytes.write_all(b"\n")?;
        }
        Ok(true)
    }
}

struct SummaryWorker {
    matcher: RegexMatcher,
    searcher: Searcher,
    mode: SearchMode,
}

impl SummaryWorker {
    fn new(
        search: &CompiledSearch,
        matcher: RegexMatcher,
        max_results: Option<usize>,
        mode: SearchMode,
    ) -> Self {
        Self {
            searcher: search.build_searcher(false, max_results),
            matcher,
            mode,
        }
    }

    fn search_file(&mut self, path: &Path) -> FileSummary {
        let sink_matcher = if self.mode == SearchMode::CountMatches {
            Some(self.matcher.clone())
        } else {
            None
        };
        let mut sink = SummarySink::new(self.mode, sink_matcher);
        let _ = self.searcher.search_path(&self.matcher, path, &mut sink);
        sink.finish()
    }

    fn search_candidate(
        &mut self,
        index: &Index,
        result_index: usize,
        id: usize,
        output: SearchOutput,
        stop: &AtomicBool,
    ) -> FileResult {
        if stop.load(Ordering::SeqCst) {
            return FileResult {
                index: result_index,
                output: ChunkOutput {
                    bytes: Vec::new(),
                    matched: false,
                },
            };
        }

        let Some(candidate) = index.file_path(id) else {
            return FileResult {
                index: result_index,
                output: ChunkOutput {
                    bytes: Vec::new(),
                    matched: false,
                },
            };
        };

        let actual = index.root.join(candidate);
        let result = self.search_file(&actual);
        let matched = mode_is_success(output.mode, result);
        let mut bytes = Vec::new();
        let _ = write_summary_record(&mut bytes, output, &actual, result);
        if output.emission == OutputEmission::Quiet && mode_is_success(output.mode, result) {
            stop.store(true, Ordering::SeqCst);
        }

        FileResult {
            index: result_index,
            output: ChunkOutput { bytes, matched },
        }
    }
}

struct FileResult {
    index: usize,
    output: ChunkOutput,
}

struct ChunkOutput {
    bytes: Vec<u8>,
    matched: bool,
}

fn flush_chunk_output(outputs: impl IntoIterator<Item = ChunkOutput>) -> crate::Result<bool> {
    let mut stdout = io::stdout().lock();
    let mut any_match = false;
    for output in outputs {
        any_match |= output.matched;
        if output.bytes.is_empty() {
            continue;
        }
        stdout.write_all(&output.bytes)?;
    }
    Ok(any_match)
}

#[derive(Clone, Copy)]
struct FileSummary {
    matched: bool,
    count: usize,
}

struct SummarySink {
    mode: SearchMode,
    matcher: Option<RegexMatcher>,
    matched: bool,
    count: usize,
}

impl SummarySink {
    const fn new(mode: SearchMode, matcher: Option<RegexMatcher>) -> Self {
        Self {
            mode,
            matcher,
            matched: false,
            count: 0,
        }
    }

    fn finish(self) -> FileSummary {
        FileSummary {
            matched: self.matched,
            count: self.count,
        }
    }
}

impl Sink for SummarySink {
    type Error = io::Error;

    fn matched(&mut self, _: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        self.matched = true;
        if self.mode == SearchMode::CountMatches {
            if let Some(ref matcher) = self.matcher {
                let line = mat.bytes();
                let mut n = 0;
                let _ = matcher.find_iter(line, |_| {
                    n += 1;
                    true
                });
                self.count += n;
            }
        } else {
            self.count += 1;
        }
        Ok(matches!(
            self.mode,
            SearchMode::Count | SearchMode::CountMatches
        ))
    }
}

fn write_summary_record(
    out: &mut Vec<u8>,
    output: SearchOutput,
    path: &Path,
    result: FileSummary,
) -> io::Result<()> {
    if output.emission == OutputEmission::Quiet {
        return Ok(());
    }
    match output.mode {
        SearchMode::Count | SearchMode::CountMatches => {
            if result.count == 0 {
                return Ok(());
            }
            let print_filename = output.filename_mode != FilenameMode::Never;
            if print_filename {
                writeln!(out, "{}:{}", path.display(), result.count)
            } else {
                writeln!(out, "{}", result.count)
            }
        }
        SearchMode::FilesWithMatches => {
            if result.matched {
                writeln!(out, "{}", path.display())
            } else {
                Ok(())
            }
        }
        SearchMode::FilesWithoutMatch => {
            if result.matched {
                Ok(())
            } else {
                writeln!(out, "{}", path.display())
            }
        }
        SearchMode::Standard | SearchMode::OnlyMatching => unreachable!(),
    }
}

fn write_standard_prefix(
    out: &mut Vec<u8>,
    output: SearchOutput,
    path: &Path,
    line_number: Option<u64>,
) -> io::Result<()> {
    let print_filename = output.filename_mode != FilenameMode::Never;
    if print_filename {
        write!(out, "{}:", path.display())?;
    }
    if output.line_number {
        write!(out, "{}:", line_number.unwrap_or(0))?;
    }
    Ok(())
}

const fn mode_is_success(mode: SearchMode, result: FileSummary) -> bool {
    match mode {
        SearchMode::Count | SearchMode::CountMatches | SearchMode::FilesWithMatches => {
            result.matched
        }
        SearchMode::FilesWithoutMatch => !result.matched,
        SearchMode::Standard | SearchMode::OnlyMatching => unreachable!(),
    }
}

/// Walk a directory tree and return all indexed file paths relative to `root`.
///
/// # Errors
///
/// Returns an error when canonicalizing `root` or while walking the tree.
pub fn walk_file_paths(root: &Path) -> crate::Result<HashSet<PathBuf>> {
    let root = root.canonicalize()?;
    let mut set = HashSet::new();
    let walker = ignore::WalkBuilder::new(&root).follow_links(false).build();
    for entry in walker {
        let entry = entry.map_err(crate::Error::Ignore)?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = entry.path();
        let display = path.strip_prefix(&root).unwrap_or(path).to_path_buf();
        set.insert(display);
    }
    Ok(set)
}

pub fn parallel_candidate_min_files() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    let rayon_threads = std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let effective = rayon_threads
        .filter(|&n| n > 0)
        .map_or(cpus, |rt| rt.min(cpus))
        .max(1);
    if effective <= 1 {
        usize::MAX
    } else {
        effective.saturating_mul(8)
    }
}

#[cfg(test)]
struct CollectSink {
    path: PathBuf,
    only_matching: bool,
    matcher: RegexMatcher,
    matches: Vec<Match>,
}

#[cfg(test)]
impl CollectSink {
    fn new(path: PathBuf, only_matching: bool, matcher: RegexMatcher) -> Self {
        Self {
            path,
            only_matching,
            matcher,
            matches: Vec::new(),
        }
    }

    fn into_matches(self) -> Vec<Match> {
        self.matches
    }
}

#[cfg(test)]
impl grep_searcher::Sink for CollectSink {
    type Error = io::Error;

    fn matched(
        &mut self,
        _: &grep_searcher::Searcher,
        mat: &grep_searcher::SinkMatch<'_>,
    ) -> Result<bool, Self::Error> {
        let line = usize::try_from(mat.line_number().unwrap_or(0)).unwrap_or(0);
        let line_bytes = mat.bytes();
        if self.only_matching {
            let _ = self
                .matcher
                .find_iter(line_bytes, |m: grep_matcher::Match| {
                    self.matches.push(Match {
                        file: self.path.clone(),
                        line,
                        text: String::from_utf8_lossy(&line_bytes[m.start()..m.end()]).into_owned(),
                    });
                    true
                });
        } else {
            self.matches.push(Match {
                file: self.path.clone(),
                line,
                text: String::from_utf8_lossy(line_bytes).into_owned(),
            });
        }
        Ok(true)
    }
}
