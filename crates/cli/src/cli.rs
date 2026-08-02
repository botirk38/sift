use std::process::ExitCode;

use crate::grep::Argv;
use crate::grep::engine::{EngineDecl, MultilineDecl, ThreadingDecl, WalkerDecl};
use crate::grep::filter::FilterResolution;
use crate::grep::filter::{FilterConfig, FilterDecl, GlobFlags, TypeCatalog};
use crate::grep::ignore::MessageFlags;
use crate::grep::ignore::{
    ContextDecl, IgnoreDotDecl, IgnoreExcludeDecl, IgnoreFilesDecl, IgnoreGitDecl,
    IgnoreGlobalDecl, IgnoreMessagesDecl, IgnoreNoDecl, IgnoreParentDecl, IgnoreResolution,
    IgnoreVcsDecl, MessagesDecl, UnrestrictedDecl,
};
use crate::grep::input::ContentTransformConfig;
use crate::grep::output::OutputArgv;
use crate::grep::output::OutputDecl;
use crate::grep::output::{
    ColumnDecl, ColumnsDecl, ExtraOutputDecl, FilenameDecl, HeadingDecl, JsonDecl, LineNumberDecl,
    NullColorDecl, ReplaceDecl, SeparatorDecl, StatsDecl,
};
use crate::grep::paths::PathArgs;
use crate::grep::pattern::PatternArgv;
use crate::grep::pattern::PatternDecl;
use crate::grep::pattern::{
    BinaryDecl, GrepFlags, GrepScope, PatternArgs, RegexFlagsA, RegexFlagsB,
};
use crate::grep::run::{Run, RunConfig, RunResult};
use crate::index::{
    IndexDecl, IndexExecution, IndexJob, IndexOperation, IndexRequest, IndexSelection,
};
use crate::update;
use sift_core::SearchMode;
use sift_core::ZeroCounts;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub patterns: PatternArgs,
    #[command(flatten)]
    pub search_scope: GrepScope,
    #[command(flatten)]
    pub regex1: RegexFlagsA,
    #[command(flatten)]
    pub regex2: RegexFlagsB,
    #[command(flatten)]
    pub line_number_decl: LineNumberDecl,
    #[command(flatten)]
    pub search_flags: GrepFlags,
    #[command(flatten)]
    pub filename_decl: FilenameDecl,
    #[command(flatten)]
    pub heading_decl: HeadingDecl,
    #[command(flatten)]
    pub column_decl: ColumnDecl,
    #[command(flatten)]
    pub columns_decl: ColumnsDecl,
    #[command(flatten)]
    pub glob_flags: GlobFlags,
    #[command(flatten)]
    pub ignore_no: IgnoreNoDecl,
    #[command(flatten)]
    pub ignore_vcs: IgnoreVcsDecl,
    #[command(flatten)]
    pub ignore_dot: IgnoreDotDecl,
    #[command(flatten)]
    pub ignore_git: IgnoreGitDecl,
    #[command(flatten)]
    pub ignore_global: IgnoreGlobalDecl,
    #[command(flatten)]
    pub ignore_exclude: IgnoreExcludeDecl,
    #[command(flatten)]
    pub ignore_parent: IgnoreParentDecl,
    #[command(flatten)]
    pub ignore_files_decl: IgnoreFilesDecl,
    #[command(flatten)]
    pub messages_decl: MessagesDecl,
    #[command(flatten)]
    pub ignore_messages_decl: IgnoreMessagesDecl,
    #[command(flatten)]
    pub unrestricted: UnrestrictedDecl,
    #[command(flatten)]
    pub context_decl: ContextDecl,
    #[command(flatten)]
    pub null_color: NullColorDecl,
    #[command(flatten)]
    pub paths: PathArgs,
    #[command(flatten)]
    pub stats_decl: StatsDecl,
    #[command(flatten)]
    pub json_decl: JsonDecl,
    #[command(flatten)]
    pub separator_decl: SeparatorDecl,
    #[command(flatten)]
    pub filter_decl: FilterDecl,
    #[command(flatten)]
    pub binary_decl: BinaryDecl,
    #[command(flatten)]
    pub replace_decl: ReplaceDecl,
    #[command(flatten)]
    pub extra_output: ExtraOutputDecl,
    #[command(flatten)]
    pub threading: ThreadingDecl,
    #[command(flatten)]
    pub multiline_decl: MultilineDecl,
    #[command(flatten)]
    pub walker_decl: WalkerDecl,
    #[command(flatten)]
    pub engine_decl: EngineDecl,
}

