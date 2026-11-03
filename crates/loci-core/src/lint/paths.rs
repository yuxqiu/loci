//! Path rules (m0 §3, §6.2, §6.4, §6.6), shared with `new` and `mv`, and the inbox layout.

use unicode_normalization::is_nfc;

use super::{Code, Ctx};
use crate::envelope::Diagnostic;
use crate::path::InboxPlace;

pub(super) fn paths(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for path in ctx.index.files() {
        if let Some(problem) = path.problem() {
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
        // Directly in `inbox/`, or directly in `inbox/journal/` (not in a stream).
        let root_file = match path.inbox_place() {
            Some(InboxPlace::Root) => true,
            Some(InboxPlace::Folder("journal")) => path.components().count() == 3,
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
