//! Status subcommand binary facade (CLO-597).
//!
//! Re-exports the library's `gcm::status` surface and hosts the CLI-specific
//! `run_status_subcommand` entry point, which passes the binary-side
//! `SCHEMA_VERSION` and `crate::cli::VERSION` into the library's pure
//! `build_report` function.

pub use gcm::status::*;

use crate::cli::Cli;
use crate::config;
use crate::output::SCHEMA_VERSION;

/// Entry point for the `status` subcommand. Pure introspection: loads the config
/// and reads the environment, builds the report, prints it (JSON or human), and
/// returns exit code 0 (misconfiguration is reported as fields, not a failure).
/// A non-zero exit is reserved for a catastrophic internal error - per AC-9, a
/// JSON serialization failure (infallible for these owned types in practice) is
/// the one such case. Dispatched at the top of `run()` before any repo/provider/
/// LLM work.
pub fn run_status_subcommand(args: &Cli) -> i32 {
    let config = config::load();
    let report = build_report(
        args.provider,
        args.model.as_deref(),
        config.as_ref(),
        |var| std::env::var(var).ok(),
        SCHEMA_VERSION,
        crate::cli::VERSION,
    );

    if args.json {
        match serde_json::to_string(&report) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                // AC-9: catastrophic internal error -> stderr + non-zero exit.
                eprintln!("gcm: error: could not serialize status report: {e}");
                return 1;
            }
        }
    } else {
        print_human(&report);
    }
    0
}
