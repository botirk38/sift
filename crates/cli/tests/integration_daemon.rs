//! Integration tests for the sift daemon.
//!
//! Short-lived daemon runs use startup reconcile plus `--idle-timeout-secs`.
//! The `daemon_reindexes_on_file_changes` test runs a long-lived daemon.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const LONG_IDLE: Duration = Duration::from_mins(10);

use common::TestProject;
use common::assert_exit_code;
use common::assert_success;
use common::normalize;
use common::normalize_stderr;
use common::normalize_stdout;
use sift_core::VisibilityConfig;
use sift_core::{
    CorpusMeta, FilterMeta, GramWidth, IndexCoverage, IndexRecord, Indexes, SnapshotId, StoreMeta,
    WalkMeta,
};
use sift_grep::index::daemon::{Daemon, DaemonOrchestrator, ServeConfig};

fn spawn_daemon(
    sift_dir: PathBuf,
    ready_path: PathBuf,
    idle_timeout: Duration,
    shutdown: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let daemon = DaemonOrchestrator::new(sift_dir, None);
        daemon
            .serve(
                ServeConfig {
                    ready: Some(ready_path),
                    idle_timeout,
                },
                shutdown.as_ref(),
            )
            .expect("daemon serve");
    })
}

fn wait_for_ready(ready_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if ready_path.exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("daemon did not signal readiness within 10s");
}

fn sample_meta(root: PathBuf) -> StoreMeta {
    StoreMeta::new(
        CorpusMeta {
            root,
            include_paths: Vec::new(),
            exclude_paths: Vec::new(),
        },
        IndexCoverage::Complete,
        WalkMeta {
            follow_links: false,
            one_file_system: false,
            max_depth: None,
            max_filesize: None,
        },
        FilterMeta {
            visibility: VisibilityConfig::default(),
        },
        vec![IndexRecord::ngram(GramWidth::TRIGRAM)],
    )
}

fn sift_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sift"))
}

fn fresh_dir(name: &str) -> tempfile::TempDir {
    tempfile::TempDir::with_prefix(format!("sift-cli-daemon-{name}")).expect("create temp dir")
}

fn search(sift_dir: &Path, pattern: &str) -> Output {
    Command::new(sift_bin())
        .arg("--sift-dir")
        .arg(sift_dir)
        .arg(pattern)
        .env("SIFT_NO_DAEMON", "1")
        .output()
        .unwrap()
}

fn poll_until<F>(sift_dir: &Path, pattern: &str, timeout: Duration, mut predicate: F)
where
    F: FnMut(&Output) -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        let out = search(sift_dir, pattern);
        if predicate(&out) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let last = search(sift_dir, pattern);
    panic!(
        "timed out after {timeout:?} waiting for pattern {pattern:?}\n\
         last exit: {:?}\nlast stdout:\n{}\nlast stderr:\n{}",
        last.status.code(),
        normalize(&String::from_utf8_lossy(&last.stdout)),
        normalize(&String::from_utf8_lossy(&last.stderr)),
    );
}

fn path_indexed(sift_dir: &Path, rel: &str) -> bool {
    StoreMeta::read(sift_dir).ok().is_some_and(|meta| {
        Indexes::open(sift_dir, &meta).ok().is_some_and(|indexes| {
            indexes
                .files()
                .is_some_and(|files| files.contains(Path::new(rel)))
        })
    })
}

fn poll_until_indexed(sift_dir: &Path, rel: &str, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path_indexed(sift_dir, rel) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out after {timeout:?} waiting for {rel:?} to appear in the index");
}

