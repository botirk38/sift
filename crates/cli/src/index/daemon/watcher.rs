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

enum Backend {
    Native(notify::RecommendedWatcher),
    Poll(notify::PollWatcher),
}

impl Backend {
    fn watch(&mut self, root: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.watch(root, mode),
            Self::Poll(watcher) => watcher.watch(root, mode),
        }
    }

    fn unwatch(&mut self, root: &Path) -> notify::Result<()> {
        match self {
            Self::Native(watcher) => watcher.unwatch(root),
            Self::Poll(watcher) => watcher.unwatch(root),
        }
    }

    const fn is_native(&self) -> bool {
        matches!(self, Self::Native(_))
    }
}

pub(super) struct CorpusWatcher {
    backend: Backend,
    pub(super) root: PathBuf,
    events: mpsc::Sender<Event>,
}

impl CorpusWatcher {
    pub(super) fn new(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        match Self::native(events, root) {
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

    fn native(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let tx = events.clone();
        let backend = Backend::Native(notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            notify::Config::default(),
        )?);
        let mut watcher = Self {
            backend,
            root: root.to_path_buf(),
            events: events.clone(),
        };
        watcher.backend.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    }

    fn poll(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let tx = events.clone();
        let config =
            notify::Config::default().with_poll_interval(Duration::from_millis(DEBOUNCE_MS));
        let backend = Backend::Poll(notify::PollWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            config,
        )?);
        let mut watcher = Self {
            backend,
            root: root.to_path_buf(),
            events: events.clone(),
        };
        watcher.backend.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    }

    pub(super) fn rebind(&mut self, store: &Path) -> Result<(), DaemonError> {
        let meta = StorePaths::read_meta(store)?;
        let root = meta.corpus.root;
        if root == self.root {
            return Ok(());
        }
        let _ = self.backend.unwatch(self.root.as_path());
        match self.backend.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => {
                self.root = root;
                Ok(())
            }
            Err(err) if self.backend.is_native() => {
                eprintln!(
                    "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                );
                match Self::poll(&self.events, &root) {
                    Ok(replacement) => {
                        *self = replacement;
                        Ok(())
                    }
                    Err(poll_err) => Err(DaemonError::message(format!(
                        "poll fallback after native watcher failed ({err}): {poll_err}"
                    ))),
                }
            }
            Err(err) => Err(DaemonError::message(err.to_string())),
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
        assert!(!watcher.backend.is_native());
    }
}
