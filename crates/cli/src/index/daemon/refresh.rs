//! Background index refresh for the daemon.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, mpsc};

use sift_core::{Indexes, StoreMeta};

use crate::index::{ReconcileOutcome, SnapshotRefresh};

use super::ipc::DaemonResponse;
use super::watcher::CorpusWatcher;
use super::{DaemonError, Event, Phase, RefreshFollowUp, StorePaths};

pub(super) enum RefreshResult {
    Success(ReconcileOutcome),
    Failed,
}

pub(super) enum PendingIndex {
    None,
    Full,
    Paths(Vec<PathBuf>),
}

impl PendingIndex {
    pub(super) fn lock(pending: &Arc<Mutex<Self>>) -> Result<MutexGuard<'_, Self>, DaemonError> {
        pending
            .lock()
            .map_err(|_| DaemonError::message("daemon pending queue lock poisoned"))
    }

    pub(super) fn push(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            *self = Self::Full;
            return;
        }
        match self {
            Self::Full => {}
            Self::None => *self = Self::Paths(paths),
            Self::Paths(existing) => {
                for path in paths {
                    if !existing.contains(&path) {
                        existing.push(path);
                    }
                }
            }
        }
    }

    pub(super) const fn is_pending(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub(super) fn take(&mut self) -> Option<Vec<PathBuf>> {
        match std::mem::replace(self, Self::None) {
            Self::None => None,
            Self::Full => Some(Vec::new()),
            Self::Paths(paths) => Some(paths),
        }
    }

    pub(super) fn reconcile(
        &mut self,
        sift_dir: &Path,
        meta: &StoreMeta,
    ) -> Option<ReconcileOutcome> {
        let paths = self.take()?;
        let result = Indexes::open(sift_dir, meta)
            .and_then(|mut indexes| SnapshotRefresh::new(sift_dir, meta).run(&mut indexes));
        match result {
            Ok(outcome) => Some(outcome),
            Err(e) => {
                eprintln!("sift-daemon: refresh failed: {e}");
                self.push(paths);
                None
            }
        }
    }
}

pub(super) enum RefreshScope {
    CorpusAndPending,
    PendingOnly,
}

pub(super) struct IndexRefresh<'a> {
    pub(super) tx: &'a mpsc::Sender<Event>,
    pub(super) store: &'a Path,
    pub(super) pending: &'a Arc<Mutex<PendingIndex>>,
}

impl IndexRefresh<'_> {
    pub(super) fn apply_index(
        &self,
        paths: Vec<PathBuf>,
        reply: &mpsc::Sender<DaemonResponse>,
        watcher: &mut CorpusWatcher,
        store: &Path,
        phase: &mut Phase,
    ) -> Result<(), DaemonError> {
        watcher.rebind(store)?;
        PendingIndex::lock(self.pending)?.push(paths);
        let _ = reply.send(DaemonResponse::Accepted);
        if phase.is_refreshing() {
            *phase = Phase::Refreshing {
                follow_up: RefreshFollowUp::Queued,
            };
        } else {
            self.begin(RefreshScope::CorpusAndPending, phase);
        }
        Ok(())
    }

    pub(super) fn begin(&self, scope: RefreshScope, phase: &mut Phase) {
        self.spawn(scope);
        *phase = Phase::Refreshing {
            follow_up: RefreshFollowUp::None,
        };
    }

    fn spawn(&self, scope: RefreshScope) {
        let tx = self.tx.clone();
        let sift_dir = self.store.to_path_buf();
        let pending = Arc::clone(self.pending);
        std::thread::spawn(move || {
            let meta = match StorePaths::read_meta(&sift_dir) {
                Ok(meta) => meta,
                Err(e) => {
                    eprintln!("sift-daemon: {e}");
                    let _ = tx.send(Event::RefreshFinished(RefreshResult::Failed));
                    return;
                }
            };
            let mut outcome = None;
            if matches!(scope, RefreshScope::CorpusAndPending) {
                let result = Indexes::open(&sift_dir, &meta).and_then(|mut indexes| {
                    SnapshotRefresh::new(&sift_dir, &meta).run(&mut indexes)
                });
                match result {
                    Ok(committed) => outcome = Some(committed),
                    Err(e) => {
                        eprintln!("sift-daemon: refresh failed: {e}");
                        let _ = tx.send(Event::RefreshFinished(RefreshResult::Failed));
                        return;
                    }
                }
            }
            let pending_outcome = if let Ok(mut queue) = pending.lock() {
                queue.reconcile(&sift_dir, &meta)
            } else {
                eprintln!("sift-daemon: pending queue lock poisoned");
                let _ = tx.send(Event::RefreshFinished(RefreshResult::Failed));
                return;
            };
            let result = pending_outcome
                .or(outcome)
                .map_or(RefreshResult::Failed, RefreshResult::Success);
            let _ = tx.send(Event::RefreshFinished(result));
        });
    }
}
