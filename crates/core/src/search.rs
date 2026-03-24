//! Streaming search powered by `grep-searcher`.
//!
//! Scan pipeline: `grep-searcher` → `grep-regex` → user-provided `Sink`.

use std::{
    collections::HashSet,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use grep_matcher::{LineTerminator, Matcher};
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{
    Searcher, SearcherBuilder, Sink, SinkContext, SinkError, SinkFinish, SinkMatch,
};
use rayon::prelude::*;

use crate::planner::TrigramPlan;
use crate::verify;
use crate::Index;

struct SiftSinkError;

impl SinkError for SiftSinkError {
    fn error_message<T: std::fmt::Display>(_: T) -> Self {
        Self
    }
}

static PARALLEL_MIN_FILES: OnceLock<usize> = OnceLock::new();

#[must_use]
pub fn parallel_candidate_min_files() -> usize {
    *PARALLEL_MIN_FILES.get_or_init(|| {
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
    })
}

#[cfg(test)]
fn parallel_scan_min_files_inner(cpus: usize, rayon_threads: Option<usize>) -> usize {
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

#[derive(Debug, Default)]
pub struct Outcome {
    pub has_match: bool,
    pub files_with_match: Vec<PathBuf>,
    pub counts: Vec<(PathBuf, usize)>,
}

#[derive(Debug, Clone)]
pub struct CompiledSearch {
    patterns: Vec<String>,
    opts: SearchOptions,
    plan: TrigramPlan,
}

impl CompiledSearch {
    /// Create a new compiled search from patterns and options.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyPatterns`] if patterns is empty.
    /// Returns [`Error::RegexBuild`] if the combined regex cannot be built.
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

    fn build_matcher(&self) -> crate::Result<grep_regex::RegexMatcher> {
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

    fn build_searcher(&self) -> Searcher {
        let mut builder = SearcherBuilder::new();
        builder
            .line_terminator(LineTerminator::byte(b'\n'))
            .invert_match(self.opts.invert_match())
            .line_number(true);
        builder.build()
    }

    /// Search the index, returning all matches.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on file read errors.
    /// Returns [`Error::RegexBuild`] if the regex cannot be built.
    pub fn search_index(&self, index: &Index) -> crate::Result<Vec<Match>> {
        let candidates: Vec<PathBuf> = match &self.plan {
            TrigramPlan::FullScan => index.iter_files().map(|p| index.root.join(p)).collect(),
            TrigramPlan::Narrow { arms } => {
                let cids = index.candidate_file_ids(arms.as_slice());
                if cids.is_empty() {
                    return Ok(Vec::new());
                }
                cids.iter()
                    .filter_map(|&id| index.file_path(id as usize))
                    .map(|p| index.root.join(p))
                    .collect()
            }
        };
        self.search_paths(candidates)
    }

    /// Search a directory tree, returning all matches.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if root cannot be canonicalized or on file read errors.
    /// Returns [`Error::Ignore`] if directory walking fails.
    /// Returns [`Error::RegexBuild`] if the regex cannot be built.
    pub fn search_walk(
        &self,
        root: &Path,
        candidates: Option<&[PathBuf]>,
    ) -> crate::Result<Vec<Match>> {
        let root = root.canonicalize()?;

        let paths: Vec<PathBuf> = if let Some(set) = candidates {
            if set.is_empty() {
                return Ok(Vec::new());
            }
            set.iter()
                .map(|p| {
                    if p.is_absolute() {
                        p.clone()
                    } else {
                        root.join(p)
                    }
                })
                .collect()
        } else {
            let mut paths = Vec::new();
            let walker = ignore::WalkBuilder::new(&root).follow_links(false).build();
            for entry in walker {
                let entry = entry.map_err(crate::Error::Ignore)?;
                if entry.path().is_file() {
                    paths.push(entry.path().to_path_buf());
                }
            }
            paths
        };

        self.search_paths(paths)
    }

    fn search_paths(&self, paths: Vec<PathBuf>) -> crate::Result<Vec<Match>> {
        let matcher = self.build_matcher()?;
        let mut searcher = self.build_searcher();
        let mut results = Vec::new();

        for path in paths {
            if !path.is_file() {
                continue;
            }
            let Ok(file) = File::open(&path) else {
                continue;
            };
            let mut sink = MatchCollectSink::new(path, matcher.clone());
            sink.set_only_matching(self.opts.only_matching());
            if let Err(e) = searcher.search_file(&matcher, &file, &mut sink) {
                let _ = e;
            }
            results.extend(sink.into_matches());
            if let Some(limit) = self.opts.max_results {
                if results.len() >= limit {
                    results.truncate(limit);
                    break;
                }
            }
        }

        Ok(results)
    }

    /// Search the index, invoking a callback for each file's outcome.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RegexBuild`] if the regex cannot be built.
    pub fn search_index_with<F>(&self, index: &Index, mut f: F) -> crate::Result<()>
    where
        F: FnMut(&Outcome) + Send + Sync,
    {
        let matcher = self.build_matcher()?;
        let mut searcher = self.build_searcher();

        let candidates: Vec<PathBuf> = match &self.plan {
            TrigramPlan::FullScan => index.iter_files().map(|p| index.root.join(p)).collect(),
            TrigramPlan::Narrow { arms } => {
                let cids = index.candidate_file_ids(arms.as_slice());
                if cids.is_empty() {
                    return Ok(());
                }
                cids.iter()
                    .filter_map(|&id| index.file_path(id as usize))
                    .map(|p| index.root.join(p))
                    .collect()
            }
        };

        let threshold = parallel_candidate_min_files();
        let parallel = candidates.len() >= threshold && self.opts.max_results.is_none();

        if parallel {
            let chunk_size = (candidates.len()
                / std::thread::available_parallelism()
                    .map(std::num::NonZeroUsize::get)
                    .unwrap_or(1))
            .max(1);

            let results: Vec<Outcome> = candidates
                .par_chunks(chunk_size)
                .filter_map(|chunk| {
                    let mut local = Outcome::default();
                    let mut local_searcher = self.build_searcher();
                    let matcher_clone = matcher.clone();
                    for path in chunk {
                        if !path.is_file() {
                            continue;
                        }
                        if let Ok(file) = File::open(path) {
                            let mut sink = OutcomeSink::new(path.clone());
                            let _ = local_searcher.search_file(&matcher_clone, &file, &mut sink);
                            merge_outcome(&mut local, &sink.into_outcome());
                        }
                    }
                    Some(local)
                })
                .collect();

            let mut combined = Outcome::default();
            for r in results {
                merge_outcome(&mut combined, &r);
            }
            f(&combined);
        } else {
            for path in &candidates {
                if !path.is_file() {
                    continue;
                }
                if let Ok(file) = File::open(path) {
                    let mut sink = OutcomeSink::new(path.clone());
                    let _ = searcher.search_file(&matcher, &file, &mut sink);
                    let outcome = sink.into_outcome();
                    f(&outcome);
                }
            }
        }

        Ok(())
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

fn merge_outcome(into: &mut Outcome, from: &Outcome) {
    into.has_match = into.has_match || from.has_match;
    into.files_with_match
        .extend_from_slice(&from.files_with_match);
    into.counts.extend_from_slice(&from.counts);
}

struct OutcomeSink {
    path: PathBuf,
    outcome: Outcome,
}

impl OutcomeSink {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            outcome: Outcome::default(),
        }
    }

    fn into_outcome(self) -> Outcome {
        self.outcome
    }
}

impl Sink for OutcomeSink {
    type Error = SiftSinkError;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        let line = usize::try_from(mat.line_number().unwrap_or(0)).unwrap_or(0);
        self.outcome.has_match = true;
        self.outcome.files_with_match.push(self.path.clone());
        self.outcome.counts.push((self.path.clone(), line));
        Ok(true)
    }

    fn context(&mut self, _: &Searcher, _: &SinkContext<'_>) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn finish(&mut self, _: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct MatchCollectSink {
    path: PathBuf,
    budget: Option<usize>,
    matches: Vec<Match>,
    only_matching: bool,
    matcher: Option<grep_regex::RegexMatcher>,
}

impl MatchCollectSink {
    const fn new(path: PathBuf, matcher: grep_regex::RegexMatcher) -> Self {
        Self {
            path,
            budget: None,
            matches: Vec::new(),
            only_matching: false,
            matcher: Some(matcher),
        }
    }

    fn into_matches(self) -> Vec<Match> {
        self.matches
    }

    fn budget_exhausted(&self) -> bool {
        self.budget == Some(0)
    }

    const fn set_only_matching(&mut self, yes: bool) {
        self.only_matching = yes;
    }
}

impl Sink for MatchCollectSink {
    type Error = io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if self.budget_exhausted() {
            return Ok(false);
        }

        let line = usize::try_from(mat.line_number().unwrap_or(0)).unwrap_or(0);
        let line_bytes = mat.bytes();

        if self.only_matching {
            if let Some(matcher) = self.matcher.clone() {
                let _ = matcher.find_iter(line_bytes, |m| {
                    if self.budget_exhausted() {
                        return false;
                    }
                    let text =
                        String::from_utf8_lossy(&line_bytes[m.start()..m.end()]).into_owned();
                    self.matches.push(Match {
                        file: self.path.clone(),
                        line,
                        text,
                    });
                    if let Some(ref mut b) = self.budget {
                        *b = b.saturating_sub(1);
                    }
                    true
                });
            }
        } else {
            let text = String::from_utf8_lossy(line_bytes).into_owned();
            self.matches.push(Match {
                file: self.path.clone(),
                line,
                text,
            });
            if let Some(ref mut b) = self.budget {
                *b = b.saturating_sub(1);
            }
        }

        Ok(true)
    }

    fn context(&mut self, _: &Searcher, _: &SinkContext<'_>) -> Result<bool, Self::Error> {
        Ok(!self.budget_exhausted())
    }

    fn finish(&mut self, _: &Searcher, _: &SinkFinish) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Walk a directory and return all file paths.
///
/// # Errors
///
/// Returns [`Error::Io`] if root cannot be canonicalized.
/// Returns [`Error::Ignore`] if directory walking fails.
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
    fn parallel_scan_threshold_rayon_one_disables() {
        assert_eq!(parallel_scan_min_files_inner(8, Some(1)), usize::MAX);
    }

    #[test]
    fn parallel_scan_threshold_caps_at_cpus() {
        assert_eq!(parallel_scan_min_files_inner(4, Some(16)), 4);
    }

    #[test]
    fn parallel_scan_threshold_uses_rayon_when_lower() {
        assert_eq!(parallel_scan_min_files_inner(8, Some(4)), 4);
    }

    #[test]
    fn parallel_scan_threshold_single_cpu_no_parallel() {
        assert_eq!(parallel_scan_min_files_inner(1, None), usize::MAX);
    }

    #[test]
    fn parallel_scan_threshold_zero_rayon_ignored() {
        assert_eq!(parallel_scan_min_files_inner(8, Some(0)), 8);
    }

    #[test]
    fn fixed_string_substring_fast_path_matches_plain_regex() {
        let dir = tmp_corpus("fixed-fast");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "alpha beta\n").unwrap();
        fs::write(dir.join("b.txt"), "gamma\n").unwrap();

        let pat = vec!["beta".to_string()];
        let opts_fix = SearchOptions {
            flags: SearchMatchFlags::FIXED_STRINGS,
            max_results: None,
        };
        let q_fix = CompiledSearch::new(&pat, opts_fix).unwrap();
        let q_re = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();

        assert_eq!(
            q_fix.search_walk(&dir, None).unwrap(),
            q_re.search_walk(&dir, None).unwrap()
        );
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
        let hits = q.search_walk(&dir, None).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn search_finds_across_two_files() {
        let dir = tmp_corpus("two-files");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("a/x.txt"), "one\n").unwrap();
        fs::write(dir.join("b/y.txt"), "two\n").unwrap();
        let pat = vec!["o".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.search_walk(&dir, None).unwrap();
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn unsorted_candidates_match_sorted() {
        let dir = tmp_corpus("cand-order");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.txt"), "hit\n").unwrap();
        fs::write(dir.join("b.txt"), "hit\n").unwrap();
        let pat = vec!["hit".to_string()];
        let opts = SearchOptions::default();
        let sorted = vec![PathBuf::from("a.txt"), PathBuf::from("b.txt")];
        let unsorted = vec![PathBuf::from("b.txt"), PathBuf::from("a.txt")];
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let mut a = q.search_walk(&dir, Some(&sorted)).unwrap();
        let mut b = q.search_walk(&dir, Some(&unsorted)).unwrap();
        a.sort_by(|x, y| (&x.file, x.line, &x.text).cmp(&(&y.file, y.line, &y.text)));
        b.sort_by(|x, y| (&x.file, x.line, &x.text).cmp(&(&y.file, y.line, &y.text)));
        assert_eq!(a, b);
    }

    #[test]
    fn ignore_file_excludes_matches() {
        let dir = tmp_corpus("ignore");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".ignore"), "*.skip\n").unwrap();
        fs::write(dir.join("keep.txt"), "SECRET=1\n").unwrap();
        fs::write(dir.join("x.skip"), "SECRET=2\n").unwrap();
        let pat = vec!["SECRET".to_string()];
        let q = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        let hits = q.search_walk(&dir, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].file.ends_with("keep.txt"));
    }

    #[test]
    fn invert_match_selects_non_matching_lines() {
        let dir = tmp_corpus("invert");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("t.txt"), "aa\nbb\n").unwrap();
        let opts = SearchOptions {
            flags: SearchMatchFlags::INVERT_MATCH,
            max_results: None,
        };
        let pat = vec!["aa".to_string()];
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let hits = q.search_walk(&dir, None).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, "bb\n");
    }

    #[test]
    fn max_results_stops_after_n() {
        let dir = tmp_corpus("max");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("t.txt"), "a\na\na\n").unwrap();
        let opts = SearchOptions {
            flags: SearchMatchFlags::empty(),
            max_results: Some(2),
        };
        let pat = vec!["a".to_string()];
        let q = CompiledSearch::new(&pat, opts).unwrap();
        let hits = q.search_walk(&dir, None).unwrap();
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
        let hits = q.search_walk(&dir, None).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|m| m.text == "foo"));
    }

    #[test]
    fn walk_file_paths_lists_expected_files() {
        let dir = tmp_corpus("walk");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".ignore"), "x\n").unwrap();
        fs::write(dir.join("a.rs"), "").unwrap();
        fs::write(dir.join("x"), "").unwrap();
        let paths = walk_file_paths(&dir).unwrap();
        assert!(paths.iter().any(|p| p.ends_with("a.rs")));
        assert!(!paths
            .iter()
            .any(|p| p.as_path() == std::path::Path::new("x")));
    }
}
