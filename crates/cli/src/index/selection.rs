//! Index kind selection for `sift index build` / `sift index update`.
//!
//! Clap declares `--index` / `--width` / `--norm`. [`IndexSelection`] walks
//! argv so options attach to the preceding `--index` (ripgrep-style order).

use clap::{Args, ValueEnum};
use sift_core::{GramNorm, GramWidth, IndexRecord};

use crate::grep::Argv;

/// Index kind accepted by `--index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum IndexKindArg {
    Ngram,
    Ast,
}

/// Gram normalization accepted by `--norm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GramNormArg {
    Identity,
    #[value(name = "ascii-lower")]
    AsciiLower,
}

impl GramNormArg {
    const fn to_gram_norm(self) -> GramNorm {
        match self {
            Self::Identity => GramNorm::Identity,
            Self::AsciiLower => GramNorm::AsciiLower,
        }
    }
}

/// Clap-declared index selection flags (help + parse acceptance).
///
/// Effective values come from [`IndexSelection::resolve`], which associates
/// `--width` / `--norm` with the preceding `--index`.
#[derive(Args, Debug, Clone, Default)]
pub struct IndexDecl {
    /// Index kind to build (repeatable). Omit to use the default set.
    #[arg(long = "index", value_enum, value_name = "KIND")]
    pub kinds: Vec<IndexKindArg>,

    /// N-gram width for the preceding `--index`.
    #[arg(long, value_name = "N")]
    pub width: Vec<u8>,

    /// Gram normalization for the preceding `--index`.
    #[arg(long = "norm", value_enum, value_name = "NORM")]
    pub norms: Vec<GramNormArg>,
}

/// Resolved indexes selected on `index build` / `index update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexSelection {
    pub indexes: Vec<IndexRecord>,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    kind: IndexKindArg,
    width: Option<u8>,
    norm: Option<GramNormArg>,
}

impl Pending {
    fn into_record(self) -> anyhow::Result<IndexRecord> {
        match self.kind {
            IndexKindArg::Ngram => {
                let width = match self.width {
                    Some(raw) => GramWidth::try_new(raw).map_err(anyhow::Error::msg)?,
                    None => GramWidth::TRIGRAM,
                };
                let norm = self
                    .norm
                    .map_or(GramNorm::Identity, GramNormArg::to_gram_norm);
                Ok(IndexRecord::ngram_norm(width, norm))
            }
            IndexKindArg::Ast => {
                if self.width.is_some() {
                    return Err(anyhow::anyhow!("--width does not apply to --index ast"));
                }
                if self.norm.is_some() {
                    return Err(anyhow::anyhow!("--norm does not apply to --index ast"));
                }
                Ok(IndexRecord::Ast)
            }
        }
    }
}

impl IndexSelection {
    /// Walk argv associating `--width` / `--norm` with the preceding `--index`.
    ///
    /// Returns `None` when no `--index` appears (caller uses the default catalog).
    /// `--width` / `--norm` require a preceding `--index`; they never imply a kind.
    ///
    /// # Errors
    ///
    /// Returns an error when a flag value is missing or invalid, or when
    /// `--width` / `--norm` appear without a preceding `--index`.
    pub fn resolve(argv: &Argv<'_>) -> anyhow::Result<Option<Self>> {
        let lifecycle = index_lifecycle_args(argv.as_slice());
        let mut records = Vec::new();
        let mut pending: Option<Pending> = None;

        let mut i = 0;
        while i < lifecycle.len() {
            let arg = lifecycle[i].as_str();
            if arg == "--" {
                break;
            }

            if arg == "--index" {
                let kind = next_value(lifecycle, i, "--index")?;
                flush_pending(&mut pending, &mut records)?;
                pending = Some(Pending {
                    kind: parse_kind(kind)?,
                    width: None,
                    norm: None,
                });
                i += 2;
                continue;
            }
            if let Some(kind) = arg.strip_prefix("--index=") {
                flush_pending(&mut pending, &mut records)?;
                pending = Some(Pending {
                    kind: parse_kind(kind)?,
                    width: None,
                    norm: None,
                });
                i += 1;
                continue;
            }

            if arg == "--width" {
                let raw = next_value(lifecycle, i, "--width")?;
                require_index(&mut pending, "--width")?.width = Some(parse_width(raw)?);
                i += 2;
                continue;
            }
            if let Some(raw) = arg.strip_prefix("--width=") {
                require_index(&mut pending, "--width")?.width = Some(parse_width(raw)?);
                i += 1;
                continue;
            }

            if arg == "--norm" {
                let raw = next_value(lifecycle, i, "--norm")?;
                require_index(&mut pending, "--norm")?.norm = Some(parse_norm(raw)?);
                i += 2;
                continue;
            }
            if let Some(raw) = arg.strip_prefix("--norm=") {
                require_index(&mut pending, "--norm")?.norm = Some(parse_norm(raw)?);
                i += 1;
                continue;
            }

            i += 1;
        }

        flush_pending(&mut pending, &mut records)?;
        if records.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Self { indexes: records }))
        }
    }
}

