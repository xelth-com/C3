# `c3 consult` — full reference

The `consult` skill points here for the exhaustive surface. `c3 consult --help` is the
authoritative, current list; this file explains what each part is *for*. Everything below
is implemented in today's binary (built from commit `4e8d1a8`).

## Purposes, effort and word caps

`--purpose` selects the paragraph the reviewer answers under, and its default effort,
word cap and timeout:

| `--purpose` | Effort | Max words | Default timeout (s) |
|---|---|---|---|
| *(none)* | high | 700 | 900 |
| `framing` | high | 700 | 1800 |
| `decision` | high | 700 | 1800 |
| `checkpoint` | medium | 500 | 900 |
| `core-contract` | xhigh | 900 | 2400 |
| `acceptance` | high | 900 | 3600 |
| `diff-review` | high | 700 | 2400 |
| `stuck` | xhigh | 700 | 2400 |
| `chore` | low | 400 | 600 |

`chore` is a bounded search/extraction task — a plain-text reply like `--raw` (no schema,
no findings bookkeeping) for grunt work that should not cost a weighty reviewer's tokens;
hand it to a cheap roster member. `--effort` and `--max-words` override the preset.

## Flag table

| Flag | Meaning |
|---|---|
| `--task <slug>` | mandatory; groups one conversation |
| `--collab-dir <path>` | default `.collab` |
| `--mode new\|fork\|resume` | empty = auto: fork the newest thread of this lineage, else new |
| `--thread <uuid>` | pick a specific parent; needs `--mode fork` or `resume` |
| `--brief <path>` | the brief file; `c3` never writes briefs |
| `--prompt <text>` | the one-line ask (points at the brief; do not paste the brief here) |
| `--model <id>` | omit and the model comes from the roster entry, else the Codex config top level |
| `--purpose <name>` | see table above |
| `--effort low\|medium\|high\|xhigh` | overrides the purpose preset |
| `--native-effort <value>` | an effort value sent verbatim; excludes `--effort` (endpoints with no known vocabulary) |
| `--sandbox read-only\|workspace-write` | default `read-only`; `danger-full-access` is refused |
| `--max-words <n>` | 0 = purpose preset |
| `--timeout-sec <n>` | 0 = purpose default |
| `--continue-sec <n>` | -1 = min(timeout, 900); 0 = no continuation turn |
| `--stall-sec <n>` | stall cut without an event; 0 = off, -1 = roster entry's `stall_sec` else 900 |
| `--range <base..head>` | diff-review/acceptance only; a single revision is refused |
| `--reply-name <slug>` | the handoff base name; default `reply` |
| `--artifact <path>` | repeatable; hash a built artifact into the ledger so the review is bound to it |
| `--raw` | plain-text reply: no schema, no findings bookkeeping |
| `--codex-exe <path>` | env override `CODEX_CONSULT_EXE` |
| `--provider <name>` | a `[model_providers.<name>]` entry, or a roster label; needs `--model` unless a roster entry supplies one |
| `--off-peak-only` | refuse (not warn) inside the provider's peak window |
| `--skip-preflight` | bypass the preflight — only for an endpoint that needs no credential, or once after a rotation; never to push past a real refusal |
| `--codex-config k=v` | repeatable; identity/effort keys (`model`, `model_provider`, `model_reasoning_effort`, `profile`, `model_providers.*`) are refused |
| `--schema-transport output-schema\|prompt-only` | empty = the caps-v1 default for the endpoint |
| `--format-retry 0\|1` | default 1: one recorded repair turn when the reply is not valid JSON |
| `--telemetry on\|off` | default on; `CODEX_CONSULT_TELEMETRY=off` also disables |
| `--dry-run` | print the plan and exit; writes nothing, makes no network call |

Lineage rule: **a second model is consulted in its own lineage; never fork/resume across
providers.** `c3` refuses a `--thread` or automatic parent whose lineage does not match the
current `--provider`/`--model`.

`c3 consult` creates `handoffs/` and `sessions.json` when missing, and writes:
`handoffs/<NN>-<engine>-<slug>.md` (verbatim reply + rendered findings/verdict, unless
`--raw`), `.reply.json` (raw structured reply, structured mode), `.events.jsonl` (event
stream), appends findings to `findings.json` (structured mode, when there is at least one),
and one `sessions.json` ledger entry (the commit point). A failed run is still recorded as an
entry; a refusal exits non-zero with a `c3: <message>` line and writes nothing.

## Engines (`--engine`)

Omitted, the engine comes from the roster entry, else `codex`. `--engine-exe <path>` names
a non-codex launcher (env `CODEX_CONSULT_AGY_EXE` / `CODEX_CONSULT_MUSE_EXE`).

- **`codex`** (default) — `codex exec` against the ChatGPT plan or any `[model_providers.<name>]`
  table (z.ai GLM, Xiaomi MiMo, any Responses-API provider).
