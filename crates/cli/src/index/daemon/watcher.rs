//! Corpus filesystem watcher for the index daemon.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify::Watcher;

use super::{DEBOUNCE_MS, DaemonError, Event, StorePaths};

pub(super) struct CorpusWatcher {
    platform: PlatformWatcher,
    pub(super) root: PathBuf,
    events: mpsc::Sender<Event>,
}

enum PlatformWatcher {
    #[cfg(not(windows))]
    Recommended(notify::RecommendedWatcher),
    Poll(notify::PollWatcher),
}

impl PlatformWatcher {
    fn watch(&mut self, root: &Path, mode: RecursiveMode) -> notify::Result<()> {
        match self {
            #[cfg(not(windows))]
            Self::Recommended(watcher) => watcher.watch(root, mode),
            Self::Poll(watcher) => watcher.watch(root, mode),
        }
    }

    fn unwatch(&mut self, root: &Path) -> notify::Result<()> {
        match self {
            #[cfg(not(windows))]
            Self::Recommended(watcher) => watcher.unwatch(root),
            Self::Poll(watcher) => watcher.unwatch(root),
        }
    }
}

impl CorpusWatcher {
    pub(super) fn new(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        #[cfg(windows)]
        {
            return Self::poll(events, root).map_err(|e| DaemonError::message(e.to_string()));
        }
        #[cfg(not(windows))]
        {
            match Self::recommended(events, root) {
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
    }

    #[cfg(not(windows))]
    fn recommended(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let config = notify::Config::default();
        let platform = notify::RecommendedWatcher::new(Self::event_handler(events), config)?;
        let mut watcher = Self {
            platform: PlatformWatcher::Recommended(platform),
            root: root.to_path_buf(),
            events: events.clone(),
        };
        watcher.platform.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    }

    fn poll(events: &mpsc::Sender<Event>, root: &Path) -> notify::Result<Self> {
        let config =
            notify::Config::default().with_poll_interval(Duration::from_millis(DEBOUNCE_MS));
        let platform = notify::PollWatcher::new(Self::event_handler(events), config)?;
        let mut watcher = Self {
            platform: PlatformWatcher::Poll(platform),
            root: root.to_path_buf(),
            events: events.clone(),
        };
        watcher.platform.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    }

    fn event_handler(
        events: &mpsc::Sender<Event>,
    ) -> impl FnMut(Result<notify::Event, notify::Error>) + Send + 'static {
        let events = events.clone();
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = events.send(Event::FsChange(event));
            }
        }
    }

    pub(super) fn rebind(&mut self, store: &Path) -> Result<(), DaemonError> {
        let meta = StorePaths::read_meta(store)?;
        let root = meta.corpus.root;
        if root == self.root {
            return Ok(());
        }
        let _ = self.platform.unwatch(self.root.as_path());
        match self.platform.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => {
                self.root = root;
                Ok(())
            }
            Err(err) => {
                #[cfg(windows)]
                {
                    return Err(DaemonError::message(err.to_string()));
                }
                #[cfg(not(windows))]
                {
                    if !matches!(self.platform, PlatformWatcher::Recommended(_)) {
                        return Err(DaemonError::message(err.to_string()));
                    }
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_watcher_binds_temp_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel();
        let watcher = CorpusWatcher::poll(&tx, dir.path()).expect("poll watcher");
        assert_eq!(watcher.root.as_path(), dir.path());
        assert!(matches!(watcher.platform, PlatformWatcher::Poll(_)));
    }
}
