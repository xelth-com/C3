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

## Chunk 2 — parent scheduler (next)

The parent run: reserve every member's `.consult.pending-<NN>.json` (state `reserved`, `panel{id,
position, of, parent_pid, parent_start_time}`) before launching; launch each seat as
`c3 consult --task <t> --panel-spec <b64>` (JSON compact, UTF-8, base64), redirecting stdout/stderr
to `<temp>/codex-consult-panel-<id>/<NN>.out|.err`, stdin an empty file; poll with the endpoint-group
concurrency plan + `-PanelConcurrency`; collect each member's **last `codex-consult: ` stdout line**
as its outcome (`Read-PanelMemberOutput`); print the progress/summary block (`Write-Summary`); patch
every member's `panel.started`/`panel.usable` after all finish; propagate a required-member failure
or `-PanelConcurrency 1` survivors as exit 5 / blocked. See `docs/port/m4-spec.md` §2–§4.
