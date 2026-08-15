//! Corpus filesystem watcher for the index daemon.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify::Watcher;

use super::{DaemonError, Event, StorePaths, DEBOUNCE_MS};

pub(super) struct CorpusWatcher {
    platform: PlatformWatcher,
    pub(super) root: PathBuf,
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
            return Self::poll(events, root);
        }
        #[cfg(not(windows))]
        {
            match Self::recommended(events, root) {
                Ok(watcher) => Ok(watcher),
                Err(err) => {
                    eprintln!(
                        "sift-daemon: native filesystem watcher unavailable ({err}); falling back to polling"
                    );
                    Self::poll(events, root)
                }
            }
        }
    }

    #[cfg(not(windows))]
    fn recommended(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        let config = notify::Config::default();
        let platform = notify::RecommendedWatcher::new(Self::callback(events), config)
            .map_err(|e| DaemonError::message(e.to_string()))?;
        let mut watcher = Self {
            platform: PlatformWatcher::Recommended(platform),
            root: root.to_path_buf(),
        };
        watcher.watch(root)?;
        Ok(watcher)
    }

    fn poll(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        let config = notify::Config::default()
            .with_poll_interval(Duration::from_millis(DEBOUNCE_MS));
        let platform = notify::PollWatcher::new(Self::callback(events), config)
            .map_err(|e| DaemonError::message(e.to_string()))?;
        let mut watcher = Self {
            platform: PlatformWatcher::Poll(platform),
            root: root.to_path_buf(),
        };
        watcher.watch(root)?;
        Ok(watcher)
    }

    fn callback(
        events: &mpsc::Sender<Event>,
    ) -> impl FnMut(Result<notify::Event, notify::Error>) + Send + 'static {
        let events = events.clone();
        move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = events.send(Event::FsChange(event));
            }
        }
    }

    fn watch(&mut self, root: &Path) -> Result<(), DaemonError> {
        self.platform
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| DaemonError::message(e.to_string()))
    }

    pub(super) fn rebind(&mut self, store: &Path) -> Result<(), DaemonError> {
        let meta = StorePaths::read_meta(store)?;
        let root = meta.corpus.root;
        if root == self.root {
            return Ok(());
        }
        let _ = self.platform.unwatch(self.root.as_path());
        self.watch(&root)?;
        self.root = root;
        Ok(())
    }
}
