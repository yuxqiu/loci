"""Writes one directory per arm under <out dir>: a variant of the framework rules and
a `loci` shim that prints it for `loci rules` and runs the real binary otherwise.
Each arm is a function from the current rules (share/loci/rules.md) to its variant.

Usage: make_arms.py <out dir> <loci binary> [arm ...]   (default: every arm)
"""
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def finding(rules):
    """The "Finding things" bullets from the `rg` one to the `loci skill show` one."""
    return rules.index("- Search with `rg` (ripgrep)."), rules.index("- `loci skill show` lists")


def bare(rules):
    start, end = finding(rules)
    return rules[:start] + "- Search the vault with `rg`.\n" + rules[end:]


def patterns(rules):
    _, end = finding(rules)
    return rules[: rules.index("- After a hit, `loci links <note>`")] + rules[end:]


def nolinks(rules):
    """Steps 3 and 4 of "Adding a concept" without pointing at `loci links`."""
    step3 = rules.index("  3. Create it with `loci new")
    step4_end = rules.index("- Frontmatter holds only")
    return (
        rules[:step3]
        + "  3. Create it with `loci new concepts/<area>/.../<slug>.md` (never invent ids),\n"
        "     write it, and link the first mention of every concept that has a note.\n"
        "  4. Link it from the notes that mention it. Adding a link to an existing note\n"
        "     is part of adding a concept, even in text the user wrote.\n"
        + rules[step4_end:]
    )


ARMS = {"current": lambda r: r, "bare": bare, "patterns": patterns, "nolinks": nolinks}

if __name__ == "__main__":
    out, loci, names = sys.argv[1], os.path.abspath(sys.argv[2]), sys.argv[3:] or list(ARMS)
    rules = open(os.path.join(ROOT, "share/loci/rules.md")).read()
    for name in names:
        d = os.path.join(out, name)
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "rules.md"), "w") as f:
            f.write(ARMS[name](rules))
        shim = os.path.join(d, "loci")
        with open(shim, "w") as f:
            f.write(
                "#!/bin/sh\n"
                'case " $* " in *" rules "*) cat "$(dirname "$0")/rules.md"; exit 0;; esac\n'
                f'exec "{loci}" "$@"\n'
            )
        os.chmod(shim, 0o755)
