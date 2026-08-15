use clap::Args;
use sift_core::search::{InputEncoding, RegexEngine};

/// Regex engine and configuration flags.
#[derive(Args, Clone)]
pub struct EngineDecl {
    #[arg(long = "no-config")]
    pub no_config: bool,
    #[arg(long = "unicode")]
    pub unicode: bool,
    #[arg(long = "no-unicode")]
    pub no_unicode: bool,
    #[arg(long = "colors", value_name = "COLOR_SPEC")]
    pub colors: Vec<String>,
    #[arg(long = "hyperlink-format", value_name = "FORMAT")]
    pub hyperlink_format: Option<String>,
    #[arg(long = "hostname-bin", value_name = "COMMAND")]
    pub hostname_bin: Option<String>,
    #[arg(long = "regex-size-limit", value_name = "NUM+SUFFIX?")]
    pub regex_size_limit: Option<String>,
    #[arg(long = "dfa-size-limit", value_name = "NUM+SUFFIX?")]
    pub dfa_size_limit: Option<String>,
    #[command(flatten)]
    pub content: ContentDecl,
    #[arg(short = 'E', long = "encoding", value_name = "ENCODING")]
    pub encoding: Option<InputEncoding>,
    #[command(flatten)]
    pub regex: RegexEngineDecl,
}

#[derive(Args, Clone, Default)]
pub struct ContentDecl {
    #[arg(short = 'z', long = "search-zip")]
    pub search_zip: bool,
    #[arg(long = "pre", value_name = "COMMAND")]
    pub pre: Option<String>,
    #[arg(long = "pre-glob", value_name = "GLOB")]
    pub pre_glob: Vec<String>,
}

/// Regex engine selection flags.
#[derive(Args, Clone)]
pub struct RegexEngineDecl {
    #[arg(long = "engine", value_name = "ENGINE")]
    pub engine: Option<RegexEngine>,
    #[command(flatten)]
    pub pcre2: Pcre2EngineDecl,
    #[arg(long = "pcre2-version")]
    pub pcre2_version: bool,
}

/// PCRE2 engine selection shortcuts.
#[derive(Args, Clone)]
pub struct Pcre2EngineDecl {
    #[arg(short = 'P', long = "pcre2")]
    pub pcre2: bool,
    #[arg(long = "no-pcre2")]
    pub no_pcre2: bool,
    #[arg(long = "auto-hybrid-regex")]
    pub auto_hybrid_regex: bool,
}

/// Threading and output-buffering flags.
#[derive(Args, Clone)]
pub struct ThreadingDecl {
    /// Process-global Rayon thread pool size for this process.
    #[arg(
        short = 'j',
        long = "threads",
        value_name = "NUM",
        help = "Set the process-global Rayon thread pool size for this process"
    )]
    pub threads: Option<usize>,
    #[arg(long = "line-buffered")]
    pub line_buffered: bool,
    #[arg(long = "block-buffered")]
    pub block_buffered: bool,
    #[arg(long = "path-separator", value_name = "SEPARATOR")]
    pub path_separator: Option<String>,
}

/// Filesystem-level flags for the walker.
#[derive(Args)]
pub struct WalkerDecl {
    #[arg(long = "one-file-system")]
    pub one_file_system: bool,
    #[arg(long = "mmap")]
    pub mmap: bool,
    #[arg(long = "no-mmap")]
    pub no_mmap: bool,
}

/// Multiline and CRLF flags.
#[derive(Args, Clone)]
pub struct MultilineDecl {
    #[arg(short = 'U', long = "multiline")]
    pub multiline: bool,
    #[arg(long = "multiline-dotall")]
    pub multiline_dotall: bool,
    #[command(flatten)]
    pub line_terminator: LineTerminatorDecl,
}

#[derive(Args, Clone)]
pub struct LineTerminatorDecl {
    #[arg(long = "crlf")]
    pub crlf: bool,
    #[arg(long = "null-data")]
    pub null_data: bool,
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;
    use clap::Parser;
    use sift_core::search::{InputEncoding, RegexEngine};

    #[test]
    fn engine_no_config_flag() {
        let cli = Cli::try_parse_from(["sift", "--no-config", "pat"]).unwrap();
        assert!(cli.engine_decl.no_config);
    }

    #[test]
    fn engine_unicode_flag() {
        let cli = Cli::try_parse_from(["sift", "--unicode", "pat"]).unwrap();
        assert!(cli.engine_decl.unicode);
    }

    #[test]
    fn engine_no_unicode_flag() {
        let cli = Cli::try_parse_from(["sift", "--no-unicode", "pat"]).unwrap();
        assert!(cli.engine_decl.no_unicode);
    }

    #[test]
    fn engine_colors_flag() {
        let cli = Cli::try_parse_from(["sift", "--colors", "path:fg:red", "pat"]).unwrap();
        assert!(!cli.engine_decl.colors.is_empty());
    }

