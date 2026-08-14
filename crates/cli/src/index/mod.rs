//! Index lifecycle (`sift index build`, `sift index update`) and background refresh.

use std::path::PathBuf;
use std::process::ExitCode;

use sift_core::VisibilityConfig;
use sift_core::{
    CorpusMeta, FilterMeta, IndexCoverage, IndexRecord, Indexes, SnapshotId, StoreMeta, WalkMeta,
};

use std::str::FromStr;

use crate::grep::filter::ByteSize;
use crate::grep::ignore::IgnoreResolution;
use crate::grep::paths::CorpusScope;

pub mod daemon;
mod selection;

pub use daemon::{Daemon, DaemonError, DaemonOrchestrator, ServeConfig};
pub use selection::{GramNormArg, IndexDecl, IndexKindArg, IndexSelection};

/// Result of reconciling store metadata and corpus state into a committed snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub snapshot_id: SnapshotId,
    pub changed: bool,
}

/// Which index subcommand was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOperation {
    Build,
    Update,
}

/// Whether index work runs in-process or is delegated to the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexExecution {
    Blocking,
    Background,
}

/// Parameters for resolving an index build or update.
pub struct IndexRequest {
    pub operation: IndexOperation,
    pub execution: IndexExecution,
    pub build_coverage: IndexCoverage,
    pub path: PathBuf,
    pub indexes: Option<Vec<IndexRecord>>,
    pub sift_dir: PathBuf,
    pub follow_links: bool,
    pub one_file_system: bool,
    pub max_depth: Option<usize>,
    pub max_filesize: Option<String>,
}

/// Resolved `sift index build` / `sift index update` request.
pub struct IndexJob {
    pub operation: IndexOperation,
    pub execution: IndexExecution,
    pub build_coverage: IndexCoverage,
    pub root: PathBuf,
    pub include_paths: Vec<PathBuf>,
    pub corpus_kind: sift_core::CorpusKind,
    pub indexes: Vec<IndexRecord>,
    pub follow_links: bool,
    pub one_file_system: bool,
    pub max_depth: Option<usize>,
    pub max_filesize: Option<u64>,
    pub exclude_paths: Vec<PathBuf>,
    pub sift_dir: PathBuf,
    pub ignore: IgnoreResolution,
}

impl IndexJob {
    /// Resolve an index request from parsed CLI args.
    ///
    /// # Errors
    ///
    /// Returns an error if the index path cannot be canonicalised.
    pub fn resolve(req: IndexRequest, ignore: IgnoreResolution) -> anyhow::Result<Self> {
        let canonical = req.path.canonicalize()?;
        let (root, include_paths, corpus_kind) = if canonical.is_file() {
            let parent = canonical.parent().unwrap_or(&canonical).to_path_buf();
            let filename = PathBuf::from(canonical.file_name().unwrap_or_default());
            (parent, vec![filename], sift_core::CorpusKind::SingleFile)
        } else {
            (canonical, Vec::new(), sift_core::CorpusKind::Directory)
        };
        let indexes: Vec<IndexRecord> = req
            .indexes
            .clone()
            .unwrap_or_else(IndexRecord::default_catalog);
        let exclude_paths = CorpusScope::excluded_paths(&root, &req.sift_dir);
        let max_filesize = req
            .max_filesize
            .as_ref()
            .map(|s| ByteSize::from_str(s).map(ByteSize::bytes))
            .transpose()?;
        Ok(Self {
            operation: req.operation,
            execution: req.execution,
            build_coverage: req.build_coverage,
            root,
            include_paths,
            corpus_kind,
            indexes,
            follow_links: req.follow_links,
            one_file_system: req.one_file_system,
            max_depth: req.max_depth,
            max_filesize,
            exclude_paths,
            sift_dir: req.sift_dir,
            ignore,
        })
    }

