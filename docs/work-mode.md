# `c3 work` — a worker run (design v1, 2026-10-20)

Operator decision (2026-10-20): besides reviews and panels, `c3` must be able to run a roster
model as an ordinary CODING WORKER for low-stakes or test work: the model reads and edits the
repository, runs the test command it is allowed, and reports. First workers: GLM-5.3,
GLM-5.3-flash (z.ai endpoint), DeepSeek-4.1-flash (byteplus endpoint) — all through the `claude`
engine, which already drives them for reviews. Nothing about reviews changes.

## Command

```
c3 work --task <slug> --brief <file> --provider <roster label> [--model <id>]
        [--prompt <text>] [--effort low|medium|high|xhigh] [--max-turns <n>=80]
        [--timeout-sec <s>=3600] [--allow <tool pattern>]... [--collab-dir <dir>=.collab]
        [--detach] [--status [--id <id>]] [--wait [--id <id>] [--wait-timeout-sec <s>]]
        [--max-resumes <n>=3] [--quota-window-sec <s>] [--unrestricted]
```

- `--provider` names a roster entry with `"engine": "claude"` and `"roles"` containing
  `"worker"` (the EXISTING `roles` key — no roster schema change). `--model` overrides the
  entry's model id (same endpoint). Any other entry is refused with the list of worker entries;
  an entry with another engine is refused in v1 ("workers run on the claude engine only").
- `--allow` adds Claude Code `--allowedTools` patterns (repeatable), e.g.
  `--allow "Bash(cargo test:*)"`. Default allow list: `Bash(cargo test:*)`, `Bash(cargo check:*)`,
  `Bash(cargo clippy:*)`, `Bash(cargo fmt:*)`. `Bash(git:*)` is REFUSED on the command line: a
  worker never commits.
- One live work run per task; a second `c3 work` on the same task is refused while one runs.
- The prompt on stdin = a fixed worker preamble + the brief file + `--prompt`. Preamble (verbatim
  in the code, tested):
  > You are a coding worker inside this repository. Do only what the brief asks, touch only the
  > files it names or clearly implies, never run git, never leave the repository, and never
  > commit. Finish with a final report: what you changed (file list), the exact test commands you
  > ran and their result counts, any deviation from the brief and why, and what you could not do.
  The prompt is refused above 1 MiB (the engine's `MAX_PROMPT_BYTES`).

## The child

Argv (built by a pure function with a unit test, like `claude_args`):

```
claude -p --output-format stream-json --verbose --strict-mcp-config --disable-slash-commands
       --permission-mode acceptEdits --tools Read,Edit,Write,Grep,Glob,Bash
       --allowedTools <allow list joined by ','> --model <id> [--effort <e>]
       --max-turns <n> --session-id <uuid>
```

No `--restricted` (it would remove the write tools). Environment: exactly the endpoint route the
review engine uses — `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` (the value of the entry's
`env_key`, read at launch, never logged), `API_TIMEOUT_MS`; host markers scrubbed
(`c3_core::host::host_marker_names()`, recorded by NAME in `run.json` as `child_env_scrubbed`);
`CLAUDE_CONFIG_DIR` and `DISABLE_AUTOUPDATER=1` as for reviews. Reuse the engine's launcher
discovery (`lineage::launcher_names` / `install_launchers`) and the endpoint environment builder
in `engines/claude_auth.rs` rather than writing a second one. The prompt travels on stdin.

## The record

`<collab-dir>/<task>/work/<NN>-<provider>-<model>/` (NN = 2-digit sequence per task):

- `brief.md` — the prompt as sent (preamble + brief + `--prompt`).
- `events.jsonl` — the child's raw stream, **retained verbatim**, and may contain whatever the
  child printed (including a credential the child itself emitted), exactly like a consult's events
  file. It is the one record artifact that is NOT sanitized.
- **Writer contract (0.2.4, F93-1):** `run.json` is written only by its owning process (the c3
  process that claimed the record, writing its own live record) or under the task admission lock
  after a re-read; a reader never writes. The status/`--wait`/resume readers persist a dead owner's
  `died` transition only through the locked recovery (`recover_dead_run`), which re-reads and
  refuses to overwrite a newer generation.
- **Secret contract (0.2.4, F89-5):** `run.json`, `report.md` and `diff.txt` pass the project's
  single sanitizer (the pack redactor — 12 secret kinds plus an entropy pass); secret-shaped
  values are redacted with `[REDACTED:<kind>]`, the same rules as a pack. `events.jsonl` is the
  child's raw stream and is kept as-is. (The usage allow list and the argv grammar remove the most
  likely leak paths; the sanitizer is the catch-all for the report text.)
