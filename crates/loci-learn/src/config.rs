//! `plugins/learn/config.toml` (learn-plugin §2.1).

use loci_core::envelope::Refusal;
use loci_core::path::VaultPath;
use loci_core::vault::Vault;
use serde::Deserialize;

const PATH: &str = "plugins/learn/config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub new_per_day: u32,
    pub desired_retention: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self { new_per_day: 20, desired_retention: 0.9 }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let c: Config = toml::from_str(text).map_err(|e| e.to_string().trim_end().to_string())?;
        let r = c.desired_retention;
        if !(r > 0.0 && r < 1.0) {
            return Err(format!("`desired_retention` must be between 0 and 1, got {r}"));
        }
        Ok(c)
    }

    /// Reads the vault's config, or the defaults if the file is absent. An invalid
    /// file is `learn.config`; a symlink on the way to it is `file.symlink` (m0 §3).
    pub fn load(vault: &Vault) -> Result<Self, Refusal> {
        match vault.read_text(&VaultPath::fixed(PATH))? {
            Some(text) => Self::parse(&text).map_err(|e| Refusal::refused("learn.config", format!("{PATH}: {e}"))),
            None => Ok(Self::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_overrides() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
        let c = Config::parse("new_per_day = 5\ndesired_retention = 0.85\n").unwrap();
        assert_eq!(c.new_per_day, 5);
        assert!((c.desired_retention - 0.85).abs() < 1e-6);
    }

    #[test]
    fn rejects() {
        for bad in
            ["x = 1", "new_per_day = \"a\"", "new_per_day = -1", "desired_retention = 0.0", "desired_retention = 1.0"]
        {
            assert!(Config::parse(bad).is_err(), "{bad}");
        }
    }
}
