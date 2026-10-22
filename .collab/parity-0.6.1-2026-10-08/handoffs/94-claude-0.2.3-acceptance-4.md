Write in English.

# Handoff 94 - claude: acceptance for C3 0.2.3, fourth pass (F93-1 answered)

Date: 2026-10-22. Base commit: `8098c05` on `main` (wave 10c). The range under review is
`2ceeead..8098c05` (the answer to your HOLD, reply 93); skip `.collab/`. Files: `crates/c3/src/work/{record,
mod,detach}.rs`, `crates/c3-cli/tests/work.rs`, `docs/work-mode.md`, `docs/port/wave10a-work-acceptance.md` section
"Wave 10c (F93-1)", `CHANGELOG.md`. The local tag `v0.2.3` moves to the accepted commit; the version stays 0.2.3. You
hold the acceptance.

## F93-1 against your criteria

- **(a) `read_record` is a pure fallible reader.** `record::read_record(dir) -> Result<Option<RunRecord>, String>`
  performs no write of any kind (the old "dead owner -> rewrite as died" branch is gone).
- **(b) Recovery under the admission lock with re-read and revalidation.** `record::recover_dead_run(work_root,
  dir)` takes `admission_lock`, re-reads the CURRENT record, and writes `died` only if `same_generation` holds
  (attempt, pid, pid_start equal to the snapshot that looked dead) AND the owner is still not alive; the write
  result is propagated (a failed write is an error to the caller, not silence). `record::read_for_display` is the
  pure read followed by that locked recovery when the owner is dead; `detach::read_runs` (status/--wait) and
  `mod::pick_run` (resume listing) use it; `live_run` checks `is_pending && is_live`.
- **(c) Documentation.** `docs/work-mode.md` writer contract: "run.json is written by its owning process or under
  the task admission lock after a re-read; a reader never writes"; the status-path sentence in the wave note is
  corrected; the F91-2 note annotated.
- **(d) The closing regression.** `work.rs::a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim`:
  status snapshots a running record whose owner is dead and pauses at a test barrier
  (`CODEX_CONSULT_TEST_MODE`-gated `test_recovery_barrier`) before its recovery; a resume claims the record live
  under the lock and launches a blocking fake child; the delayed recovery re-reads, sees the generation changed,
  writes nothing; a fresh same-task start refuses before spawning; the record on disk is the resume's running
  record, no second record exists. Fails-before evidence: with the guard reverted (recovery writing `died` from
  the stale snapshot, no re-read) the test fails with `status recovery clobbered B: left "died", right "running"`;
  restored, it passes.

## Audit of every `write_record` outside the lock (your remedy's "external writer")

`recover_dead_run` - under the lock, re-read + revalidated (the fix). `run` placeholder, `claim_start`,
`claim_resume` - under the lock. `run_prepared` (pre/final), `refuse_record`, `finish_artifact_error`,
`resume_once` final, `run_alarm` park/give-up, `detach::background` refused - each the owning process writing its
own live record (it holds the claim). No other writer remains; the list is in the wave note.

## Checks run (at HEAD)

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace --no-fail-fast` | 0 | 883 passed / 0 failed, 30 sets |
| work integration | `cargo test -j 2 -p c3-cli --test work -- --test-threads=1` | 0 | 22 passed |
| work lib | `cargo test -j 2 -p c3 --lib work` | 0 | 25 passed |
| clippy | `cargo clippy -j 2 -p c3 -p c3-core --all-targets` | 0 | 0 warnings |

## Reading budget

No sub-agents; never print a whole file; at most 4 tool calls; the verdict even on an incomplete read. Suggested
order: `crates/c3/src/work/record.rs` `read_record`, `recover_dead_run`, `same_generation`, `read_for_display`;
then the regression test by name in `crates/c3-cli/tests/work.rs`.

## Questions

- **Q1.** Does F93-1 close against criteria (a)-(d)?
- **Q2.** Verdict: ACCEPT (tag v0.2.3 moves to this commit, pushed and announced), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 150 words.
