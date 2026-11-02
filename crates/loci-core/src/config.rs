//! `loci.toml` (m0 §11).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub const FILE: &str = "loci.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Budgets {
    pub area_paragraph: usize,
    pub note: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Self { area_paragraph: 600, note: 12000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: u32,
    /// The last vault update handled, applied or declined (`loci updates`).
    pub updates: Option<UpdateId>,
    #[serde(default)]
    pub budgets: Budgets,
}

/// A vault update's id: `u` and four digits (`u0003`), ordered by its number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UpdateId(u32);

impl FromStr for UpdateId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        s.strip_prefix('u')
            .filter(|d| d.len() == 4 && d.bytes().all(|b| b.is_ascii_digit()))
            .map(|d| Self(d.parse().expect("four digits")))
            .ok_or_else(|| format!("`{s}` is not an update id (`u` and four digits)"))
    }
}

impl fmt::Display for UpdateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "u{:04}", self.0)
    }
}

impl Serialize for UpdateId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for UpdateId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        if config.schema != 1 {
            return Err(format!(
                "`schema = {}` is not supported: this loci reads schema 1 (a newer schema needs a newer loci)",
                config.schema
            ));
        }
        Ok(config)
    }

    /// The file `loci init` writes: every key spelled out, with `updates` the newest
    /// update the binary knows.
    pub fn default_file(updates: UpdateId) -> String {
        let b = Budgets::default();
        format!(
            "schema = 1\n\
             updates = \"{updates}\"   # the last vault update handled (loci updates)\n\n\
             [budgets]           # characters (Unicode scalar values)\n\
             area_paragraph = {}\n\
             note = {}        # body characters after the frontmatter (size.note)\n",
            b.area_paragraph, b.note
        )
    }

    /// `text`, a `loci.toml`, with its `updates` key set to `id`. Edited with
    /// `toml_edit`, so comments, layout and other keys stay as written; a comment on
    /// the `updates` line is kept too.
    pub fn set_updates(text: &str, id: UpdateId) -> Result<String, String> {
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
        match doc.get_mut("updates").and_then(toml_edit::Item::as_value_mut) {
            Some(value) => {
                let decor = value.decor().clone();
                *value = id.to_string().into();
                *value.decor_mut() = decor;
            }
            None => doc["updates"] = toml_edit::value(id.to_string()),
        }
        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> UpdateId {
        s.parse().unwrap()
    }

    #[test]
    fn default_file_round_trips() {
        let c = Config::parse(&Config::default_file(id("u0003"))).unwrap();
        assert_eq!(c.updates, Some(id("u0003")));
        assert_eq!(c.budgets, Budgets::default());
    }

    #[test]
    fn budgets_may_be_omitted() {
        let c = Config::parse("schema = 1\n[budgets]\narea_paragraph = 80\n").unwrap();
        assert_eq!(c.budgets.area_paragraph, 80);
        assert_eq!(c.budgets.note, 12000);
    }

    #[test]
    fn unknown_key_and_schema_rejected() {
        assert!(Config::parse("schema = 1\nfoo = 2\n").is_err());
        assert!(Config::parse("schema = 1\n[budgets]\nbar = 1\n").is_err());
        assert!(Config::parse("schema = 2\n").is_err());
        assert!(Config::parse("schema = 1\n[learn]\nx = 1\n").is_err());
    }

    #[test]
    fn the_updates_key_is_an_id_set_in_place() {
        assert_eq!(id("u0042").to_string(), "u0042");
        assert!(id("u0009") < id("u0010"));
        assert!("u42".parse::<UpdateId>().is_err() && "x0042".parse::<UpdateId>().is_err());
        let bad = Config::parse("schema = 1\nupdates = \"nope\"\n").unwrap_err();
        assert!(bad.contains("`nope` is not an update id"), "{bad}");
        // Replaced in place, its comment, other comments and tables kept.
        let file = "schema = 1\nupdates = \"u0001\"   # note\n\n[budgets]\nnote = 5 # x\n";
        assert_eq!(
            Config::set_updates(file, id("u0004")).unwrap(),
            "schema = 1\nupdates = \"u0004\"   # note\n\n[budgets]\nnote = 5 # x\n"
        );
        // Added with the other top-level keys, before the first table.
        let bare = "# my vault\nschema = 1\n[budgets]\nnote = 5\n";
        let set = Config::set_updates(bare, id("u0002")).unwrap();
        assert_eq!(set, "# my vault\nschema = 1\nupdates = \"u0002\"\n[budgets]\nnote = 5\n");
        assert_eq!(Config::parse(&set).unwrap().updates, Some(id("u0002")));
        // A key that only starts with `updates` is another key.
        let near = "schema = 1\nupdates_note = \"x\"\n";
        assert!(
            Config::set_updates(near, id("u0001")).unwrap().contains("updates_note = \"x\"\nupdates = \"u0001\"\n")
        );
    }
}
