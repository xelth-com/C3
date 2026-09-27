---
name: consult
description: Consult OpenAI Codex (or another wired reviewer) as a second reasoning partner through the `c3` binary - at task framing, before a major decision, at a progress checkpoint, before accepting a substantial result, for a diff review, or when stuck. Runs one command, keeps the reviewer's thread alive across consultations, and records the brief, the verbatim reply and a ledger entry as files.
argument-hint: <task-id> [what you want the reviewer to judge]
allowed-tools: Bash(c3:*), Read, Write, Glob, Grep
disable-model-invocation: false
---

# Consult (c3)

This is the C3 port of the `codex-consult` plugin's `consult-codex` skill: a thin layer
over the `c3` binary instead of PowerShell scripts. Where this file is silent, the rules
are unchanged from that plugin.

The reviewer is a reasoning partner here, not an executor. **You keep the final word.**
It runs read-only by default: it reads the repository and answers, it does not edit.

Every consultation leaves two files you can commit — your brief and the reviewer's
verbatim reply — plus one entry in a JSON ledger (`sessions.json`). A `<task-id>` groups
one conversation: reuse the same id and the thread continues.

**Not yet implemented in `c3`** (as of this port; check `c3 consult --help` for the
current state before relying on this list) — do not tell the coordinator to run these,
they refuse or are stubs:

- `--panel` / `--panel-all` / `--panel-concurrency` — refused (milestone 4). Consult one
  reviewer at a time with `--provider`/`--model` instead of asking for a panel.
- `--engine agy` / `--engine muse` — only `--engine codex` (the default) actually runs
  (milestone 2d/4). Do not route a consultation to Gemini or Meta Muse through `c3` yet.
- `--detach` / `--status` / `--list` / `--wait` / `--prune` — reserved flags that parse
  but do nothing useful yet (R12/milestone 4). Every `c3 consult` run today blocks until
  it finishes; do not suggest parking it in the background.
- `--topic`, `--require`, `--role`/`--roles`, `--panel-order`, `--panel-seed`,
  `--panel-size` — not present on `c3 consult` at all yet; do not pass them.

Everything below (`--task`, `--purpose`, `--brief`, `--prompt`, `--reply-name`, a single
reviewer via `--provider`/`--model`, `--mode`, `--thread`, effort/timeout/sandbox
overrides, `--raw`, `--dry-run`) is implemented and safe to use.

## When to consult

- **Framing** — before committing to an approach, to surface options you did not list.
- **Decision** — when weighing architectures, a fix order, or a trade-off with no obvious winner.
- **Checkpoint** — at a meaningful milestone, to catch drift early rather than at the end.
- **Core-contract checkpoint** — before dependent work builds on the core: once the
  interfaces, recovery and persistence paths exist, but before anything is built on top
  of them. Re-run it whenever recovery, persistence or an interface changes — a mechanical
  wave downstream of the core does not need its own review, but a change to the core does.
- **Acceptance** — before declaring a substantial result done.
- **Diff review** — an adversarial read of a change before it lands.
- **Stuck** — a different model often has the angle you are missing.

Several of these can be merged into one brief when they coincide. Skip the consult
for one-liners; the round trip costs more than the answer is worth.

## 0. Before a review brief: reconcile

Before a checkpoint, core-contract, acceptance or diff-review brief, run

```
c3 findings --task <task> --list
```

and read the current findings. Fix any drift between what your own summary claims and
what the tool actually shows — a coordinator's "fixed" that has not been marked
`verified`, a status that no longer matches the code — before you write the handoff. A
brief built on a drifted summary makes the reviewer re-derive state that should already
have been settled.

Before planning a run against a second reviewer (`--provider`), run

```
c3 providers
```

(add `--provider <name>` to check just one, `--short` for the one-line view) and read its
verdict. **A provider it reports `unavailable` is not planned on** — do not schedule work
against a provider whose credentials are missing, or whose usage limit resets in the
future, on the strength that it "should" work; `c3 consult` preflights the same check
before every run and will refuse anyway, but knowing this before writing the brief saves
the round trip. A refused preflight is not a bridge failure — `c3 providers` says why
(missing credentials, an unresolved identity, a recent auth failure, or a usage limit
until an iso timestamp); do not plan further work on that provider until it reports
available again. Run it in the repository whose consultations you mean: health comes from
THAT repository's ledgers.

