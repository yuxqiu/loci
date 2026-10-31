//! `loci export` (m1-m4 §4).

use std::path::PathBuf;
use std::process::Command;

use loci_core::envelope::{Outcome, Report};
use loci_core::export;
use loci_core::index::Index;
use loci_core::vault::{self, Kind};
use serde_json::json;

use super::{Ctx, vault_path};

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

pub fn run(ctx: &Ctx, target: &str, depth: usize, format: Format, output: Option<PathBuf>) -> Result<Report, Report> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let target = vault_path(target)?;
    let path = if index.notes.contains_key(&target) {
        target.clone()
    } else if index.notes.contains_key(&format!("{target}.md")) {
        format!("{target}.md")
    } else {
        match index.concept(&target) {
            Ok(n) => n.path.clone(),
            Err(e) => {
                let (code, message) = e.refusal("export.unknown", &target);
                return Err(Report::fail(Outcome::refused(code, message)));
            }
        }
    };
    if !matches!(vault::kind_of(&path), Some(Kind::View | Kind::Concept)) {
        return Err(Report::fail(Outcome::refused("export.kind", format!("`{path}` is not a view or a concept"))));
    }
    let e = export::export(&index, &path, depth);
    // Absolute, because pandoc runs in the output directory.
    let out_path = match output {
        Some(o) => ctx.cwd.join(o),
        None => vault.abs(&format!(".loci/export/{}.{}", vault::stem(&path), format.ext())),
    };
    let out_dir = out_path.parent().expect("an absolute file path has a parent").to_path_buf();
    let result = (|| -> Result<(), Outcome> {
        let io = |e: std::io::Error| Outcome::internal("io", e.to_string());
        std::fs::create_dir_all(out_dir.join("assets")).map_err(io)?;
        for (src, name) in &e.assets {
            std::fs::copy(vault.abs(src), out_dir.join("assets").join(name)).map_err(io)?;
        }
        if format == Format::Md {
            return std::fs::write(&out_path, &e.markdown).map_err(io);
        }
        let md_path = out_path.with_extension("md.tmp");
        std::fs::write(&md_path, &e.markdown).map_err(io)?;
        let mut cmd = Command::new("pandoc");
        cmd.arg(&md_path).args(["-f", "markdown", "-o"]).arg(&out_path).current_dir(&out_dir);
        if format == Format::Html {
            cmd.args(["--standalone", "--mathjax"]);
        }
        let status = cmd.status();
        let _ = std::fs::remove_file(&md_path);
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(Outcome::internal("export.pandoc", format!("pandoc exited with {s}"))),
            Err(e) => Err(Outcome::refused("export.pandoc", format!("cannot run pandoc: {e}"))),
        }
    })();
    if let Err(o) = result {
        return Err(Report::fail(o));
    }
    let data = json!({
        "output": out_path,
        "notes": e.notes,
        "glossary": e.glossary,
        "assets": e.assets.iter().map(|(_, n)| format!("assets/{n}")).collect::<Vec<_>>(),
    });
    Ok(Report::new(Outcome::ok(data), format!("{}\n", out_path.display())))
}
