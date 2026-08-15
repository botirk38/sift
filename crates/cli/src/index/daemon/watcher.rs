//! Corpus filesystem watcher for the index daemon.
//!
//! Prefers notify's native [`RecommendedWatcher`], and falls back to
//! [`PollWatcher`] when the native backend cannot start (for example macOS
//! seatbelt sandboxes that deny `com.apple.FSEvents`).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify::Watcher;

use super::{DEBOUNCE_MS, DaemonError, Event, StorePaths};

/// Concrete notify backend. An enum is required because the two watcher types
/// differ and fallback must switch between them at runtime.
enum Backend {
    Native(notify::RecommendedWatcher),
    Poll(notify::PollWatcher),
}

pub(super) struct CorpusWatcher {
    backend: Backend,
    pub(super) root: PathBuf,
    /// Kept so [`Self::rebind`] can rebuild a poll watcher after native failure.
    events: mpsc::Sender<Event>,
}

impl CorpusWatcher {
    pub(super) fn new(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        let tx = events.clone();
        match notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            notify::Config::default(),
        )
        .and_then(|mut w| {
            w.watch(root, RecursiveMode::Recursive)?;
            Ok(Self {
                backend: Backend::Native(w),
                root: root.to_path_buf(),
                events: events.clone(),
            })
        }) {
            Ok(watcher) => Ok(watcher),
            Err(err) => {
                eprintln!(
                    "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                );
                Self::poll(events, root).map_err(|poll_err| {
                    DaemonError::message(format!(
                        "poll fallback after native watcher failed ({err}): {poll_err}"
                    ))
                })
            }
        }
    }

    /// Build a polling watcher. Shared by [`Self::new`] fallback and [`Self::rebind`].
    fn poll(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let tx = events.clone();
        let config =
            notify::Config::default().with_poll_interval(Duration::from_millis(DEBOUNCE_MS));
        let mut w = notify::PollWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            config,
        )?;
        w.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            backend: Backend::Poll(w),
            root: root.to_path_buf(),
            events: events.clone(),
        })
    }

    pub(super) fn rebind(&mut self, store: &Path) -> Result<(), DaemonError> {
        let meta = StorePaths::read_meta(store)?;
        let root = meta.corpus.root;
        if root == self.root {
            return Ok(());
        }
        match &mut self.backend {
            Backend::Native(w) => {
                let _ = w.unwatch(self.root.as_path());
            }
            Backend::Poll(w) => {
                let _ = w.unwatch(self.root.as_path());
            }
        }
        let watch_err = match &mut self.backend {
            Backend::Native(w) => w.watch(&root, RecursiveMode::Recursive).err(),
            Backend::Poll(w) => w.watch(&root, RecursiveMode::Recursive).err(),
        };
        match watch_err {
            None => {
                self.root = root;
                Ok(())
            }
            Some(err) if matches!(self.backend, Backend::Native(_)) => {
                eprintln!(
                    "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                );
                *self = Self::poll(&self.events, &root).map_err(|poll_err| {
                    DaemonError::message(format!(
                        "poll fallback after native watcher failed ({err}): {poll_err}"
                    ))
                })?;
                Ok(())
            }
            Some(err) => Err(DaemonError::message(err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_binds_temp_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel();
        let watcher = CorpusWatcher::new(&tx, dir.path()).expect("corpus watcher");
        assert_eq!(watcher.root.as_path(), dir.path());
    }

    #[test]
    fn poll_binds_temp_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel();
        let watcher = CorpusWatcher::poll(&tx, dir.path()).expect("poll watcher");
        assert_eq!(watcher.root.as_path(), dir.path());
        assert!(matches!(watcher.backend, Backend::Poll(_)));
    }
}
