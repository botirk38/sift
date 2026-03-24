use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TMP_ID: AtomicUsize = AtomicUsize::new(0);

fn sift_exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sift"))
}

pub fn fresh_dir(name: &str) -> PathBuf {
    let id = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "sift-cli-integration-{name}-{}-{id}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

pub fn command(cwd: Option<&Path>) -> Command {
    let mut cmd = Command::new(sift_exe());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    cmd
}

pub fn build_index(cwd: Option<&Path>, sift_dir: &Path, corpus: &Path) {
    let status = command(cwd)
        .arg("--sift-dir")
        .arg(sift_dir)
        .arg("build")
        .arg(corpus)
        .status()
        .unwrap();
    assert!(status.success(), "build index failed with status {status}");
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn normalized_stdout(output: &Output) -> String {
    stdout(output).replace("\r\n", "\n").replace('\\', "/")
}
