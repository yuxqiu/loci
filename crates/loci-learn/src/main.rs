//! `loci-learn`: the learn plugin (docs/specs/learn-plugin.md). Run as `loci learn`.

mod cards;
mod commands;
mod config;
mod reviewlog;
mod schedule;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loci_core::envelope::{Refusal, Report};
use loci_core::plugin;
use loci_core::vault::Vault;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = "learn";

#[derive(Parser)]
#[command(name = "loci-learn", version, about = "Cards, review and learning estimates for loci")]
struct Cli {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check every concept's `## Cards` section.
    Lint,
    /// Print the plugin's skills.
    Skills,
    /// Append one graded recall (a JSON object on stdin) to the review log.
    Log,
    /// Show a concept's estimate and the lines behind it.
    State { concept: String },
    /// Review due and new cards in the terminal.
    Review,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let command = match &cli.command {
        Cmd::Lint => "learn lint",
        Cmd::Skills => "learn skills",
        Cmd::Log => "learn log",
        Cmd::State { .. } => "learn state",
        Cmd::Review => "learn review",
    };
    run(cli.command).unwrap_or_else(Report::from).print(cli.json, VERSION, command).into()
}

fn run(cmd: Cmd) -> Result<Report, Refusal> {
    let root =
        plugin::check_env(std::env::var_os(plugin::ENV_VERSION), std::env::var_os(plugin::ENV_VAULT), NAME, VERSION)?;
    let vault = Vault::open(Some(&root), &root)?;
    match cmd {
        Cmd::Lint => commands::lint(&vault),
        Cmd::Skills => commands::skills(),
        Cmd::Log => commands::log(&vault, std::io::stdin().lock()),
        Cmd::State { concept } => commands::state(&vault, &concept),
        Cmd::Review => commands::review(&vault),
    }
}
