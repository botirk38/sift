use std::borrow::Cow;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;

use globset::{Glob, GlobSet, GlobSetBuilder};
use sift_core::search::{Input, Origin};
use sift_core::{Candidates, File};

#[derive(Debug, Clone, Default)]
pub struct ContentTransformConfig {
    pub search_zip: bool,
    pub pre: Option<String>,
    pub pre_globs: Vec<String>,
}

impl ContentTransformConfig {
    /// # Errors
    ///
    /// Returns an error when a `--pre-glob` pattern is not a valid glob.
    pub fn transform(&self) -> sift_core::Result<Option<ContentTransform>> {
        if !self.enabled() {
            return Ok(None);
        }
        Ok(Some(ContentTransform {
            search_zip: self.search_zip,
            pre: if let Some(command) = &self.pre {
                Some(Preprocessor {
                    command: command.clone(),
                    globs: PreprocessorGlobs::new(&self.pre_globs)?,
                })
            } else {
                None
            },
        }))
    }

    const fn enabled(&self) -> bool {
        self.search_zip || self.pre.is_some()
    }
}

pub struct ContentTransform {
    search_zip: bool,
    pre: Option<Preprocessor>,
}

impl ContentTransform {
    /// Search transformed file bytes as streams (zip / `--pre`).
    ///
    /// # Errors
    ///
    /// Returns an error if transformed content cannot be read.
    pub fn to_streams<'a>(
        &self,
        resolved: Candidates<'a>,
        mut streams: sift_core::Inputs<'a>,
        explicit: &[PathBuf],
    ) -> sift_core::Result<(Candidates<'a>, sift_core::Inputs<'a>)> {
        for candidate in resolved.into_vec() {
            let bytes = self.read_candidate(&candidate)?;
            let is_explicit = candidate.is_explicit(explicit);
            streams.push(Input::Bytes {
                origin: Origin::file(candidate),
                bytes: Cow::Owned(bytes),
                explicit: is_explicit,
            });
        }
        Ok((Candidates::empty(), streams))
    }

    /// Read transformed bytes for one candidate.
    ///
    /// # Errors
    ///
    /// Returns an error if transformed content cannot be read.
    pub fn read_candidate(&self, candidate: &File) -> sift_core::Result<Vec<u8>> {
        if let Some(pre) = &self.pre
            && pre.matches(candidate)
        {
            return pre.read(candidate.abs_path());
        }
        if self.search_zip {
            return Self::read_decompressed(candidate.abs_path());
        }
        Ok(std::fs::read(candidate.abs_path())?)
    }

    fn read_decompressed(path: &Path) -> sift_core::Result<Vec<u8>> {
        let path = external_tool_path(path);
        match ZipTool::from_path(path.as_ref()) {
            Some(tool) => tool.read(path.as_ref()),
            None => Ok(std::fs::read(path.as_ref())?),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZipTool {
    Gzip,
    Bzip2,
    Xz,
    Lzma,
    Lz4,
    Zstd,
    Brotli,
    Compress,
}

impl ZipTool {
    fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        [
            (".zstd", Self::Zstd),
            (".tbz2", Self::Bzip2),
            (".lzma", Self::Lzma),
            (".txz", Self::Xz),
            (".tgz", Self::Gzip),
            (".bz2", Self::Bzip2),
            (".lz4", Self::Lz4),
            (".zst", Self::Zstd),
            (".xz", Self::Xz),
            (".br", Self::Brotli),
            (".gz", Self::Gzip),
            (".Z", Self::Compress),
        ]
        .into_iter()
        .find_map(|(suffix, tool)| name.ends_with(suffix).then_some(tool))
    }

    const fn bin(self) -> &'static str {
        match self {
            Self::Gzip => "gzip",
            Self::Bzip2 => "bzip2",
            Self::Xz | Self::Lzma => "xz",
            Self::Lz4 => "lz4",
            Self::Zstd => "zstd",
            Self::Brotli => "brotli",
            Self::Compress => "uncompress",
        }
    }

    const fn args(self) -> &'static [&'static str] {
        match self {
            Self::Lzma => &["--format=lzma", "-d", "-c"],
            Self::Zstd => &["-q", "-d", "-c"],
            Self::Compress => &["-c"],
            Self::Gzip | Self::Bzip2 | Self::Xz | Self::Lz4 | Self::Brotli => &["-d", "-c"],
        }
    }

    fn read(self, path: &Path) -> sift_core::Result<Vec<u8>> {
        let Some(mut command) = ToolProgram::new(self.bin()).command() else {
            return Ok(std::fs::read(path)?);
        };
        command.args(self.args()).arg(path);
        match ToolOutput::spawn(command) {
            Err(_) => Ok(std::fs::read(path)?),
            Ok(output) => output.finish(&format!("`{}`", self.bin()), path),
        }
    }
}

