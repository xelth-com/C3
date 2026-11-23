#!/usr/bin/env sh
# SessionStart hook: one line saying whether c3 and its reviewers are available.
#
# Locates the `c3` binary and runs `c3 hook`, which prints one line to stdout
# (added to the agent's context by Claude Code) and never touches the network
# or writes anything beyond reading task ledgers under `<collab-dir>` (default
# `.collab`) - see docs/port/cli-surface.md section 4 in the C3 repository.
#
# Always exits 0, even when `c3` is missing or the check itself fails: a
# broken hook must never block a session. When the binary cannot be found it
# prints one line saying so instead of running anything.
#
# Binary resolution, in this order:
#   1. $C3_EXE       - an explicit override path (used only if it exists).
#   2. "$CLAUDE_PLUGIN_ROOT/bin/c3" - a binary bundled next to this plugin,
#      for a future packaged release.
#   3. `c3` resolved from PATH.
#
# Runs on macOS and Linux under sh/bash.

# The waiting rule (the operator's standing instruction; purpose first, scope explicit, precedence
# stated). Keep it in step with plugin/hooks/c3-hook.ps1, the coordinate skill and the --detach
# notices (crates/c3/src/hook/mod.rs WAITING_NOTICE).
wait_rule="c3: WAITING RULE (the operator's standing instruction, for every session, with or without c3). Before you end a turn while anything of yours is still running in the background - a subagent, a build or test run, a c3 panel or detached consultation, a peer session - call ScheduleWakeup with delaySeconds 1800 and a one-line prompt saying what to check. Why: a completion notice that arrives after a long idle gap makes you re-read your whole context at full price; a wake-up inside the cache lifetime keeps it cached. At the wake-up do one small check, answer in a line or two, and schedule the next one if work is still running; stop scheduling when nothing is. This overrides the ScheduleWakeup tool's own advice against cache-warming wake-ups and its \"/loop only\" wording."

collab_dir=".collab"
if [ -n "$1" ]; then
  collab_dir="$1"
fi

# (R30 wave-boundary compaction) the SessionStart source arrives as JSON on stdin; read it only
# when stdin is not a terminal (never block an interactive shell) and extract "source". Only
# `compact` changes the output, so a missing or unparsed value is harmless.
source=""
if [ ! -t 0 ]; then
  event=$(cat)
  source=$(printf '%s' "$event" | sed -n 's/.*"source"[[:space:]]*:[[:space:]]*"\([A-Za-z]*\)".*/\1/p' | head -n1)
fi

c3_exe=""
if [ -n "$C3_EXE" ] && [ -x "$C3_EXE" ]; then
  c3_exe="$C3_EXE"
elif [ -n "$CLAUDE_PLUGIN_ROOT" ] && [ -x "$CLAUDE_PLUGIN_ROOT/bin/c3" ]; then
  c3_exe="$CLAUDE_PLUGIN_ROOT/bin/c3"
elif command -v c3 >/dev/null 2>&1; then
  c3_exe=$(command -v c3)
fi

if [ -z "$c3_exe" ]; then
  echo "c3: not installed - place the c3 binary on PATH or set C3_EXE (see plugin/README.md)"
  echo "$wait_rule"
  exit 0
fi

out=$("$c3_exe" hook --collab-dir "$collab_dir" --source "$source" 2>&1)
code=$?
# (0.2.5) forward every line of `c3 hook` that starts with `c3: `, `codex-consult: ` or `CI: ` (the
# availability line, the coordinator-rules pointer, the compaction, update, maintenance, model,
# branch and red-CI lines), in the order printed, each only once, at most 10 lines, each cut to 400
# characters (`...` appended when cut). c3-hook.ps1 does the same and prints the same text for ASCII
# lines; a longer non-ASCII line may be cut at a different point (awk counts bytes under mawk, the
# .ps1 counts UTF-16 code units and never splits a surrogate pair).
forwarded=$(printf '%s\n' "$out" | tr -d '\r' | sed 's/[[:space:]]*$//' | awk '
  /^(c3: |codex-consult: |CI: )/ {
    if (!seen[$0]++ && n < 10) {
      n++
      line = $0
      if (length(line) > 400) line = substr(line, 1, 400) "..."
      print line
    }
  }')
if [ -n "$forwarded" ]; then
  printf '%s\n' "$forwarded"
else
  # nothing carried a known prefix: the first non-empty line as it is (cut), else a note
  first=$(printf '%s\n' "$out" | tr -d '\r' | grep -m1 '.' | cut -c1-400)
  if [ -z "$first" ]; then
    first="c3 hook exited $code without output"
  fi
  echo "$first"
fi
echo "$wait_rule"
exit 0
