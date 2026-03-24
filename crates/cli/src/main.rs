//! Thin CLI over `sift-core`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{
    value_parser, Arg, ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand,
};
use sift_core::{
    CaseMode, CompiledSearch, Index, IndexBuilder, SearchMatchFlags, SearchMode, SearchOptions,
    SearchOutput,
};

#[derive(Parser)]
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
    pattern: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    let mut patterns = Vec::new();
    for p in regexp {
        patterns.push(p.clone());
    }
    if let Some(file) = pattern_file {
        let content = std::fs::read_to_string(file)?;
        for line in content.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                patterns.push(trimmed.to_string());
            }
        }
    }
    if let Some(p) = pattern {
        patterns.push(p.to_string());
    }
    if patterns.is_empty() {
        anyhow::bail!("no pattern given");
    }
    Ok(patterns)
}

fn corpus_path_prefixes(
    index_root: &Path,
    cwd: &Path,
    requested: &[PathBuf],
) -> anyhow::Result<Vec<PathBuf>> {
    if requested.is_empty() {
        return Ok(vec![PathBuf::from("")]);
    }
    let mut out = Vec::new();
    for rel in requested {
        let abs = if rel.is_absolute() {
            rel.clone()
        } else {
            cwd.join(rel)
        };
        let abs = abs.canonicalize().unwrap_or(abs);
        let index_root = index_root
            .canonicalize()
            .unwrap_or_else(|_| index_root.to_path_buf());
        if !abs.starts_with(&index_root) {
            anyhow::bail!(
                "path {} is not under indexed corpus root {}",
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

fn resolve_case_mode(matches: &clap::ArgMatches) -> CaseMode {
    let case_flags = [
        ("ci", CaseMode::Insensitive),
        ("cs", CaseMode::Sensitive),
        ("sc", CaseMode::Smart),
    ];
    let mut last_idx = 0usize;
    let mut result = CaseMode::Sensitive;
    for (name, mode) in &case_flags {
        if let Some(mut indices) = matches.indices_of(name) {
            if let Some(last) = indices.next_back() {
                if last > last_idx {
                    last_idx = last;
                    result = *mode;
                }
            }
        }
    }
    result
}

fn search_options(cli: &Cli, case_mode: CaseMode) -> SearchOptions {
    let mut flags = SearchMatchFlags::empty();
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
        case_mode,
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

fn run_search(cli: &Cli, case_mode: CaseMode) -> anyhow::Result<bool> {
    let patterns = resolve_patterns(
        &cli.patterns.regexp,
        cli.patterns.pattern_file.as_deref(),
        cli.patterns.pattern.as_deref(),
    )?;
    let opts = search_options(cli, case_mode);
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
    let mut cmd = Cli::command();

    cmd = cmd
        .arg(
            Arg::new("ci")
                .short('i')
                .long("ignore-case")
                .action(ArgAction::Append)
                .num_args(0..=0)
                .value_parser(value_parser!(bool))
                .default_missing_value("true"),
        )
        .arg(
            Arg::new("cs")
                .short('s')
                .long("case-sensitive")
                .action(ArgAction::Append)
                .num_args(0..=0)
                .value_parser(value_parser!(bool))
                .default_missing_value("true"),
        )
        .arg(
            Arg::new("sc")
                .short('S')
                .long("smart-case")
                .action(ArgAction::Append)
                .num_args(0..=0)
                .value_parser(value_parser!(bool))
                .default_missing_value("true"),
        );

    let matches = match cmd.try_get_matches_from_mut(std::env::args_os()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }
    };

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

    let case_mode = resolve_case_mode(&matches);

    match run_search(&cli, case_mode) {
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
