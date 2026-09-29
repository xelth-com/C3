---
name: coordinate
description: How the coordinator drives the c3 reviewer bridge from a Claude Code session - the invariants, the bridge's own means (-Detach/-Status/-Wait/-Kick) and the worker-tier contract. Read it when running c3 as a coordinator session, not as a one-off consultation.
allowed-tools: Bash(c3:*), Read, Glob, Grep
disable-model-invocation: false
---

# Coordinate (c3)

The C3 port supports one coordinator host: Claude Code. This skill is the coordination layer over
the `c3` binary. (The multi-host coordination story stays with the `codex-codex-consult` PowerShell
bridge.)

## Invariants

- The coordinator's session identity never reaches a reviewer child. `c3` scrubs the host markers
  (`CODEX_SESSION_ID`, the `CLAUDE_CODE_*` session ids, the messaging socket and token, `CLAUDE_PID`,
  `CLAUDE_EFFORT`, every `CODEX_SANDBOX*`, and the `ZCODE_*` session/project and `ZCODE_PLUGIN*`)
  from every reviewer CLI and launcher probe it launches. The ledger records the removed NAMES in
  `child_env_scrubbed`, never a value. The operator's own settings (`CLAUDE_CODE_USE_BEDROCK`), the
  plugin roots and the provider keys are kept.
- The coordinator's identity is recorded once per run in the ledger `coordinator`
  `{provider, model, engine, host, source}`. `host` is `claude-code` when the Claude Code markers
  are present, otherwise `unknown`. Set `CODEX_CONSULT_COORDINATOR` to `<provider> :: <model>`
  (optionally ` [<engine>]`), a roster position `#<n>` or a provider label to name the coordinator's
  own model (`source: explicit`); an unparseable value is refused before anything starts. Keep the
  operator's `CODEX_CONSULT_COORDINATOR` when it is set - do not guess your own.

## The bridge's own means

- `c3 consult --detach ...` runs a consultation (or a panel) in the background; `--status`/`--wait`
  report or block on it; `--kick --member <NN>` stops one running member. Prefer these to the host's
  own backgrounding.

## Worker-tier contract

- Deep reasoning that needs judgment -> the heavy tier. Well-specified, pattern-following execution
  -> the default tier. Cheap read-only recon -> the recon tier (no write tools).
