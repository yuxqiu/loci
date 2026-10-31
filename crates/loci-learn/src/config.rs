//! `plugins/learn/config.toml` (learn-plugin §2.1).

use std::path::Path;

use serde::Deserialize;

pub const PATH: &str = "plugins/learn/config.toml";

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, default)]
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
        // `new_per_day` is read as a signed integer so a negative value is reported as out
        // of range rather than as a type error.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            new_per_day: Option<i64>,
            desired_retention: Option<f64>,
        }
        let raw: Raw = toml::from_str(text).map_err(|e| e.message().to_string())?;
        let mut c = Config::default();
        if let Some(n) = raw.new_per_day {
            c.new_per_day = u32::try_from(n).map_err(|_| format!("`new_per_day` must be ≥ 0, got {n}"))?;
        }
        if let Some(r) = raw.desired_retention {
            if !(r > 0.0 && r < 1.0) {
                return Err(format!("`desired_retention` must be between 0 and 1, got {r}"));
            }
            c.desired_retention = r as f32;
        }
        Ok(c)
    }

    /// Reads the vault's config, or the defaults if the file is absent.
    pub fn load(vault: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(vault.join(PATH)) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.to_string()),
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
