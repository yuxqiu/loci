//! The `loci` command-line interface (m0 §9) and plugin dispatch (m0 §2.2).

mod assets;
mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use loci_core::envelope::{Refusal, Report};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Core command names, from the CLI definition, and `help`; any other first argument
/// is dispatched to `loci-<name>`.
pub fn core_commands() -> Vec<String> {
    let cli = Cli::command();
    cli.get_subcommands().map(|c| c.get_name().to_string()).chain(["help".to_string()]).collect()
}

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
    /// Create a vault in the current directory, or the `--vault` folder.
    Init,
    /// Lint the working tree (core rules and every installed plugin).
    Lint {
        #[command(flatten)]
        fix: commands::lint::FixArgs,
    },
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
        #[arg(long, value_name = "ID", conflicts_with = "all")]
        handled: Option<String>,
    },
    /// Move or rename a file or directory, rewriting links.
    Mv {
        /// The vault path of a file or folder.
        from: String,
        /// Its new vault path.
        to: String,
        /// Report what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
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
        /// Report what would change; write nothing.
        #[arg(long)]
        dry_run: bool,
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
        /// The output file (default: `.loci/export/<note path>/<stem>.<ext>`).
        #[arg(short)]
        output: Option<PathBuf>,
    },
    #[command(external_subcommand)]
    External(Vec<String>),
}

#[derive(Subcommand)]
enum SkillCmd {
    /// Print one skill, or list them all.
    Show {
        /// The skill to print (default: list them all).
        name: Option<String>,
    },
}

fn main() -> ExitCode {
    let matches = Cli::command().get_matches();
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    let name = command_name(&matches);
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            let refusal = Refusal::io(format!("cannot read the working directory: {e}"));
            return Report::from(refusal).print(cli.json, VERSION, &name).into();
        }
    };
    let vault = loci_core::vault::VaultDir::new(cli.vault.as_deref(), &cwd);
    let ctx = commands::Ctx { vault, cwd, json: cli.json };
    let report = match cli.command {
        Cmd::Init => commands::init::run(&ctx),
        Cmd::Lint { fix } => commands::lint::run(&ctx, &fix),
        Cmd::Done { note } => commands::done::run(&ctx, &note),
        Cmd::Rules => commands::rules::run(),
        Cmd::Updates { all, handled } => commands::updates::run(&ctx, all, handled.as_deref()),
        Cmd::Mv { from, to, dry_run } => commands::mv::run(&ctx, &from, &to, dry_run),
        Cmd::Status => commands::status::run(&ctx),
        Cmd::New { path, title } => commands::new::run(&ctx, &path, title.as_deref()),
        Cmd::Skill { action: SkillCmd::Show { name } } => commands::skill::run(&ctx, name.as_deref()),
        Cmd::BlockIds { note, section } => commands::blockids::run(&ctx, &note, &section),
        Cmd::Merge { from, into, dry_run } => commands::merge::run(&ctx, &from, &into, dry_run),
        Cmd::Links { note } => commands::links::run(&ctx, &note),
        Cmd::Closure { concept, depth } => commands::closure::run(&ctx, &concept, depth),
        Cmd::Bench { agent, cases } => commands::bench::run(&ctx, &agent, cases.as_deref()),
        Cmd::Export { target, depth, format, output } => commands::export::run(&ctx, &target, depth, format, output),
        Cmd::External(args) => return commands::dispatch::run(&ctx, args),
    };

    report.unwrap_or_else(Report::from).print(ctx.json, VERSION, &name).into()
}

/// The command's name as typed, for the envelope: its subcommands joined (`skill show`).
fn command_name(matches: &clap::ArgMatches) -> String {
    let mut names = vec![];
    let mut m = matches;
    while let Some((name, sub)) = m.subcommand() {
        names.push(name);
        m = sub;
    }
    names.join(" ")
}
