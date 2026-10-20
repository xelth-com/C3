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

## v1.1 (roadmap, 2026-10-21)

More worker engines, in this order: codex (`openai :: gpt-6.1-sol`, `--sandbox workspace-write`,
for complex programs), muse (`meta :: muse-spark-1.3-contributor`, near-free), a byteplus
`DeepSeek-V4.1-Flash-20261017` worker entry once the ark model id is confirmed. Same record
layout and refusals; engine-specific write modes documented per engine. See `.eck/ROADMAP.md`.
