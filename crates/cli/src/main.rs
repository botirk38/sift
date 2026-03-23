//! Thin CLI over `sift-core` — ripgrep-shaped invocation: `PATTERN [PATH...]`, plus `sift build`.
//!
//! Patterns use the Rust `regex` dialect (ERE-like), except `-F` (fixed string). See `--help`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use sift_core::{
    build_index, walk_file_paths, CompiledSearch, Index, Match, SearchMatchFlags, SearchOptions,
};

#[derive(Parser)]
#[command(
    name = "sift",
    version,
    about = "Search the indexed corpus (ripgrep-like: PATTERN [PATH...]). Uses Rust regex unless -F. \
             Unlike ripgrep: search needs a prior `sift build` (or same workflow); the `build` \
             subcommand updates the on-disk index. Literal `build` as a pattern: use -e build or -- build."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[command(flatten)]
    patterns: PatternArgs,
    #[command(flatten)]
    search_scope: SearchScope,
    #[command(flatten)]
    regex1: RegexFlagsA,
    #[command(flatten)]
    regex2: RegexFlagsB,
    #[command(flatten)]
    out1: OutputFlagsA,
    #[command(flatten)]
    out2: OutputFlagsB,
    #[command(flatten)]
    out3: OutputFlagsC,
    #[command(flatten)]
    paths: PathArgs,
}

#[derive(Args)]
struct PatternArgs {
    /// Use PATTERN for matching (repeatable; OR like grep).
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    regexp: Vec<String>,

    /// Read patterns from file (one per line; `#` starts a comment).
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pattern_file: Option<PathBuf>,

    #[arg(value_name = "PATTERN")]
    pattern: Option<String>,
}

/// Optional path roots to search (ripgrep-style); relative to current dir, must lie under the corpus.
#[derive(Args)]
struct SearchScope {
    #[arg(value_name = "PATH", num_args = 0..)]
    paths: Vec<PathBuf>,
}

#[derive(Args)]
struct RegexFlagsA {
    #[arg(short = 'i', long, help = "Ignore case")]
    ignore_case: bool,

    #[arg(short = 'v', long, help = "Select non-matching lines")]
    invert_match: bool,

    #[arg(short = 'w', long, help = "Match whole words")]
    word_regexp: bool,
}

#[derive(Args)]
struct RegexFlagsB {
    #[arg(short = 'x', long, help = "Match whole lines")]
    line_regexp: bool,

    #[arg(short = 'F', long = "fixed-strings", help = "Fixed strings, not regex")]
    fixed_strings: bool,
}

#[derive(Args)]
struct OutputFlagsA {
    #[arg(short = 'n', long = "line-number", help = "Print line numbers")]
    line_number: bool,

    #[arg(short = 'c', long = "count", help = "Print match counts per file")]
    count: bool,

    #[arg(
        short = 'l',
        long = "files-with-matches",
        help = "Only print filenames with a match"
    )]
    files_with_matches: bool,
}

#[derive(Args)]
struct OutputFlagsB {
    #[arg(
        short = 'L',
        long = "files-without-match",
        help = "Only print filenames with no match"
    )]
    files_without_match: bool,

    #[arg(
        short = 'o',
        long = "only-matching",
        help = "Only print matched parts of a line"
    )]
    only_matching: bool,

    #[arg(
        short = 'q',
        long = "quiet",
        help = "Quiet; exit 0 if any match, 1 otherwise"
    )]
    quiet: bool,
}

#[derive(Args)]
struct OutputFlagsC {
    /// Suppress file names (grep `-h`; here `--no-filename` because `-h` is reserved for help).
    #[arg(long = "no-filename")]
    no_filename: bool,
}

#[derive(Args)]
struct PathArgs {
    #[arg(
        short = 'm',
        long = "max-count",
        value_name = "NUM",
        help = "Stop after NUM matches total"
    )]
    max_count: Option<usize>,

    #[arg(long, default_value = ".index", help = "Index directory")]
    index: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Build or refresh the trigram index for a corpus root (writes `--index` dir)
    Build {
        #[arg(default_value = ".", help = "Corpus root to index")]
        path: PathBuf,
    },
}

fn resolve_patterns(
    regexp: &[String],
    pattern_file: Option<&std::path::Path>,
    positional: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let mut v = Vec::new();
    if let Some(path) = pattern_file {
        let s = std::fs::read_to_string(path)?;
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            v.push(line.to_string());
        }
    }
    v.extend(regexp.iter().cloned());
    if let Some(p) = positional {
        v.push(p.to_string());
    }
    if v.is_empty() {
        anyhow::bail!("no patterns: use -e, -f, or PATTERN");
    }
    Ok(v)
}

/// Corpus-relative directory prefixes; empty means entire corpus.
fn corpus_path_prefixes(
    index_root: &Path,
    cwd: &Path,
    user_paths: &[PathBuf],
) -> anyhow::Result<Vec<PathBuf>> {
    let index_root = index_root.canonicalize()?;
    if user_paths.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for p in user_paths {
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            cwd.join(p)
        };
        let abs = abs
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("could not resolve path {}: {e}", abs.display()))?;
        if !abs.starts_with(&index_root) {
            anyhow::bail!(
                "search path {} is not under indexed corpus root {}",
                abs.display(),
                index_root.display()
            );
        }
        let rel = abs
            .strip_prefix(&index_root)
            .expect("prefix checked")
            .to_path_buf();
        out.push(rel);
    }
    Ok(out)
}

