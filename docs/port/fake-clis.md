# Fake CLI stand-ins (ported test fixtures)

The PowerShell plugin's test harnesses never call a real model. Instead they point the
bridge at small fake executables that print the exact JSONL event shapes the real
`codex`, `agy` (Google Antigravity CLI) and `muse` (Meta Muse Code CLI) binaries
produce, driven entirely by environment variables. These fixtures live in
`C:\Users\Dmytro\claude-codex-consult\tests\` and are **not** modified by this task;
C3's own Rust engine tests should invoke these same `.cmd`/`.ps1` files rather than
inventing new fixtures, so that behavior stays byte-for-byte comparable between the
PowerShell and Rust ports.

Every fake is a `.cmd` wrapper (invoked by the bridge/tests as the "exe") that copies
its raw command line into an env var and then runs the real logic in an adjacent
`.ps1` file via `powershell -NoProfile -ExecutionPolicy Bypass -File`. The `.cmd`
indirection exists because `powershell -File` mis-parses arguments like `-o`, `-c`,
`--json` when passed directly — the wrapper captures `%*` verbatim into
`FAKE_*_ARGS` and the `.ps1` re-parses that string itself.

## codex (`fake-codex3.ps1` / `fake-codex.ps1`)

Two variants exist:

- **`fake-codex3.ps1` + `fake-codex3.cmd`** — the current/active fake, used by the
  0.3.0+ harnesses: `harness-0.3.ps1`, `harness-engines.ps1`, `harness-format.ps1`,
  `harness-muse.ps1`, `harness-panel.ps1`, `harness-roster.ps1`,
  `harness-visibility.ps1`. This is the one C3's Rust tests should target.
- **`fake-codex.ps1` + `fake-codex.cmd`** — the older/simpler fake, used only by
  `harness-fixes.ps1`, `harness-lock2.ps1`, `harness-pending.ps1`. It differs from
  `fake-codex3.ps1` by omitting: the rollout-file simulation, `item.*` event
  emission, the resume/hang/delay/reply-map matrix, PID-directory tracking, and the
  wave-24b continuation-gate variables (`FAKE_CODEX_WRITE`, `FAKE_CODEX_STDERR_FIRST`,
  `FAKE_CODEX_RESUME_FAIL`). It has its own smaller set: `FAKE_CODEX_TOUCH`,
  `FAKE_CODEX_SLEEP`, `FAKE_CODEX_FAIL`, `FAKE_CODEX_NOUSAGE` (see below) that
  `fake-codex3.ps1` does not have.

### Wrapper (`fake-codex3.cmd`, same shape for `fake-codex.cmd`)
```
@echo off
set "FAKE_CODEX_ARGS=%*"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fake-codex3.ps1"
exit /b %ERRORLEVEL%
```

### Command-line parsing (raw string in `$env:FAKE_CODEX_ARGS`, regex-matched)
| Pattern matched | Meaning |
|---|---|
| `--version` | prints `codex-cli 0.155.1-fake`, exit 0 |
| `^\s*login\s+status(\s\|$)` | answers the bridge's credential preflight (see below) |
| `(?:^\| )-m (\S+)` | the `-m <model>` value, used to key per-model delay/reply maps |
| `(?:^\| )-o (\S+)` | the `-o <file>` output path the reply gets copied into |
| `' resume ([0-9a-fA-F-]{36})'` (fake-codex3 only, gated on `FAKE_CODEX_RESUME_REPLY`) | a `resume <thread-uuid>` turn |
| everything else (`exec`, `--sandbox`, `--json`, `-c key=value`, etc.) | read from stdin as the prompt but not otherwise branched on — the fake does not validate flags beyond the ones above |

The prompt body is always read from stdin: `[Console]::In.ReadToEnd()`.

### Environment variables — fake-codex3.ps1 (superset)
| Var | Effect |
|---|---|
| `FAKE_CODEX_NOTHREAD=1` | suppress the `thread.started` event (forces the bridge's rollout-file fallback) |
| `FAKE_CODEX_ROLLOUT=ours\|foreign\|ours+foreign` | writes rollout `.jsonl` files under `$env:CODEX_HOME\sessions\<yyyy>\<MM>\<dd>\`; `ours` embeds the prompt, `foreign` an unrelated session |
| `FAKE_CODEX_ROLLOUT_LOG=<path>` | one `"<kind> <uuid>"` line per rollout written |
| `FAKE_CODEX_PRELINE=<json>` | one raw JSONL line emitted before `thread.started` |
| `FAKE_CODEX_STDERR=<text>` | written to stderr as raw UTF-8 bytes after the event stream (simulates an SSE `data:{"error":{...}}` line) |
| `FAKE_CODEX_LOGIN=out\|hang\|utf8` | `login status` behavior: `out` → "Not logged in" + exit 1; `hang` → sleep 40s; `utf8` → a non-ASCII "Logged in..." line as raw UTF-8 bytes; default → "Logged in using ChatGPT" |
| `FAKE_CODEX_LOGIN_DELAY_MS=<ms>` | delay before `login status` answers |
| `FAKE_CODEX_EXIT=<n>` | exit n right after events, no reply file copied |
| `FAKE_CODEX_FAIL_ON=<text>` | only if the raw command line contains `<text>`: stderr `stream error: provider refused the request`, exit 1 |
| `FAKE_CODEX_HANG_ON=<text>` | only if the raw command line contains `<text>`: sleep 60s |
| `FAKE_CODEX_HANG_NEW=1` | sleep 60s only on a turn that is NOT `resume <thread>` |
| `FAKE_CODEX_ITEMS=1` | before any sleep/hang, emit a reasoning `item.completed`, a `command_execution` item (`item.started` + `item.completed`), and an `agent_message` item, each tagged `(first ...)` or `(resume ...)` |
| `FAKE_CODEX_RESUME_REPLY=<file>` | on `resume <thread>`: copy this file to `-o` output instead of `FAKE_CODEX_REPLY`; thread id in `thread.started` is the RESUMED id (unless `FAKE_CODEX_RESUME_NEWTHREAD=1`) |
| `FAKE_CODEX_RESUME_NEWTHREAD=1` | on resume, report a brand-new thread id instead of the resumed one |
| `FAKE_CODEX_RESUME_LOG=<path>` | on resume: `"ARGS: ..."` + `"PROMPT:"` + prompt text |
| `FAKE_CODEX_DELAY_MS=<ms>` or `<model>=<ms>[\|<model>=<ms>...]` | sleep after `turn.started`; a `\|`-delimited map keyed by `-m` model (`*=<ms>` or a bare number = default for all models) |
| `FAKE_CODEX_REPLY_MAP=<model>=<file>[\|<model>=<file>...]` | reply file keyed by `-m` model (instead of `FAKE_CODEX_REPLY`; a resume turn still prefers `FAKE_CODEX_RESUME_REPLY`) |
| `FAKE_CODEX_PIDDIR=<dir>` | every exec turn writes `<dir>\<pid>.pid` containing `"<pid> <start-time-UTC-ticks>"` |
| `FAKE_CODEX_WRITE=<rel path>` | on a non-resume turn: writes that file (relative to CWD) before any sleep/hang |
| `FAKE_CODEX_STDERR_FIRST=<text>` | written to stderr (UTF-8 bytes) right after `turn.started`, before any sleep/hang |
| `FAKE_CODEX_RESUME_FAIL=<text>` | on a `resume <thread>` turn: emits an `error` event + `turn.failed` event with `<text>`, stderr `"ERROR: <text>"`, exit 1, no reply |
| `FAKE_CODEX_LOG=<path>` | (non-resume turn) `"ARGS: ..."` + `"PROMPT:"` + prompt text |
| `FAKE_CODEX_PIDFILE=<path>` | writes the fake's own `$PID` |
| `FAKE_CODEX_REPLY=<file>` | default reply file copied to `-o` output |
| `CODEX_HOME` | (read, not a `FAKE_*` var) base dir for `FAKE_CODEX_ROLLOUT` session files |

### Environment variables — fake-codex.ps1 (older, simpler set)
| Var | Effect |
|---|---|
| `FAKE_CODEX_LOGIN=out\|hang` | same idea as above but no `utf8` mode |
| `FAKE_CODEX_LOG=<path>` | `"ARGS: ..."` + `"PROMPT:"` + prompt text (every turn, no resume distinction) |
| `FAKE_CODEX_PIDFILE=<path>` | writes `$PID` |
| `FAKE_CODEX_TOUCH=<path>[;<path>...]` | appends `"changed during review"` to each `;`-separated path after events start |
| `FAKE_CODEX_SLEEP=<seconds>` | sleep that long after `turn.started` |
| `FAKE_CODEX_FAIL=<message>` | emits `{"type":"error","message":"<message>"}` and exits 1 (no `turn.completed`) |
| `FAKE_CODEX_REPLY=<file>` | copied to `-o` output if `-o` was given |
| `FAKE_CODEX_NOUSAGE=1` | suppresses the `turn.completed` usage event entirely |

### Stdout JSONL event shapes (fake-codex3.ps1)
Emitted in order, each one compact JSON line:
1. `{"type":"thread.started","thread_id":"<uuid>"}` (skipped if `FAKE_CODEX_NOTHREAD`)
2. `{"type":"turn.started"}`
3. optional `item.completed` (`type":"reasoning"`), `item.started`/`item.completed`
   (`type":"command_execution"`, fields `command`, `aggregated_output`, `exit_code`,
   `status`), `item.completed` (`type":"agent_message"`, field `text`) — from
   `FAKE_CODEX_ITEMS`