- `report.md` — the final assistant text (the `result` event's text; the last assistant text if
  the run died).
- `diff.txt` — `git status --short` before the run (the baseline) and after, plus
  `git diff --stat` after; computed by c3, not by the worker.
- `run.json` — `{task, seq, provider, model, engine:"claude", argv (no secrets — every `--allow`
  value is validated by the grammar below, so no free text travels there), child_env_scrubbed,
  started, ended, exit_code, status, pid + pid_start (the owning process — see Liveness), attempt,
  max_resumes, quota_window_sec, unrestricted, permission, turns, tool_calls: {Read, Edit, Write,
  Bash, ...}, files_changed: [...], usage, error}`. `status` is one of `done | died | timeout |
  refused | quota | running | waiting_quota | artifact_error`. `usage` is **rebuilt from an allow
  list of numeric counters** (`input_tokens`, `output_tokens`, `cache_creation_input_tokens`,
  `cache_read_input_tokens`, `total_cost_usd`, `duration_ms`, `num_turns`, and the codex/muse token
  equivalents) — never a cloned usage object, so a sentinel or secret in an extra field never
  reaches the record. `error` carries the terminal error text when the run ended `died` (e.g.
  `no terminal event`) or `artifact_error`.

### Liveness (0.2.4, F89-1)

Every `running`/`waiting_quota` record carries the OWNING process's `pid` and `pid_start` (its
start time, the anchor against pid reuse): a foreground run writes its OWN pid before it spawns the
child; a detached run's background writes its pid. A run is live only while that pid with that
start time is alive. The start grace (120 s) applies ONLY to a record that has no pid yet. So a
foreground run that lasts longer than the grace stays `running` for `--status` and still makes a
second same-task launch refuse.

### Success (0.2.4, F89-8 / F91-4)

`done` requires BOTH process exit 0 AND AFFIRMATIVE successful terminal evidence from the engine,
kept separate from message extraction: claude a `result` event with `is_error` present AND false
(and `subtype == "success"` when the stream carries a subtype) — a result missing `is_error` is
NOT success; codex the successful turn terminal `turn.completed` — an `agent_message` item alone is
NOT success; muse its explicit `completed` terminal. An error terminal or a rejecting rate-limit
takes precedence over exit 0 (→ `died`/`quota`); an empty or incomplete stream with exit 0 is
`died` with reason `no terminal event`. Exit 0 alone never establishes success.

### Artifact durability (0.2.4, F89-3 / F91-3)

Artifact write AND read failures are never swallowed: the run becomes `artifact_error` (exit
non-zero) with the error text in `run.json`, never `done`. On a resume the attempt's per-run temp
files (`.events-resume-<n>.jsonl`, never a fixed name) are preserved until their bytes were read
AND durably appended to `events.jsonl` (flushed), then deleted. The aggregate stream is read
strictly: `refresh_artifacts` never replaces `report.md` when `events.jsonl` cannot be read (a
transient read failure is an artifact error, not an empty report); only a genuinely absent file is
an empty stream.

### Admission (0.2.4, F89-2 / F91-1 / F91-2)

