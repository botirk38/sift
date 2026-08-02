//! Corpus filesystem watcher for the index daemon.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use notify::RecursiveMode;
use notify::Watcher;

use super::{DaemonError, Event, StorePaths};

#[cfg(windows)]
type PlatformWatcher = notify::PollWatcher;

#[cfg(not(windows))]
type PlatformWatcher = notify::RecommendedWatcher;

pub(super) struct CorpusWatcher {
    platform: PlatformWatcher,
    pub(super) root: PathBuf,
}

impl CorpusWatcher {
    pub(super) fn new(events: &mpsc::Sender<Event>, root: &Path) -> Result<Self, DaemonError> {
        #[cfg(windows)]
        let config = notify::Config::default()
            .with_poll_interval(std::time::Duration::from_millis(super::DEBOUNCE_MS));
        #[cfg(not(windows))]
        let config = notify::Config::default();
        let platform = PlatformWatcher::new(
            {
                let events = events.clone();
                move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(event) = res {
                        let _ = events.send(Event::FsChange(event));
                    }
                }
            },
            config,
        )
        .map_err(|e| DaemonError::message(e.to_string()))?;
        let mut watcher = Self {
            platform,
            root: root.to_path_buf(),
        };
        watcher.watch(root)?;
        Ok(watcher)
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
