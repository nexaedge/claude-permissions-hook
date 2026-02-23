pub mod config;
pub mod decision;
pub mod domain;
pub mod error;
pub mod protocol;

pub(crate) mod cli;
pub(crate) mod shell_parser;

/// Run the hook subcommand: read JSON from stdin, evaluate, write JSON to stdout.
///
/// This is the binary entry point. It exists to bridge the binary crate (`main.rs`)
/// to the library without exposing `cli` internals. For programmatic use,
/// call [`decision::evaluate`] with a [`domain::ToolRequest`]
/// (built from [`protocol::HookInput::to_request`]) and a [`config::Config`].
/// It returns `Option<(Decision, String)>` — `None` means no opinion.
/// Use [`protocol::HookOutput`] to convert the result to wire format.
pub fn run_hook(config_path: Option<&std::path::Path>) {
    cli::hook::run(config_path)
}
