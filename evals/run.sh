#!/bin/sh
# Runs a suite against every arm and model, REPS times, and summarises the results.
# Usage: evals/run.sh <suite> <model>...   e.g. evals/run.sh finding sonnet haiku
# Environment: REPS (default 3), ARMS (default: every arm in arms/make_arms.py).
set -eu
EVALS=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$EVALS")
SUITE=$1; shift
cargo build --quiet --manifest-path "$ROOT/Cargo.toml" -p loci
LOCI="$ROOT/target/debug/loci"
WORK=$(mktemp -d)
OUT="$EVALS/results/$(date -u +%Y-%m-%d)-$SUITE"
mkdir -p "$OUT"
python3 "$EVALS/vault/make_vault.py" "$WORK/vault" "$LOCI"
python3 "$EVALS/arms/make_arms.py" "$WORK/arms" "$LOCI" ${ARMS:-}
cd "$WORK/vault"
for model in "$@"; do
  for rep in $(seq "${REPS:-3}"); do
    for arm in "$WORK"/arms/*; do
      name=$(basename "$arm")
      "$LOCI" --json bench "$EVALS/agent.sh $arm $model" --cases "$EVALS/suites/$SUITE.jsonl" \
        > "$OUT/$model-$name-$rep.json" &
    done
    wait
  done
done
python3 "$EVALS/summarize.py" "$OUT"
rm -rf "$WORK"
