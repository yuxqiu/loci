//! The `loci` command-line interface (m0 §9) and plugin dispatch (m0 §2.2).

mod assets;
mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loci_core::envelope::Report;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Core command names; any other first argument is dispatched to `loci-<name>`.
pub const CORE_COMMANDS: &[&str] = &[
    "init",
    "lint",
    "done",
    "rules",
    "updates",
    "mv",
    "status",
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
    Done {
        /// Something that was hard in the task (a search that missed, an unclear
        /// rule, work a command could do); repeat for each.
        #[arg(long)]
        note: Vec<String>,
    },
    /// Print the framework rules (read at session start).
    Rules,
    /// List the vault updates waiting, with the procedure for applying them.
    Updates {
        /// Print every update this loci knows.
        #[arg(long)]
        all: bool,
        /// Record that every update through this id is handled, applied or declined.
        #[arg(long, value_name = "ID")]
        handled: Option<String>,
    },
    /// Move or rename a file or directory, rewriting links.
    Mv {
        /// The vault path of a file or folder.
        from: String,
        /// Its new vault path.
        to: String,
    },
    /// Show what has changed: uncommitted changes, agent work to review, the inbox.
    Status,
    /// Create a concept note with a new id; missing folders are created.
    New {
        /// `concepts/<area>/.../<slug>.md` (`.md` may be omitted).
        path: String,
        /// The H1 (default: the slug).
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
        /// A note path (`.md` optional) or a concept slug.
        note: String,
        /// The text of the H2 whose blocks get ids.
        #[arg(long)]
        section: String,
    },
    /// Fold concept FROM into concept INTO after merging their content.
    Merge {
        /// The concept to fold in and delete (path or slug).
        from: String,
        /// The concept that keeps the merged content (path or slug).
        into: String,
    },
    /// List a note's links and the links pointing at it.
    Links {
        /// A note path (`.md` optional), a concept slug, or any vault file.
        note: String,
    },
    /// List a concept's prerequisite closure, prerequisites first.
    Closure {
        /// A concept path or slug.
        concept: String,
        /// Keep only prerequisites at most this many steps away.
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Run an agent on each case, each on a fresh copy of the vault, and check the result.
    Bench {
        /// The agent command, run through `sh -c` with the question on stdin.
        agent: String,
        /// The cases file (default: bench/cases.jsonl in the vault).
        #[arg(long)]
        cases: Option<PathBuf>,
    },
    /// Write one self-contained document from a view or concept.
    Export {
        /// A view or concept: a note path (`.md` optional) or a concept slug.
        target: String,
        /// How many links away notes are included.
        #[arg(long, default_value_t = 1)]
        depth: usize,
        /// The output format: markdown, or a standalone HTML page (print it to PDF from a
        /// browser).
        #[arg(long = "to", value_enum, default_value = "md")]
        format: commands::export::Format,
        /// The output file (default: `.loci/export/<stem>.<ext>`).
        #[arg(short)]
        output: Option<PathBuf>,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum SkillCmd {
    Show {
        /// The skill to print (default: list them all).
        name: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let cwd = std::env::current_dir().expect("the working directory is readable");
    let ctx = commands::Ctx { explicit_vault: cli.vault.clone(), cwd, json: cli.json };
    let (name, report) = match cli.command {
        Cmd::Init => ("init", commands::init::run(&ctx)),
        Cmd::Lint => ("lint", commands::lint::run(&ctx)),
        Cmd::Done { note } => ("done", commands::done::run(&ctx, &note)),
        Cmd::Rules => ("rules", commands::rules::run()),
        Cmd::Updates { all, handled } => ("updates", commands::updates::run(&ctx, all, handled.as_deref())),
        Cmd::Mv { from, to } => ("mv", commands::mv::run(&ctx, &from, &to)),
        Cmd::Status => ("status", commands::status::run(&ctx)),
        Cmd::New { path, title } => ("new", commands::new::run(&ctx, &path, title.as_deref())),
        Cmd::Skill { action: SkillCmd::Show { name } } => ("skill show", commands::skill::run(&ctx, name.as_deref())),
        Cmd::BlockIds { note, section } => ("block-ids", commands::blockids::run(&ctx, &note, &section)),
        Cmd::Merge { from, into } => ("merge", commands::merge::run(&ctx, &from, &into)),
        Cmd::Links { note } => ("links", commands::links::run(&ctx, &note)),
        Cmd::Closure { concept, depth } => ("closure", commands::closure::run(&ctx, &concept, depth)),
        Cmd::Bench { agent, cases } => ("bench", commands::bench::run(&ctx, &agent, cases.as_deref())),
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