/// Named program spawned for `--search-zip` / `--pre`.
struct ToolProgram<'a> {
    name: &'a Path,
}

impl<'a> ToolProgram<'a> {
    fn new(name: &'a str) -> Self {
        Self {
            name: Path::new(name),
        }
    }

    const fn from_path(name: &'a Path) -> Self {
        Self { name }
    }

    fn command(self) -> Option<Command> {
        Some(Command::new(self.resolve()?))
    }

    fn resolve(self) -> Option<PathBuf> {
        if self.name.is_absolute() {
            return Some(self.name.to_path_buf());
        }
        self.on_path()
    }

    fn on_path(self) -> Option<PathBuf> {
        let paths = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&paths) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let candidate = dir.join(self.name);
            if Self::is_exe(&candidate) {
                return Some(candidate);
            }
            #[cfg(windows)]
            if candidate.extension().is_none() {
                for extension in ["com", "exe"] {
                    let candidate = candidate.with_extension(extension);
                    if Self::is_exe(&candidate) {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }

    fn is_exe(path: &Path) -> bool {
        path.metadata().is_ok_and(|meta| !meta.is_dir())
    }
}

/// Streaming stdout of a spawned decompressor or preprocessor.
struct ToolOutput {
    child: Child,
    stdout: std::process::ChildStdout,
    stderr: JoinHandle<Vec<u8>>,
}

impl ToolOutput {
    fn spawn(mut command: Command) -> std::io::Result<Self> {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("child stdout was not piped"))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| std::io::Error::other("child stderr was not piped"))?;
        let stderr = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr.read_to_end(&mut buf);
            buf
        });
        Ok(Self {
            child,
            stdout,
            stderr,
        })
    }

    fn finish(mut self, label: &str, path: &Path) -> sift_core::Result<Vec<u8>> {
        let mut stdout = Vec::new();
        let read_err = self.stdout.read_to_end(&mut stdout).err();
        drop(self.stdout);
        let status = self.child.wait()?;
        let stderr = self.stderr.join().unwrap_or_default();
        if let Some(err) = read_err {
            return Err(err.into());
        }
        if status.success() {
            Ok(stdout)
        } else {
            let stderr = String::from_utf8_lossy(&stderr);
            Err(std::io::Error::other(format!(
                "{label} failed for {}: {}",
                path.display(),
                stderr.trim()
            ))
            .into())
        }
    }
}

/// Resolved argv paths and optional stdin byte streams.
pub struct InputSources {
    pub paths: Vec<PathBuf>,
    pub stdin_bytes: Vec<Vec<u8>>,
    /// `-` appeared on argv (stdin read even when empty).
    stdin_explicit: bool,
}

impl InputSources {
    #[must_use]
    pub fn from_paths(search_paths: &[PathBuf]) -> Self {
        let mut paths = Vec::with_capacity(search_paths.len());
        let mut stdin_explicit = false;
        for path in search_paths {
            if path == Path::new("-") {
                stdin_explicit = true;
            } else {
                paths.push(path.clone());
            }
        }

        Self {
            paths,
            stdin_bytes: Vec::new(),
            stdin_explicit,
        }
    }

