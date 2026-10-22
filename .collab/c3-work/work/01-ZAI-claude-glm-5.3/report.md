All deliverables are in. Final report:

## Files changed

- **`crates/c3-cli/tests/work.rs`** (new) — 4 integration tests through the real `c3` binary, cross-platform (`#[cfg(windows)]` `.cmd` stub / `#[cfg(unix)]` `sh` stub, the `compat_wave2b` echo-JSON shape), with the same env hygiene as `claude_engine.rs` (scratch HOME/USERPROFILE/CLAUDE_CONFIG_DIR, PATH stripped of any directory holding a claude launcher):
  - `a_work_run_against_the_fake_launcher_lands_its_whole_record` — exit 0; `.collab/t/work/01-ZAI-claude-glm-5.3/` holds `brief.md` (starts with the verbatim preamble, contains the brief), `events.jsonl` (3 lines), `report.md` (the result text exactly), `diff.txt` (the touched file new in the after-block only), `run.json` (`status done`, `files_changed == ["worker-touched.txt"]`, `turns 3`, usage, the worker argv with `acceptEdits`/write-tools/default allow list); the dummy token value grepped absent from every file in the record dir.
  - `the_guards_refuse_before_any_record_exists` — unreadable brief, `--allow "Bash(git:*)"`, non-worker label (message lists `ZAI-claude :: glm-5.3`), `env_key` unset — each exit 1, no `.collab/t/work`.
  - `status_reads_synthetic_records_and_narrows_by_id` — empty task 0; all done 0; one died 1; one running (live pid) 2; `--id 01` → 0 and hides the running one; `--id zz` refused with the known ids.
  - `a_second_work_on_a_task_with_a_live_run_is_refused` — exit 1, no `02-` record.
- **`crates/c3/src/work/mod.rs`** — implementation fix, see below.
- **`README.md`** — "Worker runs (`c3 work` …)" section after the `claude`-engine/consult-panel material: purpose, not-a-consultation, the three first workers, command/record layout, refusals, live-member rule not applying.
- **`CHANGELOG.md`** — `## [Unreleased / 0.2.3-dev]` → Added: "`c3 work` — worker runs".
- **`plugin/skills/coordinate/SKILL.md`** — one paragraph in "The bridge's own background means": `--detach/--status/--wait`, a worker run is running work for the wait rule, live-member rule does not apply.

## Implementation bug found and fixed

The `env_key`-unset guard lived only in `run_prepared` (`work/mod.rs:326`), i.e. **after** `run()` had already allocated and written a `running` record — the run left a record directory with `status: "refused"`, contradicting the design's "Guards (refusals before anything starts)" and the brief's "no record directory". Fix: `prepare()` now builds the entry's child env and refuses with the same message ("the entry's endpoint is not usable: env X not set") before any record exists; the `run_prepared` check stays as the second line for the detached background.

## Verification

- `cargo test -j 2` (whole workspace, per the cargo-memory rule): **0 failures everywhere** — c3 lib 434, c3-core 202 + 8 + 26 + 9 = 245, c3-cli incl. the new `work.rs` 4 (claude_engine 15, compat_wave2b 17, compat_wave5 5, kill_record_durable 12, mcp_integration 1, notspooled_parity 21, panel_light_required 1, telemetry_parity 7, wave6_live_diffs 5, wave9_consilium 3, and the rest all green).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt` / `--check` — clean.

## Not done / caveats

- The `--detach`/`--wait` flows are covered by the existing unit tests (`work/detach.rs`) — the brief asked `--status` only, so no detach end-to-end test was written.
- The `sh` stub compiles for unix but only the `.cmd` path actually runs on this Windows machine; the unix branch is exercised nowhere here.
- One transient hiccup: the first rebuild hit "failed to remove target\debug\c3.exe (os error 5)" (a stale lock); `rm` cleared it and every later build/test ran clean.