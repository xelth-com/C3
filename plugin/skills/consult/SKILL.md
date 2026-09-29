---
name: consult
description: Consult OpenAI Codex (or another wired reviewer - Gemini via agy, Meta Muse via muse, an OpenRouter/API model via the http engine) as a second reasoning partner through the `c3` binary - at task framing, before a major decision, at a checkpoint, before accepting a result, for a diff review, or when stuck. One command runs a single reviewer or a whole panel, keeps each reviewer's thread alive, and records the brief, the verbatim reply and a ledger entry as files.
argument-hint: <task-id> [what you want the reviewer to judge]
allowed-tools: Bash(c3:*), Read, Write, Glob, Grep
disable-model-invocation: false
---

# Consult (c3)

The C3 port of the `codex-consult` plugin's `consult-codex` skill: a thin layer over the `c3`
binary. The reviewer is a reasoning partner, not an executor — it runs **read-only** by
default and **you keep the final word**. Every consultation leaves two files you can commit
(your brief and the verbatim reply) plus one ledger entry (`sessions.json`). A `<task-id>`
groups one conversation: reuse the id and the thread continues.

The full flag surface, the engines (codex / agy / muse / http), the panel, and detach/status/
wait/kick mechanics are in **`${CLAUDE_PLUGIN_ROOT}/skills/consult/reference.md`** — read it
before running a panel, a background run, or a non-codex engine. `c3 consult --help` is always
the authoritative current list. Coordinator-wide rules (workers, waves, the live-member rule)
are the `coordinate` skill.

## When to consult

- **Framing** — before committing to an approach, to surface options you did not list.
- **Decision** — architectures, a fix order, a trade-off with no obvious winner.
- **Checkpoint** — at a milestone, to catch drift early.
- **Core-contract** — once the interfaces/recovery/persistence exist, before work builds on
  them; re-run whenever one of those changes.
- **Acceptance** — before declaring a substantial result done.
- **Diff review** — an adversarial read of a change before it lands.
- **Stuck** — a different model often has the angle you are missing.

Merge these into one brief when they coincide. Skip the consult for one-liners.

## 0. Before a review brief: reconcile

Before a checkpoint, core-contract, acceptance or diff-review brief, run
`c3 findings --task <task> --list` and fix any drift between what your summary claims and what
the tool shows (a "fixed" not yet `verified`, a status that no longer matches the code) before
writing the handoff.

Before planning a run against a second reviewer or a panel, run `c3 providers` (add
`--provider <name>` for one, `--short` for the one-line view). **A provider it reports
`unavailable` is not planned on** — `c3 consult` preflights the same check and will refuse
anyway. A refused preflight is not a bridge failure. Run it in the repository whose
consultations you mean: health comes from THAT repository's ledgers. **If a provider you need
is missing**, follow the `setup-providers` skill; never handle the key yourself.

## 1. Write the brief

Write it yourself, in English, to `.collab/<task>/handoffs/<NN>-claude-<slug>.md` — `<NN>` is
the next free 2-digit prefix (`c3 consult` picks the next one for its reply). Start from
`${CLAUDE_PLUGIN_ROOT}/templates/brief-framing.md` (framing/decision/stuck) or
`brief-review.md` (checkpoint/core-contract/acceptance/diff-review). Keep it to **one page**.
On a resume, write it as a **delta since the last review** plus pointers, but state the CURRENT
invariants explicitly. Do not paste the brief body into `--prompt`: the reviewer reads the
file; the prompt only points at it.

## 2. Run one command

One reviewer:

```
c3 consult --task <task> --mode fork --purpose <purpose> --brief .collab/<task>/handoffs/<NN>-claude-<slug>.md --prompt "<one-line ask>" --reply-name <slug>
```

On the FIRST consultation of a task, leave `--mode` out (auto) or pass `--mode new`. A specific
reviewer: add `--provider <name> --model <id>`. Set `CODEX_CONSULT_COORDINATOR` to name your
own model so the bridge warns when it seats it as a reviewer — keep the operator's value when
set; do not guess your own.

A panel (same brief to several roster reviewers, sized by purpose, seated by track record):