    /// Read stdin when requested and resolve implicit piped input.
    ///
    /// When stdin is not a TTY and no paths are given, search the pipe only
    /// (ripgrep parity), whether or not an index is present. An empty pipe does
    /// not claim the search: corpus resolution still runs.
    ///
    /// # Errors
    ///
    /// Returns an error if stdin cannot be read.
    pub fn resolve(
        mut self,
        pattern_input: super::pattern::PatternInputUse,
    ) -> anyhow::Result<Self> {
        if self.stdin_explicit && self.stdin_bytes.is_empty() {
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes)?;
            if !bytes.is_empty() {
                self.stdin_bytes.push(bytes);
            }
        }

        let stream_available = pattern_input == super::pattern::PatternInputUse::None;
        let implicit_stream = stream_available
            && !self.stdin_explicit
            && self.paths.is_empty()
            && self.stdin_bytes.is_empty()
            && !std::io::stdin().is_terminal();
        if implicit_stream {
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes)?;
            if !bytes.is_empty() {
                self.stdin_bytes.push(bytes);
            }
        }
        Ok(self)
    }

    /// Whether corpus candidates should be resolved (index/walk).
    ///
    /// Returns `false` for stdin-only runs: explicit `-` (even when empty) or a
    /// non-empty implicit pipe with no paths. Mixed runs (paths plus stdin)
    /// return `true`; callers resolve corpus candidates and append streams.
    #[must_use]
    pub const fn resolve_candidates(&self) -> bool {
        if !self.paths.is_empty() {
            return true;
        }
        !self.stdin_explicit && self.stdin_bytes.is_empty()
    }
}

impl InputSources {
    /// Assemble stdin streams and candidate-to-input conversion for search.
    #[must_use]
    pub fn stdin_streams(&self) -> sift_core::Inputs<'_> {
        let mut streams = sift_core::Inputs::empty();
        for bytes in &self.stdin_bytes {
            streams.push(Input::Bytes {
                origin: Origin::stream("<stdin>"),
                bytes: Cow::Borrowed(bytes.as_slice()),
                explicit: true,
            });
        }
        streams
    }
}

struct Preprocessor {
    command: String,
    globs: PreprocessorGlobs,
}

impl Preprocessor {
    fn matches(&self, candidate: &File) -> bool {
        self.globs.matches(candidate.rel_path())
    }

    fn read(&self, path: &Path) -> sift_core::Result<Vec<u8>> {
        if self.command.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "--pre command is empty",
            )
            .into());
        }
        let path = external_tool_path(path);
        let Some(mut command) = ToolProgram::from_path(Path::new(&self.command)).command() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("preprocessor `{}` not found on PATH", self.command),
            )
            .into());
        };
        command.arg(path.as_ref());
        ToolOutput::spawn(command)?
            .finish(&format!("preprocessor `{}`", self.command), path.as_ref())
    }
}

#[cfg(not(windows))]
const fn external_tool_path(path: &Path) -> Cow<'_, Path> {
    Cow::Borrowed(path)
}

#[cfg(windows)]
fn external_tool_path(path: &Path) -> Cow<'_, Path> {
    windows_external_tool_path(path).map_or(Cow::Borrowed(path), Cow::Owned)
}

#[cfg(windows)]
fn windows_external_tool_path(path: &Path) -> Option<PathBuf> {
    use std::path::{Component, PathBuf, Prefix};

    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };

    let mut normalized = match prefix.kind() {
        Prefix::VerbatimDisk(disk) => PathBuf::from(format!("{}:\\", char::from(disk))),
        Prefix::VerbatimUNC(server, share) => {
            let mut path = PathBuf::from(r"\\");
            path.push(server);
            path.push(share);
            path
        }
        Prefix::Verbatim(path) => PathBuf::from(path),
        _ => return None,
    };

    normalized.extend(components);
    Some(normalized)
}

