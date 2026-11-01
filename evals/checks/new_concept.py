"""Passes (exit 0) when the agent added one concept, fully linked: run in the vault
the agent left, as a `loci bench` check.

Usage: new_concept.py <folder> <title regex> <note that must link it>...

- exactly one new concept (untracked or intent-to-add) whose H1 matches the regex;
- it is in <folder> or a sub-folder of it;
- every named note links to it, and something does;
- `loci links` finds no unlinked mentions of it or in it;
- `loci lint` passes.
Each failure is printed, for reading a run by hand.
"""
import json
import re
import subprocess
import sys

folder, title, *linkers = sys.argv[1:]
git = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True, check=True).stdout.split()
new = sorted(set(git("ls-files", "--others", "--exclude-standard", "concepts")) | set(git("diff", "--name-only", "--diff-filter=A", "HEAD", "--", "concepts")))
heads = {p: next((l for l in open(p).read().splitlines() if l.startswith("# ")), "") for p in new if p.endswith(".md")}
found = [p for p, h in heads.items() if re.search(title, h, re.I)]
failures = []
if len(found) != 1:
    failures.append(f"expected one new concept titled /{title}/, found {found} among {sorted(heads)}")
else:
    path = found[0]
    if not path.startswith(folder.rstrip("/") + "/"):
        failures.append(f"{path} is not in {folder}")
    links = json.loads(subprocess.run(["loci", "--json", "links", path], capture_output=True, text=True).stdout)["data"]
    incoming = {l["path"] for l in links["incoming"]}
    failures += [f"{n} does not link it" for n in linkers if n not in incoming]
    if not incoming:
        failures.append("nothing links to it")
    failures += [f"unlinked mention of it in {m['path']}:{m['line']}" for m in links["unlinked_in"]]
    failures += [f"unlinked mention in it of {m['concept']} ({m['text']})" for m in links["unlinked_out"]]
if subprocess.run(["loci", "lint"], capture_output=True).returncode != 0:
    failures.append("loci lint fails")
print("\n".join(failures))
sys.exit(1 if failures else 0)
