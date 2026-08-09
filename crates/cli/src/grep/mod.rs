//! Search (`sift PATTERN`) and related flag handling.

pub mod argv;
pub mod engine;
pub mod filter;
pub mod ignore;
pub mod input;
pub mod output;
pub mod paths;
pub mod pattern;
pub mod run;

pub use argv::Argv;
pub use run::{Run, RunConfig, RunResult};