struct PreprocessorGlobs {
    globs: Option<GlobSet>,
}

impl PreprocessorGlobs {
    fn new(patterns: &[String]) -> sift_core::Result<Self> {
        if patterns.is_empty() {
            return Ok(Self { globs: None });
        }
        let mut builder = GlobSetBuilder::new();
        for pattern in patterns {
            let glob = Glob::new(pattern).map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid --pre-glob `{pattern}`: {err}"),
                )
            })?;
            builder.add(glob);
        }
        Ok(Self {
            globs: Some(builder.build().map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("invalid --pre-glob set: {err}"),
                )
            })?),
        })
    }

    fn matches(&self, path: &Path) -> bool {
        self.globs.as_ref().is_none_or(|globs| {
            let rel = path
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            globs.is_match(&rel)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_paths_and_stdin_resolves_candidates() {
        let sources = InputSources {
            paths: vec![PathBuf::from("src")],
            stdin_bytes: vec![b"stream\n".to_vec()],
            stdin_explicit: true,
        };
        assert!(sources.resolve_candidates());
    }

    #[test]
    fn stdin_only_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: vec![b"stream\n".to_vec()],
            stdin_explicit: true,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn explicit_dash_empty_stdin_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: Vec::new(),
            stdin_explicit: true,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn implicit_stdin_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: vec![b"stream\n".to_vec()],
            stdin_explicit: false,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn paths_without_stdin_resolves_candidates() {
        let sources = InputSources {
            paths: vec![PathBuf::from("src")],
            stdin_bytes: Vec::new(),
            stdin_explicit: false,
        };
        assert!(sources.resolve_candidates());
    }

    #[test]
    fn default_corpus_without_paths_or_stdin_resolves_candidates() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: Vec::new(),
            stdin_explicit: false,
        };
        assert!(sources.resolve_candidates());
    }

    #[test]
    fn pre_globs_match_forward_slash_paths() {
        let globs = PreprocessorGlobs::new(&["src/*.txt".to_string()]).unwrap();
        assert!(globs.matches(Path::new("src/a.txt")));
        assert!(!globs.matches(Path::new("src/a.rs")));
    }

    #[test]
    fn empty_pre_globs_match_everything() {
        let globs = PreprocessorGlobs::new(&[]).unwrap();
        assert!(globs.matches(Path::new("a.rs")));
    }

    #[test]
    fn invalid_pre_glob_errors() {
        let Err(err) = PreprocessorGlobs::new(&["[".to_string()]) else {
            panic!("invalid glob unexpectedly succeeded");
        };
        assert!(err.to_string().contains("invalid --pre-glob"));
    }

    #[test]
    fn zip_tool_matches_gzip_and_zstd_suffixes() {
        assert_eq!(ZipTool::from_path(Path::new("a.gz")), Some(ZipTool::Gzip));
        assert_eq!(ZipTool::from_path(Path::new("a.tgz")), Some(ZipTool::Gzip));
        assert_eq!(ZipTool::from_path(Path::new("a.zst")), Some(ZipTool::Zstd));
        assert_eq!(ZipTool::from_path(Path::new("a.zstd")), Some(ZipTool::Zstd));
        assert_eq!(
            ZipTool::from_path(Path::new("a.Z")),
            Some(ZipTool::Compress)
        );
        assert_eq!(ZipTool::from_path(Path::new("a.rs")), None);
    }

    #[test]
    fn zip_tool_reports_decompressor_failure() {
        if ToolProgram::new("gzip").command().is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.gz");
        std::fs::write(&path, b"not gzip").unwrap();
        let err = ZipTool::Gzip.read(&path).unwrap_err();
        assert!(
            err.to_string().contains("gzip"),
            "expected decompressor failure, got {err}"
        );
    }
}
