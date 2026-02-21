pub mod config;
pub mod decision;
pub mod protocol;

pub(crate) mod cli;
pub(crate) mod command;
pub(crate) mod domain;
pub(crate) mod path;

/// Run the hook subcommand: read JSON from stdin, evaluate, write JSON to stdout.
///
/// This is the binary entry point. It exists to bridge the binary crate (`main.rs`)
/// to the library without exposing `cli` internals. Not a stable integration API —
/// callers should use [`decision::evaluate`] and [`config::Config`] directly.
pub fn run_hook(config_path: Option<&std::path::Path>) {
    cli::hook::run(config_path)
}
