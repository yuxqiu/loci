//! Identifiers: ULIDs (m0 §6.1, §9.9: concept ids and block ids) and concept slugs
//! (§6.2).

use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// A slug, unanchored: lowercase letters, other letters and digits, in `-`-joined words.
pub(crate) const SLUG_PATTERN: &str = r"[\p{Ll}\p{Lo}\p{N}]+(?:-[\p{Ll}\p{Lo}\p{N}]+)*";
static SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{SLUG_PATTERN}$")).unwrap());

/// A concept slug (m0 §6.2): lowercase letters, other letters and digits, in `-`-joined
/// words. A concept's slug is its file stem; it names the concept vault-wide.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl TryFrom<String> for Slug {
    type Error = String;

    fn try_from(s: String) -> Result<Self, String> {
        s.parse()
    }
}

impl From<Slug> for String {
    fn from(s: Slug) -> String {
        s.0
    }
}

impl Slug {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Slug {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        if !SLUG.is_match(s) {
            return Err(format!("`{s}` does not match the slug pattern"));
        }
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A ULID, read only in canonical form: Crockford base32, uppercase, 26 characters, at
/// most 128 bits (so the first character is 0-7). `ulid::Ulid::from_string` alone
/// accepts lowercase and drops the bits past 128 without an error, so a ULID is
/// canonical when it prints back as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ulid(ulid::Ulid);

impl Ulid {
    /// A new ULID for the current time.
    pub fn generate() -> Self {
        Self(ulid::Ulid::generate())
    }

    /// New ULIDs, increasing within one run (m0 §9.9).
    pub fn monotonic() -> impl FnMut() -> Self {
        let mut generator = ulid::Generator::new();
        move || Self(generator.generate().expect("a ULID generator does not overflow within one run"))
    }
}

impl FromStr for Ulid {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let id = ulid::Ulid::from_string(s).ok().filter(|u| u.to_string() == s);
        id.map(Self).ok_or_else(|| format!("`{s}` is not a ULID"))
    }
}

impl fmt::Display for Ulid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Serialize for Ulid {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Ulid {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        for ok in ["bfs", "breadth-first-search", "二分查找", "a1"] {
            assert_eq!(ok.parse::<Slug>().unwrap().as_str(), ok);
        }
        for bad in ["BFS", "a_b", "-a", "a--b", "a b", ""] {
            assert!(bad.parse::<Slug>().is_err(), "{bad}");
        }
    }

    #[test]
    fn canonical_form_only() {
        let id = "01JB9XQ7Z3K4M5N6P7Q8R9S0TV";
        assert_eq!(id.parse::<Ulid>().unwrap().to_string(), id);
        assert!("abc".parse::<Ulid>().is_err());
        assert!("01jb9xq7z3k4m5n6p7q8r9s0tv".parse::<Ulid>().is_err(), "lowercase");
        assert!("81JB9XQ7Z3K4M5N6P7Q8R9S0TV".parse::<Ulid>().is_err(), "above the 128-bit range");
        assert!("01JB9XQ7Z3K4M5N6P7Q8R9S0TI".parse::<Ulid>().is_err(), "an alias, not the canonical form");
    }

    #[test]
    fn generated_ids_round_trip() {
        let mut next = Ulid::monotonic();
        let (a, b) = (next(), next());
        assert!(a < b);
        for id in [a, b, Ulid::generate()] {
            assert_eq!(id.to_string().parse::<Ulid>(), Ok(id));
        }
    }
}