**If a provider you need is missing** (no `[model_providers.<name>]` row, `missing: env
... not set`), follow the `setup-providers` skill before planning work on it; never
handle the key yourself.

## 1. Write the brief

Write it yourself, in English, to
`.collab/<task>/handoffs/<NN>-claude-<slug>.md` — `<NN>` is the next free 2-digit
prefix in that `handoffs/` directory (`c3 consult` picks the next one for its reply).
Create the directory if it does not exist. Start from a template:
`${CLAUDE_PLUGIN_ROOT}/templates/brief-framing.md` for framing, decision or stuck;
`${CLAUDE_PLUGIN_ROOT}/templates/brief-review.md` for checkpoint, core-contract,
acceptance or diff-review. Keep it to **one page**. When the thread is being resumed,
write the brief as a **delta since the last review** plus pointers, not a retelling —
but state the CURRENT invariants explicitly; history is not an authoritative
current-state record, and a delta-only brief hides anything that was already true and
still matters.

Do not paste the brief body into `--prompt`: the reviewer reads the file from the
repository; the prompt only points at it.

## 2. Run one command

```
c3 consult --task <task> --mode fork --purpose <purpose> --brief .collab/<task>/handoffs/<NN>-claude-<slug>.md --prompt "<one-line ask>" --reply-name <slug>
```

`--purpose` selects the prompt paragraph the reviewer is asked to answer under, and its
default effort and word cap:

| `--purpose` | Effort | Max words |
|---|---|---|
| *(none)* | high | 700 |
| `framing` | high | 700 |
| `decision` | high | 700 |
| `checkpoint` | medium | 500 |
| `core-contract` | xhigh | 900 |
| `acceptance` | high | 900 |
| `diff-review` | high | 700 |
| `stuck` | xhigh | 700 |
| `chore` | low | 400 |

`chore` is a bounded search or extraction task — a plain-text reply like `--raw` (no
schema, no findings bookkeeping), meant for grunt work (searching a big file, extracting
facts) that should not cost a weighty reviewer's tokens; hand it to a cheap roster
member once panels exist. `--effort` and `--max-words` override the preset when given.

Flags (from `docs/port/cli-surface.md` in the C3 repository; run `c3 consult --help` for
the authoritative, current list):

| Flag | Meaning |
|---|---|
| `--task <slug>` | mandatory; groups one conversation |
| `--collab-dir <path>` | default `.collab` |
| `--mode new\|fork\|resume` | empty = auto: fork the newest thread of this lineage, else new |
| `--thread <uuid>` | pick a specific parent; needs `--mode fork` or `resume` |
| `--brief <path>` | the brief file; `c3` never writes briefs |
| `--prompt <text>` | the one-line ask |
| `--model <id>` | omit it and the model comes from the top level of the user's Codex config |
| `--purpose <name>` | see table above |
| `--effort low\|medium\|high\|xhigh` | overrides the purpose preset |
| `--sandbox read-only\|workspace-write` | default `read-only`; `danger-full-access` is refused |
| `--max-words <n>` | 0 = purpose preset |
| `--timeout-sec <n>` | 0 = purpose default (chore 600, checkpoint/none 900, framing/decision 1800, diff-review/core-contract/stuck 2400, acceptance 3600) |
| `--continue-sec <n>` | -1 = min(timeout, 900); 0 = no continuation |
| `--range <base..head>` | diff-review/acceptance only; a single revision is refused |
| `--reply-name <slug>` | the handoff base name; default `reply` |
| `--artifact <path>` | repeatable; hash a built artifact into the ledger |
| `--raw` | plain-text reply: no schema, no findings bookkeeping |
| `--codex-exe <path>` | env override `CODEX_CONSULT_EXE` |
| `--provider <name>` | a `[model_providers.<name>]` entry; needs `--model` unless a roster entry supplies one |
| `--native-effort <value>` | send an effort value verbatim; excludes `--effort` |
| `--off-peak-only` | refuse instead of warn inside a peak window |
| `--skip-preflight` | bypass the automatic preflight — only for an endpoint that genuinely needs no credential, or once after a credential rotation; never to push past a real refusal |
| `--codex-config k=v` | repeatable; identity/effort keys (`model`, `model_provider`, `model_reasoning_effort`, `profile`, `model_providers.*`) are refused |
| `--schema-transport output-schema\|prompt-only` | empty = the caps-v1 default for the endpoint |
| `--format-retry` / `--no-format-retry` | default on; one recorded repair turn when the reply is not valid JSON |
| `--dry-run` | print the plan and exit; writes nothing |

