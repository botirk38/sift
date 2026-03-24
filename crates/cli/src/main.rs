//! Thin CLI over `sift-core`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use sift_core::{
    CompiledSearch, Index, IndexBuilder, SearchMatchFlags, SearchMode, SearchOptions, SearchOutput,
};

#[derive(Parser)]
#[command(
    name = "sift",
    version,
    about = "Search the indexed corpus (ripgrep-like: PATTERN [PATH...]). Uses Rust regex unless -F. \
             Unlike ripgrep: search needs a prior `sift build`; the `build` subcommand updates the on-disk index."
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
    #[arg(short = 'e', long = "regexp", value_name = "PATTERN")]
    regexp: Vec<String>,
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    pattern_file: Option<PathBuf>,
    #[arg(value_name = "PATTERN")]
    pattern: Option<String>,
}

#[derive(Args)]
struct SearchScope {
    #[arg(value_name = "PATH", num_args = 0..)]
    paths: Vec<PathBuf>,
}

#[derive(Args)]
struct RegexFlagsA {
    #[arg(short = 'i', long)]
    ignore_case: bool,
    #[arg(short = 'v', long)]
    invert_match: bool,
    #[arg(short = 'w', long)]
    word_regexp: bool,
}

#[derive(Args)]
struct RegexFlagsB {
    #[arg(short = 'x', long)]
    line_regexp: bool,
    #[arg(short = 'F', long = "fixed-strings")]
    fixed_strings: bool,
}

#[derive(Args)]
struct OutputFlagsA {
    #[arg(short = 'n', long = "line-number")]
    line_number: bool,
    #[arg(short = 'c', long = "count")]
    count: bool,
    #[arg(short = 'l', long = "files-with-matches")]
    files_with_matches: bool,
}

#[derive(Args)]
struct OutputFlagsB {
    #[arg(short = 'L', long = "files-without-match")]
    files_without_match: bool,
    #[arg(short = 'o', long = "only-matching")]
    only_matching: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

#[derive(Args)]
struct OutputFlagsC {
    #[arg(long = "no-filename")]
    no_filename: bool,
}

#[derive(Args)]
struct PathArgs {
    #[arg(short = 'm', long = "max-count", value_name = "NUM")]
    max_count: Option<usize>,
    #[arg(long, default_value = ".sift")]
    sift_dir: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
}

fn resolve_patterns(
    regexp: &[String],
    pattern_file: Option<&Path>,
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
        out.push(
            abs.strip_prefix(&index_root)
                .expect("prefix checked")
                .to_path_buf(),
        );
    }
    Ok(out)
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

const fn search_mode(cli: &Cli) -> SearchMode {
    if cli.out2.only_matching && !cli.regex1.invert_match {
        SearchMode::OnlyMatching
    } else if cli.out1.count {
        SearchMode::Count
    } else if cli.out1.files_with_matches {
        SearchMode::FilesWithMatches
    } else if cli.out2.files_without_match {
        SearchMode::FilesWithoutMatch
    } else if cli.out2.quiet {
        SearchMode::Quiet
    } else {
        SearchMode::Standard
    }
}

fn run_search(cli: &Cli) -> anyhow::Result<bool> {
    let patterns = resolve_patterns(
        &cli.patterns.regexp,
        cli.patterns.pattern_file.as_deref(),
        cli.patterns.pattern.as_deref(),
    )?;
    let opts = search_options(cli);
    let query = CompiledSearch::new(&patterns, opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    let index = Index::open(&cli.paths.sift_dir)?;
    let cwd = std::env::current_dir()?;
    let prefixes = corpus_path_prefixes(&index.root, &cwd, &cli.search_scope.paths)?;
    let output = SearchOutput {
        mode: search_mode(cli),
        with_filename: !cli.out3.no_filename,
        line_number: cli.out1.line_number,
    };
    query
        .run_index(&index, &prefixes, output)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Some(Commands::Build { path }) = cli.command {
        return match IndexBuilder::new(&path)
            .with_dir(&cli.paths.sift_dir)
            .build()
        {
            Ok(_) => {
                eprintln!(
                    "indexed corpus {} → {}",
                    path.display(),
                    cli.paths.sift_dir.display()
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
            if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
                if ioe.kind() == std::io::ErrorKind::BrokenPipe {
                    return ExitCode::SUCCESS;
                }
            }
            eprintln!("sift: {e}");
            ExitCode::from(2)
        }
    }
}
