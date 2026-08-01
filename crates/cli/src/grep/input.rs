use std::borrow::Cow;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use globset::{Glob, GlobSet, GlobSetBuilder};
use grep_cli::DecompressionReaderBuilder;
use sift_core::File;

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
            decompressor: DecompressionReaderBuilder::new(),
        }))
    }

    const fn enabled(&self) -> bool {
        self.search_zip || self.pre.is_some()
    }
}

pub struct ContentTransform {
    search_zip: bool,
    pre: Option<Preprocessor>,
    decompressor: DecompressionReaderBuilder,
}

impl ContentTransform {
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
            return self.read_decompressed(candidate.abs_path());
        }
        Ok(std::fs::read(candidate.abs_path())?)
    }

    fn read_decompressed(&self, path: &Path) -> sift_core::Result<Vec<u8>> {
        let path = external_tool_path(path);
        let mut reader = self
            .decompressor
            .build(path.as_ref())
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Resolved argv paths and optional stdin byte streams.
pub struct InputSources {
    pub paths: Vec<PathBuf>,
    pub stdin_bytes: Vec<Vec<u8>>,
    /// `-` appeared on argv (stdin read even when empty).
    stdin_explicit: bool,
    /// Implicit piped stdin with no paths and no index.
    stdin_implicit: bool,
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
            stdin_implicit: false,
        }
    }

    /// Read stdin when requested and resolve implicit piped input.
    ///
    /// # Errors
    ///
    /// Returns an error if stdin cannot be read.
    pub fn resolve(
        mut self,
        pattern_input: super::pattern::PatternInputUse,
        indexes_empty: bool,
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
            && indexes_empty
            && stdin_is_pipe();
        if implicit_stream {
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes)?;
            if !bytes.is_empty() {
                self.stdin_implicit = true;
                self.stdin_bytes.push(bytes);
            }
        }
        Ok(self)
    }

    #[must_use]
    pub const fn has_paths(&self) -> bool {
        !self.paths.is_empty()
    }

    #[must_use]
    pub const fn has_streams(&self) -> bool {
        !self.stdin_bytes.is_empty()
    }

    /// Whether corpus candidates should be resolved (index/walk).
    ///
    /// Returns `false` for stdin-only runs (explicit `-`, implicit pipe, or
    /// empty explicit `-` with no paths). Mixed runs (paths plus stdin) return
    /// `true`; callers resolve corpus candidates and search them via
    /// [`sift_core::Searcher`].
    /// appends streams.
    #[must_use]
    pub const fn resolve_candidates(&self) -> bool {
        if self.has_paths() {
            return true;
        }
        !(self.stdin_explicit || self.stdin_implicit || self.has_streams())
    }
}

impl InputSources {
    /// Assemble stdin streams and candidate-to-input conversion for search.
    #[must_use]
    pub fn stdin_streams(&self) -> sift_core::Inputs<'_> {
        let mut streams = sift_core::Inputs::empty();
        for bytes in &self.stdin_bytes {
            streams = streams.with_stream(sift_core::ByteInput {
                label: Cow::Borrowed("<stdin>"),
                bytes: Cow::Borrowed(bytes.as_slice()),
                explicit: true,
            });
        }
        streams
    }
}

fn stdin_is_pipe() -> bool {
    use std::io::IsTerminal;

    !std::io::stdin().is_terminal()
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
        let output = Command::new(&self.command).arg(path.as_ref()).output()?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(std::io::Error::other(format!(
                "preprocessor `{}` failed for {}: {}",
                self.command,
                path.display(),
                stderr.trim()
            ))
            .into())
        }
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
            stdin_implicit: false,
        };
        assert!(sources.resolve_candidates());
    }

    #[test]
    fn stdin_only_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: vec![b"stream\n".to_vec()],
            stdin_explicit: true,
            stdin_implicit: false,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn explicit_dash_empty_stdin_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: Vec::new(),
            stdin_explicit: true,
            stdin_implicit: false,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn implicit_stdin_skips_candidate_resolution() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: vec![b"stream\n".to_vec()],
            stdin_explicit: false,
            stdin_implicit: true,
        };
        assert!(!sources.resolve_candidates());
    }

    #[test]
    fn paths_without_stdin_resolves_candidates() {
        let sources = InputSources {
            paths: vec![PathBuf::from("src")],
            stdin_bytes: Vec::new(),
            stdin_explicit: false,
            stdin_implicit: false,
        };
        assert!(sources.resolve_candidates());
    }

    #[test]
    fn default_corpus_without_paths_or_stdin_resolves_candidates() {
        let sources = InputSources {
            paths: Vec::new(),
            stdin_bytes: Vec::new(),
            stdin_explicit: false,
            stdin_implicit: false,
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
}
