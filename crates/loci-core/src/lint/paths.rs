//! Path rules (m0 §3, §6.2, §6.4, §6.6), shared with `new` and `mv`, and the inbox layout.

use unicode_normalization::is_nfc;

use super::{Code, Ctx};
use crate::envelope::Diagnostic;
use crate::path::{InboxPlace, Place};

pub(super) fn paths(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    let vault = ctx.index.vault();
    for path in vault.files() {
        let place = path.place();
        if let Err(problem) = &place {
            out.push(Diagnostic::error(problem.code(), problem.to_string()).at_path(path));
        }
        if !is_nfc(path.as_str()) {
            // `mv` normalises both arguments and finds the stored name from the NFC one
            // (`mv::stored_name`), so the source is written as the destination.
            let to = crate::path::nfc(path.as_str());
            out.push(
                Diagnostic::error(Code::PathNotNfc, "file name is not in Unicode NFC")
                    .at_path(path)
                    .with_hint(format!("rename it, links following: `loci mv {to} {to}`")),
            );
        }
        // A note `x.md` beside a file `x`: `[[x]]` names the file (m0 §5.3). Root files
        // are named by no path link, so they cannot clash.
        let clashes = |s: &&str| s.contains('/') && vault.resolve(s).is_some_and(|f| f != path);
        if let Some(short) = path.as_str().strip_suffix(".md").filter(clashes) {
            out.push(
                Diagnostic::warning(
                    Code::PathNameClash,
                    format!("`{short}` is also a file, so `[[{short}]]` names it, not this note"),
                )
                .at_path(path)
                .with_hint(format!("link this note as `[[{path}]]`, or rename one of them")),
            );
        }
        // Directly in `inbox/`, or directly in `inbox/journal/` (not in a stream).
        let root_file = match place {
            Ok(Some(Place::Inbox(InboxPlace::Root))) => true,
            Ok(Some(Place::Inbox(InboxPlace::Folder("journal")))) => path.components().count() == 3,
            _ => false,
        };
        if root_file {
            out.push(
                Diagnostic::warning(
                    Code::InboxRootFile,
                    "file directly in `inbox/` or `inbox/journal/`; move it into `drafts/`, `sources/` or a journal stream",
                )
                .at_path(path),
            );
        }
    }
}
