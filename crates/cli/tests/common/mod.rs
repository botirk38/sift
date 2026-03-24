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

fn normalize_path_str(path: &str) -> String {
    let mut normalized = path.replace("\r\n", "\n").replace('\\', "/");
    normalized = normalized.replace("//?/", "");
    normalized
}

pub fn normalized_stdout(output: &Output) -> String {
    normalize_path_str(&stdout(output))
}

#[allow(dead_code)]
pub fn abs(root: &Path, rel: &str) -> String {
    let joined = root.join(rel);
    let canonical = joined.canonicalize().unwrap_or(joined);
    normalize_path_str(&canonical.display().to_string())
}

#[allow(dead_code)]
pub fn abs_match(root: &Path, rel: &str, text: &str) -> String {
    format!("{}:{text}", abs(root, rel))
}

#[allow(dead_code)]
pub fn line_path<'a>(line: &'a str, candidates: &[String]) -> &'a str {
    candidates
        .iter()
        .find_map(|candidate| {
            if line == candidate || line.starts_with(&format!("{candidate}:")) {
                Some(&line[..candidate.len()])
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("could not match output line to any candidate path: {line}"))
}