4. on failure paths: `{"type":"error","message":"..."}` then
   `{"type":"turn.failed","error":{"message":"..."}}`
5. `{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":200,
   "cache_write_input_tokens":0,"output_tokens":300,"reasoning_output_tokens":40}}`
   (fixed numbers, always the same in both fakes except `fake-codex.ps1` can suppress
   it via `FAKE_CODEX_NOUSAGE`)

The `-o <file>` output file receives a byte-for-byte copy of whichever reply file is
selected (`FAKE_CODEX_RESUME_REPLY` > `FAKE_CODEX_REPLY_MAP[<model>]` >
`FAKE_CODEX_REPLY`, in that precedence) — it is never generated on the fly.

Rollout files (`FAKE_CODEX_ROLLOUT`) are two-line JSONL:
`{"type":"session_meta","payload":{"id":"<uuid>"}}` then
`{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<prompt or a placeholder>"}]}}`,
named `rollout-<timestamp>-<uuid>.jsonl`.

### Special subcommands
- `--version` → `codex-cli 0.155.1-fake`
- `login status` → stderr status line, gated by `FAKE_CODEX_LOGIN` (see table above);
  this is the bridge's credential preflight
- `resume <thread-uuid>` (fake-codex3 only) → answers with `FAKE_CODEX_RESUME_REPLY`
  and either the resumed or a new thread id

