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
| `--prompt <text>` | the one-line ask (points at the brief; do not paste the brief here). The reviewer sees the repository and the staged task files only - never your knowledge base or memory: quote what it needs in the brief (a project may forbid a topic to go to a panel at all) |
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
| `--diff <range>` | hand the reviewer the change as a patch in the task's handoffs: `base..head`, `base...head`, or ONE revision diffed against the working tree. A one-revision diff carries the tracked changes only; the untracked files left out are counted and up to ten named before the reviewer starts (stderr, the handoff's warnings, the ledger's `untracked_left_out`) |
| `--diff-untracked` | with a one-revision `--diff`: carry every untracked, not ignored file in the patch too, with its content (only secret-shaped values are redacted); the run names them the same way. MCP: `diff_untracked` on `c3_consult` / `c3_panel` |
| `--diff-tracked-only` | still accepted: names the default (tracked changes only) |
| `--reply-name <slug>` | the handoff base name; default `reply` |
| `--artifact <path>` | repeatable; hash a built artifact into the ledger so the review is bound to it |
| `--raw` | plain-text reply: no schema, no findings bookkeeping |
| `--codex-exe <path>` | env override `CODEX_CONSULT_EXE` |
| `--provider <name>` | a `[model_providers.<name>]` entry, or a roster label; needs `--model` unless a roster entry supplies one |
| `--off-peak-only` | refuse (not warn) inside the provider's peak window |
| `--skip-preflight` | bypass the preflight — only for an endpoint that needs no credential, or once after a rotation; never to push past a real refusal |
| `--spend-reserve` | seat a PROTECTED reviewer even when its budget reserve would hold it back (single and panel); without it a held protected entry is kept for acceptance / core-contract |
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
- **`muse`** — Meta's Muse Code CLI (Muse Code subscription). Three models, in priority order:
  `muse-spark-1.3-contributor` is the default - coding work and keeping a line of thought,
  everyday reviews where a second opinion is all that is needed; `muse-spark-1.3` only when the
  contributor model is down or refused; `muse-wonder-1.0` ONLY for hard reviews and consilia
  (a weighty roster seat, or an explicit `--provider meta --model muse-wonder-1.0`) - the Muse
  allowance is shared, and the contributor's turns must survive. Effort
  goes as the reasoning effort, default mode `new`, files `handoffs/<NN>-muse-<slug>.*`,
  `--max-model-steps <n>` caps its model steps. Refused while `META_API_KEY`/`MODEL_API_KEY`
  is set (would bill per token) and while no oauth sign-in is established — the user signs in
  themselves (`muse login`); never read `~/.config/muse/auth.json`.
- **`grok`** — xAI's Grok Build CLI on the SuperGrok subscription, read-only (tool allowlist
  `read_file,list_dir,grep`, MCP gateways and write/shell tools removed, `--permission-mode dontAsk`,
  no web search). Models `grok-4.7` (default), `grok-4.7-build-fast`, `grok-4.6`, `grok-4.5`;
  effort goes as `--reasoning-effort`; the thread id is read from grok's `end` event and resumed
  with `--resume`; files `handoffs/<NN>-grok-<slug>.*`. The tree check only WARNS. `XAI_API_KEY` /
  `GROK_CODE_XAI_API_KEY` are stripped from the child (they would bill the API per token) unless
  `C3_GROK_ALLOW_API_KEY=1`; `c3 providers` names the billing route. Reviewer use only (no `c3 work`).
  **Read scope:** the read-only tools are NOT confined to the repository - grok can read any path
  its user account can, e.g. a sibling worktree (`../<repo>-wt-<branch>`) or `~/.cargo/registry`
  (observed live, 2026-11-15). That is by design: no restriction is applied, a review may follow a
  dependency or a branch checked out next door. Do not brief grok with anything outside the
  repository you would not show it, and do not take "it read only the repository" for granted.
  The reply schema travels in the prompt (`prompt-only`, the default since 0.2.4;
  `--schema-transport native` passes `--json-schema` for one run): with the native flag Grok's
  opening remark came out as a schema-shaped placeholder answer that ended the turn before any file
  was read.
  On EVERY non-raw purpose a primary grok turn that answers with zero tool calls read no file, so
  rather than failing at once the bridge INSISTS: it asks again in a NEW grok conversation (the
  whole original prompt with one insisting line in front; kimi resumes the same thread), up
  to `CODEX_CONSULT_READ_INSIST` times (default 2; `0` disables = fail at once). A re-ask that reads
  (tool calls > 0) makes its reply the primary reply (usable); if every re-ask still reads nothing
  the guard failure stands (class `capability`, no provider outage) with ` after N re-asks`. The
  handoff header records `Read insist: <n> turn(s), tool calls 0 -> <k>` and the ledger entry gets
  `read_insist: {attempts, final_tool_calls, new_conversation}`. A re-ask that read and then died
  (no final answer, non-zero exit, timeout) ends the run under its own failure, named
  `read-insist attempt N after a zero-tool primary; K tool calls`, with its stream as the events
  file and its salvaged text as the reply. The failure wording and re-ask text depend on the
  purpose: the four review purposes (`acceptance`, `core-contract`, `diff-review`, `checkpoint`) say
  "a <purpose> review must read the code" and the re-ask names `git diff`; every other purpose says
  "it cannot have read the brief" and the re-ask says to open the brief. The review prompt also
  carries one mandatory-tool-reading sentence (grok/kimi); every other non-raw grok prompt carries a
  "Reading duty" paragraph (open the brief, then the files it names). The brief is ALWAYS referenced
  by path for a non-raw grok turn, never inlined - inlined, grok answered from the brief alone and
  invented code and line citations (live test, grok-4.7); by reference it opens the brief and reads
  (5+ tool calls). `raw` grok consultations keep the inline and are exempt on non-review purposes.
