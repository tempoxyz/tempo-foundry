//! Bridge module that adds `--mcp` and `--llms` support to Foundry CLIs via incur.
//!
//! Call [`intercept`] before `clap::Parser::parse()` to handle these flags.
//! If neither flag is present, this is a no-op.

/// Check for `--mcp` or `--llms` in argv and handle them if present.
///
/// This delegates to [`incur::from_clap::intercept`] which:
/// - `--mcp`: Starts an MCP stdio server exposing all subcommands as tools.
///   Each tool spawns the current binary with the appropriate subcommand,
///   capturing stdout/stderr to avoid protocol corruption.
/// - `--llms`: Prints an LLM-readable Markdown manifest of all commands.
///
/// If neither flag is found, returns immediately so normal clap parsing proceeds.
pub fn intercept<C: clap::CommandFactory>() {
    incur::from_clap::intercept::<C>();
}
