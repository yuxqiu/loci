//! `loci-learn`: the learn plugin (docs/specs/learn-plugin.md). Run as `loci learn`.

mod cards;
mod commands;
mod config;
mod reviewlog;
mod schedule;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loci_core::envelope::{Outcome, Report};
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
    let exit = run(cli.command).print(cli.json, VERSION, command);
    ExitCode::from(exit as u8)
}

fn run(cmd: Cmd) -> Report {
    let env = plugin::check_env(
        std::env::var(plugin::ENV_VERSION).ok(),
        std::env::var(plugin::ENV_VAULT).ok(),
        NAME,
        VERSION,
    );
    let env = match env {
        Ok(e) => e,
        Err((code, msg)) => return Report::fail(Outcome::refused(code, msg)),
    };
    let vault = match Vault::open(Some(&env.vault), &env.vault) {
        Ok(v) => v,
        Err(e) => return Report::fail(Outcome::refused(e.code(), e.message())),
    };
    match cmd {
        Cmd::Lint => commands::lint(&vault),
        Cmd::Skills => commands::skills(),
        Cmd::Log => commands::log(&vault, std::io::stdin().lock()),
        Cmd::State { concept } => commands::state(&vault, &concept),
        Cmd::Review => commands::review(&vault),
    }
}