fn poll_until_snapshot_validates(daemon: &Daemon, id: &SnapshotId, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if daemon.validate_snapshot(id).expect("validate snapshot") {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out after {timeout:?} waiting for snapshot {id:?} to validate");
}

fn daemon_search(sift_dir: &Path, pattern: &str) -> Output {
    Command::new(sift_bin())
        .env_remove("SIFT_NO_DAEMON")
        .arg("--sift-dir")
        .arg(sift_dir)
        .arg(pattern)
        .output()
        .unwrap()
}

#[test]
fn daemon_errors_without_meta_or_init_root() {
    let dir = fresh_dir("no-meta");
    let sift_dir = dir.path().join(".sift");
    fs::create_dir_all(&sift_dir).unwrap();

    let out: Output = Command::new(common::daemon_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .output()
        .unwrap();

    assert_exit_code(&out, 1);
    let stderr = normalize(&String::from_utf8_lossy(&out.stderr));
    assert!(
        stderr.contains("no store metadata"),
        "expected stderr to contain 'no store metadata', got: {stderr}"
    );
}

#[test]
fn daemon_exits_zero_when_lock_held() {
    let dir = fresh_dir("lock-held");
    let sift_dir = dir.path().join(".sift");
    fs::create_dir_all(&sift_dir).unwrap();

    // Hold the daemon lock before starting the daemon.
    let lock_path = sift_dir.join("lock");
    let mut lock = fslock::LockFile::open(&lock_path).unwrap();
    lock.try_lock().unwrap();

    let out: Output = Command::new(common::daemon_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .output()
        .unwrap();

    assert_exit_code(&out, 0);
}

#[test]
fn daemon_reconciles_on_startup() {
    let dir = fresh_dir("startup-build");
    let sift_dir = dir.path().join(".sift");

    let meta = sample_meta(dir.path().to_path_buf());
    std::fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();
    fs::write(dir.path().join("a.txt"), "hello world\n").unwrap();

    let out: Output = Command::new(common::daemon_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .arg("--idle-timeout-secs")
        .arg("2")
        .output()
        .unwrap();
    assert_exit_code(&out, 0);

    poll_until(&sift_dir, "hello world", Duration::from_secs(5), |out| {
        out.status.success()
    });
}

#[test]
fn daemon_reconciles_on_restart() {
    let dir = fresh_dir("restart-update");
    let sift_dir = dir.path().join(".sift");

    let meta = sample_meta(dir.path().to_path_buf());
    std::fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();

    fs::write(dir.path().join("a.txt"), "hello world\n").unwrap();
    let out = Command::new(common::daemon_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .arg("--idle-timeout-secs")
        .arg("2")
        .output()
        .unwrap();
    assert_exit_code(&out, 0);

    fs::write(dir.path().join("b.txt"), "goodbye world\n").unwrap();
    let out = Command::new(common::daemon_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .arg("--idle-timeout-secs")
        .arg("2")
        .output()
        .unwrap();
    assert_exit_code(&out, 0);

    poll_until(&sift_dir, "goodbye world", Duration::from_secs(5), |out| {
        out.status.success()
    });
}

#[test]
fn daemon_reindexes_on_file_changes() {
    let dir = fresh_dir("daemon-live");
    let root = dir.path().to_path_buf();
    let sift_dir = root.join(".sift");

    // Set up project with initial file and metadata.
    let meta = sample_meta(root.clone());
    fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();
    fs::write(root.join("a.txt"), "initial_token\n").unwrap();

    // Build index without spawning daemon.
    let status = Command::new(sift_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .args(["index", "build", "--wait"])
        .arg(&root)
        .env("SIFT_NO_DAEMON", "1")
        .status()
        .unwrap();
    assert!(status.success(), "initial build failed");

    // Verify baseline search works.
    let out = search(&sift_dir, "initial_token");
    assert_exit_code(&out, 0);
    assert!(
        normalize(&String::from_utf8_lossy(&out.stdout)).contains("a.txt"),
        "expected a.txt in baseline search"
    );

    // Run the daemon event loop in a thread.
    let ready_path = sift_dir.join("daemon-ready.test");
    let shutdown = Arc::new(AtomicBool::new(false));

    let handle = spawn_daemon(
        sift_dir.clone(),
        ready_path.clone(),
        LONG_IDLE,
        Arc::clone(&shutdown),
    );

    wait_for_ready(&ready_path);

    let mut lock = fslock::LockFile::open(&sift_dir.join("lock")).unwrap();
    assert!(!lock.try_lock().unwrap(), "daemon did not acquire lock");

    // Write all changes at once (create + modify) so they land in a single
    // FSEvent batch.  On macOS CI, `FSEvent` callbacks are slow enough that
    // sequential writes can cause timeouts (see notify-rs/notify#935).
    fs::write(root.join("b.txt"), "added_by_daemon\n").unwrap();
    fs::write(root.join("a.txt"), "initial_token modified_by_daemon\n").unwrap();

    // Both changes are picked up by a single watcher callback + refresh.
    poll_until(
        &sift_dir,
        "added_by_daemon",
        Duration::from_secs(20),
        |out| {
            out.status.success()
                && normalize(&String::from_utf8_lossy(&out.stdout)).contains("b.txt")
        },
    );
    poll_until(
        &sift_dir,
        "modified_by_daemon",
        Duration::from_secs(20),
        |out| {
            out.status.success()
                && normalize(&String::from_utf8_lossy(&out.stdout)).contains("a.txt")
        },
    );

    // --- Delete a file (requires a second watcher callback) ---
    fs::remove_file(root.join("b.txt")).unwrap();
    poll_until(
        &sift_dir,
        "added_by_daemon",
        Duration::from_secs(20),
        |out| !out.status.success(),
    );

    shutdown.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn daemon_builds_initial_index_on_startup_when_no_current() {
    let dir = fresh_dir("no-current");
    let root = dir.path().to_path_buf();
    let sift_dir = root.join(".sift");

    // Write StoreMeta but do NOT build an index (no CURRENT file).
    let meta = sample_meta(root.clone());
    fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();
    fs::write(root.join("hello.txt"), "startup_content\n").unwrap();

    // Run the daemon event loop in a thread — startup reconcile builds the index.
    let ready_path = sift_dir.join("daemon-ready.no-current");
    let shutdown = Arc::new(AtomicBool::new(false));

    let handle = spawn_daemon(
        sift_dir.clone(),
        ready_path.clone(),
        LONG_IDLE,
        Arc::clone(&shutdown),
    );

    wait_for_ready(&ready_path);

    // Verify the daemon built the index by searching within a timeout.
    poll_until(
        &sift_dir,
        "startup_content",
        Duration::from_secs(10),
        |out| {
            out.status.success()
                && normalize(&String::from_utf8_lossy(&out.stdout)).contains("hello.txt")
        },
    );

    shutdown.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn daemon_exits_after_idle_timeout() {
    let dir = fresh_dir("idle-exit");
    let root = dir.path().to_path_buf();
    let sift_dir = root.join(".sift");

    let meta = sample_meta(root.clone());
    fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();
    fs::write(root.join("a.txt"), "idle_test\n").unwrap();

    let ready_path = sift_dir.join("daemon-ready.idle");

    let handle = std::thread::spawn({
        let sift_dir = sift_dir.clone();
        let ready_path = ready_path.clone();
        move || {
            let daemon = DaemonOrchestrator::new(sift_dir, None);
            let shutdown = AtomicBool::new(false);
            daemon
                .serve(
                    ServeConfig {
                        ready: Some(ready_path),
                        idle_timeout: Duration::from_secs(2),
                    },
                    &shutdown,
                )
                .expect("daemon serve");
        }
    });

    wait_for_ready(&ready_path);

    // Do NOT generate any filesystem events.  The daemon should exit after
    // the idle timeout (~2 seconds).
    let start = Instant::now();
    handle.join().unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_secs(1),
        "daemon exited too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "daemon took too long to exit: {elapsed:?}"
    );

    // Verify the daemon lock is released.
    let lock_path = sift_dir.join("lock");
    let mut lock = fslock::LockFile::open(&lock_path).unwrap();
    assert!(lock.try_lock().unwrap(), "daemon lock should be released");
}

#[test]
fn daemon_reconciles_offline_changes() {
    let dir = fresh_dir("reconcile");
    let root = dir.path().to_path_buf();
    let sift_dir = root.join(".sift");

    // Set up project with initial file and metadata.
    let meta = sample_meta(root.clone());
    fs::create_dir_all(&sift_dir).unwrap();
    StoreMeta::write(&meta, &sift_dir).unwrap();
    fs::write(root.join("a.txt"), "original_content\n").unwrap();

    // Build the initial index (no daemon running).
    let status = Command::new(sift_bin())
        .arg("--sift-dir")
        .arg(&sift_dir)
        .args(["index", "build", "--wait"])
        .arg(&root)
        .env("SIFT_NO_DAEMON", "1")
        .status()
        .unwrap();
    assert!(status.success(), "initial build failed");

    // Verify baseline search works.
    let out = search(&sift_dir, "original_content");
    assert_exit_code(&out, 0);

    // --- Simulate offline changes (daemon is NOT running) ---
    fs::write(root.join("b.txt"), "offline_addition\n").unwrap();
    fs::write(root.join("a.txt"), "original_content offline_edit\n").unwrap();

    // Confirm the offline addition is NOT yet in the index.
    let out = search(&sift_dir, "offline_addition");
    assert_exit_code(&out, 1);

    // Start the daemon — it should reconcile on startup.
    let ready_path = sift_dir.join("daemon-ready.reconcile");
    let shutdown = Arc::new(AtomicBool::new(false));

    let handle = spawn_daemon(
        sift_dir.clone(),
        ready_path.clone(),
        LONG_IDLE,
        Arc::clone(&shutdown),
    );

    wait_for_ready(&ready_path);

    // The startup reconciliation should pick up the offline changes.
    poll_until(
        &sift_dir,
        "offline_addition",
        Duration::from_secs(10),
        |out| {
            out.status.success()
                && normalize(&String::from_utf8_lossy(&out.stdout)).contains("b.txt")
        },
    );
    poll_until(&sift_dir, "offline_edit", Duration::from_secs(10), |out| {
        out.status.success() && normalize(&String::from_utf8_lossy(&out.stdout)).contains("a.txt")
    });

    shutdown.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn build_defaults_to_async_and_becomes_searchable() {
    let p = TestProject::new("daemon-async-build");
    p.write("a.txt", "async_build_daemon_marker\n");

    let out = p
        .sift_with_daemon()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build"])
        .output()
        .unwrap();
    assert_success(&out);
    let stderr = normalize_stderr(&out);
    assert!(
        stderr.contains("queued build"),
        "expected queued build message, got: {stderr}"
    );

    poll_until_indexed(p.sift_dir(), "a.txt", Duration::from_secs(15));
    poll_until(
        p.sift_dir(),
        "async_build_daemon_marker",
        Duration::from_secs(5),
        |out| out.status.success() && normalize_stdout(out).contains("a.txt"),
    );
}

#[test]
fn index_update_async_reconciles_offline_edit() {
    let p = TestProject::new("daemon-update-async");
    p.write("a.txt", "update_async_original\n");
    p.build_index();

    let out = p.index_output(["update_async_original"]);
    assert_success(&out);

    p.write("b.txt", "update_async_offline_add\n");
    p.write("a.txt", "update_async_original update_async_offline_edit\n");

    let out = search(p.sift_dir(), "update_async_offline_add");
    assert_exit_code(&out, 1);

    let out = p
        .sift_with_daemon()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "update"])
        .output()
        .unwrap();
    assert_success(&out);
    let stderr = normalize_stderr(&out);
    assert!(
        stderr.contains("queued update"),
        "expected queued update message, got: {stderr}"
    );

    poll_until(
        p.sift_dir(),
        "update_async_offline_add",
        Duration::from_secs(15),
        |out| out.status.success() && normalize_stdout(out).contains("b.txt"),
    );
    poll_until(
        p.sift_dir(),
        "update_async_offline_edit",
        Duration::from_secs(15),
        |out| out.status.success() && normalize_stdout(out).contains("a.txt"),
    );
}

#[test]
fn index_build_lazy_persists_lazy_coverage() {
    let p = TestProject::new("index-build-lazy-coverage");
    p.write("a.txt", "lazy_coverage_marker\n");

    let status = p
        .sift()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait", "--lazy"])
        .status()
        .unwrap();
    assert!(status.success(), "lazy index build failed with {status}");

    let meta = StoreMeta::read(p.sift_dir()).expect("read meta");
    assert_eq!(meta.coverage, IndexCoverage::Lazy);
}

#[test]
fn index_update_preserves_lazy_coverage() {
    let p = TestProject::new("index-update-lazy-coverage");
    p.write("a.txt", "lazy_update_original\n");
    let status = p
        .sift()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait", "--lazy"])
        .status()
        .unwrap();
    assert!(status.success(), "lazy index build failed with {status}");

    p.write("b.txt", "lazy_update_next\n");
    let status = p
        .sift()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "update", "--wait"])
        .status()
        .unwrap();
    assert!(status.success(), "index update failed with {status}");

    let meta = StoreMeta::read(p.sift_dir()).expect("read meta");
    assert_eq!(meta.coverage, IndexCoverage::Lazy);
}

#[test]
fn failed_rebuild_does_not_change_coverage() {
    let p = TestProject::new("failed-rebuild-coverage");
    p.write("a.txt", "complete_coverage_marker\n");
    p.build_index();

    let status = p
        .sift()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait", "--lazy"])
        .status()
        .unwrap();
    assert!(!status.success(), "rebuild unexpectedly succeeded");

    let meta = StoreMeta::read(p.sift_dir()).expect("read meta");
    assert_eq!(meta.coverage, IndexCoverage::Complete);
}

#[test]
fn search_walk_hit_queues_partial_index() {
    let p = TestProject::new("daemon-walk-hit-partial");
    p.write("a.txt", "indexed_only_a\n");
    let status = p
        .sift()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait", "--lazy"])
        .status()
        .unwrap();
    assert!(status.success(), "lazy index build failed with {status}");
    p.write("b.txt", "walk_hit_partial_marker\n");

    assert!(
        path_indexed(p.sift_dir(), "a.txt"),
        "expected a.txt in baseline index"
    );
    assert!(
        !path_indexed(p.sift_dir(), "b.txt"),
        "expected b.txt to be unindexed before search"
    );

    let out = daemon_search(p.sift_dir(), "walk_hit_partial_marker");
    assert_success(&out);
    assert!(
        normalize_stdout(&out).contains("b.txt"),
        "expected walk hit on b.txt"
    );

    poll_until_indexed(p.sift_dir(), "b.txt", Duration::from_secs(15));

    let out = search(p.sift_dir(), "walk_hit_partial_marker");
    assert_success(&out);
    assert!(
        normalize_stdout(&out).contains("b.txt"),
        "expected second search to find b.txt via index"
    );
    assert!(
        path_indexed(p.sift_dir(), "b.txt"),
        "expected b.txt to remain indexed after second search"
    );
}

#[test]
fn daemon_validates_only_current_committed_snapshot() {
    let p = TestProject::new("daemon-validate-snapshot");
    p.write("a.txt", "validate_snapshot_initial\n");
    p.build_index();

    let ready_path = p.sift_dir().join("daemon-ready.validate");
    let shutdown = Arc::new(AtomicBool::new(false));
    let handle = spawn_daemon(
        p.sift_dir().to_path_buf(),
        ready_path.clone(),
        LONG_IDLE,
        Arc::clone(&shutdown),
    );
    wait_for_ready(&ready_path);

    let daemon = Daemon::new(p.sift_dir().to_path_buf());
    let meta = StoreMeta::read(p.sift_dir()).expect("read meta");
    let first = Indexes::open(p.sift_dir(), &meta).expect("open indexes");
    let first_id = first.snapshot_id().expect("committed snapshot id");
    assert!(
        daemon
            .validate_snapshot(first_id)
            .expect("validate snapshot"),
        "current snapshot should validate"
    );

    p.write("b.txt", "validate_snapshot_next\n");
    daemon.index(vec![]).expect("queue full reconcile");
    poll_until_indexed(p.sift_dir(), "b.txt", Duration::from_secs(15));

    let second = Indexes::open(p.sift_dir(), &meta).expect("open indexes");
    let second_id = second.snapshot_id().expect("committed snapshot id");
    assert_ne!(
        first_id, second_id,
        "reconcile should commit a new snapshot"
    );
    assert!(
        !daemon
            .validate_snapshot(first_id)
            .expect("validate snapshot"),
        "old snapshot should not validate after a newer commit"
    );
    poll_until_snapshot_validates(&daemon, second_id, Duration::from_secs(15));

    shutdown.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn blocking_build_starts_daemon_for_watch() {
    let p = TestProject::new("daemon-blocking-handoff");
    p.write("a.txt", "blocking_handoff_initial\n");

    let out = p
        .sift_with_daemon()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait"])
        .output()
        .unwrap();
    assert_success(&out);
    let stderr = normalize_stderr(&out);
    assert!(
        stderr.contains("indexed corpus"),
        "expected blocking build message, got: {stderr}"
    );
    assert!(
        !stderr.contains("daemon not started"),
        "expected daemon to start after blocking build, got: {stderr}"
    );

    let sift_dir = p.sift_dir().to_path_buf();
    let reachable_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < reachable_deadline {
        if DaemonOrchestrator::new(sift_dir.clone(), None)
            .start()
            .is_ok()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    DaemonOrchestrator::new(sift_dir, None)
        .start()
        .expect("daemon ipc not reachable after blocking build");

    p.write("b.txt", "blocking_handoff_watch_marker\n");

    #[cfg(windows)]
    let watch_timeout = Duration::from_secs(90);
    #[cfg(not(windows))]
    let watch_timeout = Duration::from_secs(20);

    poll_until_indexed(p.sift_dir(), "b.txt", watch_timeout);
}

#[test]
fn daemon_rebinds_watcher_when_corpus_root_changes() {
    let p = TestProject::new("daemon-watcher-rebind");
    let old_root = p.root().join("old");
    let new_root = p.root().join("new");
    fs::create_dir_all(&old_root).unwrap();
    fs::create_dir_all(&new_root).unwrap();
    p.write("old/a.txt", "watcher_rebind_initial\n");

    let out = p
        .sift_with_daemon()
        .arg("--sift-dir")
        .arg(p.sift_dir())
        .args(["index", "build", "--wait"])
        .arg(&old_root)
        .output()
        .unwrap();
    assert_success(&out);

    p.write("new/b.txt", "watcher_rebind_new_marker\n");
    StoreMeta::write(&sample_meta(new_root), p.sift_dir()).unwrap();

    Daemon::new(p.sift_dir().to_path_buf())
        .index(vec![])
        .expect("daemon index op");

    poll_until(
        p.sift_dir(),
        "watcher_rebind_new_marker",
        Duration::from_secs(20),
        |out| out.status.success() && normalize_stdout(out).contains("b.txt"),
    );
}
