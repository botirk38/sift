pub mod cli;
pub mod config;
pub mod format;
pub mod grep;
pub mod index;
pub mod update;

// Stable paths for benchmarks and integration tests that import submodules directly.
pub use grep::run::{Run, RunConfig, RunResult};
pub use grep::{Argv, engine, filter, ignore, output, paths, pattern};

use std::process::ExitCode;

use clap::Parser;
use cli::Cli;
use config::ConfigArgs;

#[must_use]
pub fn main_entry() -> ExitCode {
    let raw_args = Argv::from_env();

    if let Ok(cli) = Cli::try_parse_from(&raw_args)
        && cli.command.is_some()
    {
        let argv = Argv::new(&raw_args);
        return cli.dispatch(&argv);
    }

    let config_args = match ConfigArgs::from_env(&raw_args) {
        Ok(args) => args,
        Err(err) => {
            eprintln!("sift: {err}");
            return ExitCode::from(1);
        }
    };
    let argv_storage = config_args.apply(&raw_args);
    let cli = Cli::parse_from(&argv_storage);
    let argv = Argv::new(&argv_storage);
    cli.dispatch(&argv)
}