fn path_in_scope(rel: &Path, prefixes: &[PathBuf]) -> bool {
    if prefixes.is_empty() {
        return true;
    }
    prefixes
        .iter()
        .any(|pre| rel.starts_with(pre) || rel.as_os_str() == pre.as_os_str())
}

fn filter_matches(hits: Vec<Match>, prefixes: &[PathBuf]) -> Vec<Match> {
    if prefixes.is_empty() {
        return hits;
    }
    hits.into_iter()
        .filter(|m| path_in_scope(&m.file, prefixes))
        .collect()
}

fn filter_path_set(set: HashSet<PathBuf>, prefixes: &[PathBuf]) -> HashSet<PathBuf> {
    if prefixes.is_empty() {
        return set;
    }
    set.into_iter()
        .filter(|p| path_in_scope(p, prefixes))
        .collect()
}

fn count_lines_per_file(matches: &[Match], only_matching: bool) -> HashMap<PathBuf, usize> {
    if !only_matching {
        let mut m = HashMap::new();
        for hit in matches {
            *m.entry(hit.file.clone()).or_insert(0) += 1;
        }
        return m;
    }
    let mut seen: HashSet<(PathBuf, usize)> = HashSet::new();
    let mut m = HashMap::new();
    for hit in matches {
        if seen.insert((hit.file.clone(), hit.line)) {
            *m.entry(hit.file.clone()).or_insert(0) += 1;
        }
    }
    m
}

fn print_match(m: &Match, show_path: bool, line_number: bool) {
    if show_path {
        if line_number {
            print!("{}:{}:", m.file.display(), m.line);
        } else {
            print!("{}:", m.file.display());
        }
    } else if line_number {
        print!("{}:", m.line);
    }
    println!("{}", m.text);
}

fn search_options(cli: &Cli) -> SearchOptions {
    let mut flags = SearchMatchFlags::empty();
    if cli.regex1.ignore_case {
        flags |= SearchMatchFlags::CASE_INSENSITIVE;
    }
    if cli.regex1.invert_match {
        flags |= SearchMatchFlags::INVERT_MATCH;
    }
    if cli.regex1.word_regexp {
        flags |= SearchMatchFlags::WORD_REGEXP;
    }
    if cli.regex2.line_regexp {
        flags |= SearchMatchFlags::LINE_REGEXP;
    }
    if cli.regex2.fixed_strings {
        flags |= SearchMatchFlags::FIXED_STRINGS;
    }
    if cli.out2.only_matching && !cli.regex1.invert_match {
        flags |= SearchMatchFlags::ONLY_MATCHING;
    }

    SearchOptions {
        flags,
        max_results: cli.paths.max_count,
    }
}

/// `true` if grep should exit 0 (something matched / output produced where relevant).
fn run_search(cli: &Cli) -> anyhow::Result<bool> {
    let patterns = resolve_patterns(
        &cli.patterns.regexp,
        cli.patterns.pattern_file.as_deref(),
        cli.patterns.pattern.as_deref(),
    )?;

    let opts = search_options(cli);

    let index = Index::open(&cli.paths.index)?;
    let cwd = std::env::current_dir()?;
    let prefixes = corpus_path_prefixes(&index.root, &cwd, &cli.search_scope.paths)?;

    let query = CompiledSearch::new(&patterns, opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut hits = query.search_index(&index)?;
    hits = filter_matches(hits, &prefixes);

    let has_match = !hits.is_empty();

    if cli.out2.quiet {
        return Ok(has_match);
    }

    if cli.out1.files_with_matches {
        let mut seen = HashSet::new();
        for m in &hits {
            if seen.insert(&m.file) {
                println!("{}", m.file.display());
            }
        }
        return Ok(has_match);
    }

    if cli.out2.files_without_match {
        let all = walk_file_paths(&index.root)?;
        let all = filter_path_set(all, &prefixes);
        let mut files_with_hits = HashSet::new();
        for m in &hits {
            files_with_hits.insert(m.file.clone());
        }
        let mut rest: Vec<_> = all.difference(&files_with_hits).cloned().collect();
        rest.sort();
        for p in &rest {
            println!("{}", p.display());
        }
        return Ok(!rest.is_empty());
    }

    if cli.out1.count {
        let counts = count_lines_per_file(&hits, cli.out2.only_matching);
        let all = walk_file_paths(&index.root)?;
        let all = filter_path_set(all, &prefixes);
        let mut paths: Vec<_> = all.into_iter().collect();
        paths.sort();
        for p in paths {
            let n = counts.get(&p).copied().unwrap_or(0);
            println!("{}:{}", p.display(), n);
        }
        return Ok(true);
    }

    let show_path = !cli.out3.no_filename;

    for m in &hits {
        print_match(m, show_path, cli.out1.line_number);
    }

    Ok(has_match)
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(Commands::Build { path }) = cli.command {
        return match build_index(&path, &cli.paths.index) {
            Ok(()) => {
                eprintln!(
                    "indexed corpus {} → {}",
                    path.display(),
                    cli.paths.index.display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("sift: {e}");
                ExitCode::from(2)
            }
        };
    }

    match run_search(&cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("sift: {e}");
            ExitCode::from(2)
        }
    }
}