### Harness wiring pattern
Harnesses set `$fakeCodex = Join-Path $sp 'fake-codex3.cmd'` (or `fake-codex.cmd` for
the three older harnesses) and wire it in one of two ways:
- **env var** (most harnesses): `$env:CODEX_CONSULT_EXE = $fakeCodex` — this is the
  general launcher override the bridge reads.
- **CLI param** (`harness-0.3.ps1` only): passed directly as `-CodexExe $fake` to the
  `consult.ps1` invocation.

Example (`harness-engines.ps1`):
```powershell
$fakeCodex = Join-Path $sp 'fake-codex3.cmd'
...
$env:CODEX_CONSULT_EXE = $fakeCodex
```
`CODEX_CONSULT_EXE = ''` in a case's env-override map removes the variable (simulating
codex not being installed/wired).

## agy (`fake-agy.ps1` / `fake-agy.cmd`)

Single variant, used by `harness-engines.ps1`, `harness-muse.ps1`, `harness-panel.ps1`,
`harness-visibility.ps1`. Simulates the Google Antigravity CLI (`agy`, real CLI
version family 1.2.x), specifically its `--output-format stream-json` mode.

### Command-line parsing (raw string in `$env:FAKE_AGY_ARGS`)
| Pattern | Meaning |
|---|---|
| `^\s*models(\s\|$)` | the `agy models` subcommand (model listing) |
| `--conversation\s+(\S+)` | resume/continue an existing conversation id |
| `--model\s+(\S+)` | the model id for this turn, used for delay-map keying and `run.model.configured`-equivalent fields |
| `-p` (print mode, implied) | everything not matching `models` is treated as a one-shot print-mode turn |

The prompt itself is NOT on the command line — it arrives as one NDJSON line on
**stdin** (read as raw bytes then UTF-8-decoded), matching the bridge's transport so
that `cmd.exe` never has to parse it.

