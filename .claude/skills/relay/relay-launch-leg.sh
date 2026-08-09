#!/usr/bin/env bash
# relay: launch the next leg as a detached tmux session running a fresh claude.
#
# Usage: relay-launch-leg.sh <session-name> <project-dir>
#   <session-name>  tmux session name == claude --name (e.g. footprint-r3)
#   <project-dir>   working dir for the spawned claude (-c of tmux new-session)
#
# This script ONLY spawns the detached session. The caller (relay SKILL.md
# step 6) confirms startup via capture-pane and then sends `/relay`.
set -euo pipefail

SESSION="${1:?session name required}"
PROJECT_DIR="${2:?project dir required}"

tmux new-session -d -s "$SESSION" -c "$PROJECT_DIR" \
  "claude --dangerously-skip-permissions --name $SESSION"
