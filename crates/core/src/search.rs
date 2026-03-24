//! Indexed search execution built on ripgrep's public grep crates.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use grep_matcher::LineTerminator;
#[cfg(test)]
use grep_matcher::Matcher;
use grep_printer::{StandardBuilder, SummaryBuilder, SummaryKind};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::SearcherBuilder;
use rayon::prelude::*;
use termcolor::{BufferWriter, ColorChoice};

use crate::planner::TrigramPlan;
use crate::verify;
use crate::Index;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SearchMatchFlags: u8 {
        const CASE_INSENSITIVE = 1 << 0;
        const INVERT_MATCH     = 1 << 1;
        const FIXED_STRINGS    = 1 << 2;
        const WORD_REGEXP      = 1 << 3;
        const LINE_REGEXP      = 1 << 4;
        const ONLY_MATCHING    = 1 << 5;
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SearchOptions {
    pub flags: SearchMatchFlags,
    pub max_results: Option<usize>,
}

impl SearchOptions {
    #[must_use]
    pub const fn case_insensitive(self) -> bool {
        self.flags.contains(SearchMatchFlags::CASE_INSENSITIVE)
    }

    #[must_use]
    pub const fn invert_match(self) -> bool {
        self.flags.contains(SearchMatchFlags::INVERT_MATCH)
    }

    #[must_use]
    pub const fn fixed_strings(self) -> bool {
        self.flags.contains(SearchMatchFlags::FIXED_STRINGS)
    }

    #[must_use]
    pub const fn word_regexp(self) -> bool {
        self.flags.contains(SearchMatchFlags::WORD_REGEXP)
    }

    #[must_use]
    pub const fn line_regexp(self) -> bool {
        self.flags.contains(SearchMatchFlags::LINE_REGEXP)
    }

    #[must_use]
    pub const fn only_matching(self) -> bool {
        self.flags.contains(SearchMatchFlags::ONLY_MATCHING)
    }

    #[must_use]
    pub const fn precludes_trigram_index(self) -> bool {
        self.case_insensitive() || self.invert_match() || self.word_regexp() || self.line_regexp()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Standard,
    OnlyMatching,
    Count,
    FilesWithMatches,
    FilesWithoutMatch,
    Quiet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchOutput {
    pub mode: SearchMode,
    pub with_filename: bool,
    pub line_number: bool,
}

impl Default for SearchOutput {
    fn default() -> Self {
        Self {
            mode: SearchMode::Standard,
            with_filename: true,
            line_number: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledSearch {
    patterns: Vec<String>,
    opts: SearchOptions,
    plan: TrigramPlan,
}

impl CompiledSearch {
    /// Create a compiled search from patterns and options.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::EmptyPatterns`] when no patterns are provided,
    /// or [`crate::Error::RegexBuild`] later when the regex engine rejects the pattern.
    pub fn new(patterns: &[String], opts: SearchOptions) -> crate::Result<Self> {
        if patterns.is_empty() {
            return Err(crate::Error::EmptyPatterns);
        }
        let plan = TrigramPlan::for_patterns(patterns, &opts);
        Ok(Self {
            patterns: patterns.to_vec(),
            opts,
            plan,
        })
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    fn build_matcher(&self) -> crate::Result<RegexMatcher> {
        let branches: Vec<String> = self
            .patterns
            .iter()
            .map(|p| verify::pattern_branch(p, &self.opts))
            .collect();
        let combined = if branches.len() == 1 {
            branches[0].clone()
        } else {
            branches
                .into_iter()
                .map(|b| format!("(?:{b})"))
                .collect::<Vec<_>>()
                .join("|")
        };

        let mut builder = RegexMatcherBuilder::new();
        builder.case_insensitive(self.opts.case_insensitive());
        if self.opts.word_regexp() {
            builder.word(true);
        }
        if self.opts.line_regexp() {
            builder.whole_line(true);
        }
        builder.line_terminator(Some(b'\n'));
        builder
            .build(&combined)
            .map_err(|e| crate::Error::RegexBuild(e.to_string()))
    }

    fn build_searcher(
        &self,
        line_number: bool,
        max_matches: Option<usize>,
    ) -> grep_searcher::Searcher {
        let mut builder = SearcherBuilder::new();
        builder
            .line_terminator(LineTerminator::byte(b'\n'))
            .invert_match(self.opts.invert_match())
            .line_number(line_number)
            .max_matches(max_matches.map(|n| n as u64));
        builder.build()
    }

    const fn uses_exhaustive_candidates(mode: SearchMode) -> bool {
        matches!(mode, SearchMode::Count | SearchMode::FilesWithoutMatch)
    }

    fn candidate_file_ids(
        &self,
        index: &Index,
        prefixes: &[PathBuf],
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
                index
                    .file_path(id)
                    .is_some_and(|rel| path_in_scope(rel, prefixes))
            })
            .collect()
    }

    /// Execute a search over an opened index and print results to stdout.
    ///
    /// # Errors
    ///
    /// Returns an error if the matcher cannot be built.
    pub fn run_index(
        &self,
        index: &Index,
        prefixes: &[PathBuf],
        output: SearchOutput,
    ) -> crate::Result<bool> {
        let candidate_ids = self.candidate_file_ids(
            index,
            prefixes,
            Self::uses_exhaustive_candidates(output.mode),
        );
        if candidate_ids.is_empty() {
            return Ok(false);
        }

        let matcher = self.build_matcher()?;
        let parallel = self.opts.max_results.is_none()
            && candidate_ids.len() >= parallel_candidate_min_files();
        match output.mode {
            SearchMode::Standard | SearchMode::OnlyMatching => {
                Ok(self.run_standard(index, &candidate_ids, &matcher, output, parallel))
            }
            SearchMode::Count
            | SearchMode::FilesWithMatches
            | SearchMode::FilesWithoutMatch
            | SearchMode::Quiet => {
                Ok(self.run_summary(index, &candidate_ids, &matcher, output, parallel))
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
    ) -> bool {
        let bufwtr = BufferWriter::stdout(ColorChoice::Never);
        let mut builder = StandardBuilder::new();
        builder.path(output.with_filename);
        if matches!(output.mode, SearchMode::OnlyMatching) {
            builder.only_matching(true);
        }

        if parallel {
            let any_match = AtomicBool::new(false);
            let stop = AtomicBool::new(false);
            let chunk_size = (candidate_ids.len()
                / std::thread::available_parallelism()
                    .map(std::num::NonZeroUsize::get)
                    .unwrap_or(1))
            .max(1);

            candidate_ids.par_chunks(chunk_size).for_each(|chunk| {
                let mut searcher = self.build_searcher(output.line_number, None);
                let builder = builder.clone();
                let mut printer = builder.build(bufwtr.buffer());
                let mut actual = index.root.clone();
                for &id in chunk {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(candidate) = index.file_path(id) else {
                        continue;
                    };
                    actual.push(candidate);
                    let depth = candidate.components().count();
                    let mut sink = printer.sink_with_path(matcher, candidate);
                    let _ = searcher.search_path(matcher, &actual, &mut sink);
                    if sink.has_match() {
                        any_match.store(true, Ordering::SeqCst);
                    }
                    drop(sink);
                    for _ in 0..depth {
                        actual.pop();
                    }
                }
                if let Err(err) = bufwtr.print(printer.get_mut()) {
                    if err.kind() == std::io::ErrorKind::BrokenPipe {
                        stop.store(true, Ordering::SeqCst);
                    }
                }
            });
            any_match.load(Ordering::SeqCst)
        } else {
            let mut any_match = false;
            let mut remaining = self.opts.max_results;
            let mut printer = builder.build(bufwtr.buffer());
            let mut actual = index.root.clone();
            for &id in candidate_ids {
                let mut searcher = self.build_searcher(output.line_number, remaining);
                let Some(candidate) = index.file_path(id) else {
                    continue;
                };
                actual.push(candidate);
                let depth = candidate.components().count();
                let mut sink = printer.sink_with_path(matcher, candidate);
                let _ = searcher.search_path(matcher, &actual, &mut sink);
                if sink.has_match() {
                    any_match = true;
                }
                let used = usize::try_from(sink.match_count()).unwrap_or(usize::MAX);
                drop(sink);
                for _ in 0..depth {
                    actual.pop();
                }
                if let Some(ref mut left) = remaining {
                    *left = left.saturating_sub(used);
                    if *left == 0 {
                        break;
                    }
                }
            }
            if let Err(err) = bufwtr.print(printer.get_mut()) {
                if err.kind() == std::io::ErrorKind::BrokenPipe {
                    return any_match;
                }
            }
            any_match
        }
    }

    fn run_summary(
        &self,
        index: &Index,
        candidate_ids: &[usize],
        matcher: &RegexMatcher,
        output: SearchOutput,
        parallel: bool,
    ) -> bool {
        let bufwtr = BufferWriter::stdout(ColorChoice::Never);
        let mut builder = SummaryBuilder::new();
        builder.kind(summary_kind(output.mode));
        builder.path(output.with_filename);
        if matches!(output.mode, SearchMode::Count) {
            builder.exclude_zero(false);
        }

        if parallel {
            let any_match = AtomicBool::new(false);
            let stop = AtomicBool::new(false);
            let chunk_size = (candidate_ids.len()
                / std::thread::available_parallelism()
                    .map(std::num::NonZeroUsize::get)
                    .unwrap_or(1))
            .max(1);

            candidate_ids.par_chunks(chunk_size).for_each(|chunk| {
                let mut searcher = self.build_searcher(false, None);
                let builder = builder.clone();
                let mut printer = builder.build(bufwtr.buffer());
                let mut actual = index.root.clone();
                for &id in chunk {
                    if stop.load(Ordering::SeqCst) {
                        break;
                    }
                    let Some(candidate) = index.file_path(id) else {
                        continue;
                    };
                    actual.push(candidate);
                    let depth = candidate.components().count();
                    let mut sink = printer.sink_with_path(matcher, candidate);
                    let _ = searcher.search_path(matcher, &actual, &mut sink);
                    if sink.has_match() {
                        any_match.store(true, Ordering::SeqCst);
                    }
                    let file_matched = sink.has_match();
                    drop(sink);
                    for _ in 0..depth {
                        actual.pop();
                    }
                    if matches!(output.mode, SearchMode::Quiet) && file_matched {
                        stop.store(true, Ordering::SeqCst);
                        break;
                    }
                }
                if let Err(err) = bufwtr.print(printer.get_mut()) {
                    if err.kind() == std::io::ErrorKind::BrokenPipe {
                        stop.store(true, Ordering::SeqCst);
                    }
                }
            });
            any_match.load(Ordering::SeqCst)
        } else {
            let mut any_match = false;
            let mut remaining = self.opts.max_results;
            let mut printer = builder.build(bufwtr.buffer());
            let mut actual = index.root.clone();
            for &id in candidate_ids {
                let mut searcher = self.build_searcher(false, remaining);
                let Some(candidate) = index.file_path(id) else {
                    continue;
                };
                actual.push(candidate);
                let depth = candidate.components().count();
                let mut sink = printer.sink_with_path(matcher, candidate);
                let _ = searcher.search_path(matcher, &actual, &mut sink);
                let file_matched = sink.has_match();
                if file_matched {
                    any_match = true;
                }
                let used = usize::from(file_matched);
                drop(sink);
                for _ in 0..depth {
                    actual.pop();
                }
                if let Some(ref mut left) = remaining {
                    *left = left.saturating_sub(used);
                    if *left == 0 {
                        break;
                    }
                }
                if matches!(output.mode, SearchMode::Quiet) && file_matched {
                    break;
                }
            }
            if let Err(err) = bufwtr.print(printer.get_mut()) {
                if err.kind() == std::io::ErrorKind::BrokenPipe {
                    return any_match;
                }
            }
            any_match
        }
    }

    #[cfg(test)]
    pub(crate) fn collect_index_matches(&self, index: &Index) -> crate::Result<Vec<Match>> {
        let candidate_ids = self.candidate_file_ids(index, &[], false);
        self.collect_index_candidates(index, &candidate_ids)
    }

    #[cfg(test)]
    pub(crate) fn collect_walk_matches(&self, root: &Path) -> crate::Result<Vec<Match>> {
        let root = root.canonicalize()?;
        let mut candidates = Vec::new();
        let walker = ignore::WalkBuilder::new(&root).follow_links(false).build();
        for entry in walker {
            let entry = entry.map_err(crate::Error::Ignore)?;
            if entry.path().is_file() {
                let actual = entry.path().to_path_buf();
                candidates.push(actual);
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

fn summary_kind(mode: SearchMode) -> SummaryKind {
    match mode {
        SearchMode::Count => SummaryKind::Count,
        SearchMode::FilesWithMatches => SummaryKind::PathWithMatch,
        SearchMode::FilesWithoutMatch => SummaryKind::PathWithoutMatch,
        SearchMode::Quiet => SummaryKind::QuietWithMatch,
        SearchMode::Standard | SearchMode::OnlyMatching => unreachable!(),
    }
}

fn path_in_scope(rel: &Path, prefixes: &[PathBuf]) -> bool {
    if prefixes.is_empty() {
        return true;
    }
    prefixes
        .iter()
        .any(|pre| rel.starts_with(pre) || rel.as_os_str() == pre.as_os_str())
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
        if !entry.path().is_file() {
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
        effective
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
    type Error = std::io::Error;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_corpus(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sift-search-{name}-{}", std::process::id()))
    }

    #[test]
    fn empty_patterns_rejected() {
        assert!(matches!(
            CompiledSearch::new(&[], SearchOptions::default()),
            Err(crate::Error::EmptyPatterns)
        ));
    }

    #[test]
    fn alternation_finds_both_branches() {
        let dir = tmp_corpus("regex-or");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "x foo y\n").unwrap();
        fs::write(dir.join("b.txt"), "x bar z\n").unwrap();
        let pat = vec![r"foo|bar".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.collect_walk_matches(&dir).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn only_matching_emits_spans() {
        let dir = tmp_corpus("only-o");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("t.txt"), "foo bar foo\n").unwrap();
        let opts = SearchOptions {
            flags: SearchMatchFlags::ONLY_MATCHING,
            max_results: None,
        };
        let pat = vec!["foo".to_string()];
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let hits = q.collect_walk_matches(&dir).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|m| m.text == "foo"));
    }
}
