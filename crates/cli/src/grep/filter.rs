use std::path::PathBuf;
use std::str::FromStr;

use clap::{Arg, ArgAction, ArgMatches, Args, Command, FromArgMatches};
use ignore::types::TypesBuilder;
use sift_core::{FileFilterConfig, GlobConfig, IgnoreConfig, TypeFilterRule, VisibilityConfig};
use sift_core::{FileOrder, FileOrderDirection, FileOrderKey};

use super::argv::Argv;
use super::ignore::{IgnoreResolution, MessageFlags};
use super::output::OutputArgv;

#[derive(Clone)]
pub struct FilterConfig {
    pub decl: FilterDecl,
    pub glob_patterns: Vec<String>,
    pub follow_links: bool,
    pub one_file_system: bool,
}

impl FilterConfig {
    /// Build a [`FileFilterConfig`] from CLI declarations and resolved filter context.
    ///
    /// # Errors
    ///
    /// Returns an error if `max_filesize` parsing fails.
    pub fn file_config(
        &self,
        filter: FilterResolution,
        type_filters: Vec<TypeFilterRule>,
        scopes: Vec<PathBuf>,
        exclude_paths: Vec<PathBuf>,
    ) -> anyhow::Result<FileFilterConfig> {
        let max_filesize = self
            .decl
            .max_filesize
            .as_ref()
            .map(|s| s.parse::<ByteSize>().map(ByteSize::bytes))
            .transpose()?;

        let mut glob_patterns = self.glob_patterns.clone();
        for ig in &self.decl.iglob {
            glob_patterns.push(ig.clone());
        }

        let glob_ci = filter.glob_case_insensitive || !self.decl.iglob.is_empty();

        Ok(FileFilterConfig {
            scopes,
            exclude_paths,
            glob: GlobConfig {
                patterns: glob_patterns,
                case_insensitive: glob_ci,
            },
            visibility: VisibilityConfig {
                hidden: filter.ignore.hidden_mode(),
                ignore: IgnoreConfig {
                    sources: filter.ignore.sources,
                    custom_files: if filter
                        .ignore
                        .msg_flags
                        .contains(MessageFlags::NO_IGNORE_FILES)
                    {
                        Vec::new()
                    } else {
                        self.decl.ignore_file.clone()
                    },
                    require_git: filter.ignore.require_git,
                },
            },
            follow_links: self.follow_links,
            max_depth: self.decl.max_depth,
            max_filesize,
            type_filters,
            one_file_system: self.one_file_system,
        })
    }
}

#[derive(Args, Clone, Default)]
pub struct FilterDecl {
    #[arg(long = "max-depth", value_name = "NUM")]
    pub max_depth: Option<usize>,
    #[arg(long = "max-filesize", value_name = "NUM+SUFFIX?")]
    pub max_filesize: Option<String>,
    #[arg(long = "iglob", action = ArgAction::Append, value_name = "GLOB")]
    pub iglob: Vec<String>,
    #[arg(long = "ignore-file", action = ArgAction::Append, value_name = "PATH")]
    pub ignore_file: Vec<PathBuf>,
    #[arg(long = "files")]
    pub files: bool,
    #[arg(short = 't', long = "type", action = ArgAction::Append, value_name = "TYPE")]
    pub type_include: Vec<String>,
    #[arg(short = 'T', long = "type-not", action = ArgAction::Append, value_name = "TYPE")]
    pub type_exclude: Vec<String>,
    #[arg(long = "type-list")]
    pub type_list: bool,
    #[arg(long = "type-add", action = ArgAction::Append, value_name = "TYPE_SPEC")]
    pub type_add: Vec<String>,
    #[arg(long = "type-clear", action = ArgAction::Append, value_name = "TYPE")]
    pub type_clear: Vec<String>,
    #[arg(long = "sort", action = ArgAction::Append, value_name = "SORTBY")]
    pub sort: Vec<String>,
    #[arg(long = "sortr", action = ArgAction::Append, value_name = "SORTBY")]
    pub sortr: Vec<String>,
    #[arg(long = "sort-files")]
    pub sort_files: bool,
}

