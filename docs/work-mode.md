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
- `events.jsonl` — the child's stream-json verbatim.
- `report.md` — the final assistant text (the `result` event's text; the last assistant text if
  the run died).
- `diff.txt` — `git status --short` before the run (the baseline) and after, plus
  `git diff --stat` after; computed by c3, not by the worker.
- `run.json` — `{task, seq, provider, model, engine:"claude", argv (no secrets), child_env_scrubbed,
  started, ended, exit_code, status: done|died|timeout|refused, turns, tool_calls: {Read, Edit,
  Write, Bash, ...}, files_changed: [...], usage: <the result event's usage/cost when present>}`.

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
  status stays `quota` with `exhausted: true`.
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

**`--allow` applies to claude only** (it maps to Claude Code `--allowedTools`). For codex and muse
the write mode IS the sandbox / workspace flag above; there is no per-tool allow list. The
`git`-naming refusal on `--allow` still holds on all three, and the preamble still says never run
git — but for codex/muse nothing beyond the preamble enforces it (codex's `workspace-write` and
muse's disabled sandbox both permit `git`). Treat a codex/muse worker's tree as the worker's to
change and review its `diff.txt` as always.

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
