use std::path::PathBuf;

use sift_core::candidates::{CandidateSource, ScanScope, SnapshotFreshness};
use sift_core::search::{Query, SearchInputs, SearchMode, SearchOptions, Searcher, ZeroCounts};
use sift_core::{
    CandidateFilter, CandidateOrder, Candidates, CorpusKind, IndexCoverage, Narrowing, Plan,
};

use crate::format::output::mode::ZeroCountMode;
use crate::format::{PrintExtras, PrintMode, SearchPrinter};

use crate::index::daemon::Daemon;

use super::argv::Argv;
use super::filter::FilterConfig;
use super::input::{ContentTransformConfig, InputSources};
use super::output::{FilenameContext, OutputArgv, OutputDecl};
use super::paths::CorpusScope;
use super::pattern::{PatternArgv, PatternDecl, ResolvedPatterns};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Search,
    ListFiles,
}

/// Resolved configuration for a search invocation.
#[derive(Clone)]
pub struct RunConfig {
    pub pattern: PatternDecl,
    pub filter: FilterConfig,
    pub output: OutputDecl,
    pub sift_dir: PathBuf,
    pub search_paths: Vec<PathBuf>,
    pub threads: Option<usize>,
    pub mode: RunMode,
    pub content: ContentTransformConfig,
    pub candidate_order: CandidateOrder,
}

impl RunConfig {
    /// Build a resolved run configuration from parsed CLI state.
    ///
    /// # Errors
    ///
    /// Returns an error if sort/order flags are invalid.
    pub fn from_cli(cli: &crate::Cli, argv: &Argv<'_>) -> Result<Self, anyhow::Error> {
        cli.run_config(argv)
    }
}

/// CLI search runner.
pub struct Run {
    config: RunConfig,
}

/// Result of a search run; variant reflects `--files` vs pattern search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunResult {
    Files { found: bool },
    Search { matched: bool },
}

struct SearchSession {
    indexes: Option<sift_core::Indexes>,
    scope: CorpusScope,
    search_filter: CandidateFilter,
    store_meta: Option<sift_core::StoreMeta>,
}

impl RunResult {
    #[must_use]
    pub const fn succeeded(self) -> bool {
        match self {
            Self::Files { found } => found,
            Self::Search { matched } => matched,
        }
    }
}

impl Run {
    #[must_use]
    pub const fn new(config: RunConfig) -> Self {
        Self { config }
    }

    /// # Errors
    ///
    /// Returns an error if I/O operations fail, paths are invalid, or filter config building fails.
    pub fn execute(&self, argv: &Argv<'_>, daemon: Option<&Daemon>) -> anyhow::Result<RunResult> {
        match self.config.mode {
            RunMode::ListFiles => self.run_files(argv).map(|found| RunResult::Files { found }),
            RunMode::Search => self
                .run_search(argv, daemon)
                .map(|matched| RunResult::Search { matched }),
        }
    }

    fn configure_threads(&self) {
        if let Some(threads) = self.config.threads {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()
                .ok();
        }
    }

    fn prepare_session(
        &self,
        argv: &Argv<'_>,
        search_paths: &[PathBuf],
    ) -> anyhow::Result<SearchSession> {
        self.configure_threads();
        let cwd = std::env::current_dir()?;
        let store_meta = sift_core::StoreMeta::read(&self.config.sift_dir).ok();
        let indexes = sift_core::Indexes::load(&self.config.sift_dir)?;
        let scope = CorpusScope::resolve(
            indexes.as_ref(),
            store_meta.as_ref(),
            &cwd,
            search_paths,
            &self.config.sift_dir,
        )?;
        let filter_config = self.config.filter.candidate_config(
            argv,
            scope.prefixes.clone(),
            scope.exclude_paths.clone(),
        )?;
        let search_filter = CandidateFilter::new(&filter_config, &scope.filter_root)?;
        Ok(SearchSession {
            indexes,
            scope,
            search_filter,
            store_meta,
        })
    }

