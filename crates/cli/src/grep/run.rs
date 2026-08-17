use std::path::PathBuf;

use sift_core::candidates::{Scan, ScanScope, SnapshotFreshness};
use sift_core::search::{Input, Inputs, Query, SearchMode, SearchOptions, Searcher};
use sift_core::{FileFilter, FileOrder, IndexCoverage, Narrowing, Plan, TypeFilterRule};

use crate::index::daemon::Daemon;

use super::filter::{FilterConfig, FilterResolution};
use super::ignore::IgnoreResolution;
use super::input::{ContentTransformConfig, InputSources};
use super::output::{FilenameContext, OutputArgv, OutputDecl};
use super::paths::CorpusScope;
use super::pattern::{PatternArgv, PatternDecl, PatternInputUse, ResolvedPatterns};

/// Resolved configuration for a search invocation (no Decl bags / Argv at execute).
#[derive(Clone)]
pub struct RunConfig {
    pub pattern: PatternDecl,
    pub pattern_argv: PatternArgv,
    pub filter: FilterConfig,
    pub filter_resolution: FilterResolution,
    pub type_filters: Vec<TypeFilterRule>,
    pub output: OutputDecl,
    pub output_argv: OutputArgv,
    pub sift_dir: PathBuf,
    pub search_paths: Vec<PathBuf>,
    pub threads: Option<usize>,
    pub content: ContentTransformConfig,
    pub file_order: FileOrder,
    pub search_mode: SearchMode,
    pub ignore: IgnoreResolution,
}

/// CLI search runner.
pub struct Run {
    config: RunConfig,
}

/// Outcome of a search / path-list run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunResult {
    Found,
    NotFound,
}

struct SearchSession {
    indexes: Option<sift_core::Indexes>,
    scope: CorpusScope,
    search_filter: FileFilter,
    store_meta: Option<sift_core::StoreMeta>,
}

impl SearchSession {
    fn queryable(&self) -> bool {
        self.indexes
            .as_ref()
            .is_some_and(sift_core::Indexes::queryable)
    }
}

impl RunResult {
    #[must_use]
    pub const fn succeeded(self) -> bool {
        matches!(self, Self::Found)
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
    pub fn execute(&self, daemon: Option<&Daemon>) -> anyhow::Result<RunResult> {
        let matched = self.run_search(daemon)?;
        Ok(if matched {
            RunResult::Found
        } else {
            RunResult::NotFound
        })
    }

    fn configure_threads(&self) {
        if let Some(threads) = self.config.threads {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build_global()
                .ok();
        }
    }

    fn prepare_session(&self, search_paths: &[PathBuf]) -> anyhow::Result<SearchSession> {
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
        let filter_config = self.config.filter.file_config(
            self.config.filter_resolution,
            self.config.type_filters.clone(),
            scope.prefixes.clone(),
            scope.exclude_paths.clone(),
        )?;
        let search_filter = FileFilter::new(&filter_config, &scope.filter_root)?;
        Ok(SearchSession {
            indexes,
            scope,
            search_filter,
            store_meta,
        })
    }

    const fn scan(session: &SearchSession, scope: ScanScope) -> Scan<'_> {
        Scan::new(
            session.indexes.as_ref(),
            &session.search_filter,
            session.store_meta.as_ref(),
            scope,
        )
    }