**A second model is consulted in its own lineage; never fork/resume across providers** —
`c3` enforces this by refusing a `--thread` or automatic parent whose lineage does not
match the current `--provider`/`--model`; do not try to work around it by hand either.

`--engine`, `--engine-exe`, `--denial-retry`, `--max-model-steps` exist on the CLI but
only `--engine codex` (the implicit default) is functional right now — see "Not yet
implemented" above.

`c3 consult` creates `handoffs/` and `sessions.json` when missing, and writes:

- `handoffs/<NN>-codex-<slug>.md` — header, the verbatim reply, and (unless `--raw`)
  the rendered findings/verdict/blockers/unproven sections;
- `handoffs/<NN>-codex-<slug>.reply.json` — the raw structured reply, byte for byte
  (structured mode only);
- `handoffs/<NN>-codex-<slug>.events.jsonl` — the raw event stream;
- `findings.json` — every finding from this reply, appended (structured mode only,
  only when there is at least one finding);
- `sessions.json` — one ledger entry appended, the commit point for this consult.

A launched run that fails exits non-zero and is still recorded as an entry, so the ledger
is a complete history and not just a success log. A refusal (bad arguments, a preflight
refusal, a live previous run, `--off-peak-only`) exits non-zero with a `c3: <message>`
line and writes nothing.

## 3. Read, verify, record

Read the reply file. **Verify every finding yourself** before acting on it — open the
cited location, run the build, run the test. The reviewer proposes; you verify and
decide. It may answer in plain prose instead of the requested JSON object (most often on
a route that does not enforce the schema). With `--format-retry` (the default) `c3` then
spends ONE recorded repair turn on the same thread when the prose is substantive; when
the repair was not attempted or failed, the prose is kept with no verdict and no
findings, which is not a bridge failure. Only then re-ask once, explicitly asking it to
"return the JSON object", if you need this consultation's findings tracked.

Then record the outcome with the findings tool, not by paraphrasing it into a summary:

```
c3 findings --task <task> --id F04-1 --status implemented|verified|rejected|wontfix|superseded --note "<why>" --evidence "<what you ran / where the proof is>"
```

`verified` requires `--evidence` — what you ran and what it showed, not just that you
believe it; `rejected` and a reopen (`--status proposed` on a non-`proposed` finding)
require `--note`; `superseded` requires neither. A reviewer reporting a finding "fixed"
in a later reply is evidence you can cite, never a status change by itself: the
coordinator still moves the status.

`c3 findings --task <task> --stats` prints effort, wall time, tokens and finding counts
per consultation — the measurement of what each review purpose actually cost and
produced.

`--id`/`--status` holds the same task lock as a running consultation and is refused
while one is in progress for that task. `--list` and `--stats` only read and never take
the lock.

## 4. Rate the consultation

After recording the findings (or confirming a prose reply raised none), mark whether it
was actually useful:

```
c3 findings --task <task> --rate <n> --useful yes|partly|no --note "<why, required for no>"
```

`<n>` is the ledger entry number of that consultation, not a finding id. Rate **every**
consultation, including a plain-prose reply that produced no ids — skipping those means
the telemetry only ever counts structured reviewers, which biases the scoreboard toward
whoever happens to answer in JSON.

```
c3 scoreboard
```

