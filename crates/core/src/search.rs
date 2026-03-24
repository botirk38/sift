//! Naive full-corpus search: `ignore::WalkBuilder` + byte line scan + `regex::bytes::Regex`.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use bitflags::bitflags;
use ignore::WalkBuilder;
use rayon::prelude::*;
use regex::bytes::Regex;

use crate::planner::TrigramPlan;
use crate::verify;
use crate::Index;

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
        parallel_scan_min_files_inner(cpus, rayon_threads)
    })
}

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

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct SearchMatchFlags: u8 {
        const CASE_INSENSITIVE = 1 << 0;
        const INVERT_MATCH = 1 << 1;
        const FIXED_STRINGS = 1 << 2;
        const WORD_REGEXP = 1 << 3;
        const LINE_REGEXP = 1 << 4;
        const ONLY_MATCHING = 1 << 5;
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

#[derive(Debug, Clone)]
pub struct CompiledSearch {
    re: Regex,
    opts: SearchOptions,
    patterns: Vec<String>,
    plan: TrigramPlan,
    substring_literals: Option<Vec<Vec<u8>>>,
}

impl CompiledSearch {
    /// Compile patterns and options once.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::EmptyPatterns`] if `patterns` is empty, or [`crate::Error::Regex`]
    /// if compilation fails.
    pub fn new(patterns: &[String], opts: SearchOptions) -> crate::Result<Self> {
        if patterns.is_empty() {
            return Err(crate::Error::EmptyPatterns);
        }
        let re = verify::compile_search_pattern(patterns, &opts)?;
        let plan = TrigramPlan::for_patterns(patterns, &opts);
        let substring_literals = if opts.fixed_strings()
            && !opts.case_insensitive()
            && !opts.word_regexp()
            && !opts.line_regexp()
        {
            Some(patterns.iter().map(|p| p.as_bytes().to_vec()).collect())
        } else {
            None
        };
        Ok(Self {
            re,
            opts,
            patterns: patterns.to_vec(),
            plan,
            substring_literals,
        })
    }

    /// Search using an open [`Index`] (trigram narrowing when applicable).
    ///
    /// # Errors
    ///
    /// Same as [`Self::search_walk`].
    pub fn search_index(&self, index: &Index) -> crate::Result<Vec<Match>> {
        match &self.plan {
            TrigramPlan::FullScan => {
                search_files_impl(self, &index.root, Some(index.files.as_slice()))
            }
            TrigramPlan::Narrow { arms } => {
                let cands = index.candidate_file_ids(arms.as_slice());
                if cands.is_empty() {
                    return Ok(Vec::new());
                }
                let paths: Vec<PathBuf> = cands
                    .iter()
                    .filter_map(|&id| index.files.get(id as usize).cloned())
                    .collect();
                search_files_impl(self, &index.root, Some(&paths))
            }
        }
    }

    /// Walk `root` with ignore rules (or scan only `candidates` when `Some`).
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::Io`] if the corpus root cannot be canonicalized, or [`crate::Error::Ignore`]
    /// for directory walk failures.
    pub fn search_walk(
        &self,
        root: &Path,
        candidates: Option<&[PathBuf]>,
    ) -> crate::Result<Vec<Match>> {
        let root = root.canonicalize()?;
        search_files_impl(self, &root, candidates)
    }

    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

fn search_files_impl(
    compiled: &CompiledSearch,
    root: &Path,
    candidates: Option<&[PathBuf]>,
) -> crate::Result<Vec<Match>> {
    if let Some(set) = candidates {
        if set.is_empty() {
            return Ok(Vec::new());
        }
    }
    let mut out = Vec::new();
    let mut budget = compiled.opts.max_results;

    if let Some(set) = candidates {
        let sorted = set.len() <= 1 || set.windows(2).all(|w| w[0] <= w[1]);
        if compiled.opts.max_results.is_none() && set.len() >= parallel_candidate_min_files() {
            let paths: Vec<PathBuf> = if sorted {
                set.to_vec()
            } else {
                let mut v = set.to_vec();
                v.sort();
                v
            };
            return Ok(parallel_scan_candidate_files(compiled, root, &paths));
        }
        if sorted {
            'subset: for display in set {
                if budget == Some(0) {
                    break 'subset;
                }
                let path = root.join(display);
                if !path.is_file() {
                    continue;
                }
                if scan_lines(display, &path, compiled, &mut budget, &mut out) {
                    break 'subset;
                }
            }
        } else {
            let mut paths: Vec<PathBuf> = set.to_vec();
            paths.sort();
            'subset: for display in paths {
                if budget == Some(0) {
                    break 'subset;
                }
                let path = root.join(&display);
                if !path.is_file() {
                    continue;
                }
                if scan_lines(&display, &path, compiled, &mut budget, &mut out) {
                    break 'subset;
                }
            }
        }
        return Ok(out);
    }

    let walker = WalkBuilder::new(root).follow_links(false).build();

    'files: for entry in walker {
        let entry = entry.map_err(crate::Error::Ignore)?;
        if !entry.path().is_file() {
            continue;
        }
        let path = entry.path();
        let display = path.strip_prefix(root).unwrap_or(path).to_path_buf();

        if scan_lines(&display, path, compiled, &mut budget, &mut out) {
            break 'files;
        }
    }

    Ok(out)
}

fn parallel_scan_candidate_files(
    compiled: &CompiledSearch,
    root: &Path,
    paths: &[PathBuf],
) -> Vec<Match> {
    let chunks: Vec<Vec<Match>> = paths
        .par_iter()
        .map(|display| {
            let path = root.join(display);
            if !path.is_file() {
                return Vec::new();
            }
            let mut out = Vec::new();
            let mut budget = None;
            let _ = scan_lines(display, &path, compiled, &mut budget, &mut out);
            out
        })
        .collect();
    let mut matches: Vec<Match> = chunks.into_iter().flatten().collect();
    matches.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.text.cmp(&b.text))
    });
    matches
}

fn bytes_contains_any(line: &[u8], needles: &[Vec<u8>]) -> bool {
    needles
        .iter()
        .any(|n| memchr::memmem::find(line, n).is_some())
}

fn scan_lines(
    display: &Path,
    path: &Path,
    compiled: &CompiledSearch,
    budget: &mut Option<usize>,
    out: &mut Vec<Match>,
) -> bool {
    let re = &compiled.re;
    let opts = compiled.opts;
    let literals = compiled.substring_literals.as_deref();
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut line_no = 0usize;

    loop {
        if *budget == Some(0) {
            return true;
        }
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if n == 0 {
                    break;
                }
            }
        }
        line_no += 1;
        while line.len() > 1 && (line[line.len() - 1] == b'\n' || line[line.len() - 1] == b'\r') {
            line.pop();
        }
        if line.len() == 1 && line[0] == b'\n' {
            line.pop();
        }

        let matched = match literals {
            Some(needles) if !opts.only_matching() => bytes_contains_any(&line, needles),
            Some(needles) if !bytes_contains_any(&line, needles) => false,
            _ => re.is_match(&line),
        };

        let take = if opts.invert_match() {
            !matched
        } else {
            matched
        };

        if !take {
            continue;
        }

        if opts.only_matching() && !opts.invert_match() {
            for m in re.find_iter(&line) {
                if *budget == Some(0) {
                    return true;
                }
                out.push(Match {
                    file: display.to_path_buf(),
                    line: line_no,
                    text: String::from_utf8_lossy(m.as_bytes()).into_owned(),
                });
                if let Some(b) = *budget {
                    *budget = Some(b - 1);
                }
            }
        } else {
            out.push(Match {
                file: display.to_path_buf(),
                line: line_no,
                text: String::from_utf8_lossy(&line).into_owned(),
            });
            if let Some(b) = *budget {
                *budget = Some(b - 1);
            }
        }
    }
    false
}

/// All readable file paths under `root` (respecting ignore rules), relative to `root`.
///
/// # Errors
///
/// Propagates [`crate::Error::Io`] and [`crate::Error::Ignore`] from canonicalization / walking.
pub fn walk_file_paths(root: &Path) -> crate::Result<HashSet<PathBuf>> {
    let root = root.canonicalize()?;
    let mut set = HashSet::new();
    let walker = WalkBuilder::new(&root).follow_links(false).build();
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
        assert!(q_fix.substring_literals.is_some());

        let q_re = CompiledSearch::new(&pat, SearchOptions::default()).unwrap();
        assert!(q_re.substring_literals.is_none());

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
        let a = q.search_walk(&dir, Some(&sorted)).unwrap();
        let b = q.search_walk(&dir, Some(&unsorted)).unwrap();
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
        assert_eq!(hits[0].text, "bb");
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
