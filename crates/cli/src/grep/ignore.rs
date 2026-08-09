use clap::{ArgAction, Args};
use sift_core::IgnoreSources;

use super::argv::Argv;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct MessageFlags: u8 {
        const NO_MESSAGES        = 1 << 0;
        const NO_IGNORE_MESSAGES = 1 << 1;
        const NO_IGNORE_FILES    = 1 << 2;
    }
}

/// Resolved visibility / ignore state from argv.
#[derive(Clone, Copy)]
pub struct IgnoreResolution {
    pub hidden: bool,
    pub sources: sift_core::IgnoreSources,
    pub require_git: bool,
    pub msg_flags: MessageFlags,
}

impl Default for IgnoreResolution {
    fn default() -> Self {
        Self {
            hidden: false,
            sources: sift_core::IgnoreSources::all(),
            require_git: false,
            msg_flags: MessageFlags::empty(),
        }
    }
}

impl IgnoreResolution {
    #[must_use]
    pub const fn hidden_mode(&self) -> sift_core::HiddenMode {
        if self.hidden {
            sift_core::HiddenMode::Include
        } else {
            sift_core::HiddenMode::Respect
        }
    }

    /// Hidden files, ignore rules, and `require_git` — processed in argv order (ripgrep-style).
    #[must_use]
    pub fn resolve(argv: &Argv<'_>) -> Self {
        const DEFAULT_SOURCES: IgnoreSources = IgnoreSources::DOT
            .union(IgnoreSources::VCS)
            .union(IgnoreSources::EXCLUDE)
            .union(IgnoreSources::GLOBAL)
            .union(IgnoreSources::PARENT);

        let mut sources = DEFAULT_SOURCES;
        let mut require_git = false;
        let mut hidden = false;
        let mut u_count: u8 = 0;
        let mut msg_flags = MessageFlags::empty();

        for arg in argv.as_slice() {
            if arg == "--unrestricted" {
                u_count = u_count.saturating_add(1).min(3);
                if u_count == 1 {
                    sources = IgnoreSources::empty();
                } else if u_count == 2 {
                    hidden = true;
                }
                continue;
            }
            if arg.len() >= 2 {
                let bytes = arg.as_bytes();
                if bytes[0] == b'-'
                    && bytes.get(1) != Some(&b'-')
                    && arg[1..].chars().all(|c| c == 'u')
                {
                    for _ in 0..arg.len().saturating_sub(1) {
                        u_count = u_count.saturating_add(1).min(3);
                        if u_count == 1 {
                            sources = IgnoreSources::empty();
                        } else if u_count == 2 {
                            hidden = true;
                        }
                    }
                    continue;
                }
            }

            match arg.as_str() {
                "--no-ignore" => sources = IgnoreSources::empty(),
                "--ignore" => sources = DEFAULT_SOURCES,
                "--no-ignore-vcs" => sources.remove(IgnoreSources::VCS),
                "--ignore-vcs" => sources.insert(IgnoreSources::VCS),
                "--no-ignore-dot" => sources.remove(IgnoreSources::DOT),
                "--ignore-dot" => sources.insert(IgnoreSources::DOT),
                "--no-ignore-global" => sources.remove(IgnoreSources::GLOBAL),
                "--ignore-global" => sources.insert(IgnoreSources::GLOBAL),
                "--no-ignore-exclude" => sources.remove(IgnoreSources::EXCLUDE),
                "--ignore-exclude" => sources.insert(IgnoreSources::EXCLUDE),
                "--no-ignore-parent" => sources.remove(IgnoreSources::PARENT),
                "--ignore-parent" => sources.insert(IgnoreSources::PARENT),
                "--no-require-git" => require_git = false,
                "--require-git" => require_git = true,
                "--hidden" | "-." => hidden = true,
                "--no-hidden" => hidden = false,
                "--no-messages" => msg_flags.insert(MessageFlags::NO_MESSAGES),
                "--messages" => msg_flags.remove(MessageFlags::NO_MESSAGES),
                "--no-ignore-messages" => msg_flags.insert(MessageFlags::NO_IGNORE_MESSAGES),
                "--ignore-messages" => msg_flags.remove(MessageFlags::NO_IGNORE_MESSAGES),
                "--no-ignore-files" => msg_flags.insert(MessageFlags::NO_IGNORE_FILES),
                "--ignore-files" => msg_flags.remove(MessageFlags::NO_IGNORE_FILES),
                _ => {}
            }
        }

        Self {
            hidden,
            sources,
            require_git,
            msg_flags,
        }
    }
}