One task-level admission lock covers starts, resumes and the detached alarm. Under the lock the
claimer re-reads the record and validates EVERYTHING against that locked read — the permitted
status (no record for a start; `quota`/`timeout`/`waiting_quota` for a resume), exhaustion, owner
liveness, and the expected attempt generation (the resume count must be unchanged) — then computes
the next attempt number from the locked record and writes `running` + new pid/start + attempt
durably BEFORE spawning. A stale claim (the record went `done`/`exhausted` or advanced by a
concurrent resume since the caller's unlocked read) refuses before any launch. Two concurrent
`--resume` of one record: exactly one spawns.

Admission **fails closed on inspection errors** (F91-2): `run_dirs`, `read_record`, `live_run` and
`other_live_run` return errors; only a confirmed `NotFound` is absence. A directory with the `NN-`
prefix whose `run.json` exists in any form (including a replaced or permission-denied file) is a
candidate, so an unreadable record is never silently treated as an absent run — admission refuses,
naming the directory, never the contents.

No evidence check, no findings, no rating prompt: a work run is not a consultation. It does not
touch the review ledger; `c3 consult --status` does not list it.

## Detach / status / wait

Same shape as the consult flags: `--detach` starts the run in the background and prints the
work id, the record directory and the come-back commands; `--status` prints every work run of
the task (id, provider/model, status, started/ended, files changed) — exit 0 all done, 1 a run
died/timed out, 2 running; `--wait` blocks until done (bounded by `--wait-timeout-sec`, below
the host tool's limit) then prints as `--status`. Reuse `consult/detach.rs` if its machinery
fits without a `DetachArgs` explosion; otherwise a small `work/detach.rs` with a `run.json`
state field is acceptable in v1 — say which in the report.

## Quota, resume and the alarm (0.2.3)

A worker that runs out of its provider's usage limit is not a failure: it is paused and resumed
when the window reopens. The operator's wording: "we leave GLM writing code, it runs out of its
limits, they come back when the 5-hour cycle refreshes — at that moment nudge it so it starts
again."

- **Classification.** After the child ends, a rejecting `rate_limit_event`, or a result/stderr
  matching the quota wording, makes the status `quota` (not `died`). The detection is the review
  engine's own `quota_evidence` (`engines/claude.rs`), so a worker and a reviewer read one wording
  and one regex. `run.json` gains `quota: {text, reset_at: <RFC3339 utc or null>, resume_at:
  <RFC3339 utc>, resumes: <n>, exhausted: <bool>}`. `resume_at` = `reset_at` + 60 s when the
  provider gave a reset time, else now + `--quota-window-sec` (default 18000 = 5 h; a roster
  entry's `ext.quota_window_sec` overrides the default).
- **`--resume [<id>]`** (same `--task`): the run must be `quota` or `timeout`. c3 relaunches the
  child with the same argv but `claude --resume <session-id>` instead of `--session-id`, the prompt
  on stdin "Your usage limit has reset. Continue the brief from where you stopped; do not redo
  finished steps; finish with the final report." (the brief is NOT re-sent — the session has it).
  The attempt is appended to `events.jsonl` after a `{"type":"c3","subtype":"resume","n":<k>,
  "at":<utc>}` separator; `report.md`/`diff.txt`/`run.json` are recomputed over the whole record;
  `quota.resumes` increments. Resumes are capped by `--max-resumes` (default 3); past the cap the
  status stays `quota` with `exhausted: true`. The effective `max_resumes` and `quota_window_sec`
  are persisted in `run.json` at the first run and reused by a later plain `--resume` (an explicit
  flag still overrides), so following the printed `--resume` line never silently resets a custom
  cap (0.2.4, F89-6).
- **The alarm (detached runs).** The background does NOT exit on quota. It writes status
  `waiting_quota` with `quota.resume_at`, sleeps in 30 s slices until then (checking a kick file
  `<record dir>/.kick` each slice), then resumes in-process; it repeats on a further quota hit up
  to the cap. The liveness check treats `waiting_quota` like `running` (the background pid is
  alive), so `--status` reads it as running (exit 2) and the one-live-run guard still holds.
- **`--kick [--id <id>]`.** On a `waiting_quota` run it creates the kick file (the sleeper resumes
  at once). On a foreground `quota` run (no sleeper) it resumes directly — so `--kick` is a
  one-word "try now" in both cases. On any other run it refuses with the state.
- **Foreground runs** on quota print the `resume_at` time and the exact `c3 work --task <t>
  --resume <id>` line, exit code 3. `--status`: `waiting_quota until <local time> (resume
  <n>/<max>)` plus the `quota` line; exit codes — `waiting_quota` counts as running (2), a `quota`
  run (exhausted or not) needs a hand (1). `--wait` keeps waiting through `waiting_quota`.
- **Secrets.** Nothing new is logged; the resume reuses the first launch's endpoint environment.

## Guards (refusals before anything starts)

- missing `--task`/`--brief`/`--provider`; brief unreadable; prompt > 1 MiB;
- the entry is not a worker (roles) or not engine claude; its `env_key` variable is unset;
- the launcher is not found; `--allow` contains `git`;
- a live work run exists for the task.
- (0.2.4, tests role) `--role` other than `tests`; `--of` without `--role tests`; a `--scope` that
  is not a valid glob; `--role tests` on the muse worker; `--of` naming a run that is not done or
  has no `patch.diff`; a tests run whose change is empty.

## Cross-model tests (0.2.4, consilium 2026-10-29)

Operator decision: the coordinator (Claude) authors the code; a roster worker of another family
writes or completes the tests. Two modes, the same mechanism: the tester sees the change and the
existing tests and writes only what is missing; or a cheap worker writes first and the
coordinator completes. The coordinator arbitrates a failing test ("code bug" or "test bug");
c3 records, never decides.

- `patch.diff` (every run): for each `files_changed` path outside the collab directory, the
  unified diff from its start content - the stored blob of a pre-dirty path (`baseline_hashes`
  now writes it with `hash-object -w`; a loose object, no commit, no ref), else HEAD's blob, else
  nothing - to its content now. Hunks are `git diff <blob> <blob>`; the header names the path.
  An untracked directory (`?? dir/`) expands to its files. Sanitized like `diff.txt`.
- `--role tests [--of <id>]`: the prompt is the work preamble, the tests preamble, the change
  between `<author-change>` tags (sanitized), then the brief. Without `--of` the change is the
  working tree against HEAD (a staged rename includes the old path's deletion); with `--of` the
  done run's `patch.diff`. `run.json` records `role`, `of` (the resolved id) and `scope`.
- `--scope <glob>` (any run; a tests run adds `tests/**`, `**/tests/**`, `**/tests.rs`,
  `**/*_test.rs`, `**/*_tests.rs`): after the run, `files_changed` outside every glob and outside
  the collab directory go to `scope_violations`; the run prints `OUT OF SCOPE` and exits 1 - on
  the first launch and on a resumed or detached run alike. Inline `#[cfg(test)]` modules live in
  source files, so a tests run names those files in `--scope`; a path scope cannot tell a test
  hunk from a code hunk there (accepted; the coordinator reads `patch.diff`). `--status` prints
  the violations but keeps its status exit code.
- Not in v1 (by the consilium): enforcing a different family (warning only, on the author's own
  provider), a fail-before/pass-after validity gate (manual for now), a `--continue` of a done
  author session (the coordinator is the author).

## Tests (harness: `cargo test && cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`)

- `work_args` unit test: the exact argv above for a sample entry, `--allow` joined, `--effort`
  present/absent, `--max-turns` default 80.
- roster selection: a worker entry is found by label; a non-worker label is refused with the
  worker list; a non-claude engine is refused; `--model` override keeps the endpoint.
- a fake launcher (cmd stub on Windows, sh elsewhere, like the existing `claude_auth` stubs)
  that echoes three stream-json lines (init, an assistant text, a result) and writes one file in
  the repo: the record directory gets all five files, `diff.txt` shows the file, `run.json` has
  `status: done`, `files_changed` = that file, `tool_calls` counted from the events.
- the preamble is tested verbatim (it is a contract with the worker).
- `--status` exit codes 0/1/2 from synthetic `run.json` files.

## Docs

README: a "Worker runs" section (what it is for, the three first workers, the refusals);
CHANGELOG: "0.2.3-dev: `c3 work`"; the plugin skill `coordinate` gets one paragraph (a worker
run is "running work" for the wait rule; the live-member rule does NOT apply to it — a worker
is expected to change the tree).

## Roster (the operator's file, edited by the coordinator, not by c3)

Add `"roles": ["worker"]` to `ZAI-claude glm-5.3`, add an entry `ZAI-claude glm-5.3-flash`
(same endpoint, `roles: ["worker"]`), add `"roles": ["worker"]` to `byteplus-claude
deepseek-v4-1-flash`.

## v1.1 engines (2026-10-21)

A work run may drive the `codex` or `muse` engine beside `claude`. `select_worker` accepts a
worker entry (role `"worker"`) on any of the three; the refusal names all three. The record
layout, the detach/status/wait lifecycle and the quota alarm are unchanged; only the child argv,
the prompt channel, the resume form and the write mode differ. The seam is `work/engines.rs`
(`enum WorkerEngine { Claude, Codex, Muse }` with per-engine argv / prompt-delivery / summary /
quota / session functions); each engine reuses the review adapter's own stream parsers
(`engines/claude.rs`, `engines/codex.rs`, `engines/muse.rs`), so a worker and a reviewer read one
event stream the same way. `run.json` gains the engine in its `engine` field and the `--status`
line shows a `[codex]`/`[muse]` suffix like the review lineage.

| engine | child argv (first launch) | prompt channel | resume | write mode |
|--------|---------------------------|----------------|--------|------------|
| `claude` | `claude -p … --permission-mode acceptEdits --tools Read,Edit,Write,Grep,Glob,Bash --allowedTools <list> --model <id> [--effort] --max-turns <n> --session-id <uuid>` | stdin | `claude --resume <uuid>` (same argv, `--session-id`→`--resume`) | the Claude Code write tools under the `--allow` list |
| `codex` | `codex exec --sandbox workspace-write --color never --json -C <repo> -m <model> [-c model_reasoning_effort="<e>"] -` | stdin (`-`) | `codex exec … resume <thread> -` (the thread parsed from the first run's stream) | the `workspace-write` sandbox — `--allow` does not apply |
| `muse` | `muse exec --json --prompt-file <brief.md> --model <id> [--reasoning-effort <e>] --workspace <repo> --approval-mode never --trust-workspace --disable-sandbox --session-id <uuid>` | `--prompt-file` | `muse exec … --session-id <uuid> --prompt-file <resume-prompt>` (same minted session) | writes + shell ON; the workspace is the write mode — `--allow` does not apply |

**`--allow` applies to claude only** (it maps to Claude Code `--allowedTools`); `run.json`'s
`permission` field states this. For codex the boundary is the real `--sandbox workspace-write`; for
muse it is the (unrestricted) `--disable-sandbox` workspace. The allow list does not apply to
codex or muse.

**The allow-list grammar (0.2.4, F89-4).** Without `--unrestricted`, every operator `--allow`
pattern must match `<Tool>`, `<Tool>(<exe>)` or `<Tool>(<exe>:*)` where `<exe>` is a plain
executable name `[A-Za-z0-9_.+-]+`, optionally followed by plain subcommand words of the same
alphabet separated by single spaces (`Bash(cargo test:*)`, `Bash(npm run lint:*)`; a word never
starts with `-`, so `Bash(python -c:*)` is refused; no `*`, no quotes, no shell operators), and the
executable is not one of
`git`, `sh`, `bash`, `zsh`, `pwsh`, `powershell`, `cmd`. `Bash(*)`, `*`, `Bash(python -c:*)` and
the like are refused, naming the position (never echoing the pattern, which could carry a
credential). The built-in default allow list is trusted and exempt from the grammar.

**`--unrestricted` (0.2.4, F89-4)** is the explicit operator opt-in (flag, or roster
`ext.unrestricted: true`): it lifts the allow-list grammar (every pattern accepted, git included)
and is **required** for the muse worker, whose `--disable-sandbox` has no shell containment. With
it, `run.json` records `unrestricted: true`. codex always keeps `--sandbox workspace-write` (a real
boundary) regardless. Treat a codex/muse/unrestricted worker's tree as the worker's to change and
review its `diff.txt` as always.

**Credentials / launch guard.** claude reuses the endpoint route (`env_key` token, host markers
scrubbed, `env: None` becomes the endpoint env). codex and muse inherit the parent environment
(`env: None`, as the review adapters do); codex relies on the ChatGPT login (a missing/unsupported
model fails the run as `died`, with the provider's message in `report.md`), and muse is guarded by
the per-token-billing / oauth launch block (`get_muse_launch_block`) before anything starts — a
set `META_API_KEY`/`MODEL_API_KEY` or a non-oauth sign-in refuses the run.

**Muse write-mode findings (probed live, muse-spark-1.3-contributor, 2026-10-21).** `muse exec`
writes files headlessly with `--approval-mode never --trust-workspace` and a `--workspace <root>`.
Running a shell command (e.g. `cargo`/`dir`) additionally needs `--disable-sandbox`: the default
shell sandbox reports "sandbox enforcement unavailable (windows_elevated setup_required)" on this
Windows host and every shell call fails without it. `--trust-workspace` IS accepted by `exec`. No
`--permission-profile` id is documented or needed; `--approval-mode never` is the headless switch.
Resume is NOT the `muse resume <id>` subcommand (interactive, no prompt/`--json`): it is
`muse exec --json --session-id <uuid>` with a fresh `--prompt-file`, and a minted `--session-id`
both creates and later continues the session (verified: a second exec on the same id recalled the
first turn's work). The byteplus worker's ark model id is `deepseek-v4-1-flash-261017` (the roster
now carries a second byteplus worker entry with it).