impl Cli {
    #[must_use]
    pub fn pattern_config(&self) -> PatternDecl {
        PatternDecl {
            patterns: self.patterns.clone(),
            search_flags: self.search_flags.clone(),
            regex1: self.regex1.clone(),
            regex2: self.regex2.clone(),
            multiline: self.multiline_decl.clone(),
            engine: self.engine_decl.clone(),
            binary: self.binary_decl.clone(),
            replace: self.replace_decl.clone(),
            max_count: self.paths.max_count,
        }
    }

    #[must_use]
    pub fn filter_config(&self) -> FilterConfig {
        FilterConfig {
            decl: self.filter_decl.clone(),
            glob_patterns: self.glob_flags.glob.clone(),
            follow_links: self.paths.follow,
            one_file_system: self.walker_decl.one_file_system,
        }
    }

    #[must_use]
    fn output_config(&self, search_paths: &[PathBuf]) -> OutputDecl {
        OutputDecl {
            column: self.column_decl.clone(),
            columns: self.columns_decl.clone(),
            extra: self.extra_output.clone(),
            replace_trim: self.replace_decl.trim,
            path_separator: self.threading.path_separator.clone(),
            colors: self.engine_decl.colors.clone(),
            hyperlink_format: self.engine_decl.hyperlink_format.clone(),
            hostname_bin: self.engine_decl.hostname_bin.clone(),
            line_number: self.line_number_decl.line_number,
            separators: self.separator_decl.clone(),
            search_paths: search_paths.to_vec(),
            null_data: self.multiline_decl.line_terminator.null_data,
        }
    }

    /// Build a resolved run configuration from parsed CLI state and argv ordering.
    ///
    /// # Errors
    ///
    /// Returns an error if sort/order flags or type filters are invalid.
    pub fn run_config(&self, argv: &Argv<'_>) -> Result<RunConfig, anyhow::Error> {
        let search_paths = self.search_scope.paths.clone();
        let zeros = if self.extra_output.include_zero {
            ZeroCounts::Include
        } else {
            ZeroCounts::Omit
        };
        let mut pattern_argv = PatternArgv::resolve(argv, zeros);
        let search_mode = if self.filter_decl.files {
            SearchMode::Paths
        } else {
            pattern_argv.mode
        };
        pattern_argv.mode = search_mode;
        let filter_resolution = FilterResolution::resolve(argv);
        let type_catalog = TypeCatalog::from_argv(argv)?;
        let type_filters = type_catalog.filters(argv)?;
        let output_argv = OutputArgv::resolve(argv);
        let ignore = IgnoreResolution::resolve(argv);
        let file_order = self.filter_decl.file_order(argv)?;
        Ok(RunConfig {
            pattern: self.pattern_config(),
            pattern_argv,
            filter: self.filter_config(),
            filter_resolution,
            type_filters,
            output: self.output_config(&search_paths),
            output_argv,
            sift_dir: self.paths.sift_dir.clone(),
            search_paths,
            threads: self.threading.threads,
            content: ContentTransformConfig {
                search_zip: self.engine_decl.content.search_zip,
                pre: self.engine_decl.content.pre.clone(),
                pre_globs: self.engine_decl.content.pre_glob.clone(),
            },
            file_order,
            search_mode,
            ignore,
        })
    }

