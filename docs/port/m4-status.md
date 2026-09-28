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

### Deferred to a follow-on (still open after chunk 3)

- **Commit-blocked (D3) + commit-interruption recovery** (member side): BLOCKED/ORPHAN/MEMBERKILL.
- **Member-record writer-liveness during the parent-death/pause window**: SPEC/PARENT#1/INFLIGHT#1.
- **agy-in-panel** specifics (summary `[agy]`, forced failure, tree-check sibling exclusion): AGY
  (fails in both harness-panel AGY and harness-detach AGY).
- **Range on a panel**: the `Range:` line is computed, but the per-member range record is not passed.
- **`partial_reply`-on-any-failure** (wave 26b): a member render/ledger change, not done.
- **`findings_tool` panel-holder lock refusal + member-record judging** (INFLIGHT codex-findings
  legs): only the `findings --list` detached line landed; the `-Status`/`-Rate` panel-holder refusal
  and member-record judging are not wired.
- **Not owned here**: c3-core `roster.rs` wave-26b string validation (`::`, `[`, `]`, `|`, `,`, `#`,
  edge whitespace — ROSTER fail-closed); the machine-wide health file `~/.codex/codex-consult-health.json`.
- **Environmental (documented, not bugs)**: TIMEOUT/GUARD/WAITTIME (the fake's `HANG_ON`/`FAIL_ON`
  need PowerShell `""` quoting; Rust's Windows argv uses `\"`); muse PANEL command byte-match (the
  c3 schema path under `<CODEX_HOME>`).
