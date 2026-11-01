#!/bin/sh
# The agent `loci bench` runs for each case, in that case's copy of the vault, with
# the question on stdin. Usage: agent.sh <arm dir> <model>
# Claude Code, headless: user settings and the user's own CLAUDE.md are not loaded
# (--setting-sources); it reads the vault's AGENTS.md by itself; tools are
# read-and-search only.
PATH="$1:$PATH" exec claude -p --model "$2" --setting-sources project,local \
  --allowedTools "Bash Read Glob Grep" \
  --disallowedTools "Edit Write NotebookEdit WebFetch WebSearch Task" \
  --no-session-persistence