    fn run_search(&self, daemon: Option<&Daemon>) -> anyhow::Result<bool> {
        let mode = self.config.search_mode;
        let patterns = if matches!(mode, SearchMode::Paths) {
            ResolvedPatterns {
                patterns: Vec::new(),
                input: PatternInputUse::None,
            }
        } else {
            ResolvedPatterns::resolve(&self.config.pattern)?
        };
        let sources =
            InputSources::from_paths(&self.config.search_paths).resolve(patterns.input)?;
        let pattern_argv = &self.config.pattern_argv;
        let output_argv = &self.config.output_argv;
        let line_number_override = self.line_number_override();
        let session = self.prepare_session(&sources.paths)?;
        let transform = self.config.content.transform()?;
        let filename_ctx = Self::filename_context(mode, &sources);
        let print_spec = self
            .config
            .output
            .print_spec(
                output_argv,
                mode,
                pattern_argv.quiet,
                line_number_override,
                filename_ctx,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
        let separators = self.config.output.separators();
        let freshness = Self::snapshot_freshness(&session, daemon);
        let scan_scope = self.scan_scope(&session, freshness, &sources);
        let scan = Self::scan(&session, scan_scope);
        let format = OutputDecl::format(output_argv, mode);
        let print_stats = OutputDecl::print_stats(output_argv, format);
        let query = self.query(mode, patterns.patterns, transform.is_some())?;
        let explicit_files = Self::explicit_files(&session);
        let streams = sources.stdin_streams();
        let searcher = Searcher::new(query).map_err(|e| anyhow::anyhow!("{e}"))?;
        let resolved = Plan::new(&scan, searcher.query(), mode.coverage())
            .resolve(&scan)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let candidate_bound = resolved.bound();
        let (candidates, streams) = match transform.as_ref() {
            Some(transform) => transform
                .to_streams(resolved, streams, &explicit_files)
                .map_err(|e| anyhow::anyhow!("{e}"))?,
            None => (resolved, streams),
        };
        if output_argv.debug {
            super::output::Debug {
                sift_dir: &self.config.sift_dir,
                corpus_root: &session.scope.filter_root,
                indexes: session.indexes.as_ref(),
                mode,
                patterns: searcher.query().patterns(),
                scan: scan_scope,
                narrowing: searcher.query().narrowing(),
                transform: transform.is_some(),
                candidates: candidate_bound,
                streams: streams.len(),
            }
            .write();
        }
        let mut inputs = Inputs::with_capacity(candidates.bound() + streams.len());
        for file in candidates.into_vec() {
            inputs.push(Input::from_file(file, &explicit_files));
        }
        for stream in streams {
            inputs.push(stream);
        }
        let report = print_spec
            .print(&searcher, inputs, mode, &separators)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        if print_stats && !matches!(mode, SearchMode::Paths) {
            OutputDecl::write_stats(&report.stats);
        }
        let selected = report.found();
        Self::queue_lazy_hits(daemon, &session, report.listed.corpus_hit_paths());
        Ok(selected)
    }

    /// Paths listing uses a dummy pattern; content transforms disable index narrowing.
    fn query(
        &self,
        mode: SearchMode,
        patterns: Vec<String>,
        transform: bool,
    ) -> anyhow::Result<Query> {
        let query = if matches!(mode, SearchMode::Paths) {
            Query::new(vec![".".to_string()], SearchOptions::default())
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .with_narrowing(Narrowing::Disabled)
        } else {
            self.config
                .pattern
                .query(patterns, &self.config.pattern_argv)
                .map_err(|e| anyhow::anyhow!("{e}"))?
        };
        Ok(if transform {
            query.with_narrowing(Narrowing::Disabled)
        } else {
            query
        })
    }

    const fn line_number_override(&self) -> Option<bool> {
        if self.config.output.column.pretty || self.config.output.column.vimgrep {
            Some(true)
        } else {
            self.config.output_argv.line_number
        }
    }

    fn scan_scope(
        &self,
        session: &SearchSession,
        freshness: SnapshotFreshness,
        sources: &InputSources,
    ) -> ScanScope {
        if !sources.resolve_candidates() {
            return ScanScope::StreamsOnly;
        }
        if !session.queryable() || matches!(self.config.search_mode, SearchMode::Paths) {
            return ScanScope::Walk {
                order: self.config.file_order,
            };
        }
        ScanScope::Index {
            order: self.config.file_order,
            freshness,
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

    const fn filename_context(mode: SearchMode, sources: &InputSources) -> FilenameContext {
        if mode.is_path_mode() {
            FilenameContext::PathMode
        } else if !sources.stdin_bytes.is_empty() && sources.paths.is_empty() {
            FilenameContext::SingleStream
        } else {
            FilenameContext::Directory
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