- **`agy`** — Gemini through Google's Antigravity CLI. `--provider` is a free label (e.g.
  `gemini`), `--model` the full id with its tier (`gemini-3.8-flash-high`); no effort is sent,
  default mode `new`; `fork`, `--sandbox workspace-write`, `--codex-config` and
  `--schema-transport output-schema` are refused. Files `handoffs/<NN>-agy-<slug>.*`.
  `--denial-retry 0|1` (default 1): one more turn when a tool was auto-denied and the turn
  produced nothing.
- **`muse`** — Meta's Muse Code CLI (Muse Code subscription; `--model muse-spark-1.3`). Effort
  goes as the reasoning effort, default mode `new`, files `handoffs/<NN>-muse-<slug>.*`,
  `--max-model-steps <n>` caps its model steps. Refused while `META_API_KEY`/`MODEL_API_KEY`
  is set (would bill per token) and while no oauth sign-in is established — the user signs in
  themselves (`muse login`); never read `~/.config/muse/auth.json`.
- **`http`** — an OpenAI-compatible `chat/completions` request built from a reviewer pack; no
  CLI, no subscription, no tools (OpenRouter by default). Wired via `setup-providers` section
  4b (`ext.c3.reviewers`). The key is read from an environment variable the user sets, bound to
  the one host it belongs to; a custom endpoint needs `C3_KEY_<HOST>`. See `setup-providers`.

**Live-member / read-only-by-evidence rule.** An `agy` or `muse` run FAILS (agy) or WARNS
(muse) if the working tree or the collab directory changed during it — do not edit the
repository or run another consultation here while one runs. `http` is write-disabled and only
warns. This binds workers too (see the `coordinate` skill).

A reviewer killed on its timeout gets ONE continuation turn on its own thread
(`--continue-sec`); its salvaged partial is `handoffs/<NN>-<engine>-<slug>.partial.md` and the
summary prints the exact `--mode resume` command. **A timed-out reviewer is resumed, not
re-asked from scratch.** Skip a *rating* when a timeout or plan limit killed it; resume instead.

## The panel

`--panel` sends the **same brief to as many roster reviewers as the purpose needs** (chore/none
/checkpoint 1, diff-review 2, framing/decision 3, core-contract/acceptance 4, stuck every
eligible one; `--panel-size <n>` overrides). Needs a roster; refused with `--provider`,
`--thread` or `--mode resume`.

- `--panel-all` — include `"weighty"` roster entries whatever the purpose (forces size 0).
- `--panel-order routed` (default, a seeded weighted draw by track record) or `roster`.
- `--panel-seed <nonce>` — pins the draw so a dry run and the real run seat the same members.
- `--panel-concurrency <k>` — 0 = no cap, 1 = strictly sequential, k = at most k at once.
- Each member is a complete, independent consultation: its own preflight, lineage and reply
  file. Members see the same pre-panel open-findings snapshot. A failing member does not stop
  the rest (at the default concurrency). The run exits 0 only when every member produced a
  usable reply.

`--require <matcher>` (repeatable) — reviewers that must take part: `#<n>`, a provider label,
or `<provider> :: <model> [engine]`. One that is out refuses the run before anything starts;
`--require none` alone drops the roster's requirement (only after the operator agreed).

`--role <name>` (one role for every member) / `--roles <a> --roles <b>` (roles by score rank):
`edge-cases`, `security`, `tests`, `docs` ship as `templates/role-*.md`; a repo adds its own as
`<collab>/roles/<name>.md`. The role narrows what the reviewer looks at; the format and verdict
rules stay.

`--topic <slug>` (repeatable) — what the consultation is about; recorded on the ledger and its
rating, and a routed panel scores its members on those topics.

## Parking a run (detach) and member control

A panel acceptance runs 15–60 minutes. Add `--detach` and carry on.

- `--detach` — run in the background, return at once. It is checked first: a refusal (missing
  brief, no available reviewer, a live consultation of the task) comes back immediately, exit 1,
  nothing written. Success prints the detach id, the status file and the come-back commands.
- `--status [--id <id>]` — read status files only, never blocks: exit 0 = done and every member
  usable, 1 = done with a failure (or the background died), 2 = still running. An ambiguous id
  prefix is refused. `--status --prune` deletes status files/logs of runs done/died > 7 days.
- `--wait [--id <id>] [--wait-timeout-sec <s>]` — block until done; without a timeout, up to the
  run's own budget. Use a value below your tool's command timeout.
- `--kick --member <NN> [--id <id>]` — stop ONE running member by its handoff number; its
  partial is salvaged, it is recorded `failed: stopped by the operator`, and the panel goes on.
  Exit 0 once acknowledged, 3 if not within 10 s. Ask the operator before kicking a merely-slow
  member.

Record the detach id, the brief path and the come-back command in the task's `state.md`. While
a detached run is live: never start a second consultation on the task, never edit the brief or
the files under review, and do not act on a reply you have not read.
