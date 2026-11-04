//! `loci export` (m1-m4 §4).

use std::path::PathBuf;

use loci_core::envelope::{Refusal, Report};
use loci_core::export;
use loci_core::index::Index;
use loci_core::path::Kind;
use loci_core::path::VaultPath;
use loci_core::vault::Vault;
use serde_json::json;

use super::Ctx;

#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Md,
    Html,
}

impl Format {
    fn ext(self) -> &'static str {
        match self {
            Format::Md => "md",
            Format::Html => "html",
        }
    }
}

pub fn run(ctx: &Ctx, target: &str, depth: usize, format: Format, output: Option<PathBuf>) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let index = Index::open(&vault);
    let note = index.note_named(target)?;
    let path = note.path.clone();
    if !matches!(note.kind, Kind::View | Kind::Concept) {
        return Err(Refusal::refused("note.kind", format!("`{path}` is not a view or a concept")));
    }

    let e = export::Export::of(&index, note, depth);
    let text = match format {
        Format::Md => e.markdown.clone(),
        Format::Html => html(&e, note.title()).map_err(|m| Refusal::internal("export.html", m))?,
    };
    let out_path = write_out(&vault, &e, &path, format, output.map(|o| ctx.cwd.join(o)), text)?;
    let data = json!({
        "output": out_path,
        "notes": e.notes,
        "assets": e.assets.iter().map(|(_, n)| format!("{}/{n}", export::ASSETS)).collect::<Vec<_>>(),
    });

    Ok(Report::ok(data, format!("{}\n", out_path.display())))
}

/// Writes the export and its images: to `output` when given (the user's path, outside
/// the vault), else into `.loci/export/` (in the vault, gitignored, so written as vault
/// files are). Returns where the document went.
fn write_out(
    vault: &Vault,
    e: &export::Export,
    note: &VaultPath,
    format: Format,
    output: Option<PathBuf>,
    text: String,
) -> Result<PathBuf, Refusal> {
    let read = |src: &VaultPath| std::fs::read(vault.abs(src)).map_err(Refusal::io);
    let Some(out_path) = output else {
        let in_export = |name: &str| VaultPath::new(format!(".loci/export/{name}")).expect("a file name");
        for (src, name) in &e.assets {
            vault.write(&in_export(&format!("{}/{name}", export::ASSETS)), &read(src)?)?;
        }
        let out = in_export(&format!("{}.{}", note.stem(), format.ext()));
        vault.write(&out, text.as_bytes())?;
        return Ok(vault.abs(&out));
    };
    let Some(out_dir) = out_path.parent() else {
        return Err(Refusal::io(format!("`{}` names no file", out_path.display())));
    };
    let assets_dir = out_dir.join(export::ASSETS);
    std::fs::create_dir_all(if e.assets.is_empty() { out_dir } else { &assets_dir }).map_err(Refusal::io)?;
    for (src, name) in &e.assets {
        std::fs::write(assets_dir.join(name), read(src)?).map_err(Refusal::io)?;
    }
    std::fs::write(&out_path, text).map_err(Refusal::io)?;
    Ok(out_path)
}

/// A standalone HTML page for the export, which needs nothing from the network: the
/// markdown rendered by `pulldown-cmark`, each heading given the identifier its
/// `#anchor` links use, and math rendered to MathML by `pulldown-latex`, which
/// browsers display natively (TeX it cannot render is shown, in red, not dropped).
fn html(e: &export::Export, title: &str) -> Result<String, String> {
    use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html::push_html};
    // Notes are read with loci's options; the export has no wikilinks (they are
    // rewritten), so the same headings are read in the same order as the identifiers
    // were made.
    let options = loci_core::parse::options() - Options::ENABLE_WIKILINKS;
    let mut ids = e.heading_ids.iter();
    let mut events = vec![];
    for event in Parser::new_ext(&e.markdown, options) {
        events.push(match event {
            Event::Start(Tag::Heading { level, classes, attrs, .. }) => {
                let id = ids.next().ok_or("more headings than identifiers")?;
                Event::Start(Tag::Heading { level, id: Some(CowStr::from(id.clone())), classes, attrs })
            }
            Event::InlineMath(m) => Event::Html(mathml(&m, false)?.into()),
            Event::DisplayMath(m) => Event::Html(mathml(&m, true)?.into()),
            other => other,
        });
    }

    if ids.next().is_some() {
        return Err("fewer headings than identifiers".to_string());
    }

    let mut body = String::new();
    push_html(&mut body, events.into_iter());
    Ok(format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n</head>\n\
         <body>\n{body}</body>\n</html>\n",
        escape(title)
    ))
}

/// TeX `tex` as a MathML `<math>` element, its source kept as an annotation.
fn mathml(tex: &str, display: bool) -> Result<String, String> {
    use pulldown_latex::config::DisplayMode;
    use pulldown_latex::{Parser, RenderConfig, Storage, push_mathml};
    let storage = Storage::new();
    let mut config = RenderConfig::with_annotation(tex);
    config.display_mode = if display { DisplayMode::Block } else { DisplayMode::Inline };
    let mut out = String::new();
    push_mathml(&mut out, Parser::new(tex, &storage), config).map_err(|e| format!("rendering math: {e}"))?;
    Ok(out)
}

/// `text` with the characters HTML gives meaning escaped.
fn escape(text: &str) -> String {
    let mut out = String::new();
    pulldown_cmark_escape::escape_html(&mut out, text).expect("writing to a String does not fail");
    out
}