    #[must_use]
    pub fn dispatch(self, argv: &Argv<'_>) -> ExitCode {
        if self.engine_decl.regex.pcre2_version {
            let (major, minor) = grep_pcre2::version();
            println!("PCRE2 {major}.{minor}");
            return ExitCode::SUCCESS;
        }

        if self.filter_decl.type_list {
            return match TypeCatalog::from_argv(argv) {
                Ok(catalog) => {
                    catalog.print_list();
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("sift: {e}");
                    ExitCode::from(2)
                }
            };
        }

        match self.command {
            Some(Commands::Update) => match update::run_binary_update() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("sift: {e}");
                    ExitCode::from(2)
                }
            },
            Some(Commands::Index { command }) => {
                let daemon = self.paths.daemon();
                let indexes = match IndexSelection::resolve(argv) {
                    Ok(selection) => selection.map(|s| s.indexes),
                    Err(e) => {
                        eprintln!("sift: {e}");
                        return ExitCode::from(2);
                    }
                };
                let (path, operation, execution, build_coverage) = command.into_request_parts();
                let req = IndexRequest {
                    operation,
                    execution,
                    build_coverage,
                    path,
                    indexes,
                    sift_dir: self.paths.sift_dir,
                    follow_links: self.paths.follow,
                    one_file_system: self.walker_decl.one_file_system,
                    max_depth: self.filter_decl.max_depth,
                    max_filesize: self.filter_decl.max_filesize,
                };
                let ignore = IgnoreResolution::resolve(argv);
                match IndexJob::resolve(req, ignore) {
                    Ok(index) => index.run(daemon.as_ref()),
                    Err(e) => {
                        eprintln!("sift: {e}");
                        ExitCode::from(2)
                    }
                }
            }
            None => {
                let daemon = self.paths.daemon();
                let config = match self.run_config(argv) {
                    Ok(config) => config,
                    Err(e) => {
                        eprintln!("sift: {e}");
                        return ExitCode::from(2);
                    }
                };
                let suppress_errors = config.ignore.msg_flags.contains(MessageFlags::NO_MESSAGES);
                let run = Run::new(config);
                Self::exit_from_run(run.execute(daemon.as_ref()), suppress_errors)
            }
        }
    }

    fn exit_from_run(
        result: Result<RunResult, anyhow::Error>,
        suppress_error_messages: bool,
    ) -> ExitCode {
        match result {
            Ok(outcome) if outcome.succeeded() => ExitCode::SUCCESS,
            Ok(RunResult::Found | RunResult::NotFound) => ExitCode::from(1),
            Err(e) => {
                if let Some(ioe) = e.downcast_ref::<std::io::Error>()
                    && ioe.kind() == std::io::ErrorKind::BrokenPipe
                {
                    return ExitCode::SUCCESS;
                }
                if !suppress_error_messages {
                    eprintln!("sift: {e}");
                }
                ExitCode::from(2)
            }
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Download and install the latest release over the current binary.
    Update,
    Index {
        #[command(subcommand)]
        command: IndexCommands,
    },
}

#[derive(Subcommand)]
pub enum IndexCommands {
    /// Create an index at `--sift-dir` (fails if an index already exists).
    Build {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[command(flatten)]
        indexes: IndexDecl,

        /// Block until the index build completes (default: async via daemon).
        #[arg(long)]
        wait: bool,

        /// Build a lazy index that may be completed incrementally by searches.
        #[arg(long)]
        lazy: bool,
    },
    /// Incrementally refresh an existing index (fails if no index exists).
    Update {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[command(flatten)]
        indexes: IndexDecl,

        /// Block until the index update completes.
        #[arg(long)]
        wait: bool,
    },
}

impl IndexCommands {
    fn into_request_parts(
        self,
    ) -> (
        PathBuf,
        IndexOperation,
        IndexExecution,
        sift_core::IndexCoverage,
    ) {
        match self {
            Self::Build {
                path,
                indexes: _,
                wait,
                lazy,
            } => {
                let execution = if wait {
                    IndexExecution::Blocking
                } else {
                    IndexExecution::Background
                };
                let coverage = if lazy {
                    sift_core::IndexCoverage::Lazy
                } else {
                    sift_core::IndexCoverage::Complete
                };
                (path, IndexOperation::Build, execution, coverage)
            }
            Self::Update {
                path,
                indexes: _,
                wait,
            } => {
                let execution = if wait {
                    IndexExecution::Blocking
                } else {
                    IndexExecution::Background
                };
                (
                    path,
                    IndexOperation::Update,
                    execution,
                    sift_core::IndexCoverage::Complete,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::path::Path;

    #[test]
    fn cli_parses_positional_pattern() {
        let cli = Cli::try_parse_from(["sift", "pattern"]).unwrap();
        assert_eq!(cli.patterns.pattern.as_deref(), Some("pattern"));
    }

    #[test]
    fn cli_parses_regexp_flag() {
        let cli = Cli::try_parse_from(["sift", "-e", "pattern"]).unwrap();
        assert_eq!(cli.patterns.regexp, vec!["pattern"]);
    }

    #[test]
    fn cli_parses_multiple_regexp_flags() {
        let cli = Cli::try_parse_from(["sift", "-e", "foo", "-e", "bar"]).unwrap();
        assert_eq!(cli.patterns.regexp, vec!["foo", "bar"]);
    }

    #[test]
    fn cli_parses_update_subcommand() {
        let cli = Cli::try_parse_from(["sift", "update"]).unwrap();
        assert!(matches!(cli.command, Some(Commands::Update)));
    }

    #[test]
    fn cli_parses_index_build_subcommand() {
        let cli = Cli::try_parse_from(["sift", "index", "build"]).unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { path, .. },
            }) => assert_eq!(path, PathBuf::from(".")),
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_parses_index_build_subcommand_with_path() {
        let cli = Cli::try_parse_from(["sift", "index", "build", "/tmp"]).unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { path, .. },
            }) => assert_eq!(path, PathBuf::from("/tmp")),
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_parses_index_kind_and_options() {
        let cli = Cli::try_parse_from([
            "sift",
            "index",
            "build",
            "--index",
            "ngram",
            "--width",
            "5",
            "--norm",
            "ascii-lower",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { indexes, .. },
            }) => {
                assert_eq!(indexes.kinds.len(), 1);
                assert_eq!(indexes.width, vec![5]);
                assert_eq!(indexes.norms, vec![crate::index::GramNormArg::AsciiLower]);
            }
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_parses_index_update_subcommand() {
        let cli = Cli::try_parse_from(["sift", "index", "update"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Index {
                command: IndexCommands::Update { .. }
            })
        ));
    }

    #[test]
    fn cli_parses_no_search_scope() {
        let cli = Cli::try_parse_from(["sift", "pattern"]).unwrap();
        assert!(cli.search_scope.paths.is_empty());
    }

    #[test]
    fn cli_parses_search_scope() {
        let cli = Cli::try_parse_from(["sift", "pattern", "src/"]).unwrap();
        assert_eq!(cli.search_scope.paths, vec![PathBuf::from("src/")]);
    }

    #[test]
    fn cli_parses_multiple_paths() {
        let cli = Cli::try_parse_from(["sift", "pattern", "src/", "tests/"]).unwrap();
        assert_eq!(
            cli.search_scope.paths,
            vec![PathBuf::from("src/"), PathBuf::from("tests/")]
        );
    }

    #[test]
    fn cli_parses_version_flag() {
        let result = Cli::try_parse_from(["sift", "--version"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_rejects_unknown_flags() {
        let result = Cli::try_parse_from(["sift", "--nonexistent-flag"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_pattern_and_search_scope() {
        let cli = Cli::try_parse_from(["sift", "pat", "dir1", "dir2"]).unwrap();
        assert_eq!(cli.patterns.pattern.as_deref(), Some("pat"));
        assert_eq!(
            cli.search_scope.paths,
            vec![PathBuf::from("dir1"), PathBuf::from("dir2")]
        );
    }

    #[test]
    fn cli_parses_short_flags_before_pattern() {
        let cli = Cli::try_parse_from(["sift", "-n", "-H", "pattern"]).unwrap();
        assert!(cli.line_number_decl.line_number);
        assert!(cli.filename_decl.with_filename);
    }

    #[test]
    fn cli_parses_long_flags_before_pattern() {
        let cli =
            Cli::try_parse_from(["sift", "--line-number", "--with-filename", "pattern"]).unwrap();
        assert!(cli.line_number_decl.line_number);
        assert!(cli.filename_decl.with_filename);
    }

    #[test]
    fn cli_parses_empty_pattern_file() {
        let cli = Cli::try_parse_from(["sift", "-f", "ignore.txt"]).unwrap();
        assert_eq!(
            cli.patterns.pattern_file.as_deref(),
            Some(Path::new("ignore.txt"))
        );
    }

    #[test]
    fn cli_parses_index_build_defaults_to_background() {
        let cli = Cli::try_parse_from(["sift", "index", "build"]).unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { wait, .. },
            }) => assert!(!wait),
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_parses_index_build_wait_flag() {
        let cli = Cli::try_parse_from(["sift", "index", "build", "--wait"]).unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { wait, .. },
            }) => assert!(wait),
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_parses_index_build_lazy_flag() {
        let cli = Cli::try_parse_from(["sift", "index", "build", "--lazy"]).unwrap();
        match cli.command {
            Some(Commands::Index {
                command: IndexCommands::Build { lazy, .. },
            }) => assert!(lazy),
            _ => panic!("expected index build subcommand"),
        }
    }

    #[test]
    fn cli_rejects_index_update_lazy_flag() {
        let result = Cli::try_parse_from(["sift", "index", "update", "--lazy"]);
        assert!(result.is_err());
    }
}
