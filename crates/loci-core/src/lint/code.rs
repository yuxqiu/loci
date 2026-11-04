//! The codes core lint reports (m0 §7.3), each written once.

use std::fmt;
use std::str::FromStr;

use crate::config::UpdateId;

/// Declares [`Code`]: each variant with its code, so the name and the string cannot
/// drift apart.
macro_rules! codes {
    ($($variant:ident => $code:literal,)*) => {
        /// A code core lint reports. Plugins report their own, prefixed codes, which stay
        /// strings.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Code {
            $($variant,)*
            /// The old form of something vault update `id` changes (updates spec §2).
            Update(UpdateId),
        }

        impl Code {
            /// Every code but `update.<id>`, with its string.
            const NAMED: &[(Code, &'static str)] = &[$((Code::$variant, $code),)*];
        }
    };
}

codes! {
    KindUnknown => "kind.unknown",
    SourcesLayout => "sources.layout",
    SlugFormat => "slug.format",
    JournalPath => "journal.path",
    PathNotNfc => "path.not-nfc",
    PathNameClash => "path.name-clash",
    InboxRootFile => "inbox.root-file",
    FileBom => "file.bom",
    FmUnterminated => "fm.unterminated",
    FmYaml => "fm.yaml",
    FmNotAllowed => "fm.not-allowed",
    H1Missing => "h1.missing",
    H1Multiple => "h1.multiple",
    H1NotFirst => "h1.not-first",
    BlockIdDuplicate => "block.id-duplicate",
    SizeNote => "size.note",
    LinkMarkdown => "link.markdown",
    LinkDangling => "link.dangling",
    LinkPathForm => "link.path-form",
    LinkDanglingHeading => "link.dangling-heading",
    LinkDanglingBlock => "link.dangling-block",
    FmUnknownField => "fm.unknown-field",
    FmMissingField => "fm.missing-field",
    FmIdFormat => "fm.id-format",
    FmFieldType => "fm.field-type",
    FmIdDuplicate => "fm.id-duplicate",
    SlugDuplicate => "slug.duplicate",
    PrereqsCycle => "prereqs.cycle",
    AreaMissing => "area.missing",
    AreaFormat => "area.format",
    AreaTooLong => "area.too-long",
    SourcesChanged => "sources.changed",
    MaterialDropped => "material-dropped",
    GitFailed => "git.failed",
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Code::Update(id) => write!(f, "update.{id}"),
            named => {
                let (_, code) = Code::NAMED.iter().find(|(c, _)| c == named).expect("every other code is named");
                f.write_str(code)
            }
        }
    }
}

impl FromStr for Code {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        if let Some(id) = s.strip_prefix("update.") {
            return id.parse().map(Code::Update);
        }
        Code::NAMED.iter().find(|(_, code)| *code == s).map(|(c, _)| *c).ok_or_else(|| format!("`{s}` is no core code"))
    }
}

impl From<Code> for String {
    fn from(code: Code) -> String {
        code.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for (code, s) in Code::NAMED {
            assert_eq!((code.to_string(), s.parse::<Code>()), (s.to_string(), Ok(*code)));
        }
        let update = Code::Update(UpdateId::new(1));
        assert_eq!(update.to_string().parse::<Code>(), Ok(update));
        assert!("link.nope".parse::<Code>().is_err());
    }
}