### Environment variables
| Var | Effect |
|---|---|
| `FAKE_AGY_MODELS=ok\|out\|hang\|nolist` | `agy models` behavior: `ok` (default) → `"<id>\t<name>"` lines for `gemini-3.8-flash-high`, `gemini-3.8-flash-low`, `gemini-3.1-pro-high`; `out` → not-signed-in stderr message + exit 1; `hang` → sleep 40s; `nolist` → exit 0 with no list |
| `FAKE_AGY_MODELS_LOG=<path>` | appends one `"models"` line per `agy models` call |
| `FAKE_AGY_REPLY=<file>` | the reply; if the file's text parses as a JSON object it becomes `result.structured_output`, and `result.response` gets that same object plus two extra keys `toolAction:"finish"`, `toolSummary:"done"` (mirrors the real CLI's behavior); otherwise the raw text is `result.response` |
| `FAKE_AGY_NOSTRUCTURED=1` | forces `FAKE_AGY_REPLY`'s text to be used as plain response text only (skips JSON parsing), except on a resume turn |
| `FAKE_AGY_RESUME_REPLY=<file>` | on a `--conversation` turn: this reply instead of `FAKE_AGY_REPLY` |
| `FAKE_AGY_STATUS=<S>` | `result.status` (default `SUCCESS`); any other value causes exit 1 unless `FAKE_AGY_EXIT` is set |
| `FAKE_AGY_ERROR=<text>` | sets `result.error` |
| `FAKE_AGY_STDERR=<text>` | written to stderr (UTF-8 bytes) |
| `FAKE_AGY_CONVERSATION=notfound\|other\|mismatch\|notuuid` | `notfound` → stderr warning + brand-new id; `other` → new id, no warning; `mismatch` → `init` and `result` carry different ids; `notuuid` → id is the literal string `conv-1`; default → echoes the requested `--conversation` id, or a new uuid if none given |
| `FAKE_AGY_DENIED=1\|all` | a turn WITHOUT `--conversation` gets a `run_command` tool step, an "auto-denied" stderr line (jetski permission message), `result.status=SUCCESS` with empty response and `result.denied_actions`; `=all` applies to every turn including the retry |
| `FAKE_AGY_DENIED_REPLY=1` | even when denied, the reply text is still included (a notice WITH a reply) |
| `FAKE_AGY_PARTIAL=1` | emits a "print timeout after 20s" stderr line, `SUCCESS` with empty response |
| `FAKE_AGY_NORESULT=1` | omit the `result` event entirely |
| `FAKE_AGY_TWORESULTS=1` | emit the `result` event twice |
| `FAKE_AGY_BADLINE=1` | emit a non-JSON line ("this is not json") before the result |
| `FAKE_AGY_WRITE=<rel path>[\|all]` | writes that file (relative to CWD) on a turn without `--conversation`; `\|all` suffix writes on every turn |
| `FAKE_AGY_HANG=1\|new` | `1` → sleep 60s after init; `new` → only on a turn without `--conversation` |
| `FAKE_AGY_HANG_ON=<text>` | sleep 60s only if the raw args contain `<text>` |
| `FAKE_AGY_TEXT=1` | before hang, emits two `agent_response` `text_delta` steps and a `run_command` tool step, tagged `(first ...)`/`(resume ...)` |
| `FAKE_AGY_TRAILING=<text>` | written to stdout after the result event, no trailing newline (simulates trailing garbage) |
| `FAKE_AGY_HANG_AFTER=1` | sleep 60s after all events are written |
| `FAKE_AGY_EXIT=<n>` | exit n after all events |
| `FAKE_AGY_LOG=<path>` | (non-`--conversation` turn) `"ARGS: ..."` + `"STDIN:"` + stdin text, plus raw stdin bytes to `<path>.stdin` |
| `FAKE_AGY_RESUME_LOG=<path>` | same, for a `--conversation` turn |
| `FAKE_AGY_PIDFILE=<path>` | writes `$PID` |
| `FAKE_AGY_DELAY_MS=<ms>` or `<model>=<ms>[\|<model>=<ms>...]` | sleep before the answer; map keyed by `--model` (`*=<ms>` or bare number = default) |