- **`kimi`** — Moonshot's Kimi CLI (`kimi -p <pointer> --output-format stream-json`) on the Kimi
  Code plan (managed oauth). Models `kimi-code/k3` (default), `kimi-code/k3-256k`,
  `kimi-code/k3.1-pro`, `kimi-code/kimi-for-coding`, … (the ids kimi's `config.toml` declares). The
  full assembled prompt is written to a temp file; the argv carries only a short POINTER to it
  (`-p "Your brief is the file <abs>. Read it completely first …"`), which keeps a large review
  prompt off the Windows command line (refused over 30000 argv chars, class `capability`). The reply
  is the final assistant `content` line, the schema is PROMPT-ONLY (kimi has no schema flag), effort
  is the model's own `default_effort` (no flag to override), the session id (`session_<uuid>`) is
  read from the `session.resume_hint` line and resumed with `-S <id>`; files
  `handoffs/<NN>-kimi-<slug>.*`. **Write isolation:** kimi `-p` has NO tool-restriction flag
  (`--yolo`/`--auto`/`--plan` all refuse to combine with `--prompt`) and CAN write, so every kimi
  turn runs with its cwd set to a throwaway detached git worktree of HEAD (removed after the run; the
  handoff header's `Isolation:` line records the path and HEAD sha, and notes when the main tree was
  dirty at launch). The brief is copied into that worktree at its repo-relative path (an untracked
  brief of a new task too); a brief that is neither committed at HEAD nor copyable (a relative path
  that leaves the repository) refuses the seat BEFORE launch with `the kimi seat is not usable: the
  brief ...` - a preflight-class refusal, so a panel re-seats it from its reserve. When the live tree
  is dirty, the prompt itself names the uncommitted paths (at most 20, then `(+N more)`) the reviewer
  cannot see as they are. The strict tree check on the MAIN tree stays as the second guard: any
  working-tree or collab change there FAILS the run (stricter than grok/muse, which only warn). The
  same zero-tool-call review guard as grok applies on the reviewing purposes, including the
  read-insist re-ask (`CODEX_CONSULT_READ_INSIST`) before the guard fails. Reviewer use only (no
  `c3 work`).
- **`http`** — an OpenAI-compatible `chat/completions` request built from a reviewer pack; no
  CLI, no subscription, no tools (OpenRouter by default). Wired via `setup-providers` section
  4b (`ext.c3.reviewers`). The key is read from an environment variable the user sets, bound to
  the one host it belongs to; a custom endpoint needs `C3_KEY_<HOST>`. See `setup-providers`.

**Live-member / read-only-by-evidence rule.** An `agy` or `muse` run FAILS (agy) or WARNS
(muse) if the working tree or the collab directory changed during it — do not edit the
repository or run another consultation here while one runs. `http` is write-disabled and only
warns. This binds workers too (see the `coordinate` skill). agy fails only on evidence from its
own tool log: a tool step that names a path, file, directory or command and is not a known
reader (`read_file`, `view_file`, ...), or one whose parameters were not logged. Its `finish` step
(the reply hand-over) and a tool given no path or command are not writes. When agy's reply is
complete and schema-valid, a change its log does not explain is a WARNING (`... - reply kept`):
the ledger entry and the scoreboard mark are written. `c3 providers` shows, per reviewer over 14
days, `usable / failed with a reply / failed without a reply` (`--json`: `reply_outcomes`), so a
reviewer whose "failures" kept a reply is visible before it is dropped from the roster.

A reviewer killed on its timeout gets ONE continuation turn on its own thread
(`--continue-sec`); its salvaged partial is `handoffs/<NN>-<engine>-<slug>.partial.md` and the
summary prints the exact `--mode resume` command. **A timed-out reviewer is resumed, not
re-asked from scratch.** Skip a *rating* when a timeout or plan limit killed it; resume instead.

## The panel

`--panel` sends the **same brief to as many roster reviewers as the purpose needs** (chore/none
/checkpoint 1, diff-review 2, framing/decision 3, core-contract/acceptance 4, stuck every
eligible one; `--panel-size <n>` overrides). Needs a roster; refused with `--provider`,
`--thread` or `--mode resume`.

- **One seat per lab.** A panel seats at most one roster entry per lab — the model's maker, from
  the entry's `"lab"` or its model id's prefix (`glm` zhipu, `qwen` alibaba, `deepseek`, `kimi`/`k3`
  moonshot, `mimo` xiaomi, `gemini` google, `muse` meta, `grok` xai, `gpt`/`o<n>` openai,
  `claude` anthropic, `dola`/`seed` bytedance), NOT the provider label: `kimi` and `kimi-claude`
  are one lab, Qwen and DeepSeek behind `alibaba-claude` are two. Each lab is represented by its
  best-weighted eligible entry (the first in roster order without ratings), then the draw runs
  among labs. A sibling shows `not picked: lab zhipu already seated by #3`; a size above the lab
  count is cut to one seat per lab with a warning. Every `--require`d entry is seated (two of one
  lab with a warning); `--panel-all` seats every eligible entry and warns once; the bare default's
  single pick is unchanged. `c3 router explain` marks an entry its lab's pick keeps out.
- **The upper tier on weighty questions.** On framing / decision / core-contract / acceptance /
  stuck, a panel of two or more seats gives one seat to a `"panel": "weighty"` entry when one is
  eligible (available, not held, not on-request) and no required entry already is one (rule
  `tier-draw`, or `tier` in roster order).
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

## Protected and on-request reviewers (roster `ext`)

Two per-entry settings live under a roster entry's `ext` object (invisible to the plugin, which
treats `ext` as a free-form object):

- `"ext": {"reserve_percent": N}` (integer 1..50) — a **protected** entry. On every LONG budget
  window (24 h or more — weekly, monthly; the 5-hour window is exempt), non-stale, reset still ahead,
  the entry is HELD when `used% >= min(elapsed% + N, 100 - N)`: it may not run more than N points
  ahead of the calendar, and its last N % is kept back. The hold is waived on the standing purposes
  **acceptance** and **core-contract**, and by `--spend-reserve`. A held entry is skipped by the bare
  default and by a panel (skip kind `reserve`); an explicit `--provider`/`--model` naming it is
  refused in preflight; a `--require` naming it refuses the run (exit 5). `c3 providers` adds the
  hold to the entry's row (VERDICT stays `available`) and `c3 router explain` marks it `[held:
  reserve]`.
- `"ext": {"on_request": true}` — an **on-request** entry. It is never seated automatically: not by
  the bare default, not by any panel draw, not by `--panel-all`. It runs only when named exactly —
  `--provider <label> --model <its model>`, or a `--require` matcher that names the entry itself
  (`#<n>` or `<provider> :: <model> [engine]`). In listings it shows as skipped with kind `request`
  ("on request only"); `c3 router explain` marks it `[on request]`.

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
