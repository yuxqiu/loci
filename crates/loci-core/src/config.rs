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
    #[serde(default)]
    pub budgets: Budgets,
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

    /// The file `loci init` writes: every key spelled out.
    pub fn default_file() -> String {
        let b = Budgets::default();
        format!(
            "schema = 1\n\n[budgets]           # characters (Unicode scalar values)\n\
             area_paragraph = {}\n\
             note = {}        # body characters after the frontmatter (size.note)\n",
            b.area_paragraph, b.note
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_round_trips() {
        let c = Config::parse(&Config::default_file()).unwrap();
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
}