    const fn candidate_source(session: &SearchSession, scope: ScanScope) -> CandidateSource<'_> {
        CandidateSource::new(
            session.indexes.as_ref(),
            &session.search_filter,
            session.store_meta.as_ref(),
            scope,
        )
    }

    fn run_files(&self, argv: &Argv<'_>) -> anyhow::Result<bool> {
        let output_argv = OutputArgv::resolve(argv);
        let session = self.prepare_session(argv, &self.config.search_paths)?;
        let scope = ScanScope::Walk {
            order: self.config.candidate_order,
        };
        let source = Self::candidate_source(&session, scope);
        let query = Query::new(vec![".".to_string()], SearchOptions::default())
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .with_narrowing(Narrowing::Disabled);
        let searcher = Searcher::new(query).map_err(|e| anyhow::anyhow!("{e}"))?;
        let candidates = Plan::new(&source, searcher.query(), SearchMode::Lines.coverage())
            .resolve(&source)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let all_paths: Vec<_> = candidates
            .into_vec()
            .into_iter()
            .map(|candidate| candidate.rel_path().to_path_buf())
            .collect();
        let sep = if output_argv.path.nul_terminated {
            '\0'
        } else {
            '\n'
        };
        let mut any = false;
        for p in &all_paths {
            any = true;
            let display = session.scope.filter_root.join(p);
            print!("{}{sep}", display.display());
        }
        Ok(any)
    }

    fn run_search(&self, argv: &Argv<'_>, daemon: Option<&Daemon>) -> anyhow::Result<bool> {
        let patterns = ResolvedPatterns::resolve(&self.config.pattern)?;
        let sources = InputSources::from_paths(&self.config.search_paths);
        let pattern_argv = PatternArgv::resolve(argv);
        let output_argv = OutputArgv::resolve(argv);

        let effective_mode = if pattern_argv.only_matching {
            PrintMode::OnlyMatching
        } else {
            pattern_argv.mode
        };

        let line_number_override = self.line_number_override(&output_argv);

        let session = self.prepare_session(argv, &sources.paths)?;
        let indexes_empty = session
            .indexes
            .as_ref()
            .is_none_or(|indexes| !indexes.queryable());
        let sources = sources.resolve(patterns.input, indexes_empty)?;
        let transform = self.config.content.transform()?;

        let filename_ctx = Self::filename_context(effective_mode, &sources, &session);
        let print_spec = self
            .config
            .output
            .print_spec(
                &output_argv,
                effective_mode,
                pattern_argv.quiet,
                line_number_override,
                filename_ctx,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        let separators = self.config.output.separators();
        let freshness = Self::snapshot_freshness(&session, daemon);
        let scan_scope = self.scan_scope(freshness, sources.resolve_candidates());
        let candidate_source = Self::candidate_source(&session, scan_scope);

        let mut query = self
            .config
            .pattern
            .query(patterns.patterns, &pattern_argv)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        if transform.is_some() {
            query = query.with_narrowing(Narrowing::Disabled);
        }
        let explicit_files = Self::explicit_files(&session);
        let mut streams = sources.stdin_streams();
        let print_stats = OutputDecl::print_stats(&output_argv, effective_mode);
        let extras = PrintExtras::hits().with_stats(print_stats);
        let mode = Self::search_mode(effective_mode, print_spec.include_zero);
        let stats = if print_stats {
            sift_core::StatsMode::On
        } else {
            sift_core::StatsMode::Off
        };
        let searcher = Searcher::new(query).map_err(|e| anyhow::anyhow!("{e}"))?;
        let resolved = Plan::new(&candidate_source, searcher.query(), mode.coverage())
            .resolve(&candidate_source)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let (candidates, streams) = match transform.as_ref() {
            Some(transform) => {
                for candidate in resolved.into_vec() {
                    let bytes = transform
                        .read_candidate(&candidate)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let is_explicit = explicit_files
                        .iter()
                        .any(|path| path == candidate.rel_path() || path == candidate.abs_path());
                    streams.push_candidate_bytes(candidate, bytes, is_explicit);
                }
                (Candidates::empty(), streams)
            }
            None => (resolved, streams),
        };
        let report = SearchPrinter::print(
            &searcher,
            SearchInputs {
                candidates,
                streams,
                explicit: &explicit_files,
            },
            mode,
            stats,
            print_spec,
            &separators,
            extras,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;

        if let Some(s) = report.stats.as_ref() {
            OutputDecl::write_stats(s);
        }
        let selected = report.found();
        Self::queue_lazy_hits(daemon, &session, report.listed.corpus_hit_paths());
        Ok(selected)
    }

    const fn line_number_override(&self, output_argv: &OutputArgv) -> Option<bool> {
        if self.config.output.column.pretty || self.config.output.column.vimgrep {
            Some(true)
        } else {
            output_argv.line_number
        }
    }

    const fn scan_scope(
        &self,
        freshness: SnapshotFreshness,
        resolve_candidates: bool,
    ) -> ScanScope {
        if !resolve_candidates {
            return ScanScope::StreamsOnly;
        }
        ScanScope::Index {
            order: self.config.candidate_order,
            freshness,
        }
    }

    const fn search_mode(mode: PrintMode, zeros: ZeroCountMode) -> SearchMode {
        let zeros = match zeros {
            ZeroCountMode::Omit => ZeroCounts::Omit,
            ZeroCountMode::Include => ZeroCounts::Include,
        };
        match mode {
            PrintMode::Standard => SearchMode::Lines,
            PrintMode::OnlyMatching => SearchMode::Matches,
            PrintMode::Count => SearchMode::CountLines { zeros },
            PrintMode::CountMatches => SearchMode::CountMatches { zeros },
            PrintMode::FilesWithMatches => SearchMode::FilesWithMatches,
            PrintMode::FilesWithoutMatch => SearchMode::FilesWithoutMatch,
        }
    }

    fn queue_lazy_hits(daemon: Option<&Daemon>, session: &SearchSession, hit_paths: Vec<PathBuf>) {
        if let Some(daemon) = daemon
            && session
                .store_meta
                .as_ref()
                .is_some_and(|meta| meta.coverage == IndexCoverage::Lazy)
            && !hit_paths.is_empty()
            && let Err(e) = daemon.index(hit_paths)
        {
            eprintln!("sift: warning: index request failed: {e}");
        }
    }

    fn filename_context(
        effective_mode: PrintMode,
        sources: &InputSources,
        session: &SearchSession,
    ) -> FilenameContext {
        if OutputDecl::is_path_mode(effective_mode) {
            FilenameContext::PathMode
        } else if !sources.stdin_bytes.is_empty() && sources.paths.is_empty() {
            FilenameContext::SingleFileCorpus
        } else {
            match session
                .indexes
                .as_ref()
                .map(sift_core::Indexes::corpus_kind)
            {
                Some(CorpusKind::SingleFile) => FilenameContext::SingleFileCorpus,
                Some(_) | None => FilenameContext::DirectoryCorpus,
            }
        }
    }

    fn snapshot_freshness(session: &SearchSession, daemon: Option<&Daemon>) -> SnapshotFreshness {
        daemon
            .and_then(|daemon| {
                session
                    .indexes
                    .as_ref()
                    .and_then(sift_core::Indexes::snapshot_id)
                    .map(|id| daemon.validate_snapshot(id))
            })
            .map_or(SnapshotFreshness::Current, |validation| match validation {
                Ok(false) => SnapshotFreshness::Stale,
                Ok(true) | Err(_) => SnapshotFreshness::Current,
            })
    }

    fn explicit_files(session: &SearchSession) -> Vec<PathBuf> {
        session
            .scope
            .prefixes
            .iter()
            .filter(|prefix| session.scope.filter_root.join(prefix).is_file())
            .cloned()
            .collect()
    }
}