    #[test]
    fn engine_hyperlink_format_flag() {
        let cli = Cli::try_parse_from(["sift", "--hyperlink-format", "vscode", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.hyperlink_format.as_deref(), Some("vscode"));
    }

    #[test]
    fn engine_hostname_bin_flag() {
        let cli = Cli::try_parse_from(["sift", "--hostname-bin", "hostname", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.hostname_bin.as_deref(), Some("hostname"));
    }

    #[test]
    fn engine_regex_size_limit() {
        let cli = Cli::try_parse_from(["sift", "--regex-size-limit", "10M", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.regex_size_limit.as_deref(), Some("10M"));
    }

    #[test]
    fn engine_dfa_size_limit() {
        let cli = Cli::try_parse_from(["sift", "--dfa-size-limit", "50M", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.dfa_size_limit.as_deref(), Some("50M"));
    }

    #[test]
    fn engine_search_zip_flag() {
        let cli = Cli::try_parse_from(["sift", "--search-zip", "pat"]).unwrap();
        assert!(cli.engine_decl.content.search_zip);
    }

    #[test]
    fn engine_search_zip_short_flag() {
        let cli = Cli::try_parse_from(["sift", "-z", "pat"]).unwrap();
        assert!(cli.engine_decl.content.search_zip);
    }

    #[test]
    fn engine_preprocessor_flags() {
        let cli =
            Cli::try_parse_from(["sift", "--pre", "upper", "--pre-glob", "*.txt", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.content.pre.as_deref(), Some("upper"));
        assert_eq!(cli.engine_decl.content.pre_glob, ["*.txt"]);
    }

    #[test]
    fn engine_selection_flag() {
        let cli = Cli::try_parse_from(["sift", "--engine", "pcre2", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.regex.engine, Some(RegexEngine::Pcre2));
    }

    #[test]
    fn engine_pcre2_flags() {
        let cli = Cli::try_parse_from(["sift", "--pcre2", "--no-pcre2", "pat"]).unwrap();
        assert!(cli.engine_decl.regex.pcre2.pcre2);
        assert!(cli.engine_decl.regex.pcre2.no_pcre2);
    }

    #[test]
    fn engine_pcre2_short_flag() {
        let cli = Cli::try_parse_from(["sift", "-P", "pat"]).unwrap();
        assert!(cli.engine_decl.regex.pcre2.pcre2);
    }

    #[test]
    fn engine_auto_hybrid_flag() {
        let cli = Cli::try_parse_from(["sift", "--auto-hybrid-regex", "pat"]).unwrap();
        assert!(cli.engine_decl.regex.pcre2.auto_hybrid_regex);
    }

    #[test]
    fn engine_pcre2_version_flag() {
        let cli = Cli::try_parse_from(["sift", "--pcre2-version", "pat"]).unwrap();
        assert!(cli.engine_decl.regex.pcre2_version);
    }

    #[test]
    fn engine_encoding_flag() {
        let cli = Cli::try_parse_from(["sift", "--encoding", "utf-16le", "pat"]).unwrap();
        assert!(matches!(
            cli.engine_decl.encoding,
            Some(InputEncoding::Explicit(_))
        ));
    }

    #[test]
    fn engine_encoding_short_flag() {
        let cli = Cli::try_parse_from(["sift", "-E", "none", "pat"]).unwrap();
        assert_eq!(cli.engine_decl.encoding, Some(InputEncoding::Raw));
    }

    #[test]
    fn threading_threads_flag() {
        let cli = Cli::try_parse_from(["sift", "-j", "4", "pat"]).unwrap();
        assert_eq!(cli.threading.threads, Some(4));
    }

    #[test]
    fn threading_line_buffered() {
        let cli = Cli::try_parse_from(["sift", "--line-buffered", "pat"]).unwrap();
        assert!(cli.threading.line_buffered);
    }

    #[test]
    fn threading_block_buffered() {
        let cli = Cli::try_parse_from(["sift", "--block-buffered", "pat"]).unwrap();
        assert!(cli.threading.block_buffered);
    }

    #[test]
    fn threading_path_separator() {
        let cli = Cli::try_parse_from(["sift", "--path-separator", "/", "pat"]).unwrap();
        assert_eq!(cli.threading.path_separator.as_deref(), Some("/"));
    }

    #[test]
    fn walker_one_file_system() {
        let cli = Cli::try_parse_from(["sift", "--one-file-system", "pat"]).unwrap();
        assert!(cli.walker_decl.one_file_system);
    }

    #[test]
    fn walker_mmap() {
        let cli = Cli::try_parse_from(["sift", "--mmap", "pat"]).unwrap();
        assert!(cli.walker_decl.mmap);
    }

    #[test]
    fn walker_no_mmap() {
        let cli = Cli::try_parse_from(["sift", "--no-mmap", "pat"]).unwrap();
        assert!(cli.walker_decl.no_mmap);
    }

    #[test]
    fn multiline_flag() {
        let cli = Cli::try_parse_from(["sift", "-U", "pat"]).unwrap();
        assert!(cli.multiline_decl.multiline);
    }

    #[test]
    fn multiline_long_flag() {
        let cli = Cli::try_parse_from(["sift", "--multiline", "pat"]).unwrap();
        assert!(cli.multiline_decl.multiline);
    }

    #[test]
    fn multiline_dotall_flag() {
        let cli = Cli::try_parse_from(["sift", "--multiline-dotall", "pat"]).unwrap();
        assert!(cli.multiline_decl.multiline_dotall);
    }

    #[test]
    fn crlf_flag() {
        let cli = Cli::try_parse_from(["sift", "--crlf", "pat"]).unwrap();
        assert!(cli.multiline_decl.line_terminator.crlf);
    }

    #[test]
    fn null_data_flag() {
        let cli = Cli::try_parse_from(["sift", "--null-data", "pat"]).unwrap();
        assert!(cli.multiline_decl.line_terminator.null_data);
    }

    #[test]
    fn combined_engine_flags() {
        let cli = Cli::try_parse_from([
            "sift",
            "--no-config",
            "--unicode",
            "-j",
            "8",
            "--one-file-system",
            "-U",
            "--crlf",
            "pat",
        ])
        .unwrap();
        assert!(cli.engine_decl.no_config);
        assert!(cli.engine_decl.unicode);
        assert_eq!(cli.threading.threads, Some(8));
        assert!(cli.walker_decl.one_file_system);
        assert!(cli.multiline_decl.multiline);
        assert!(cli.multiline_decl.line_terminator.crlf);
    }
}
