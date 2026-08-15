use std::path::PathBuf;

use sift_core::candidates::{Scan, ScanScope, SnapshotFreshness};
use sift_core::search::{Query, SearchInputs, SearchMode, SearchOptions, Searcher};
use sift_core::{
    Candidates, FileFilter, FileOrder, IndexCoverage, Narrowing, Plan, TypeFilterRule,
};

use crate::index::daemon::Daemon;

use crate::format::PrintFormat;

use super::filter::{FilterConfig, FilterResolution};
use super::ignore::IgnoreResolution;
use super::input::{ContentTransform, ContentTransformConfig, InputSources};
use super::output::{DebugNote, FilenameContext, IndexLoad, OutputArgv, OutputDecl, SearchDebug};
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

/// Values captured for `--debug` stderr diagnostics.
#[derive(Clone, Copy)]
struct DebugProbe<'a> {
    session: &'a SearchSession,
    mode: SearchMode,
    patterns: &'a [String],
    scan_scope: ScanScope,
    narrowing: Narrowing,
    freshness: SnapshotFreshness,
    content_transform: bool,
    candidate_bound: usize,
    stream_count: usize,
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
        let sources = InputSources::from_paths(&self.config.search_paths);
        let pattern_argv = &self.config.pattern_argv;
        let output_argv = &self.config.output_argv;

        let line_number_override = self.line_number_override();

        let session = self.prepare_session(&sources.paths)?;
        let indexes_empty = session
            .indexes
            .as_ref()
            .is_none_or(|indexes| !indexes.queryable());
        let sources = sources.resolve(patterns.input, indexes_empty)?;
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
        let scan_scope = self.scan_scope(freshness, sources.resolve_candidates());
        let scan = Self::scan(&session, scan_scope);

        // Collect stats for `--stats` (human lines) and for `--json` (summary /
        // end events). Human stderr lines are text-only (`--stats` without JSON).
        let format = OutputDecl::format(output_argv, mode);
        let print_stats = OutputDecl::print_stats(output_argv, format);
        let stats = if print_stats || matches!(format, PrintFormat::Json) {
            sift_core::StatsMode::On
        } else {
            sift_core::StatsMode::Off
        };
        let pattern_summary = patterns.patterns.clone();
        let query = self.search_query(mode, patterns.patterns, transform.is_some())?;
        let explicit_files = Self::explicit_files(&session);
        let streams = sources.stdin_streams();
        let searcher = Searcher::new(query).map_err(|e| anyhow::anyhow!("{e}"))?;
        let resolved = Plan::new(&scan, searcher.query(), mode.coverage())
            .resolve(&scan)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let candidate_bound = resolved.bound();
        let (candidates, streams) =
            Self::apply_transform(transform.as_ref(), resolved, streams, &explicit_files)?;
        self.emit_debug(DebugProbe {
            session: &session,
            mode,
            patterns: &pattern_summary,
            scan_scope,
            narrowing: searcher.query().narrowing(),
            freshness,
            content_transform: transform.is_some(),
            candidate_bound,
            stream_count: streams.len(),
        });
        let report = print_spec
            .print(
                &searcher,
                SearchInputs {
                    candidates,
                    streams,
                    explicit: &explicit_files,
                },
                mode,
                stats,
                &separators,
            )
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        // Ripgrep emits no human `--stats` for `--files`; path listing is not a search.
        if print_stats
            && !matches!(mode, SearchMode::Paths)
            && let Some(s) = report.stats.as_ref()
        {
            OutputDecl::write_stats(s);
        }
        let selected = report.found();
        Self::queue_lazy_hits(daemon, &session, report.listed.corpus_hit_paths());
        Ok(selected)
    }

    fn search_query(
        &self,
        mode: SearchMode,
        patterns: Vec<String>,
        content_transform: bool,
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
        Ok(if content_transform {
            query.with_narrowing(Narrowing::Disabled)
        } else {
            query
        })
    }

    fn apply_transform<'a>(
        transform: Option<&ContentTransform>,
        resolved: Candidates<'a>,
        mut streams: sift_core::Inputs<'a>,
        explicit_files: &[PathBuf],
    ) -> anyhow::Result<(Candidates<'a>, sift_core::Inputs<'a>)> {
        match transform {
            Some(transform) => {
                for candidate in resolved.into_vec() {
                    let bytes = transform
                        .read_candidate(&candidate)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let is_explicit = candidate.is_explicit(explicit_files);
                    streams.push_file_bytes(candidate, bytes, is_explicit);
                }
                Ok((Candidates::empty(), streams))
            }
            None => Ok((resolved, streams)),
        }
    }

    fn emit_debug(&self, probe: DebugProbe<'_>) {
        if !self.config.output_argv.debug {
            return;
        }
        let notes = Self::debug_notes(
            probe.session,
            probe.scan_scope,
            probe.narrowing,
            probe.freshness,
            probe.content_transform,
        );
        SearchDebug {
            sift_dir: &self.config.sift_dir,
            corpus_root: &probe.session.scope.filter_root,
            index: Self::index_load(probe.session),
            search_mode: probe.mode,
            patterns: probe.patterns,
            scan_scope: probe.scan_scope,
            candidate_bound: probe.candidate_bound,
            stream_count: probe.stream_count,
            notes: &notes,
        }
        .write();
    }

    const fn line_number_override(&self) -> Option<bool> {
        if self.config.output.column.pretty || self.config.output.column.vimgrep {
            Some(true)
        } else {
            self.config.output_argv.line_number
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
        if matches!(self.config.search_mode, SearchMode::Paths) {
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

    fn index_load(session: &SearchSession) -> IndexLoad {
        session
            .indexes
            .as_ref()
            .map_or(IndexLoad::Absent, |indexes| IndexLoad::Present {
                queryable: indexes.queryable(),
            })
    }

    fn debug_notes(
        session: &SearchSession,
        scan_scope: ScanScope,
        narrowing: Narrowing,
        freshness: SnapshotFreshness,
        content_transform: bool,
    ) -> Vec<DebugNote> {
        let mut notes = Vec::new();
        match session.indexes.as_ref() {
            None => notes.push(DebugNote::IndexAbsent),
            Some(indexes) if !indexes.queryable() => notes.push(DebugNote::IndexNotQueryable),
            Some(_) => {}
        }
        if matches!(narrowing, Narrowing::Disabled) {
            notes.push(DebugNote::NarrowingDisabled);
        }
        if matches!(freshness, SnapshotFreshness::Stale) {
            notes.push(DebugNote::SnapshotStale);
        }
        if matches!(scan_scope, ScanScope::StreamsOnly) {
            notes.push(DebugNote::StreamsOnly);
        }
        if content_transform {
            notes.push(DebugNote::ContentTransform);
        }
        notes
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