// ── Clap declarations (ignore/hidden/messages flags) ──

/// Clap declarations only; effective values come from [`IgnoreResolution::resolve`].
#[derive(Args)]
pub struct IgnoreNoDecl {
    #[arg(long = "no-ignore", action = ArgAction::SetTrue)]
    pub no_ignore: bool,
    #[arg(long = "ignore", action = ArgAction::SetTrue)]
    pub ignore_flag: bool,
}

#[derive(Args)]
pub struct IgnoreVcsDecl {
    #[arg(long = "no-ignore-vcs", action = ArgAction::SetTrue)]
    pub no_ignore_vcs: bool,
    #[arg(long = "ignore-vcs", action = ArgAction::SetTrue)]
    pub ignore_vcs: bool,
}

#[derive(Args)]
pub struct IgnoreDotDecl {
    #[arg(long = "no-ignore-dot", action = ArgAction::SetTrue)]
    pub no_ignore_dot: bool,
    #[arg(long = "ignore-dot", action = ArgAction::SetTrue)]
    pub ignore_dot: bool,
}

#[derive(Args)]
pub struct IgnoreGitDecl {
    #[arg(long = "no-require-git", action = ArgAction::SetTrue)]
    pub no_require_git: bool,
    #[arg(long = "require-git", action = ArgAction::SetTrue)]
    pub require_git: bool,
}

#[derive(Args)]
pub struct IgnoreGlobalDecl {
    #[arg(long = "no-ignore-global", action = ArgAction::SetTrue)]
    pub no_ignore_global: bool,
    #[arg(long = "ignore-global", action = ArgAction::SetTrue)]
    pub ignore_global: bool,
}

#[derive(Args)]
pub struct IgnoreExcludeDecl {
    #[arg(long = "no-ignore-exclude", action = ArgAction::SetTrue)]
    pub no_ignore_exclude: bool,
    #[arg(long = "ignore-exclude", action = ArgAction::SetTrue)]
    pub ignore_exclude: bool,
}

#[derive(Args)]
pub struct IgnoreParentDecl {
    #[arg(long = "no-ignore-parent", action = ArgAction::SetTrue)]
    pub no_ignore_parent: bool,
    #[arg(long = "ignore-parent", action = ArgAction::SetTrue)]
    pub ignore_parent: bool,
}

#[derive(Args)]
pub struct IgnoreFilesDecl {
    #[arg(long = "no-ignore-files", action = ArgAction::SetTrue)]
    pub no_ignore_files: bool,
    #[arg(long = "ignore-files", action = ArgAction::SetTrue)]
    pub ignore_files: bool,
}

#[derive(Args)]
pub struct MessagesDecl {
    #[arg(long = "no-messages", action = ArgAction::SetTrue)]
    pub no_messages: bool,
    #[arg(long = "messages", action = ArgAction::SetTrue)]
    pub messages: bool,
}

#[derive(Args)]
pub struct IgnoreMessagesDecl {
    #[arg(long = "no-ignore-messages", action = ArgAction::SetTrue)]
    pub no_ignore_messages: bool,
    #[arg(long = "ignore-messages", action = ArgAction::SetTrue)]
    pub ignore_messages: bool,
}

#[derive(Args)]
pub struct UnrestrictedDecl {
    #[arg(short = 'u', long = "unrestricted", action = ArgAction::Count)]
    pub unrestricted: u8,
}