impl FilterDecl {
    /// Resolve ripgrep-style sort flags from raw argv.
    ///
    /// # Errors
    ///
    /// Returns an error if a sort flag is missing a value or uses an unknown key.
    pub fn file_order(&self, argv: &Argv<'_>) -> anyhow::Result<FileOrder> {
        let mut order = self.sort_files.then_some(FileOrder::new(
            FileOrderKey::Path,
            FileOrderDirection::Ascending,
        ));
        let mut iter = argv.as_slice().iter().skip(1);
        while let Some(arg) = iter.next() {
            if arg == "--" {
                break;
            }
            if arg == "--sort-files" {
                order = Some(FileOrder::new(
                    FileOrderKey::Path,
                    FileOrderDirection::Ascending,
                ));
                continue;
            }
            if let Some(value) = arg.strip_prefix("--sort=") {
                order = Some(FileOrder::new(
                    Self::parse_sort_key(value)?,
                    FileOrderDirection::Ascending,
                ));
                continue;
            }
            if let Some(value) = arg.strip_prefix("--sortr=") {
                order = Some(FileOrder::new(
                    Self::parse_sort_key(value)?,
                    FileOrderDirection::Descending,
                ));
                continue;
            }
            if arg == "--sort" {
                let Some(value) = iter.next() else {
                    anyhow::bail!("--sort requires a sort key");
                };
                order = Some(FileOrder::new(
                    Self::parse_sort_key(value)?,
                    FileOrderDirection::Ascending,
                ));
                continue;
            }
            if arg == "--sortr" {
                let Some(value) = iter.next() else {
                    anyhow::bail!("--sortr requires a sort key");
                };
                order = Some(FileOrder::new(
                    Self::parse_sort_key(value)?,
                    FileOrderDirection::Descending,
                ));
            }
        }

        Ok(order.unwrap_or_default())
    }

    fn parse_sort_key(value: &str) -> anyhow::Result<FileOrderKey> {
        match value {
            "none" => Ok(FileOrderKey::None),
            "path" => Ok(FileOrderKey::Path),
            "modified" => Ok(FileOrderKey::Modified),
            "accessed" => Ok(FileOrderKey::Accessed),
            "created" => Ok(FileOrderKey::Created),
            other => anyhow::bail!(
                "unknown sort key '{other}': expected none, path, modified, accessed, or created"
            ),
        }
    }
}

/// Resolved visibility, ignore sources, and glob case for [`FileFilterConfig`].
#[derive(Clone, Copy, Default)]
pub struct FilterResolution {
    ignore: IgnoreResolution,
    glob_case_insensitive: bool,
}

impl FilterResolution {
    #[must_use]
    pub fn resolve(argv: &Argv<'_>) -> Self {
        let output = OutputArgv::resolve(argv);
        Self {
            ignore: IgnoreResolution::resolve(argv),
            glob_case_insensitive: output.path.glob_case_insensitive,
        }
    }
}

#[derive(Clone)]
pub struct GlobFlags {
    pub glob: Vec<String>,
}

impl GlobFlags {
    const fn new() -> Self {
        Self { glob: Vec::new() }
    }
}

