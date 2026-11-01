"""Prints pass counts per model and arm, and per case, from one run directory of
`loci --json bench` outputs named <model>-<arm>-<rep>.json.

Usage: summarize.py <results dir>
"""
import collections
import glob
import json
import os
import sys

runs = collections.defaultdict(list)
for path in sorted(glob.glob(os.path.join(sys.argv[1], "*.json"))):
    model, rest = os.path.basename(path)[:-5].split("-", 1)
    arm, _rep = rest.rsplit("-", 1)
    env = json.load(open(path))
    assert env["ok"], f"{path}: {env['error']}"
    runs[(model, arm)].append(env["data"])
print(f"{'model':8}{'arm':12}{'pass':>9}{'median s':>10}")
for (model, arm), datas in sorted(runs.items()):
    passed = sum(r["pass"] for d in datas for r in d["results"])
    total = sum(len(d["results"]) for d in datas)
    median = sorted(d["median_s"] for d in datas)[len(datas) // 2]
    print(f"{model:8}{arm:12}{passed:>5}/{total:<3}{median:>10.1f}")
print("\nper case (passes over all runs of each model and arm):")
keys = sorted(runs)
print("    " + " ".join(f"{m[:1]}-{a}" for m, a in keys))
questions = [r["q"] for r in runs[keys[0]][0]["results"]]
for i, q in enumerate(questions):
    counts = [sum(d["results"][i]["pass"] for d in runs[k]) for k in keys]
    print(f"{i + 1:3} " + " ".join(f"{c:>{len(m[:1]) + 1 + len(a)}}" for c, (m, a) in zip(counts, keys)) + f"  {q[:60]}")