sums these marks per reviewer and purpose (add `--task <task>` for one task, `--json`
for row objects). A **failed** consultation may be rated too: `--useful no` only when the
failure was the reviewer's (a refusal, an invented finding, a reply it could not put in
the format); **skip the rating** when a timeout or a plan limit killed it — resume it
instead.

## Role split: the operator's council

Treat every reviewer in play — Claude (this coordinator) and the reviewer — as a council
with these rules, not as a primary reviewer plus optional extras:

- **The coordinator is an equal participant, not a rubber stamp**, and is the judge by
  default: its own ideas are on the table alongside the reviewer's. For a genuinely hard
  question, the judge role can be handed to the reviewer explicitly — a
  `--purpose decision` consultation whose brief references prior reply files by path and
  asks for a verdict; record that verdict in `state.md` as the decision, but the
  coordinator still executes and verifies it.
- **A provider without a credential or without tokens left is simply not used** —
  `c3 providers` records why; do not plan work on it until it reports available again.
- **Disagreement is the signal, not noise.** Record it in `state.md` as an open item
  until evidence settles who was right; `c3 findings --stats`'s per-consultation record
  shows, over time, who actually finds what.
- **Before picking a reviewer for a hard question, check the scoreboard.** Run
  `c3 scoreboard` to see which reviewer has actually been useful on that purpose so far,
  not just who is cheapest or fastest.
- **Chores go to cheap reviewers.** Hand bounded search/extraction work to a cheap
  `--provider` with `--purpose chore`, and pre-digest large inputs before a weighty
  brief — put the extract in the brief, not the raw file.
- **Acceptance authority for a release stays with the reviewer who raised the
  findings.** A stand-in's ACCEPT is recorded, but the tag waits for the one who raised
  them.

When a fresh-context verifier of the same model family (Claude, in the mechanics role)
is also reviewing, that split is a further, orthogonal division of labor, not exclusive
responsibilities — either may challenge anything the other says:

- **The verifier** is best used for mechanics: lint, interpreter/version compatibility,
  build correctness, test wiring, whether the code does what a summary claims it does.
- **The reviewer** (Codex or another wired provider) is best used for protocol and
  state-machine correctness, and for naming what the evidence does not show.

## The panel (not yet implemented)

The `codex-consult` plugin this port follows can send one brief to several roster
reviewers at once (`-Panel`), routed, seated by track record, with lab diversity and
required reviewers. `c3` parses `--panel`/`--panel-all`/`--panel-concurrency` but refuses
every panel run today (milestone 4) — there is no roster walk, no routing, no parallel
launch behind those flags yet. Until then, run reviewers one at a time with
`--provider`/`--model` and compare their replies yourself; do not tell the coordinator
that a panel command will work.

## Invariants

1. **Read-only by default.** Use `--sandbox workspace-write` only when the user has
   explicitly made the reviewer the author of a stage. `danger-full-access` is refused
   and there is no flag to force it.
2. **Claude keeps the final word.** A consultation is evidence, not an instruction.
3. **Project isolation.** Everything is scoped to the git repository you run from: the
   ledger lives under `<repo>/<collab-dir>/<task>/`, the parent thread for
   `fork`/`resume` comes only from *that* repository's `sessions.json`. Never pass
   `--thread <uuid>` taken from another project's ledger, and never point a brief at
   files outside the repository.
4. **One reviewer thread belongs to one task directory.** Not enforced by `c3` —
   resuming the same thread from two different task directories is on you to avoid.
5. **A verdict is not an outcome.** The bridge succeeding means a reply came back and
   was written to disk. The verdict says what the reviewer decided. A delivered HOLD is
   a success of the bridge: the run worked exactly as intended.
6. **Never use a provider without a credential or without tokens left.** `c3 providers`
   exists so this is checked automatically, not assumed; a provider it reports
   unavailable is not planned on.
7. **Judge delegation is explicit.** The coordinator is the judge by default; handing
   that role to the reviewer for a hard question is a deliberate `--purpose decision`
   consultation, recorded in `state.md` — never an implicit "whoever answered ACCEPT
   wins".
