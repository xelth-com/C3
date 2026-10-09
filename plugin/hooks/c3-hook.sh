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

collab_dir=".collab"
if [ -n "$1" ]; then
  collab_dir="$1"
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
  exit 0
fi

out=$("$c3_exe" hook --collab-dir "$collab_dir" 2>&1)
code=$?
first=$(printf '%s\n' "$out" | grep -m1 '.')
if [ -z "$first" ]; then
  first="c3 hook exited $code without output"
fi
echo "$first"
# (wave 27, R13 D5) the second line: the pointer to the coordinator's rules (and the telemetry switch)
pointer=$(printf '%s\n' "$out" | grep -m1 '^codex-consult: coordinator rules - ')
if [ -n "$pointer" ] && [ "$pointer" != "$first" ]; then
  echo "$pointer"
fi
exit 0
