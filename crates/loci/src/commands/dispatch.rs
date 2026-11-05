//! Plugin dispatch (m0 §2.2): `loci <name> args...` runs `loci-<name> args...`.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitCode;

use loci_core::envelope::{Refusal, Report};

use super::{Ctx, plugins};
use crate::{VERSION, core_commands};

pub fn run(ctx: &Ctx, args: Vec<String>) -> ExitCode {
    let (name, rest) = args.split_first().expect("clap gives an external subcommand a name");
    let installed = plugins();
    let Some(p) = installed.iter().find(|p| &p.name == name) else {
        let names: Vec<&str> = installed.iter().map(|p| p.name.as_str()).collect();
        let msg = format!(
            "unknown command `{name}`; core commands: {}; installed plugins: {}",
            core_commands().join(", "),
            if names.is_empty() { "none".to_string() } else { names.join(", ") }
        );
        return fail(ctx, name, Refusal::refused("cmd.unknown", msg));
    };

    let vault = match ctx.vault() {
        Ok(v) => v,
        Err(r) => return fail(ctx, name, r),
    };

    let mut cmd = p.command(vault.root(), VERSION);
    cmd.args(rest);
    if ctx.json {
        cmd.arg("--json");
    }

    match cmd.status() {
        Ok(status) => match status.code() {
            Some(c) => ExitCode::from(c as u8),
            None => {
                let signal = status.signal().map_or("a signal".to_string(), |s| format!("signal {s}"));
                fail(
                    ctx,
                    name,
                    Refusal::internal("plugin.failed", format!("{} was killed by {signal}", p.path.display())),
                )
            }
        },
        Err(e) => fail(ctx, name, Refusal::internal("plugin.failed", format!("cannot run {}: {e}", p.path.display()))),
    }
}

fn fail(ctx: &Ctx, name: &str, refusal: Refusal) -> ExitCode {
    Report::from(refusal).print(ctx.json, VERSION, name).into()
}