impl Args for GlobFlags {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(
            Arg::new("glob")
                .short('g')
                .long("glob")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("glob_case_insensitive")
                .long("glob-case-insensitive")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_glob_case_insensitive")
                .long("no-glob-case-insensitive")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("hidden")
                .short('.')
                .long("hidden")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_hidden")
                .long("no-hidden")
                .action(ArgAction::SetTrue),
        )
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

impl FromArgMatches for GlobFlags {
    fn from_arg_matches(matches: &ArgMatches) -> Result<Self, clap::Error> {
        let glob = matches
            .get_many::<String>("glob")
            .map(|v| v.cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        Ok(Self { glob })
    }

    fn update_from_arg_matches(&mut self, matches: &ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(matches)?;
        Ok(())
    }
}

impl Default for GlobFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed byte size from CLI strings like `10K` or `2MB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSize(u64);

impl ByteSize {
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.0
    }
}

impl FromStr for ByteSize {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let (num_part, suffix) = s.find(|c: char| c.is_ascii_alphabetic()).map_or_else(
            || (s, String::new()),
            |i| (&s[..i], s[i..].to_ascii_uppercase()),
        );
        let base: u64 = num_part
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid size: '{s}'"))?;
        let multiplier: u64 = match suffix.as_str() {
            "" | "B" => 1,
            "K" | "KB" => 1024,
            "M" | "MB" => 1024 * 1024,
            "G" | "GB" => 1024 * 1024 * 1024,
            _ => anyhow::bail!("unknown size suffix: '{suffix}'"),
        };
        Ok(Self(base * multiplier))
    }
}

/// Built-in and user-defined file type definitions.
pub struct TypeCatalog {
    defs: Vec<TypeDef>,
}

#[derive(Clone)]
pub struct TypeDef {
    name: String,
    globs: Vec<String>,
}

impl TypeCatalog {
    fn builder() -> TypesBuilder {
        let mut builder = TypesBuilder::new();
        builder.add_defaults();
        builder
    }

    /// Resolve ripgrep-compatible built-in and custom type definitions.
    ///
    /// # Errors
    ///
    /// Returns an error when `--type-add` uses invalid syntax or references an
    /// unknown type in an `include:` directive.
    pub fn from_argv(argv: &Argv<'_>) -> anyhow::Result<Self> {
        let mut builder = Self::builder();
        let mut iter = argv.as_slice().iter().skip(1);
        while let Some(arg) = iter.next() {
            if arg == "--" {
                break;
            }
            if let Some(spec) = arg.strip_prefix("--type-add=") {
                Self::add_def(&mut builder, spec)?;
                continue;
            }
            if arg == "--type-add" {
                let Some(spec) = iter.next() else {
                    anyhow::bail!("--type-add requires a type definition");
                };
                Self::add_def(&mut builder, spec)?;
                continue;
            }
            if let Some(name) = arg.strip_prefix("--type-clear=") {
                builder.clear(name);
                continue;
            }
            if arg == "--type-clear" {
                let Some(name) = iter.next() else {
                    anyhow::bail!("--type-clear requires a type name");
                };
                builder.clear(name);
            }
        }

        Ok(Self::from_builder(&builder))
    }

    fn add_def(builder: &mut TypesBuilder, spec: &str) -> anyhow::Result<()> {
        builder
            .add_def(spec)
            .map_err(|e| anyhow::anyhow!("invalid type definition '{spec}': {e}"))
    }

    fn from_builder(builder: &TypesBuilder) -> Self {
        let defs = builder
            .definitions()
            .into_iter()
            .map(|def| TypeDef {
                name: def.name().to_string(),
                globs: def.globs().to_vec(),
            })
            .collect();
        Self { defs }
    }

    /// Resolve ordered `-t` / `-T` filters against this catalog.
    ///
    /// # Errors
    ///
    /// Returns an error when a type flag is missing a value or references an unknown type.
    pub fn filters(&self, argv: &Argv<'_>) -> anyhow::Result<Vec<TypeFilterRule>> {
        let mut filters = Vec::new();
        let mut iter = argv.as_slice().iter().skip(1);
        while let Some(arg) = iter.next() {
            if arg == "--" {
                break;
            }
            if let Some(name) = arg.strip_prefix("--type=") {
                filters.push(self.filter(name, false)?);
                continue;
            }
            if arg == "--type" || arg == "-t" {
                let Some(name) = iter.next() else {
                    anyhow::bail!("{arg} requires a file type");
                };
                filters.push(self.filter(name, false)?);
                continue;
            }
            if let Some(name) = arg.strip_prefix("--type-not=") {
                filters.push(self.filter(name, true)?);
                continue;
            }
            if arg == "--type-not" || arg == "-T" {
                let Some(name) = iter.next() else {
                    anyhow::bail!("{arg} requires a file type");
                };
                filters.push(self.filter(name, true)?);
            }
        }
        Ok(filters)
    }

    fn filter(&self, name: &str, exclude: bool) -> anyhow::Result<TypeFilterRule> {
        let globs = self.globs(name)?;
        if exclude {
            Ok(TypeFilterRule::Exclude {
                name: name.to_string(),
                globs,
            })
        } else {
            Ok(TypeFilterRule::Include {
                name: name.to_string(),
                globs,
            })
        }
    }

    fn globs(&self, name: &str) -> anyhow::Result<Vec<String>> {
        if name == "all" {
            return Ok(self
                .defs
                .iter()
                .flat_map(|def| def.globs.iter().cloned())
                .collect());
        }
        self.defs
            .iter()
            .find(|def| def.name == name)
            .map(|def| def.globs.clone())
            .ok_or_else(|| anyhow::anyhow!("unknown file type: '{name}'"))
    }

    #[must_use]
    pub fn definitions(&self) -> &[TypeDef] {
        &self.defs
    }

    /// Print sorted type definitions for `--type-list`.
    pub fn print_list(&self) {
        let mut defs = self.definitions().to_vec();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        for def in &defs {
            println!("{}: {}", def.name, def.globs.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(args: &[&str]) -> TypeCatalog {
        let raw_args = args
            .iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>();
        TypeCatalog::from_argv(&Argv::new(&raw_args)).expect("type catalog")
    }

    #[test]
    fn size_suffix_plain_number() {
        assert_eq!(ByteSize::from_str("42").unwrap().bytes(), 42);
    }

    #[test]
    fn size_suffix_bytes() {
        assert_eq!(ByteSize::from_str("100B").unwrap().bytes(), 100);
    }

    #[test]
    fn size_suffix_kilobytes() {
        assert_eq!(ByteSize::from_str("2K").unwrap().bytes(), 2048);
        assert_eq!(ByteSize::from_str("2KB").unwrap().bytes(), 2048);
    }

    #[test]
    fn size_suffix_megabytes() {
        assert_eq!(ByteSize::from_str("3M").unwrap().bytes(), 3 * 1024 * 1024);
        assert_eq!(ByteSize::from_str("3MB").unwrap().bytes(), 3 * 1024 * 1024);
    }

    #[test]
    fn size_suffix_gigabytes() {
        assert_eq!(
            ByteSize::from_str("1G").unwrap().bytes(),
            1024 * 1024 * 1024
        );
    }

    #[test]
    fn size_suffix_unknown_unit() {
        assert!(ByteSize::from_str("5X").is_err());
    }

    #[test]
    fn size_suffix_invalid_number() {
        assert!(ByteSize::from_str("abc").is_err());
    }

    #[test]
    fn builtin_type_defs_contains_rust() {
        let catalog = catalog(&["sift"]);
        let defs = catalog.definitions();
        assert!(defs.iter().any(|d| d.name == "rust"));
    }

    #[test]
    fn builtin_type_defs_contains_python() {
        let catalog = catalog(&["sift"]);
        let defs = catalog.definitions();
        assert!(defs.iter().any(|d| d.name == "py"));
        assert!(defs.iter().any(|d| d.name == "python"));
    }

    #[test]
    fn builtin_type_defs_non_empty() {
        let catalog = catalog(&["sift"]);
        assert!(!catalog.definitions().is_empty());
    }

    #[test]
    fn resolve_type_defs_clear_removes_type() {
        let catalog = catalog(&["sift", "--type-clear", "rust", "--type-clear", "py"]);
        let defs = catalog.definitions();
        assert!(!defs.iter().any(|d| d.name == "rust"));
        assert!(!defs.iter().any(|d| d.name == "py"));
    }

    #[test]
    fn resolve_type_defs_add_custom_type() {
        let catalog = catalog(&["sift", "--type-add", "mytype:*.my"]);
        let defs = catalog.definitions();
        assert!(defs.iter().any(|d| d.name == "mytype"));
    }

    #[test]
    fn resolve_type_defs_add_extends_existing() {
        let catalog = catalog(&["sift", "--type-add", "rust:*.rsx"]);
        let defs = catalog.definitions();
        let rust = defs.iter().find(|d| d.name == "rust").unwrap();
        assert!(rust.globs.contains(&"*.rsx".to_string()));
        assert!(rust.globs.contains(&"*.rs".to_string()));
    }

    #[test]
    fn resolve_type_defs_add_include() {
        let catalog = catalog(&["sift", "--type-add", "combined:include:rust,py"]);
        let defs = catalog.definitions();
        let combined = defs.iter().find(|d| d.name == "combined").unwrap();
        assert!(combined.globs.contains(&"*.rs".to_string()));
        assert!(combined.globs.contains(&"*.py".to_string()));
    }

    #[test]
    fn resolve_type_defs_clear_then_add_uses_argv_order() {
        let catalog = catalog(&["sift", "--type-clear", "rust", "--type-add", "rust:*.rsx"]);
        let rust = catalog
            .definitions()
            .iter()
            .find(|d| d.name == "rust")
            .cloned()
            .unwrap();
        assert_eq!(rust.globs, ["*.rsx"]);
    }

    #[test]
    fn resolve_type_defs_add_then_clear_uses_argv_order() {
        let catalog = catalog(&[
            "sift",
            "--type-add",
            "custom:*.custom",
            "--type-clear",
            "custom",
        ]);
        assert!(!catalog.definitions().iter().any(|d| d.name == "custom"));
    }

    #[test]
    fn filter_resolution_hidden_mode_include() {
        use crate::grep::ignore::IgnoreResolution;
        use sift_core::IgnoreSources;
        let ctx = FilterResolution {
            ignore: IgnoreResolution {
                hidden: true,
                sources: IgnoreSources::empty(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            ctx.ignore.hidden_mode(),
            sift_core::HiddenMode::Include
        ));
    }

    #[test]
    fn filter_resolution_hidden_mode_respect() {
        use crate::grep::ignore::IgnoreResolution;
        use sift_core::IgnoreSources;
        let ctx = FilterResolution {
            ignore: IgnoreResolution {
                sources: IgnoreSources::empty(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(matches!(
            ctx.ignore.hidden_mode(),
            sift_core::HiddenMode::Respect
        ));
    }

    #[test]
    fn glob_flags_default_empty() {
        let g = GlobFlags { glob: vec![] };
        assert!(g.glob.is_empty());
    }
}