### Stdout JSONL event shapes
1. `{"event":"init","conversation_id":"<id>","init":{"model":"<model>","cwd":"<path>","tools":["view_file","grep_search","run_command","write_to_file"],"permission_mode":"request-review"}}`
2. `{"event":"step_update","step_update":{"conversation_id":"<id>","step_index":0,"state":"DONE","step_type":"user_input"}}`
3. optional (`FAKE_AGY_TEXT`) `step_update` events with `step_type:"agent_response"` +
   `text_delta`, and `step_type:"tool"` with `tool_name`, `tool_info.parameters`
4. a `step_update` with `step_type:"tool"`, `tool_name:"view_file"`, `duration_seconds`,
   `tool_info.output`
5. if denied: two more `tool` step_updates (`run_command`, `agy --version`) plus a
   stderr "auto-denied" message
6. a final `step_update` with `step_type:"agent_response"`, `state:"DONE"`,
   `duration_seconds`, `usage:{input_tokens,output_tokens,thinking_tokens,
   cache_read_tokens,total_tokens}`
7. `{"event":"result","result":{"conversation_id":"<id>","status":"...","response":
   "...","duration_seconds":3.1,"num_turns":1,"structured_output"?,"error"?,
   "usage":{...},"denied_actions"?}}` — `usage` numbers differ for a fresh turn
   (13000/500/120/4000/13500) vs. a `--conversation` resume turn (54000/300/80/30000/54300)

### Special subcommands
- `models` — the model listing described above (analogous to `codex models` /
  a "models" catalog call)
- there is no separate `agy login status`; sign-in state is only surfaced through
  `FAKE_AGY_MODELS=out` ("you are not signed in")

### Harness wiring pattern
```powershell
$fakeAgy = Join-Path $sp 'fake-agy.cmd'
...
$env:CODEX_CONSULT_AGY_EXE = $fakeAgy
```
`CODEX_CONSULT_AGY_EXE = ''` removes the variable (agy not wired).

## muse (`fake-muse.ps1` / `fake-muse.cmd`)

Single variant, used by `harness-muse.ps1` and `harness-visibility.ps1`. Simulates
Meta's Muse Code CLI (real CLI: Muse Code 1.4.0) in `exec --json` mode, emitting MSP
(Muse Structured Protocol) JSONL records. It never reads a real credential.

