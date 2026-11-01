//! `loci export` (m1-m4 §4).

use std::path::PathBuf;
use std::process::Command;

use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::export;
use loci_core::index::Index;
use loci_core::vault::{self, Kind};
use serde_json::json;

use super::Ctx;

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Md,
    Html,
    Pdf,
}

impl Format {
    fn ext(self) -> &'static str {
        match self {
            Format::Md => "md",
            Format::Html => "html",
            Format::Pdf => "pdf",
        }
    }
}

pub fn run(ctx: &Ctx, target: &str, depth: usize, format: Format, output: Option<PathBuf>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let target = vault::path_arg(target)?;
    let path = index.note(&target).map_err(|e| e.refusal("export.unknown", &target))?.path.clone();
    if !matches!(vault::kind_of(&path), Some(Kind::View | Kind::Concept)) {
        return Err(Refusal::refused("export.kind", format!("`{path}` is not a view or a concept")));
    }
    let e = export::export(&index, &path, depth);
    // Absolute, because pandoc runs in the output directory.
    let out_path = match output {
        Some(o) => ctx.cwd.join(o),
        None => vault.abs(&format!(".loci/export/{}.{}", vault::stem(&path), format.ext())),
    };
    let out_dir = out_path.parent().expect("an absolute file path has a parent").to_path_buf();
    std::fs::create_dir_all(out_dir.join("assets")).map_err(Refusal::io)?;
    for (src, name) in &e.assets {
        std::fs::copy(vault.abs(src), out_dir.join("assets").join(name)).map_err(Refusal::io)?;
    }
    if format == Format::Md {
        std::fs::write(&out_path, &e.markdown).map_err(Refusal::io)?;
    } else {
        let md_path = out_path.with_extension("md.tmp");
        std::fs::write(&md_path, &e.markdown).map_err(Refusal::io)?;
        let mut cmd = Command::new("pandoc");
        cmd.arg(&md_path).args(["-f", "markdown", "-o"]).arg(&out_path).current_dir(&out_dir);
        if format == Format::Html {
            cmd.args(["--standalone", "--mathjax"]);
        }
        let status = cmd.status();
        let removed = std::fs::remove_file(&md_path);
        // A pandoc failure is reported before a failure to remove the temporary file.
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => return Err(Refusal::internal("export.pandoc", format!("pandoc exited with {s}"))),
            Err(e) => return Err(Refusal::refused("export.pandoc", format!("cannot run pandoc: {e}"))),
        }
        removed.map_err(Refusal::io)?;
    }
    let data = json!({
        "output": out_path,
        "notes": e.notes,
        "glossary": e.glossary,
        "assets": e.assets.iter().map(|(_, n)| format!("assets/{n}")).collect::<Vec<_>>(),
    });
    Ok(Report::new(Outcome::ok(data), format!("{}\n", out_path.display())))
}