/// Declares `-A`/`-B`/`-C` for clap; effective values use [`crate::grep::pattern::PatternArgv::context`].
#[derive(Args)]
pub struct ContextDecl {
    #[arg(short = 'A', long = "after-context", value_name = "NUM", action = ArgAction::Append)]
    pub after: Vec<usize>,
    #[arg(short = 'B', long = "before-context", value_name = "NUM", action = ArgAction::Append)]
    pub before: Vec<usize>,
    #[arg(short = 'C', long = "context", value_name = "NUM", action = ArgAction::Append)]
    pub context: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sift_core::IgnoreSources;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn default_resolution() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "pat"])));
        assert!(!r.hidden);
        assert!(!r.require_git);
        assert!(r.sources.contains(IgnoreSources::DOT));
        assert!(r.sources.contains(IgnoreSources::VCS));
        assert!(r.sources.contains(IgnoreSources::EXCLUDE));
        assert!(r.sources.contains(IgnoreSources::GLOBAL));
        assert!(r.sources.contains(IgnoreSources::PARENT));
        assert!(r.msg_flags.is_empty());
    }

    #[test]
    fn unrestricted_one_disables_all_ignore() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--unrestricted", "pat"])));
        assert!(!r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn unrestricted_two_enables_hidden() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "-uu", "pat"])));
        assert!(r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn unrestricted_three_saturates() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "-uuu", "pat"])));
        assert!(r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn unrestricted_long_flag_counting() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "sift",
            "--unrestricted",
            "--unrestricted",
            "pat",
        ])));
        assert!(r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn dash_u_short_flag() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "-u", "pat"])));
        assert!(!r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn no_ignore_clears_all() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--no-ignore", "pat"])));
        assert!(r.sources.is_empty());
    }

    #[test]
    fn ignore_restores_defaults() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "sift",
            "--no-ignore",
            "--ignore",
            "pat",
        ])));
        assert!(r.sources.contains(IgnoreSources::DOT));
        assert!(r.sources.contains(IgnoreSources::VCS));
        assert!(r.sources.contains(IgnoreSources::EXCLUDE));
        assert!(r.sources.contains(IgnoreSources::GLOBAL));
        assert!(r.sources.contains(IgnoreSources::PARENT));
    }

    #[test]
    fn ignore_vcs_toggle_last_wins() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--no-ignore-vcs",
            "--ignore-vcs",
            "pat",
        ])));
        assert!(r.sources.contains(IgnoreSources::VCS));
    }

    #[test]
    fn no_ignore_vcs_last_wins() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--ignore-vcs",
            "--no-ignore-vcs",
            "pat",
        ])));
        assert!(!r.sources.contains(IgnoreSources::VCS));
    }

    #[test]
    fn hidden_flag() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--hidden", "pat"])));
        assert!(r.hidden);
    }

    #[test]
    fn dot_short_flag() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "-.", "pat"])));
        assert!(r.hidden);
    }

    #[test]
    fn no_hidden_resets() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "sift",
            "--hidden",
            "--no-hidden",
            "pat",
        ])));
        assert!(!r.hidden);
    }

    #[test]
    fn require_git_toggle() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--no-require-git",
            "--require-git",
            "pat",
        ])));
        assert!(r.require_git);
    }

    #[test]
    fn message_no_messages() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--no-messages", "pat"])));
        assert!(r.msg_flags.contains(MessageFlags::NO_MESSAGES));
    }

    #[test]
    fn message_no_ignore_messages() {
        let r =
            IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--no-ignore-messages", "pat"])));
        assert!(r.msg_flags.contains(MessageFlags::NO_IGNORE_MESSAGES));
    }

    #[test]
    fn message_no_ignore_files() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&["sift", "--no-ignore-files", "pat"])));
        assert!(r.msg_flags.contains(MessageFlags::NO_IGNORE_FILES));
    }

    #[test]
    fn message_toggle_last_wins() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "sift",
            "--no-messages",
            "--messages",
            "pat",
        ])));
        assert!(!r.msg_flags.contains(MessageFlags::NO_MESSAGES));
    }

    #[test]
    fn ignore_messages_toggle_last_wins() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--no-ignore-messages",
            "--ignore-messages",
            "pat",
        ])));
        assert!(!r.msg_flags.contains(MessageFlags::NO_IGNORE_MESSAGES));
    }

    #[test]
    fn ignore_files_toggle_last_wins() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--no-ignore-files",
            "--ignore-files",
            "pat",
        ])));
        assert!(!r.msg_flags.contains(MessageFlags::NO_IGNORE_FILES));
    }

    #[test]
    fn combined_message_flags() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--no-messages",
            "--no-ignore-files",
            "pat",
        ])));
        assert!(r.msg_flags.contains(MessageFlags::NO_MESSAGES));
        assert!(r.msg_flags.contains(MessageFlags::NO_IGNORE_FILES));
    }

    #[test]
    fn unrestricted_then_ignore_restores() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "sift",
            "--unrestricted",
            "--ignore",
            "pat",
        ])));
        assert!(!r.hidden);
        assert!(r.sources.contains(IgnoreSources::DOT));
    }

    #[test]
    fn unrestricted_two_then_no_hidden() {
        let r =
            IgnoreResolution::resolve(&Argv::new(&args(&["sift", "-uu", "--no-hidden", "pat"])));
        assert!(!r.hidden);
        assert!(r.sources.is_empty());
    }

    #[test]
    fn non_flag_args_are_ignored() {
        let r = IgnoreResolution::resolve(&Argv::new(&args(&[
            "--hidden",
            "some_file.rs",
            "--no-hidden",
        ])));
        assert!(!r.hidden);
    }
}