    /// Run `sift index build` or `sift index update`.
    #[must_use]
    pub fn run(&self, daemon: Option<&Daemon>) -> ExitCode {
        if self.execution == IndexExecution::Background {
            return self.run_background(daemon);
        }

        let existing_meta = StoreMeta::read(&self.sift_dir).ok();
        let meta = self.store_meta(self.ignore, existing_meta.as_ref());

        let mut indexes = match Indexes::open(&self.sift_dir, &meta) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sift: {e}");
                return ExitCode::from(2);
            }
        };

        let has_snapshot = indexes.current_id().is_some();
        match self.operation {
            IndexOperation::Build if has_snapshot => {
                eprintln!(
                    "sift: index already exists at {}; run `sift index update` to refresh it",
                    self.sift_dir.display()
                );
                return ExitCode::from(2);
            }
            IndexOperation::Update if !has_snapshot => {
                eprintln!(
                    "sift: no index at {}; run `sift index build` first",
                    self.sift_dir.display()
                );
                return ExitCode::from(2);
            }
            IndexOperation::Build | IndexOperation::Update => {}
        }

        if let Err(e) = indexes.refresh_meta(&meta) {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }

        if let Err(e) = SnapshotRefresh::run(&mut indexes) {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }

        if let Some(daemon) = daemon
            && let Err(e) = DaemonOrchestrator::new(daemon.sift_dir.clone(), None).start()
        {
            eprintln!("sift: warning: daemon not started: {e}");
        }

        let verb = match self.operation {
            IndexOperation::Build => "indexed",
            IndexOperation::Update => "updated index for",
        };
        eprintln!(
            "{verb} corpus {} → {}",
            self.root.display(),
            self.sift_dir.display()
        );
        ExitCode::SUCCESS
    }

    fn run_background(&self, daemon: Option<&Daemon>) -> ExitCode {
        let existing_meta = StoreMeta::read(&self.sift_dir).ok();
        let meta = self.store_meta(self.ignore, existing_meta.as_ref());

        let indexes = match Indexes::open(&self.sift_dir, &meta) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sift: {e}");
                return ExitCode::from(2);
            }
        };
        let has_snapshot = indexes.current_id().is_some();
        match self.operation {
            IndexOperation::Build if has_snapshot => {
                eprintln!(
                    "sift: index already exists at {}; run `sift index update` to refresh it",
                    self.sift_dir.display()
                );
                return ExitCode::from(2);
            }
            IndexOperation::Update if !has_snapshot => {
                eprintln!(
                    "sift: no index at {}; run `sift index build` first",
                    self.sift_dir.display()
                );
                return ExitCode::from(2);
            }
            IndexOperation::Build | IndexOperation::Update => {}
        }

        let Some(daemon) = daemon else {
            eprintln!(
                "sift: error: background index requires the daemon; unset SIFT_NO_DAEMON or use --wait"
            );
            return ExitCode::from(2);
        };

        if let Err(e) = std::fs::create_dir_all(&self.sift_dir) {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }
        if let Err(e) = meta.write(&self.sift_dir) {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }

        if let Err(e) = daemon.index(vec![]) {
            eprintln!("sift: {e}");
            return ExitCode::from(2);
        }

        let verb = match self.operation {
            IndexOperation::Build => "queued build for",
            IndexOperation::Update => "queued update for",
        };
        eprintln!(
            "{verb} corpus {} → {}",
            self.root.display(),
            self.sift_dir.display()
        );
        ExitCode::SUCCESS
    }

    fn store_meta(
        &self,
        ignore_res: IgnoreResolution,
        existing_meta: Option<&StoreMeta>,
    ) -> StoreMeta {
        let coverage = match self.operation {
            IndexOperation::Build => self.build_coverage,
            IndexOperation::Update => {
                existing_meta.map_or(self.build_coverage, |meta| meta.coverage)
            }
        };
        StoreMeta::new(
            CorpusMeta {
                root: self.root.clone(),
                kind: self.corpus_kind,
                include_paths: self.include_paths.clone(),
                exclude_paths: self.exclude_paths.clone(),
            },
            coverage,
            WalkMeta {
                follow_links: self.follow_links,
                one_file_system: self.one_file_system,
                max_depth: self.max_depth,
                max_filesize: self.max_filesize,
            },
            FilterMeta {
                visibility: VisibilityConfig {
                    hidden: ignore_res.hidden_mode(),
                    ignore: sift_core::IgnoreConfig {
                        sources: ignore_res.sources,
                        require_git: ignore_res.require_git,
                        custom_files: Vec::new(),
                    },
                },
            },
            self.indexes.clone(),
        )
    }
}

/// CLI orchestration for a full snapshot rebuild.
pub struct SnapshotRefresh;

impl SnapshotRefresh {
    /// Rebuild index files for the full configured corpus.
    ///
    /// # Errors
    ///
    /// Propagates build failures from the underlying index kinds.
    pub fn run(indexes: &mut Indexes) -> sift_core::Result<ReconcileOutcome> {
        let snapshot_id = SnapshotId::new(indexes.build()?);
        Ok(ReconcileOutcome {
            snapshot_id,
            changed: true,
        })
    }
}
