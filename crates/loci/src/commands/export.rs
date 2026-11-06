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
    let note = index.note_named(target)?.of_kind(|k| matches!(k, Kind::View | Kind::Concept), "a view or a concept")?;

    // Images go in `<stem>.assets/` beside the document, so exports written to one
    // folder keep their own.
    let output = output.map(|o| ctx.cwd.join(o));
    let stem = match &output {
        None => note.path.stem(),
        Some(o) => o
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| Refusal::io(format!("`{}` names no file with a UTF-8 name", o.display())))?,
    };
    let assets_folder = format!("{stem}.assets");
    let doc = export::Export::of(&index, note, depth, &assets_folder);
    let text = match format {
        Format::Md => doc.markdown.clone(),
        Format::Html => html(&doc, &note.title()).map_err(|m| Refusal::internal("export.html", m))?,
    };
    let out_path = write_out(&vault, &doc, &assets_folder, &note.path, format, output, text)?;
    let data = json!({
        "output": out_path,
        "notes": doc.notes,
        "assets": doc.assets.iter().map(|(_, n)| format!("{assets_folder}/{n}")).collect::<Vec<_>>(),
    });

    Ok(Report::ok(data, format!("{}\n", out_path.display())))
}

/// Writes the export, and its images into folder `assets_folder` beside it: to `output` when
/// given (the user's path, written as given, not as a vault file), else into
/// `.loci/export/<note path>/` (in the vault, gitignored, so written as vault files
/// are; a folder named by the note's whole path, which no other note's folder can be
/// inside, so no export overwrites another's images). Returns where the document went.
fn write_out(
    vault: &Vault,
    doc: &export::Export,
    assets_folder: &str,
    note_path: &VaultPath,
    format: Format,
    output: Option<PathBuf>,
    text: String,
) -> Result<PathBuf, Refusal> {
    let read = |src: &VaultPath| vault.read(src)?.ok_or_else(|| Refusal::io(format!("{src}: not found")));
    match output {
        None => {
            let in_export =
                |name: &str| VaultPath::new(format!(".loci/export/{note_path}/{name}")).expect("a file name");
            for (src, name) in &doc.assets {
                vault.write(&in_export(&format!("{assets_folder}/{name}")), &read(src)?)?;
            }
            let out = in_export(&format!("{}.{}", note_path.stem(), format.ext()));
            vault.write(&out, text.as_bytes())?;
            Ok(vault.abs(&out))
        }
        Some(out_path) => {
            let out_dir = out_path.parent().expect("an absolute path with a file name has a folder");
            let assets_dir = out_dir.join(assets_folder);
            let dir = if doc.assets.is_empty() { out_dir } else { &assets_dir };
            std::fs::create_dir_all(dir).map_err(Refusal::io_at(dir))?;
            for (src, name) in &doc.assets {
                let dest = assets_dir.join(name);
                std::fs::write(&dest, read(src)?).map_err(Refusal::io_at(&dest))?;
            }
            std::fs::write(&out_path, text).map_err(Refusal::io_at(&out_path))?;
            Ok(out_path)
        }
    }
}

/// A standalone HTML page for the export, which needs nothing from the network: the
/// markdown rendered by `pulldown-cmark`, each heading given the identifier its
/// `#anchor` links use, and math rendered to MathML by `pulldown-latex`, which
/// browsers display natively (TeX it cannot render is shown, in red, not dropped).
fn html(e: &export::Export, title: &str) -> Result<String, String> {
    use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html::push_html};
    // Notes are read with loci's options; the export has no wikilinks (they are
    // rewritten), so the same top-level headings are read in the same order as the
    // identifiers were made. Only those have identifiers (m0 §4.6): a heading in a block
    // quote or an item has none.
    let options = loci_core::parse::options() - Options::ENABLE_WIKILINKS;
    let mut ids = e.heading_ids.iter();
    let mut events = vec![];
    let mut depth = 0usize;
    for event in Parser::new_ext(&e.markdown, options) {
        let top_level = depth == 0;
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
        events.push(match event {
            Event::Start(Tag::Heading { level, classes, attrs, .. }) if top_level => {
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
