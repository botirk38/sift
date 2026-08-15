//! Minimal macOS FSEvents repro matching sift-daemon's notify usage.
//!
//! Observed in production (`sift-daemon` / sift#242):
//! `unable to start FSEvent stream`
//!
//! That string is emitted by `notify` when `FSEventStreamStart` returns false
//! (`notify` 9.0.0-rc.4 `src/fsevent.rs`).
//!
//! Usage:
//!   cargo run --manifest-path repros/notify-fsevent-stream-start/Cargo.toml -- /path/to/dir
//!   cargo run --manifest-path repros/notify-fsevent-stream-start/Cargo.toml -- /path/to/dir --stress 100

use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

fn main() {
    let mut args = env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let mut stress = 1usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stress" => {
                stress = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(50);
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(64);
            }
        }
    }

    println!("notify dep 9.0.0-rc.4 (RecommendedWatcher / FSEvents)");
    println!("root {}", root.display());
    println!("stress cycles {stress}");
    println!("os {} {}", env::consts::OS, env::consts::ARCH);

    for i in 0..stress {
        if let Err(err) = watch_once(&root) {
            eprintln!("FAIL at cycle {i}: {err}");
            std::process::exit(2);
        }
        if stress > 1 && i % 10 == 0 {
            println!("ok through {i}");
        }
        if stress > 1 {
            thread::sleep(Duration::from_millis(10));
        }
    }

    println!("ok ({stress} cycle(s))");
}

/// Same shape as sift's `CorpusWatcher::new`: RecommendedWatcher + recursive watch.
fn watch_once(root: &Path) -> notify::Result<()> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<notify::Event, notify::Error>| {
            // sift ignores channel errors; surface them here for diagnosis.
            if let Err(err) = res {
                eprintln!("event channel error: {err}");
            }
        },
        Config::default(),
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    drop(watcher);
    while rx.try_recv().is_ok() {}
    Ok(())
}
