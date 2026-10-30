# Handoff 01 - Claude: core-contract review of the 2026-10-29/30 waves

Date: 2026-10-30. Base `0b712c4`, head `958aa5b` (`git log 0b712c4..958aa5b`; crates/: 87 files,
+14,760 / -6,020). Unreleased 0.2.4-dev. Every wave: fmt, clippy -D warnings, c3 lib (551),
c3-core (270), full c3-cli suite single-threaded (17 binaries, 153 tests) green; CI green on Linux
and - for the first time - Windows.

## Question

Is anything in these waves wrong at the CONTRACT level - a changed invariant, a lost guard, a
security hole, a behaviour change hidden inside a "pure refactor" - before they become 0.2.4?

## What landed (read in this order; most security-relevant first)

1. **C3 now commits** (`crates/c3/src/finish/`, `docs/eck-port.md`): `c3 finish` = `git add -A`,
   unstage a never-commit set (`finish/never_commit.rs`), a secret gate on the ADDED lines of each
   staged file (`finish/secret_gate.rs`, pack redactor, kind-only report, `--allow-secret`), print
   the staged list, commit, journal + AnswerToSA, a hand-off snapshot into `.eck/` (hidden paths
   dropped except `.eck/*.md`; `pack/discover.rs` `eck_hidden_drop`). MCP tools
   `c3_finish_task`/`c3_fail_task`/`c3_manifest_edit` (`mcp/mod.rs`). Invariant changed by operator
   decision: "C3 never commits" -> "C3 commits only via `c3 finish`".
2. **Read-only drift check** (`consult/orchestrate/tree.rs`, `engines/tree_check.rs`, `engines/agy.rs`):
   the claude reviewer engine is now write-disabled (warns instead of fails); the collab compare
   skips other tasks, `.consult.*`, state.md/findings.json/sessions.json; agy fails only when its
   stream names a writing tool (`agy_tool_is_writing`, a deny-by-default name classifier).
3. **Per-model health mark + panel re-seat** (`c3-core/src/health.rs` `model_unsupported_message`,
   `verdict.rs`, `panel/{plan,run,routing}.rs` reserve + `lost_before_start`).
4. **Death records** of dead detached runs (`consult/detached.rs` `build_death_record`,
   `consult/detach.rs` `ensure_reader_death_record`; `work/record.rs` `recover_dead_run`).
5. **Grok inline brief** (`c3-core/src/lineage.rs` `inline_brief`, `consult/prompt.rs`).
6. **Cross-model tests in `c3 work`** (`work/mod.rs` `--role tests`/`--of`/`--scope`,
   `work/record.rs` `run_patch`/`worktree_patch` - baseline content stored with `hash-object -w`).
7. **Refactors claimed behaviour-neutral**: `c3-core/src/retention.rs` (all retention/clearing rules
   as a table, health.rs/quota.rs call it); parameter structs replacing
   `#[allow(clippy::too_many_arguments)]`; `consult/orchestrate.rs` split into `consult/orchestrate/`
   ({mod, secondary, kill, member, render, engine, ledger, tree}.rs) and `consult::render` renamed
   `consult::section`.
8. **Signed priors** (`router/sign.rs` `PRIORS_KEYS` now pins `xelth-2026-1`).

## Questions

- **Q1.** `c3 finish`: can it commit something it must not (a secret the gate misses because it
  scans added lines only; a never-commit path it fails to match; a staged path outside the repo)?
  Is "added lines only" an acceptable trade-off for the false re-flags it removes?
- **Q2.** The drift-check relaxations (claude write-disabled, collab scope, agy tool classifier):
  is any of them a hole through which a reviewer that DID write would now pass as usable?
- **Q3.** The "pure" refactors (retention.rs, parameter structs, orchestrate split): pick the spots
  where a move most plausibly changed behaviour (evaluation order, a borrowed temporary, a
  visibility that let a new caller in, a precedence order in retention) and check them.
- **Q4.** Re-seat + per-model mark: can a re-seat break the panel's recorded draw or its
  reproducibility (`--panel-seed`), or seat a member twice?
- **Q5.** Death records and the reader-side write: any race with a background that is still alive
  (a false "died")?
- **Q6.** Anything else that should block 0.2.4.

Answer by number with file:line evidence; findings in the schema. Keep it under 1200 words.