### Command-line parsing
`fake-muse.cmd` passes the raw command line as `FAKE_MUSE_ARGS`; the `.ps1` re-splits
it itself with `Split-CommandLine`, replicating **MS C runtime argv quoting** (blanks
separate args outside double quotes, `""` inside quotes is one literal quote) so a
path containing spaces arrives whole. The fake is strict — an unrecognized flag,
missing value, or invalid value produces `stderr "error: ..."` + `Usage: muse exec
[OPTIONS]` and exit 2 (mirroring the real CLI's usage-error behavior).

| Argument | Kind | Notes |
|---|---|---|
| `--version` | subcommand | prints `muse <FAKE_MUSE_VERSION or 9.9.9-fake>`; logged via `FAKE_MUSE_VERSION_LOG` |
| `exec` | subcommand (required first arg) | anything else → usage error `unrecognized subcommand` |
| `--json` | switch (required) | fake only speaks `--json`; omitting it is a usage error |
| `--prompt-file <path>` | value (required) | must be a readable file; its content is the prompt |
| `--output-schema <path>` | value (optional) | must be a readable file if given |
| `--model <id>` | value (required by the fake) | drives `run.model.configured` |
| `--reasoning-effort <v>` | value (optional) | must be one of `none, minimal, low, medium, high, xhigh, max, ultra` (case-sensitive) |
| `--approval-mode <v>` | value (optional) | fake only accepts `never` |
| `--max-model-steps <n>` | value (optional) | must parse as a positive integer |
| `--session-id <id>` | value (optional) | resume/continue an existing session |
| `--no-foreign-personal-context` | switch | accepted, no behavioral effect noted |
| `--disable-web-tools` | switch | accepted, no behavioral effect noted |
| `--disable-write` | switch | accepted, no behavioral effect noted |
| `--disable-shell` | switch | accepted, no behavioral effect noted |

Stdin is also read (byte length logged) though the real prompt comes from
`--prompt-file`.

### Environment variables
| Var | Effect |
|---|---|
| `FAKE_MUSE_REPLY=<file>` | the terminal text (default `{"answer":"fake"}`) |
| `FAKE_MUSE_RESUME_REPLY=<file>` | on a `--session-id` turn: this reply instead |
| `FAKE_MUSE_TERMINAL=failed\|cancelled` | terminal kind (default `completed`); text becomes `null`, `reason` = `FAKE_MUSE_REASON`, exit 1 unless `FAKE_MUSE_EXIT` set |
| `FAKE_MUSE_REASON=<text>` | the terminal record's `payload.reason` |
| `FAKE_MUSE_EXIT=<n>` | exit n after all records (1, 2, 130, 143, etc.) |
| `FAKE_MUSE_USAGE_ERROR=1` | emits no records at all: `stderr "error: ..."` + exit 2 |
| `FAKE_MUSE_STDERR=<text>` | one extra stderr line after the two informational lines |
| `FAKE_MUSE_SESSION=other\|notuuid` | `other` → a session id mismatch (new id ignoring `--session-id`); `notuuid` → literal `session-1`; default → echoes `--session-id` or a new uuid |
| `FAKE_MUSE_TWOSESSIONS=1` | the second half of records switch to another session stream id |
| `FAKE_MUSE_MODEL=<model>` | `run.model.configured` serves this model (simulates model drift) |
| `FAKE_MUSE_NOMODEL=1` | suppress the `run.model.configured` record entirely |
| `FAKE_MUSE_SCHEMA_VERSION=<n>` | every record's `schema_version` (default 1) |
| `FAKE_MUSE_TWOTERMINALS=1` | emit two `run.terminal.*` records |
| `FAKE_MUSE_NOTERMINAL=1` | emit none |
| `FAKE_MUSE_TERMINAL_STREAM=run` | put the terminal record on a `run` stream instead of the session stream |
| `FAKE_MUSE_LINK=none\|task\|two` | `none` → no `session.run.linked` record; `task` → that record lives on a task sub-stream; `two` → a second `session.run.linked` record naming a different (foreign) run |
| `FAKE_MUSE_MODEL_STREAM=task` | `run.model.configured` record placed on a task sub-stream |
| `FAKE_MUSE_MODEL_RUN=other` | `run.model.configured` names a different (foreign) run stream |
| `FAKE_MUSE_TERMINAL_RUN=other` | the terminal record names a different (foreign) run stream |
| `FAKE_MUSE_PARTIAL=1` | a truncated JSON line (no trailing newline) written after the terminal record |
| `FAKE_MUSE_WRITE=<rel path>[\|all]` | writes that file (relative to CWD) on a turn without `--session-id`; `\|all` = every turn |
| `FAKE_MUSE_HANG=1\|new\|resume` | `1` → sleep 60s after the first records; `new` → only on a non-`--session-id` turn; `resume` → only on a `--session-id` turn |
| `FAKE_MUSE_TEXT=1` | before the hang point, emits a `tool.search` task and two `run.output.delta` text chunks, tagged `(first ...)`/`(resume ...)` |
| `FAKE_MUSE_DELAY_MS=<ms>` | sleep that long before the answer |
| `FAKE_MUSE_LOG=<path>` | (non-`--session-id` turn) `"RAW: <args>"`, one `"ARG: <arg>"` per parsed argument, `"STDIN-BYTES: <n>"`, `"PROMPT-FILE: <path>"`, `"PROMPT:"` + prompt text |
| `FAKE_MUSE_RESUME_LOG=<path>` | same, for a `--session-id` turn |
| `FAKE_MUSE_COUNT=<path>` | appends one `"exec <session-id or 'new'>"` line per exec turn |
| `FAKE_MUSE_PIDFILE=<path>` | writes `$PID` |
| `FAKE_MUSE_VERSION=<v>` / `FAKE_MUSE_VERSION_LOG=<path>` | `--version` output override / call log |

### Stdout MSP JSONL record shape
Every record:
```json
{
  "schema_version": 1,
  "id": "018f0000-0000-7000-8000-<hex>",
  "stream": {"kind": "session|task|run", "id": "<uuid>"},
  "sequence": <n>,
  "recorded_at": <epoch-micros-ish int>,
  "record_type": "reconciliation|event|status",
  "durability": "durable|ephemeral",
  "causation_id": "<command-uuid>",
  "payload_type": "<type>",
  "payload_schema_version": 1,
  "payload": { ... }
}
```
Sequence of emitted records (in order):
1. `reconciliation` / `runtime.command.accepted` — `{kind:"command_accepted", command_id, client_id:null, command_kind:"turn.submit"}`
2. (unless `FAKE_MUSE_LINK=none`) `event` / `session.run.linked` — `{kind:"session_run_linked", command_id, run_stream:{kind:"run", id:<command-uuid>}}`; `FAKE_MUSE_LINK=two` adds a second one naming a foreign run stream
3. (unless `FAKE_MUSE_NOMODEL=1`) `event` / `run.model.configured` — `{provider_id:"meta", profile_id:"tbh", model_id, display_label, source:"startup", command_id, run_stream}`
4. `status` (ephemeral) / `turn.input.user` — `{prompt:<first 200 chars>, command_id, run_stream}`
5. `event` / `run.lifecycle.started` — `{prompt:<first 200 chars>, command_id, run_stream}`
6. optional (`FAKE_MUSE_TEXT`) a `tool.search` task-record group, then two `status`/`run.output.delta` chunks
7. a `model.meta.response` task-record group, a `tool.read_file` task-record group
   (each task group = `task.stream.linked` + `task.lifecycle.proposed/accepted/
   scheduled/side_effect_intent/started` + optional `task.lifecycle.output` +
   `task.lifecycle.completed`)
8. `event` / `tool.result` — `{kind:"tool_result", command_id, run_stream, call_id:"call_0000000000000000000000000000fake", text}`
9. two `status` / `run.output.delta` chunks splitting the reply text in half (only if
   terminal is `completed`)
10. `event` / `run.terminal.<kind>` (kind = `completed|failed|cancelled`) —
    `{kind:"run_terminal", terminal, text (null unless completed), reason,
    command_id, run_stream}`; `FAKE_MUSE_TWOTERMINALS=1` repeats this record

The reply text comes from `FAKE_MUSE_REPLY`/`FAKE_MUSE_RESUME_REPLY` verbatim (default
`{"answer":"fake"}`) — same "copy a fixture file's bytes" pattern as codex/agy.

### Special subcommands / conventions
- `--version` → `muse <version>` (default `9.9.9-fake`), logged via
  `FAKE_MUSE_VERSION_LOG`
- `exec` is the only real subcommand the fake implements; anything else is a usage
  error (exit 2)
- there is no `muse login status` equivalent in this fake — the header comment
  explicitly notes it "never reads a credential"
- two informational stderr lines are always printed before any records:
  `muse: workspace root: <cwd> (cwd default)` and
  `muse: Agent delegation: auto unavailable: workspace is untrusted.`

### Harness wiring pattern
```powershell
$fakeMuse = Join-Path $sp 'fake-muse.cmd'
...
$env:CODEX_CONSULT_MUSE_EXE = $fakeMuse
```
`CODEX_CONSULT_MUSE_EXE = ''` removes the variable (muse not wired/installed).

## Summary: env vars documented per fake

- **fake-codex3.ps1**: `FAKE_CODEX_NOTHREAD`, `FAKE_CODEX_ROLLOUT`,
  `FAKE_CODEX_ROLLOUT_LOG`, `FAKE_CODEX_PRELINE`, `FAKE_CODEX_STDERR`,
  `FAKE_CODEX_LOGIN`, `FAKE_CODEX_LOGIN_DELAY_MS`, `FAKE_CODEX_EXIT`,
  `FAKE_CODEX_FAIL_ON`, `FAKE_CODEX_HANG_ON`, `FAKE_CODEX_HANG_NEW`,
  `FAKE_CODEX_ITEMS`, `FAKE_CODEX_RESUME_REPLY`, `FAKE_CODEX_RESUME_NEWTHREAD`,
  `FAKE_CODEX_RESUME_LOG`, `FAKE_CODEX_DELAY_MS`, `FAKE_CODEX_REPLY_MAP`,
  `FAKE_CODEX_PIDDIR`, `FAKE_CODEX_WRITE`, `FAKE_CODEX_STDERR_FIRST`,
  `FAKE_CODEX_RESUME_FAIL`, `FAKE_CODEX_LOG`, `FAKE_CODEX_PIDFILE`,
  `FAKE_CODEX_REPLY`, plus reads `CODEX_HOME`.
- **fake-codex.ps1** (older): `FAKE_CODEX_LOGIN`, `FAKE_CODEX_LOG`,
  `FAKE_CODEX_PIDFILE`, `FAKE_CODEX_TOUCH`, `FAKE_CODEX_SLEEP`, `FAKE_CODEX_FAIL`,
  `FAKE_CODEX_REPLY`, `FAKE_CODEX_NOUSAGE`.
- **fake-agy.ps1**: `FAKE_AGY_MODELS`, `FAKE_AGY_MODELS_LOG`, `FAKE_AGY_REPLY`,
  `FAKE_AGY_NOSTRUCTURED`, `FAKE_AGY_RESUME_REPLY`, `FAKE_AGY_STATUS`,
  `FAKE_AGY_ERROR`, `FAKE_AGY_STDERR`, `FAKE_AGY_CONVERSATION`, `FAKE_AGY_DENIED`,
  `FAKE_AGY_DENIED_REPLY`, `FAKE_AGY_PARTIAL`, `FAKE_AGY_NORESULT`,
  `FAKE_AGY_TWORESULTS`, `FAKE_AGY_BADLINE`, `FAKE_AGY_WRITE`, `FAKE_AGY_HANG`,
  `FAKE_AGY_HANG_ON`, `FAKE_AGY_TEXT`, `FAKE_AGY_TRAILING`, `FAKE_AGY_HANG_AFTER`,
  `FAKE_AGY_EXIT`, `FAKE_AGY_LOG`, `FAKE_AGY_RESUME_LOG`, `FAKE_AGY_PIDFILE`,
  `FAKE_AGY_DELAY_MS`.
- **fake-muse.ps1**: `FAKE_MUSE_REPLY`, `FAKE_MUSE_RESUME_REPLY`,
  `FAKE_MUSE_TERMINAL`, `FAKE_MUSE_REASON`, `FAKE_MUSE_EXIT`,
  `FAKE_MUSE_USAGE_ERROR`, `FAKE_MUSE_STDERR`, `FAKE_MUSE_SESSION`,
  `FAKE_MUSE_TWOSESSIONS`, `FAKE_MUSE_MODEL`, `FAKE_MUSE_NOMODEL`,
  `FAKE_MUSE_SCHEMA_VERSION`, `FAKE_MUSE_TWOTERMINALS`, `FAKE_MUSE_NOTERMINAL`,
  `FAKE_MUSE_TERMINAL_STREAM`, `FAKE_MUSE_LINK`, `FAKE_MUSE_MODEL_STREAM`,
  `FAKE_MUSE_MODEL_RUN`, `FAKE_MUSE_TERMINAL_RUN`, `FAKE_MUSE_PARTIAL`,
  `FAKE_MUSE_WRITE`, `FAKE_MUSE_HANG`, `FAKE_MUSE_TEXT`, `FAKE_MUSE_DELAY_MS`,
  `FAKE_MUSE_LOG`, `FAKE_MUSE_RESUME_LOG`, `FAKE_MUSE_COUNT`, `FAKE_MUSE_PIDFILE`,
  `FAKE_MUSE_VERSION`, `FAKE_MUSE_VERSION_LOG`.

## Open questions

- The four `muse exec` switch flags `--no-foreign-personal-context`,
  `--disable-web-tools`, `--disable-write`, `--disable-shell` are accepted by the fake
  (added to `$opt`) but have no observable effect on the emitted records — it's
  unclear from the fake alone whether the real bridge/tests ever assert on them, or
  whether they exist purely so the fake doesn't reject a real bridge invocation that
  passes them. Worth checking the corresponding bridge source (not read for this doc)
  before assuming Rust tests can ignore them.
- `fake-codex.ps1` and `fake-codex3.ps1` are functionally a strict superset relationship
  (fake-codex3 = fake-codex + more), but they were not diffed line-by-line beyond
  reading both files once; if C3 ever needs the three older harnesses' exact behavior
  (harness-fixes/lock2/pending), re-verify against fake-codex.ps1 directly rather than
  assuming fake-codex3.ps1's superset behavior with unset vars matches it exactly.
- This doc covers only the fake CLIs and their direct harness wiring
  (`CODEX_CONSULT_EXE` / `CODEX_CONSULT_AGY_EXE` / `CODEX_CONSULT_MUSE_EXE`, and
  `-CodexExe` for `harness-0.3.ps1`). It does not cover `CODEX_CONSULT_ROSTER` or other
  bridge-side config env vars beyond what's needed to show the wiring pattern.
