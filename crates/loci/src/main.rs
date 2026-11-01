//! The `loci` command-line interface (m0 §9) and plugin dispatch (m0 §2.2).

mod assets;
mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loci_core::envelope::Report;
use loci_core::vault::Kind;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Core command names; any other first argument is dispatched to `loci-<name>`.
pub const CORE_COMMANDS: &[&str] = &[
    "init",
    "lint",
    "done",
    "find",
    "digest",
    "mv",
    "status",
    "fetch",
    "new",
    "skill",
    "block-ids",
    "merge",
    "closure",
    "links",
    "bench",
    "export",
    "help",
];

#[derive(Parser)]
#[command(name = "loci", version, about = "An agent-maintained knowledge base")]
#[command(allow_external_subcommands = true)]
struct Cli {
    /// Vault root (default: nearest ancestor containing loci.toml).
    #[arg(long, global = true)]
    vault: Option<PathBuf>,
    /// Print one JSON envelope on stdout.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a vault in the current directory.
    Init,
    /// Lint the working tree (core rules and every installed plugin).
    Lint,
    /// End an agent task: lint, then record the agent's work for review.
    Done,
    /// Full-text search.
    Find {
        query: String,
        #[arg(short, default_value_t = 10)]
        k: usize,
        #[arg(long = "kind", value_enum)]
        kinds: Vec<commands::find::KindArg>,
    },
    /// Print the root index, or one area's index.
    Digest {
        #[arg(long)]
        area: Option<String>,
    },
    /// Move or rename a file or directory, rewriting links.
    Mv { from: String, to: String },
    /// Show install state, plugins, changes, inbox, index and rules.
    Status,
    /// File a source from a URL or an inbox path into sources/<type>/<slug>.
    Fetch { from: String, dest: String },
    /// Create a concept note.
    New {
        path: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// List skills, or print one.
    Skill {
        #[command(subcommand)]
        action: SkillCmd,
    },
    /// Assign ULID block ids to the top-level blocks of a section.
    #[command(name = "block-ids")]
    BlockIds {
        note: String,
        #[arg(long)]
        section: String,
    },
    /// Fold concept FROM into concept INTO after merging their content.
    Merge { from: String, into: String },
    /// List a note's links and the links pointing at it.
    Links { note: String },
    /// List a concept's prerequisite closure, prerequisites first.
    Closure {
        concept: String,
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Run the retrieval benchmark in bench/retrieval.jsonl.
    Bench {
        /// Also run this agent command (via `sh -c`, question on stdin) per case.
        #[arg(long)]
        agent: Option<String>,
        #[arg(short, default_value_t = 10)]
        k: usize,
    },
    /// Write one self-contained document from a view or concept.
    Export {
        target: String,
        #[arg(long, default_value_t = 1)]
        depth: usize,
        #[arg(long = "to", value_enum, default_value = "md")]
        format: commands::export::Format,
        #[arg(short)]
        output: Option<PathBuf>,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum SkillCmd {
    Show { name: Option<String> },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().expect("the working directory is readable");
    let ctx = commands::Ctx { explicit_vault: cli.vault.clone(), cwd, json: cli.json };
    let (name, report) = match cli.command {
        Cmd::Init => ("init", commands::init::run(&ctx)),
        Cmd::Lint => ("lint", commands::lint::run(&ctx)),
        Cmd::Done => ("done", commands::done::run(&ctx)),
        Cmd::Find { query, k, kinds } => {
            ("find", commands::find::run(&ctx, &query, k, &kinds.into_iter().map(Kind::from).collect::<Vec<_>>()))
        }
        Cmd::Digest { area } => ("digest", commands::digest::run(&ctx, area.as_deref())),
        Cmd::Mv { from, to } => ("mv", commands::mv::run(&ctx, &from, &to)),
        Cmd::Status => ("status", commands::status::run(&ctx)),
        Cmd::Fetch { from, dest } => ("fetch", commands::fetch::run(&ctx, &from, &dest)),
        Cmd::New { path, title } => ("new", commands::new::run(&ctx, &path, title.as_deref())),
        Cmd::Skill { action: SkillCmd::Show { name } } => ("skill show", commands::skill::run(&ctx, name.as_deref())),
        Cmd::BlockIds { note, section } => ("block-ids", commands::blockids::run(&ctx, &note, &section)),
        Cmd::Merge { from, into } => ("merge", commands::merge::run(&ctx, &from, &into)),
        Cmd::Links { note } => ("links", commands::links::run(&ctx, &note)),
        Cmd::Closure { concept, depth } => ("closure", commands::closure::run(&ctx, &concept, depth)),
        Cmd::Bench { agent, k } => ("bench", commands::bench::run(&ctx, agent.as_deref(), k)),
        Cmd::Export { target, depth, format, output } => {
            ("export", commands::export::run(&ctx, &target, depth, format, output))
        }
        Cmd::External(args) => return commands::dispatch::run(&ctx, args),
    };
    report.unwrap_or_else(Report::from).print(ctx.json, VERSION, name).into()
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn core_commands_match_the_cli() {
        let mut cli: Vec<String> = Cli::command().get_subcommands().map(|c| c.get_name().to_string()).collect();
        cli.push("help".to_string());
        cli.sort();
        let mut listed: Vec<String> = CORE_COMMANDS.iter().map(|c| c.to_string()).collect();
        listed.sort();
        assert_eq!(listed, cli, "CORE_COMMANDS must list every subcommand, so plugins cannot shadow one");
    }
}
