//! Text compiled into the binary: the `AGENTS.md` stub, the framework rules and the
//! core skills (m0 §9.1, §12).

use loci_core::plugin::Skill;

pub const AGENTS_STUB: &str = "# Agents

This vault is managed by loci (https://github.com/yuxqiu/loci).

1. Check that the `loci` command is installed; if not, install it as described at
   the URL above.
2. At the start of a session, read the framework rules (`loci rules`), then
   `RULES.md` (this vault's rules and lessons) and `STYLE.md` (the user's writing
   style), which override the framework rules where they differ.
3. Before each task, run `loci status` and act on what it reports.
";

pub const RULES: &str = include_str!("../../../share/loci/rules.md");

/// The vault updates (`loci updates`): every entry, append-only.
pub const UPDATES: &str = include_str!("../../../share/loci/updates.md");

const SKILL_FILES: &[(&str, &str)] = &[
    ("concepts", include_str!("../../../share/loci/skills/concepts/SKILL.md")),
    ("improve", include_str!("../../../share/loci/skills/improve/SKILL.md")),
    ("intake", include_str!("../../../share/loci/skills/intake/SKILL.md")),
    ("migrate", include_str!("../../../share/loci/skills/migrate/SKILL.md")),
];

/// The core skills.
pub fn skills() -> Vec<Skill> {
    Skill::compiled(SKILL_FILES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_a_description_and_matching_name() {
        for s in skills() {
            assert!(!s.description.is_empty(), "{}", s.name);
            assert!(s.body.contains(&format!("name: {}\n", s.name)), "{}", s.name);
        }
    }
}