fn require_index<'a>(
    pending: &'a mut Option<Pending>,
    flag: &str,
) -> anyhow::Result<&'a mut Pending> {
    pending.as_mut().ok_or_else(|| {
        anyhow::anyhow!("{flag} requires a preceding --index KIND (e.g. --index ngram {flag} …)")
    })
}

fn index_lifecycle_args(args: &[String]) -> &[String] {
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--" {
            return &[];
        }
        if args[i] == "index"
            && i + 1 < args.len()
            && matches!(args[i + 1].as_str(), "build" | "update")
        {
            return &args[i + 2..];
        }
        i += 1;
    }
    &[]
}

fn flush_pending(
    pending: &mut Option<Pending>,
    records: &mut Vec<IndexRecord>,
) -> anyhow::Result<()> {
    if let Some(p) = pending.take() {
        records.push(p.into_record()?);
    }
    Ok(())
}

fn next_value<'a>(args: &'a [String], i: usize, flag: &str) -> anyhow::Result<&'a str> {
    args.get(i + 1)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn parse_kind(raw: &str) -> anyhow::Result<IndexKindArg> {
    match raw {
        "ngram" => Ok(IndexKindArg::Ngram),
        "ast" => Ok(IndexKindArg::Ast),
        other => Err(anyhow::anyhow!(
            "unknown index kind '{other}' (expected ngram or ast)"
        )),
    }
}

fn parse_width(raw: &str) -> anyhow::Result<u8> {
    raw.parse::<u8>()
        .map_err(|_| anyhow::anyhow!("invalid --width value '{raw}'"))
}

fn parse_norm(raw: &str) -> anyhow::Result<GramNormArg> {
    match raw {
        "identity" => Ok(GramNormArg::Identity),
        "ascii-lower" => Ok(GramNormArg::AsciiLower),
        other => Err(anyhow::anyhow!(
            "unknown --norm '{other}' (expected identity or ascii-lower)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Argv<'static> {
        let owned: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        // Leak for 'static test argv — test-only.
        let leaked: &'static [String] = Box::leak(owned.into_boxed_slice());
        Argv::new(leaked)
    }

    #[test]
    fn resolve_associates_width_and_norm_with_index() {
        let selection = IndexSelection::resolve(&argv(&[
            "sift",
            "index",
            "build",
            "--index",
            "ngram",
            "--width",
            "5",
            "--norm",
            "ascii-lower",
        ]))
        .expect("resolve")
        .expect("selection");
        assert_eq!(selection.indexes.len(), 1);
        assert_eq!(selection.indexes[0].name(), "ngram-5-ascii-lower");
    }

    #[test]
    fn width_without_index_errors() {
        let err = IndexSelection::resolve(&argv(&["sift", "index", "build", "--width", "5"]))
            .expect_err("must error");
        assert!(
            err.to_string()
                .contains("--width requires a preceding --index")
        );
    }

    #[test]
    fn resolve_accepts_ast_kind() {
        let selection = IndexSelection::resolve(&argv(&[
            "sift", "index", "build", "--index", "ngram", "--index", "ast",
        ]))
        .expect("resolve")
        .expect("selection");
        let names: Vec<String> = selection.indexes.iter().map(|r| r.name()).collect();
        assert_eq!(names, vec!["ngram-3".to_string(), "ast".to_string()]);
    }

    #[test]
    fn ast_rejects_width_and_norm() {
        let err = IndexSelection::resolve(&argv(&[
            "sift", "index", "build", "--index", "ast", "--width", "3",
        ]))
        .expect_err("must error");
        assert!(err.to_string().contains("--width does not apply"));

        let err = IndexSelection::resolve(&argv(&[
            "sift",
            "index",
            "build",
            "--index",
            "ast",
            "--norm",
            "ascii-lower",
        ]))
        .expect_err("must error");
        assert!(err.to_string().contains("--norm does not apply"));
    }

    #[test]
    fn no_index_returns_none() {
        let selection =
            IndexSelection::resolve(&argv(&["sift", "index", "build"])).expect("resolve");
        assert!(selection.is_none());
    }
}
