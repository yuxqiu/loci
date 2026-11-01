//! `loci fetch` (m0 §9.7).

use std::io::Read;

use loci_core::envelope::{Outcome, Refusal, Report};
use loci_core::sources::{SourceMeta, hex_sha256};
use loci_core::vault::{is_slug, nfc, write_atomic};
use serde_json::json;

use super::{Ctx, vault_path};

pub fn run(ctx: &Ctx, from: &str, dest: &str) -> Result<Report, Refusal> {
    let vault = ctx.vault()?;
    let dest = vault_path(dest)?;
    let Some((ty, slug)) = dest.split_once('/') else {
        return Err(Refusal::refused("fetch.dest", "the destination is `<type>/<slug>`"));
    };
    if !is_slug(ty) || !is_slug(slug) {
        return Err(Refusal::refused(
            "fetch.dest",
            format!("`{dest}` must be <type>/<slug>, both slugs (for example `papers/dijkstra-1959`)"),
        ));
    }
    let folder_rel = format!("sources/{ty}/{slug}");
    let folder = vault.abs(&folder_rel);
    if folder.exists() {
        return Err(Refusal::refused("fetch.dest", format!("`{folder_rel}` already exists")));
    }
    let is_url = from.starts_with("http://") || from.starts_with("https://");
    // The inbox file to move, when `from` is not a URL.
    let mut inbox_file = None;
    let (file_name, bytes, url) = if is_url {
        (file_name_from_url(from), download(from)?, Some(from.to_string()))
    } else {
        let rel = vault_path(from)?;
        if !rel.starts_with("inbox/") {
            return Err(Refusal::refused("fetch.from", format!("`{rel}` is neither a URL nor under inbox/")));
        }
        match std::fs::read(vault.abs(&rel)) {
            Ok(b) => {
                let name = rel.rsplit('/').next().expect("rsplit yields at least one part").to_string();
                inbox_file = Some(vault.abs(&rel));
                (name, b, None)
            }
            Err(e) => return Err(Refusal::refused("fetch.from", format!("cannot read `{rel}`: {e}"))),
        }
    };
    if matches!(file_name.as_str(), "source.yaml" | "." | "..") {
        return Err(Refusal::refused("fetch.from", format!("a source file cannot be named `{file_name}`")));
    }
    let meta = SourceMeta { sha256: hex_sha256(&bytes), url: url.clone() };
    let result = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&folder)?;
        match &inbox_file {
            Some(src) => std::fs::rename(src, folder.join(&file_name))?,
            None => std::fs::write(folder.join(&file_name), &bytes)?,
        }
        write_atomic(&folder.join("source.yaml"), meta.render().as_bytes())
    })();
    if let Err(e) = result {
        // Undo: put an inbox file back, then remove the new folder. If the file cannot
        // be put back, the folder is kept so that nothing is lost.
        let undo = (|| -> std::io::Result<()> {
            let moved = folder.join(&file_name);
            if let Some(src) = &inbox_file
                && moved.exists()
            {
                std::fs::rename(&moved, src)?;
            }
            std::fs::remove_dir_all(&folder)
        })();
        let message = match undo {
            Ok(()) => e.to_string(),
            Err(u) => format!("{e}; undoing it also failed, see `{folder_rel}`: {u}"),
        };
        return Err(Refusal::internal("io", message));
    }
    let data = json!({"path": folder_rel, "file": file_name, "sha256": meta.sha256, "url": url});
    Ok(Report::new(Outcome::ok(data), format!("{folder_rel}/{file_name}\n")))
}

fn download(url: &str) -> Result<Vec<u8>, Refusal> {
    let network = |e: &dyn std::fmt::Display| Refusal::internal("fetch.network", e.to_string());
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
    let mut resp = agent.get(url).call().map_err(|e| network(&e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Refusal::refused("fetch.http", format!("{url} returned HTTP {status}")));
    }
    let mut bytes = vec![];
    resp.body_mut().as_reader().read_to_end(&mut bytes).map_err(|e| network(&e))?;
    Ok(bytes)
}

/// The last path segment of a URL: percent-decoded, NFC, `/` and control characters
/// replaced by `-`; `index.html` if empty (m0 §9.7).
fn file_name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let after_host = path.splitn(4, '/').nth(3).unwrap_or("");
    let last = after_host.rsplit('/').next().unwrap_or("");
    let decoded = percent_decode(last);
    let name: String = nfc(&decoded).chars().map(|c| if c == '/' || c.is_control() { '-' } else { c }).collect();
    if matches!(name.as_str(), "" | "." | "..") { "index.html".to_string() } else { name }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(b) = u8::from_str_radix(hex, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(file_name_from_url("https://ex.org/notes/heaps.pdf"), "heaps.pdf");
        assert_eq!(file_name_from_url("https://ex.org/"), "index.html");
        assert_eq!(file_name_from_url("https://ex.org"), "index.html");
        assert_eq!(file_name_from_url("https://ex.org/a%20b%2Fc.pdf?x=1"), "a b-c.pdf");
        assert_eq!(file_name_from_url("https://ex.org/%E4%BA%8C.md"), "二.md");
        assert_eq!(file_name_from_url("https://ex.org/a/%2E%2E"), "index.html");
        assert_eq!(file_name_from_url("https://ex.org/."), "index.html");
    }
}
