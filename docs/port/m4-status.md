# M4 status — panel port

Tracks the M4 panel port (`docs/port/m4-spec.md`) against the plugin's `harness-panel.ps1` and
`harness-companions.ps1`. One row per acceptance area; the harness section is in parentheses.

## Chunk 1 — member path, CLI surface, panel plan/dry-run (landed)

| Area | Rows covered | State |
|---|---|---|
| `MemberSpec` wire (`to_wire`/`from_wire`, `names_member`) | member.rs unit tests | done; `from_wire` now also decodes the plugin's wire (`nn` and the counts as strings) |
| `--panel-spec` decode + refusals | (SPEC) garbage/`names_member` refusals; `-PanelSpec`+`-Panel` combine | done |
| Member `build_context` (n/nn/consult_id, `listed_ids`, role line, `panel_warnings`, member pending ref, `panel{}` ledger record, roster `panel` rule) | orchestrate `member_tests` (`build_context` with a fabricated spec); roster-changed refusal | done |
| Member accept/rewrite guard + parent-alive (first check) | (SPEC) dead parent, missing record, `n` mismatch | done |
| Member pre-launch parent-alive check | (SPEC) parent dies during preflight | check implemented; the harness rows also need the record-before-preflight ordering + `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS` → chunk 2 |
| CLI: `--panel`/`--panel-all`/`--panel-size`/`--panel-order`/`--panel-seed`/`--require`/`--role`/`--roles`/`--panel-concurrency`/`--topic`/`--panel-spec` | args.rs `panel_*` validation tests | done |
| Panel validation refusals (`--panel` with `--provider`/`--thread`/`--mode resume`; `--panel-size`/`--panel-concurrency` range; `--panel-order`/`--panel-seed`; panel-only-option gating) | args.rs tests; (DRY) `-PanelConcurrency` refusals | done, exact wording |
| Panel plan (`Select-PanelMembers` availability + weighty gate → `select_panel_routing` → `panel_plan`; required-reviewer resolution incl. exit 5) | (DRY) header/member lines/concurrency; (REQUIRE) exit 5 | done |
| Panel dry-run block (header, member lines, routing lines, topics, warnings, `Concurrency:`, `Timeout:`, pending) | (DRY) plan text, pre-assigned numbers, pending recovery line | done for the plan block |
| Non-dry `--panel` refusal ("runtime lands in the next chunk", exit 1) | — | done |

### Known gaps / deferred to chunk 2 (the parent scheduler)

- **Per-member dry-run recursion**: the panel dry-run should launch each seated member as a child
  `-PanelSpec` process with `dry_run=true`, print its single-run dry-run block, then a
  `  <lineage>   planned` summary. Chunk 1 prints the plan block only. (DRY / SIZE rows.)
- **Member lifecycle ordering**: the member must rewrite its reserved record *before* its preflight
  (c3 currently rewrites in `run_live`, after `build_context` runs the preflight), plus the
  `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS` test hook. (SPEC rows for the "during preflight" / F07-1/F11-6
  cases.)
- **Routing with real ratings**: `select_panel_routing` is fed an empty rating set, so a routed
  panel falls back to roster order (`fallback: "no ratings"`). Wire the all-task rating reader
  (scoreboard territory) to route on real ratings. (ROUTED rows.)
