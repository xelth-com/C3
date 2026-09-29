---
name: coordinate
description: How the coordinator drives the c3 reviewer bridge from a Claude Code session - the invariants, the live-member rule, the bridge's own background means (--detach / --status / --wait / --kick), the coordinator record (CODEX_CONSULT_COORDINATOR), the worker-tier contract, and the supporting c3 commands (pack, explain, snapshot, index, router). Read it when running c3 as a coordinator session, not as a one-off consultation.
argument-hint: "[what you are coordinating]"
allowed-tools: Bash(c3:*), Read, Glob, Grep
disable-model-invocation: false
---

# Coordinate (c3)

The C3 port supports one coordinator host: **Claude Code**. This skill is the coordination
layer over the `c3` binary. (The multi-host coordination story — Codex CLI, Z Code, Kimi Code,
a plain shell — stays with the `codex-consult` PowerShell bridge and is deliberately not ported
here.) The consultation procedure itself is the `consult` skill; wiring reviewers is
`setup-providers`.

## Invariants

- **You are the coordinator and the judge by default.** Workers execute, reviewers advise, you
  decide and verify. Never redo a worker's work once it reports — read the report and verify in
  your own loop; what still needs checking is a new, scoped task.
- **One objective per worker.** Brief it precisely the first time: the objective, the exact
  files, the numbers its result is measured against, what it must not touch, the report shape. A
  launch, a wait and a re-brief cost more than a brief written once.
- **English to reviewers and workers.** Briefs, prompts and handoff titles are in English;
  source material in another language is translated or summarised, never pasted.
- **Rate every consultation** once its reply is read (`c3 findings --task <task> --rate <n>
  --useful yes|partly|no`). A routed panel draws its seats from these marks.
- **Ask the operator before going on without a required reviewer** — when one the topic needs is
  out, or `--require` refuses the run, tell the operator who is out and until when.

## The live-member rule

While an `agy` or `muse` member runs (a single run or a panel member, detached or not), write
NOTHING under the collab directory or the working tree — `state.md`, notes and findings stores
included — and run no git command. Both engines are checked by evidence: a change during an
`agy` run FAILS that member; during a `muse` run it WARNS; either way the reviewer read a moving
target. This binds your workers too: hold a writing worker until `--status` says the run is
done, then write what you queued. (The `http` engine never touches the machine — it only warns.)

## The coordinator record

- The coordinator's session identity never reaches a reviewer child. `c3` scrubs the host
  markers (`CODEX_SESSION_ID`, the `CLAUDE_CODE_*` session ids, the messaging socket and token,
  `CLAUDE_PID`, `CLAUDE_EFFORT`, every `CODEX_SANDBOX*`, and the `ZCODE_*` / `ZCODE_PLUGIN*`)
  from every reviewer CLI and launcher probe. The ledger records the removed NAMES in
  `child_env_scrubbed`, never a value. The operator's own settings, the plugin roots and the
  provider keys are kept.
- The coordinator's identity is recorded once per run in the ledger `coordinator`
  `{provider, model, engine, host, source}`. `host` is `claude-code` when the Claude Code
  markers are present, else `unknown`. Set `CODEX_CONSULT_COORDINATOR` to `<provider> :: <model>`
  (optionally ` [<engine>]`), a roster position `#<n>` or a provider label to name the
  coordinator's own model; a reviewer the bridge seats that IS your model gets a warning (never a
  refusal), and an unparseable value is refused before anything starts. Keep the operator's value
  when it is set — do not guess your own.

## The bridge's own background means

Prefer these to the host's own backgrounding. Full mechanics and exit codes: the `consult`
skill's `reference.md`.

- `c3 consult --detach ...` — run a consultation or a panel in the background, return at once
  with the detach id, its status file and the come-back commands. Checked first: a refusal comes
  back immediately, nothing written.
- `c3 consult --task <task> --status [--id <id>]` — read status only, never blocks (0 done+usable,
  1 done with a failure or died, 2 running). `--status --prune` removes files of runs done/died
  > 7 days.
- `c3 consult --task <task> --wait [--id <id>] [--wait-timeout-sec <s>]` — block until done; keep
  `<s>` below your tool's command timeout.
- `c3 consult --task <task> --kick --member <NN> [--id <id>]` — stop ONE hung member; its partial
  is salvaged and the panel goes on. Ask the operator before kicking a merely-slow member.

Park a detached run in the task's `state.md` (the detach id, the brief path, the come-back
command); a later session finds it there and in the SessionStart line.

## Supporting c3 commands

- `c3 pack --brief <b> --out <o> [--focus <glob>] [--budget <n>] [--task <t>]` — build a
  sanitizing reviewer pack (with a `.pack.json` sidecar) for the `http` engine or for a chat
  model; the index feeds the periphery when present, else a lexical neighbourhood.
- `c3 explain --claim "<c>" [--focus <glob>] [--audience <who>] --yes` — an explainer pack for
  one claim. **It leaves the machine only after you confirm** (`--yes` skips the prompt).
- `c3 snapshot [--delta] [--depth 0-9] [--budget <n>] [--focus <glob>]` — a repository snapshot,
  optionally a git delta since the last full snapshot's anchor.
- `c3 index build|rebuild|query|stats` — the local code index (embedded SurrealDB, per project).
  Never required for a consultation; `query` is BM25 + reciprocal-rank with a bounded 1-hop
  expansion.
- `c3 router simulate|replay|priors|explain` — routing insight: `explain` prints the score table
  the next panel draw would use, one line per roster lineage; `replay` confirms a routed panel's
  seats from the ledger.
- `c3 telemetry status` (the on/off status and instance id), `c3 complain "<text>"` and
  `c3 forget-me` (the maintainer intake; both confirm before sending). Telemetry is on by
  default, carries outcome classes only — never task names, prompts, briefs, paths or keys — and
  `CODEX_CONSULT_TELEMETRY=off` switches it off.

## Worker-tier contract

Pick the tier by what the task needs; the names stay, the models behind them change.

- **Deep reasoning that needs judgment** (`opus-worker`) — novel code, multi-file debugging,
  build-and-fix loops. Writes.
- **Well-specified, pattern-following execution** (`sonnet-worker`, the default) — a known change
  across files, tests to a pattern, doc updates, running a suite and reporting. Writes.
- **Cheap read-only recon** (`haiku-worker`) — locating code, mapping a surface, log analysis,
  run-and-report. Never writes.

Delegate a chunk that is self-contained, heavy/noisy, or one of several independent pieces (fan
those out in one turn). A worker never starts or finishes the overall task and never commits
unless its brief says so.