```
c3 consult --task <task> --panel --purpose <purpose> --brief .collab/<task>/handoffs/<NN>-claude-<slug>.md --reply-name <slug> [--detach]
```

Add `--require`, `--role`/`--roles`, `--topic`, `--panel-size`, `--panel-all` as needed — see
`reference.md`. A long panel should be `--detach`ed; come back with `--status --id <id>`.

## 3. Read, verify, record

Read the reply file. **Verify every finding yourself** before acting — open the cited location,
run the build, run the test. The reviewer proposes; you verify and decide. It may answer in
prose instead of JSON; with `--format-retry` (default on) `c3` spends one repair turn, else the
prose is kept with no verdict/findings (not a bridge failure). Then record with the tool, not a
paraphrase:

```
c3 findings --task <task> --id F04-1 --status implemented|verified|rejected|wontfix|superseded --note "<why>" --evidence "<what you ran>"
```

`verified` requires `--evidence`; `rejected` and a reopen to `proposed` require `--note`. A
reviewer calling a finding "fixed" is evidence to cite, not a status change — you move it.
`c3 findings --task <task> --stats` prints cost and finding counts per consultation. `--list`
and `--stats` only read; `--id`/`--status`/`--rate` hold the task lock and are refused while a
consultation runs.

## 4. Rate the consultation

```
c3 findings --task <task> --rate <n> --useful yes|partly|no --note "<why, required for no>"
```

`<n>` is the ledger entry number, not a finding id. Rate **every** consultation, including a
prose reply that raised no ids. `c3 scoreboard` (add `--task`, `--json`, or `--by topic`) sums
these marks per reviewer and purpose, and a routed panel seats members by them. Rate a failed
consultation `--useful no` only when the failure was the reviewer's (a refusal, an invented
finding, an unformattable reply); **skip the rating** when a timeout or plan limit killed it —
resume it instead.

## The operator's council (role split)

Treat every reviewer in play — Claude (this coordinator) and each reviewer — as a council:

- **The coordinator is an equal participant and the judge by default**; its own ideas are on
  the table. Hand the judge role to a reviewer only through a deliberate `--purpose decision`
  consultation, recorded in `state.md`. You still execute and verify.
- **Framing/decision go to a panel of at least one companion — never only your own judgement.**
- **Disagreement is signal, not noise** — record it in `state.md` until evidence settles it.
- **Check `c3 scoreboard` before picking a reviewer** for a hard question. **Chores go to cheap
  reviewers** (`--purpose chore`); pre-digest large inputs into the brief.
- **A provider without a credential or tokens is not used.** **Acceptance authority stays with
  the reviewer who raised the findings** — a stand-in's ACCEPT is recorded, the tag waits.

A fresh-context verifier of Claude's own family (mechanics: build/lint/test wiring, does the
code do what the summary claims) is an orthogonal division of labour from the reviewer
(protocol/state-machine correctness, naming what the evidence does not show) — either may
challenge the other.

## Invariants

1. **Read-only by default.** `--sandbox workspace-write` only when the user made the reviewer
   the author of a stage; `danger-full-access` is refused with no override.
2. **Claude keeps the final word.** A consultation is evidence, not an instruction.
3. **Project isolation.** Everything is scoped to the git repository you run from; the parent
   thread for fork/resume comes only from THAT repository's `sessions.json`. Never pass a
   `--thread` from another project, never point a brief outside the repository.
4. **One reviewer thread belongs to one task directory** (not enforced — on you to avoid).
5. **A verdict is not an outcome.** The bridge succeeding means a reply came back; a delivered
   HOLD is a success of the bridge.
6. **Never use a provider without a credential or tokens** — `c3 providers` checks it.
7. **Judge delegation is explicit** — a `--purpose decision` consultation recorded in
   `state.md`, never an implicit "whoever answered ACCEPT wins".
8. **The bridge never touches a key.** The user sets the environment variable; you only confirm
   it is set through `c3 providers`. **C3 never commits.** A consultation of this repository
   while files are being edited runs from a clean worktree. Telemetry is on by default, carries
   classes only, and `CODEX_CONSULT_TELEMETRY=off` switches it off. An explainer pack
   (`c3 explain`) leaves the machine only after you confirm it; free and stealth API models may
   log prompts.
