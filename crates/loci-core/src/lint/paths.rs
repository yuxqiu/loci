//! Path rules (m0 §3, §6.2, §6.4, §6.6), shared with `new` and `mv`, and the inbox layout.

use unicode_normalization::is_nfc;

use super::Ctx;
use crate::envelope::{Applicability, Diagnostic, Fix};
use crate::vault::Kind;

/// Path rules (§3, §6.2, §6.4, §6.6), shared with `new` and `mv`, and the inbox layout.
pub(super) fn paths(ctx: &Ctx, out: &mut Vec<Diagnostic>) {
    for path in &ctx.index.files {
        if let Some((code, message)) = path.problem() {
            out.push(Diagnostic::error(code, message).at_path(path.as_str()));
        }
        if !is_nfc(path.as_str()) {
            let to = crate::vault::nfc(path.as_str());
            let fix = Fix::new(Applicability::Safe, format!("rename to `{to}`")).rename(path.as_str(), to);
            out.push(Diagnostic::error("path.not-nfc", "file name is not in Unicode NFC").at_path(path).with_fix(fix));
        }
        if path.kind() == Some(Kind::Inbox) {
            let parts: Vec<&str> = path.components().collect();
            let root_file = parts.len() == 2 || (parts.len() == 3 && parts[1] == "journal");
            if root_file {
                out.push(
                    Diagnostic::warning(
                        "inbox.root-file",
                        "file directly in `inbox/` or `inbox/journal/`; move it into `drafts/`, `sources/` or a journal stream",
                    )
                    .at_path(path),
                );
            }
        }
    }
}