- **`-Roles` seat assignment** (`Select-RoleAssignment`, Kuhn's matching) and the `roles_note`; only
  `-Role` (one role for every seat) is applied in the dry-run today.
- **Range on a panel**: the `Range:` dry-run line and the per-member range record are not computed.
- **`panel{}` ledger `started`/`usable`**: written `null` by design and patched by the panel run
  after every member finishes; the patch is chunk 2. (Also: the c3-core `Panel` type skips
  `started`/`usable` when `None`, so a member's committed record currently omits them rather than
  writing explicit `null` — reconcile when the parent patch lands.)

## Chunk 2 — parent scheduler (landed; see harness-results.md Run 8)

The parent run is `panel::run` → `build` (plan, no disk) + `schedule` (lock, records, launch, poll,
summary). Landed and verified (harness-panel UNIT/DRY/RUN/NOLOSS/SEQ pass every run; companions 24/1;
full `cargo test` + `--no-default-features` + clippy `-D warnings` green):

| Area | State |
|---|---|
| Task lock + reserved `.consult.pending-<NN>.json` per seat (`panel{id, position, of, parent_pid, parent_start_time}`), consumed-record cleanup | done |
| Launch each seat as `c3 consult --task <t> --panel-spec <b64>` (stdout/stderr to `<temp>/codex-consult-panel-<id>/<NN>.out|.err`, stdin null) | done |
| Endpoint-group scheduling (parallel across groups to the caps, sequential within a group, `-PanelConcurrency`), F15-3 survivor stop | done |
| Collect outcomes (last `codex-consult: ` stdout line + the committed ledger entry via `Find-PanelEntry`), progress lines, console output | done |
| Byte-identical summary block (head + member column layout + not-picked + required-missing exit 5); unit-tested against panel-01 | done |
| Patch `panel.started`/`panel.usable` under the write lock after all finish; exit codes 0/1/5 | done |
| Per-member dry-run recursion (`dry_run=true` members, `planned`/`refused`) — the same scheduler | done |
| Routed draw on real ratings (`read_all_task_ratings` with topic pooling); seed parity with `reference-draw.py` (35d4a32) | done |
| `Select-RoleAssignment` (`-Roles`, Kuhn matching) + the `roles_note` | done |
| Member lifecycle: rewrite reserved record + `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS` pause + parent check BEFORE preflight (`member_early_accept`) | done (writer-liveness during the parent-death window still off — SPEC/PARENT#1/INFLIGHT#1) |
| c3-core ledger: `panel.started`/`usable` explicit `null` + parent patch; `panel.roles_note`; `routing.size_asked`; `routing.reserve`; `apply_findings_delta` id-ordered insertion; ledger `when` = run start; `commit_wait_ms` + `write lock : waited` + `CODEX_CONSULT_TEST_COMMIT_PAUSE_MS`; the task-lock refusal names the panel holder | done |

## Chunk 3 — the detached lifecycle (landed; see harness-results.md Run 9)

The non-blocking consultation (wave 25, R12), built on the complete `consult::detached` core:
`consult::detach` (the query surface, the `--detach` foreground, the `--detach-id` background + the
status-file sink) wired into `orchestrate::run_inner`, `orchestrate::finish` (single-run member +
summary), `panel::run` (`detach_foreground` + per-member/summary sink hooks), `hook` (the phrase)
and `findings_tool` (the `--list` line). harness-detach **48 / 3** (its first full completion); the
3 remaining are all environmental / by-design / a chunk-2 leftover (below). harness-panel unchanged
at 38 / 15 (no regression; UNIT/DRY/RUN/NOLOSS/SEQ green). Full `cargo test` 352/0; clippy clean.

| Area | State |
|---|---|
| `--status`/`--wait`/`--prune`/`--id`/`--wait-timeout-sec` query surface: refusals + exit codes 0/1/2/3/4, newest-first report, worst-state exit, id-prefix select/ambiguity, prune (done/died/never-started/unreadable > 7 d), `-Status abcd1234` → `-CollabDir` | done |
| `--detach` foreground (single + panel): pre-lock refusals (brief/launcher/preflight/active record/roster), D4 budget, planned members, plan line, `starting` record, background spawn; three console lines | done |
| `--detach-id` background: self-report `running`, the run in-process with a status-file sink (members as they progress + the summary block), `done` on every path, exit 6 on a failed terminal write (F08-2) | done |
| Windows spawn: `CreateProcessW` with `bInheritHandles = FALSE` (the foreground returns at once) + `cmd.exe /c "... <NUL 1>log 2>&1"` under `CREATE_NO_WINDOW` (cmd's redirection captures the console into the `.log`) | done |
| args = the port's JSON wire (not base64 CLIXML); inline `-Prompt` → `.consult.detached-<id8>.prompt.txt` (F11-2) | done (JSON, not CLIXML — F11-2's `$spec` decode leg differs by design) |
| `hook` SessionStart phrase (`Get-DetachedPhrase`); `findings --list` detached line (`Format-DetachedListLine`) | done |

### Deferred to a follow-on (resolved in chunk 4)

The items below were closed in chunk 4 (see harness-results.md Run 10). harness-panel is **54 / 0**
and harness-detach **50 / 1** (the one remaining is by-design).

## Chunk 4 — remaining panel parity (landed; see harness-results.md Run 10)

| Area | Rows | State |
|---|---|---|
| Windows argv parity for `.cmd`/`.bat` (hand the batch path to `std::process`, not `cmd /c`, so Rust's batch quote-doubling matches the fake's `""`) | TIMEOUT/GUARD/WAITTIME; harness-0.3 argv rows | done |
| Commit-blocked (D3): record `committing` + kept `.reply.json` before the lock; `commit blocked:` outcome; exit 1 | BLOCKED | done |
| Commit-interruption recovery: pause moved INTO `store.commit` (between findings.json/sessions.json via `CommitRequest.commit_pause_ms`); `committing` record recovered next run | ORPHAN/MEMBERKILL | done |
| Launcher fate-sharing + writer-liveness: shim exports `CODEX_CONSULT_TEST_BRIDGE_PID`; c3 records it as the lock/record/parent pid (`bridge_identity`, `LockRecord::now`), `watch_bridge` held-handle `WaitForSingleObject` exits c3 when the bridge dies, panel clears the var for members, `process_start_iso` returns `None` for a terminated (handle-held) process | INFLIGHT/PARENT/SPEC/ORPHAN | done |
| agy-in-panel tree-check sibling exclusion (`panel_ignore_prefixes` = the two stores + siblings' handoff prefixes) | AGY (panel + detach) | done |
| Per-member `range` record (already wired: `build_spec` passes `range`, member computes `range_record`) | — | done |
| `findings_tool` panel-holder refusal + member-record judging (already through the shared write-lock/liveness path) | INFLIGHT/TIMEOUT/BLOCKED codex-findings legs | done |

### Still open (next worker — wave 26b, out of scope for chunk 4)

- **`partial_reply`-on-any-failure** (wave 26b): a member render/ledger change; per-member
  `timeout_sec`/`stall_sec`/`context_tokens`, `-Kick`, the machine-wide health file.
- **Not owned here**: c3-core `roster.rs` wave-26b string validation (`::`, `[`, `]`, `|`, `,`, `#`,
  edge whitespace — ROSTER fail-closed).
- **By design (harness, not c3)**: harness-detach CARRY F11-2 (`record.args` is the port's JSON
  wire, not base64 CLIXML, so the harness's `ConvertFrom-DetachArgs` decode fails); harness-0.3 CFG
  comma-split (the harness's `$b.Preview` returns nothing for the multi-item quoted `-CodexConfig`
  value; c3's dry-run output is byte-correct); muse PANEL command byte-match (the c3 schema path
  under `<CODEX_HOME>`).

### c3-core contract changes made in chunk 4

- `store::CommitRequest` gained `commit_pause_ms: u64` (the pause held between findings.json and
  sessions.json — the ORPHAN window and write-lock-contention window).
- `store::LockRecord::now` reads `CODEX_CONSULT_TEST_BRIDGE_PID` for the record pid (own pid when
  unset — production).
- `store::write_lock_timeout_secs()` made `pub` (the "commit blocked" message quotes it).

## Wave 26b landed (Run 11, 2026-09-29)

The "still open" list above is now implemented (D10–D16); see
`docs/port/harness-results.md` Run 11. In brief:

- **D11 timeout_sec / D12 stall_sec / D16 context_tokens** — applied per member in
  `consult/orchestrate.rs::build_context` (roster override of the resolved timeout/stall, the
  context-window 80 % fork→new fallback and the context prompt line) and `consult/args.rs`
  (`--stall-sec`, `Resolved.stall_sec/stall_given`). The panel forwards timeout/continue/stall to a
  member only when explicit, so a member re-derives its own roster override; the panel `Timeout:`
  header lists per-member roster exceptions (`panel/run.rs`).
- **D12 stall detection** lives in `engines/subprocess.rs::run_turn` (byte-growth reset + tool-call
  suspension via per-engine `tool_delta`), surfaced as `AttemptOutcome::Stopped { StopKind }`
  (c3-core `engine.rs`, additive — `TimedOut` and `http_engine` untouched).
- **D15 salvage** — `build_partial_reply` now fires for any non-usable run whose stream holds
  content, with the "the run ended: <why>" footer.
- **D10 -Kick** — new `consult/kick.rs`; the primary turn polls `<task>/.consult.kick-<NN>`.
- **D13 machine health file** — c3-core `health.rs` (read/write/lock/prune/running[]/merge +
  `machine_endpoint_consults`); `providers::read_all_task_consults_health` folds it into every
  endpoint-health decision; `orchestrate` registers/unregisters the running row and writes the
  outcome; `panel/run.rs` enforces the cross-repository parallel limit.

### c3-core contract changes made in this pass

- `engine::AttemptOutcome::Stopped { kind: StopKind, partial, survivors, conversation, wall_seconds }`
  and `engine::StopKind { Stall { silent_seconds, last_event }, Kick }` — new, additive.
- `roster::RosterEntry` gained `timeout_sec: i64`, `stall_sec: i64`, `context_tokens: i64` (parsed
  and validated; `60..86400` / `0..86400` / `32000..100000000`).
- `health.rs` gained the machine-wide file: `MachineEndpoint`, `MachineRunning`, `MachineHealth`,
  `MachineFailure`, `machine_health_path`, `read_machine_health`, `add_machine_health_record`,
  `register_machine_running`, `unregister_machine_running`, `machine_running_count`,
  `machine_endpoint_consults[_all]`; `endpoint_health`'s sort gained a `until` tiebreak (26c D2).
- `store`: `LockRecord::now` no longer reads the environment; `set_bridge_pid`/`bridge_pid` take the
  validated pid from the c3 runtime (test-hook hardening — a live-ancestor-only bridge).
- `ledger` `Stall`/`ModeFallback` are now populated (were present-in-order `null`).
