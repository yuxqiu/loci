//! `loci.toml` (m0 §11).

use serde::Deserialize;

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
    pub updates: Option<String>,
    #[serde(default)]
    pub budgets: Budgets,
}

/// The number of update id `id` (`u0003` is 3), if it has the form `u` and four
/// digits.
pub fn update_number(id: &str) -> Option<u32> {
    let digits = id.strip_prefix('u')?;
    (digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit())).then(|| digits.parse().expect("four digits"))
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
        if let Some(id) = config.updates.as_deref().filter(|id| update_number(id).is_none()) {
            return Err(format!("`updates = \"{id}\"` is not an update id (`u` and four digits)"));
        }
        Ok(config)
    }

    /// The file `loci init` writes: every key spelled out, with `updates` the newest
    /// update the binary knows.
    pub fn default_file(updates: &str) -> String {
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
    pub fn set_updates(text: &str, id: &str) -> Result<String, String> {
        let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| e.to_string())?;
        match doc.get_mut("updates").and_then(toml_edit::Item::as_value_mut) {
            Some(value) => {
                let decor = value.decor().clone();
                *value = id.into();
                *value.decor_mut() = decor;
            }
            None => doc["updates"] = toml_edit::value(id),
        }
        Ok(doc.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_round_trips() {
        let c = Config::parse(&Config::default_file("u0003")).unwrap();
        assert_eq!(c.updates.as_deref(), Some("u0003"));
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
        assert_eq!(update_number("u0042"), Some(42));
        assert!(update_number("u42").is_none() && update_number("x0042").is_none());
        assert!(Config::parse("schema = 1\nupdates = \"nope\"\n").is_err());
        // Replaced in place, its comment, other comments and tables kept.
        let file = "schema = 1\nupdates = \"u0001\"   # note\n\n[budgets]\nnote = 5 # x\n";
        assert_eq!(
            Config::set_updates(file, "u0004").unwrap(),
            "schema = 1\nupdates = \"u0004\"   # note\n\n[budgets]\nnote = 5 # x\n"
        );
        // Added with the other top-level keys, before the first table.
        let bare = "# my vault\nschema = 1\n[budgets]\nnote = 5\n";
        let set = Config::set_updates(bare, "u0002").unwrap();
        assert_eq!(set, "# my vault\nschema = 1\nupdates = \"u0002\"\n[budgets]\nnote = 5\n");
        assert_eq!(Config::parse(&set).unwrap().updates.as_deref(), Some("u0002"));
        // A key that only starts with `updates` is another key.
        let near = "schema = 1\nupdates_note = \"x\"\n";
        assert!(Config::set_updates(near, "u0001").unwrap().contains("updates_note = \"x\"\nupdates = \"u0001\"\n"));
    }
}
