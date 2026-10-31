//! Source folders (m0 §6.4): `source.yaml` and hashing.

use sha2::{Digest, Sha256};

/// A source folder's `source.yaml` (m0 §6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMeta {
    pub sha256: String,
    pub url: Option<String>,
}

impl SourceMeta {
    pub fn parse(text: &str) -> Result<Self, String> {
        use saphyr::{LoadableYamlNode, Scalar, Yaml};
        let docs = Yaml::load_from_str(text).map_err(|e| e.info().to_string())?;
        let Some(Yaml::Mapping(map)) = docs.into_iter().next() else {
            return Err("not a mapping".into());
        };
        let (mut sha256, mut url) = (None, None);
        for (k, v) in map {
            let (Yaml::Value(Scalar::String(k)), Yaml::Value(Scalar::String(v))) = (&k, &v) else {
                return Err("keys and values must be strings".into());
            };
            match k.as_ref() {
                "sha256" => sha256 = Some(v.to_string()),
                "url" => url = Some(v.to_string()),
                other => return Err(format!("unknown key `{other}`")),
            }
        }
        let sha256 = sha256.ok_or("no `sha256`")?;
        Ok(Self { sha256, url })
    }

    pub fn render(&self) -> String {
        match &self.url {
            Some(u) => format!("url: {u}\nsha256: {}\n", self.sha256),
            None => format!("sha256: {}\n", self.sha256),
        }
    }
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_round_trip() {
        let m = SourceMeta { sha256: "ab".into(), url: Some("https://x/y.pdf".into()) };
        assert_eq!(SourceMeta::parse(&m.render()).unwrap(), m);
        let m = SourceMeta { sha256: "ab".into(), url: None };
        assert_eq!(SourceMeta::parse(&m.render()).unwrap(), m);
    }

    #[test]
    fn meta_rejects() {
        assert!(SourceMeta::parse("url: x\n").is_err());
        assert!(SourceMeta::parse("sha256: a\nextra: b\n").is_err());
    }

    #[test]
    fn sha() {
        assert_eq!(hex_sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
