//! Thin CLI over `sift-core`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{value_parser, Arg, ArgAction, Args, Command, FromArgMatches, Parser, Subcommand};
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
    search_flags: SearchFlags,
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
}

#[derive(Args)]
struct OutputFlagsA {
    #[arg(short = 'n', long = "line-number")]
    line_number: bool,
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

#[derive(Clone)]
pub struct SearchFlags {
    pub case_mode: CaseMode,
    pub fixed_strings: bool,
}

fn resolve_case_mode_from_args(args: &[String]) -> CaseMode {
    let mut last_idx = 0usize;
    let mut result = CaseMode::Sensitive;
    let case_flags = [
        ("ci", CaseMode::Insensitive),
        ("cs", CaseMode::Sensitive),
        ("sc", CaseMode::Smart),
    ];
    for (i, arg) in args.iter().enumerate() {
        let bytes = arg.as_bytes();
        let is_short = bytes.len() == 2 && bytes[0] == b'-';
        let is_long = bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-';
        let flag = if is_short {
            match bytes.get(1) {
                Some(&b'i') => Some("ci"),
                Some(&b's') => Some("cs"),
                Some(&b'S') => Some("sc"),
                _ => None,
            }
        } else if is_long {
            let suffix = &bytes[2..];
            if suffix == b"ignore-case" {
                Some("ci")
            } else if suffix == b"case-sensitive" {
                Some("cs")
            } else if suffix == b"smart-case" {
                Some("sc")
            } else {
                None
            }
        } else {
            None
        };
        if let Some(name) = flag {
            for (id, mode) in &case_flags {
                if *id == name {
                    if i > last_idx {
                        last_idx = i;
                        result = *mode;
                    }
                    break;
                }
            }
        }
    }
    result
}

fn resolve_flag_from_args(args: &[String], short: Option<char>, long: &str) -> bool {
    for arg in args {
        if arg == "--" {
            return false;
        }
        let bytes = arg.as_bytes();
        if bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-' {
            let suffix = &arg[2..];
            if suffix == long {
                return true;
            }
        }
        if let Some(s) = short {
            if bytes.len() == 2 && bytes[0] == b'-' && bytes[1] == s as u8 {
                return true;
            }
        }
    }
    false
}

fn resolve_invert_match_from_args(args: &[String]) -> bool {
    for arg in args {
        if arg == "--" {
            return false;
        }
        let bytes = arg.as_bytes();
        let is_long = bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-';
        if is_long && &bytes[2..] == b"invert-match" {
            return true;
        }
        let is_short = bytes.len() == 2 && bytes[0] == b'-';
        if is_short && bytes[1] == b'v' {
            return true;
        }
    }
    false
}

#[allow(clippy::fn_params_excessive_bools)]
fn resolve_output_mode(
    invert_match: bool,
    count: bool,
    files_with_matches: bool,
    files_without_match: bool,
    only_matching: bool,
    quiet: bool,
) -> Result<SearchMode, String> {
    let mut modes = 0usize;
    if count {
        modes += 1;
    }
    if files_with_matches {
        modes += 1;
    }
    if files_without_match {
        modes += 1;
    }
    if only_matching {
        modes += 1;
    }
    if quiet {
        modes += 1;
    }

    if modes > 1 {
        return Err("conflicting output options specified".to_string());
    }

    if quiet {
        Ok(SearchMode::Quiet)
    } else if only_matching && invert_match {
        Ok(SearchMode::Count)
    } else if only_matching {
        Ok(SearchMode::OnlyMatching)
    } else if count {
        Ok(SearchMode::Count)
    } else if files_with_matches {
        Ok(SearchMode::FilesWithMatches)
    } else if files_without_match {
        Ok(SearchMode::FilesWithoutMatch)
    } else {
        Ok(SearchMode::Standard)
    }
}

impl Args for SearchFlags {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(
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
        )
        .arg(
            Arg::new("fixed_strings")
                .short('F')
                .long("fixed-strings")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("count")
                .short('c')
                .long("count")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files_with_matches")
                .short('l')
                .long("files-with-matches")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("files_without_match")
                .short('L')
                .long("files-without-match")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("only_matching")
                .short('o')
                .long("only-matching")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue),
        )
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

impl FromArgMatches for SearchFlags {
    fn from_arg_matches(matches: &clap::ArgMatches) -> Result<Self, clap::Error> {
        let args: Vec<String> = std::env::args().collect();
        let case_mode = resolve_case_mode_from_args(&args);
        let fixed_strings = matches.get_flag("fixed_strings");

        Ok(Self {
            case_mode,
            fixed_strings,
        })
    }

    fn update_from_arg_matches(&mut self, matches: &clap::ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl SearchFlags {
    fn to_options(&self) -> SearchOptions {
        let mut flags = SearchMatchFlags::empty();
        if self.fixed_strings {
            flags |= SearchMatchFlags::FIXED_STRINGS;
        }
        SearchOptions {
            flags,
            case_mode: self.case_mode,
            max_results: None,
        }
    }
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

fn run_search(cli: &Cli) -> anyhow::Result<bool> {
    let patterns = resolve_patterns(
        &cli.patterns.regexp,
        cli.patterns.pattern_file.as_deref(),
        cli.patterns.pattern.as_deref(),
    )?;

    let args: Vec<String> = std::env::args().collect();
    let count = resolve_flag_from_args(&args, Some('c'), "count");
    let files_with_matches = resolve_flag_from_args(&args, Some('l'), "files-with-matches");
    let files_without_match = resolve_flag_from_args(&args, Some('L'), "files-without-match");
    let only_matching = resolve_flag_from_args(&args, Some('o'), "only-matching");
    let quiet = resolve_flag_from_args(&args, Some('q'), "quiet");
    let invert_match = resolve_invert_match_from_args(&args);

    let mode = resolve_output_mode(
        invert_match,
        count,
        files_with_matches,
        files_without_match,
        only_matching,
        quiet,
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut opts = cli.search_flags.to_options();
    opts.max_results = cli.paths.max_count;
    if cli.regex1.invert_match {
        opts.flags |= SearchMatchFlags::INVERT_MATCH;
    }
    if cli.regex1.word_regexp {
        opts.flags |= SearchMatchFlags::WORD_REGEXP;
    }
    if cli.regex2.line_regexp {
        opts.flags |= SearchMatchFlags::LINE_REGEXP;
    }
    if only_matching {
        opts.flags |= SearchMatchFlags::ONLY_MATCHING;
    }

    let output = SearchOutput {
        mode,
        with_filename: !cli.out3.no_filename,
        line_number: cli.out1.line_number,
    };

    let query = CompiledSearch::new(&patterns, opts).map_err(|e| anyhow::anyhow!("{e}"))?;
    let index = Index::open(&cli.paths.sift_dir)?;
    let cwd = std::env::current_dir()?;
    let prefixes = corpus_path_prefixes(&index.root, &cwd, &cli.search_scope.paths)?;
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
