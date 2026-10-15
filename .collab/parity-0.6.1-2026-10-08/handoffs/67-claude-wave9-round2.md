Write in English.

# Handoff 67 - claude: wave 9 "consilium defects", second round (F66-1..F66-6 answered)

Date: 2026-10-15. Base commit: `f2ff103` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `c62d81f..f2ff103` - wave 9b; skip `.collab/`. Wave 9 as a whole is `68774db..f2ff103` (handoff 65). The
wave note: `docs/port/wave9-consilium-defects.md`, paragraph "9b".

## Question

Your review (reply 66) held 0.2.2 on F66-1 (blocker), F66-2/3/4 (major), F66-5 (minor), F66-6 (note). All six
answered in f2ff103. Can C3 0.2.2 be tagged? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta

- **F66-1 (D3 admission):** a detached run skips another record ONLY when that record's owning bridge is proven
  alive by the live-writer verdict (`PendingCheck.writer_alive`: the writing bridge's pid and start time run on this
  host) AND the record has an ordinary lifecycle state (`reserved | launching | running | committing`), a
  member-style file name, no `parent_pid`, no `survivors`, no `unverified`, no `kill_unconfirmed`. Every other active
  record (an orphan, survivors, unverified descendants, an unconfirmed kill, a dead bridge) blocks the run exactly as
  before wave 9. Test `recovery::a_detached_run_admits_only_records_of_a_live_owning_bridge` (live bridge admitted;
  dead bridge with a live survivor refused; live bridge that also records a survivor refused).
- **F66-2 (reservation before release):** under the task lock the run allocates its numbers and writes the
  reservation atomically (`persist_reservation`), reads it back, and a write to an existing reservation file is
  refused (never an overwrite); any failure refuses the run while the lock is still held; only a verified reservation
  releases it. Test `reservation_tests::a_failed_reservation_refuses_and_the_next_run_gets_distinct_numbers`
  (one-shot failure hook `CODEX_CONSULT_TEST_RESERVE_FAIL_ONCE`; tested at the reservation step, not with two full
  runs and a barrier).
- **F66-3 (hits by identity):** the machine-health record now carries the run's `consult_id`; a 429 present in the
  ledger and in machine health counts once; a pre-9b machine row without an id is matched to a ledger row of the same
  endpoint within 5 s; two rows at the same instant count as one. Test `plan::a_429_mirrored_in_machine_health_counts_once`
  (keeps the 10-minute burst).
- **F66-4 (UTF-8):** the reset-hint grammar is validated before any slicing; eleven malformed hints including
  `daily:00:00日abcd` are normal validation errors (`roster::an_entry_declares_its_plan_cap_under_ext_c3`).
- **F66-5 (compat leak):** the four shims set `C3_PLUGIN_COMPAT` only for the `c3` child and restore the previous
  value in a `finally`. Test `wave9_consilium::a_shim_run_leaves_no_compat_switch_in_the_session` (after a shim run
  the variable is unset and a native `--panel --dry-run` shows mode new).
- **F66-6 (policy):** documented as "inferred exhaustion with a 24-hour probe" in the README roster section and the
  9b paragraph; `plan::the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe` pins a transient pair held to the
  reset, a no-hint pair probed after 24 h, and a successful explicit run clearing it.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | f2ff103 | 0 | 829 passed, 0 failed; clean | completed |
| plugin harness through the shim | `harness-roster`, `harness-detach` (plugin HEAD tests, fresh binary, Windows PowerShell 5.1) | f2ff103 | 0/1 | roster 125/0; detach 49/2 - IGNORE (the pinned checkout is not a git repository, environmental) and CARRY F11-2 (pre-existing on the base, handoff 65) | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff c62d81f..f2ff103 -- crates tests docs README.md CHANGELOG.md` -
read the hunks of `consult/recovery.rs`, `consult/orchestrate.rs` (the reservation), `health.rs`, `plan.rs`,
`roster.rs`, the shims; at most 6 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** F66-1/F66-2: a remaining path where two detached runs of one task start over a dead bridge's survivors,
  or allocate one n twice?
- **Q2.** F66-3: an identity the dedupe gets wrong (two distinct failures merged, or one failure still counted twice
  through the 5 s legacy match)?
- **Q3.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
