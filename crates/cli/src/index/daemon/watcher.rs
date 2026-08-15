//! Corpus filesystem watcher for the index daemon.
//!
//! Prefers notify's recommended (native) backend and falls back to polling when
//! that backend cannot start (for example macOS seatbelt denying
//! `com.apple.FSEvents`).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify::Watcher;

use super::{DEBOUNCE_MS, DaemonError, Event};

/// Active notify implementation. Held as an enum because fallback switches the
/// concrete watcher type at runtime.
enum Backend {
    Recommended(notify::RecommendedWatcher),
    Poll(notify::PollWatcher),
}

impl Backend {
    /// Recursive corpus watch — the only watch mode this daemon uses.
    fn watch(&mut self, root: &Path) -> notify::Result<()> {
        match self {
            Self::Recommended(watcher) => watcher.watch(root, RecursiveMode::Recursive),
            Self::Poll(watcher) => watcher.watch(root, RecursiveMode::Recursive),
        }
    }

    fn unwatch(&mut self, root: &Path) -> notify::Result<()> {
        match self {
            Self::Recommended(watcher) => watcher.unwatch(root),
            Self::Poll(watcher) => watcher.unwatch(root),
        }
    }
}

/// Watches one corpus root and forwards filesystem events onto the daemon channel.
pub(super) struct CorpusWatcher {
    backend: Backend,
    pub(super) root: PathBuf,
    events: mpsc::Sender<Event>,
}

impl CorpusWatcher {
    pub(super) fn new(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        let tx = events.clone();
        let native = notify::RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            notify::Config::default(),
        )
        .and_then(|mut watcher| {
            watcher.watch(root, RecursiveMode::Recursive)?;
            Ok(Self {
                backend: Backend::Recommended(watcher),
                root: root.to_path_buf(),
                events: events.clone(),
            })
        });

        match native {
            Ok(watcher) => Ok(watcher),
            Err(err) => {
                eprintln!(
                    "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                );
                Self::polling(events, root).map_err(|poll_err| {
                    DaemonError::message(format!(
                        "poll fallback after native watcher failed ({err}): {poll_err}"
                    ))
                })
            }
        }
    }

    /// Polling backend used when recommended watch fails at start or rebind.
    fn polling(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let tx = events.clone();
        let config =
            notify::Config::default().with_poll_interval(Duration::from_millis(DEBOUNCE_MS));
        let mut watcher = notify::PollWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(Event::FsChange(event));
                }
            },
            config,
        )?;
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(Self {
            backend: Backend::Poll(watcher),
            root: root.to_path_buf(),
            events: events.clone(),
        })
    }

    /// Point the watcher at a new corpus root (no-op when unchanged).
    pub(super) fn rebind(&mut self, root: &Path) -> Result<(), DaemonError> {
        if root == self.root {
            return Ok(());
        }
        let _ = self.backend.unwatch(self.root.as_path());
        match self.backend.watch(root) {
            Ok(()) => {
                self.root = root.to_path_buf();
                Ok(())
            }
            Err(err) if matches!(self.backend, Backend::Recommended(_)) => {
                eprintln!(
                    "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                );
                *self = Self::polling(&self.events, root).map_err(|poll_err| {
                    DaemonError::message(format!(
                        "poll fallback after native watcher failed ({err}): {poll_err}"
                    ))
                })?;
                Ok(())
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
    fn polling_binds_temp_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel();
        let watcher = CorpusWatcher::polling(&tx, dir.path()).expect("poll watcher");
        assert_eq!(watcher.root.as_path(), dir.path());
        assert!(matches!(watcher.backend, Backend::Poll(_)));
    }
}
