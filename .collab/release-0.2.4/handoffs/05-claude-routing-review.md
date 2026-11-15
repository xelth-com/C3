# Handoff 05 - Claude: diff-review of the reviewer-protection routing

Date: 2026-11-15. Base commit: `d7ce658` + uncommitted (the working tree you are in IS the change).

The change as one patch: `.collab/release-0.2.4/handoffs/05-claude-routing-review.patch`
(`git diff d7ce658` including the one new file). You may have no shell: read the patch and the
files it names with your file tools.

## Question

Is the new reviewer selection correct and safe to install? Look for a path where the wrong
reviewer runs, a protected reviewer is spent when it should be held, a run is refused that should
go through, or an older binary breaks on the roster.

## Delta since the last review

Follows: `handoffs/01-claude-diff-review.md` (all 30 findings of that review are closed).

Operator's complaint: the standing reviewer (roster entry #1, the ChatGPT subscription, weekly
window) "is constantly at the front" and gets drained to zero; and the heavy Muse model must run
only when he asks for it. The change:

- **Two per-entry roster settings under `ext`** (`crates/c3-core/src/roster.rs`): `on_request: true`
  (never seated automatically - not by the bare default, a panel draw or `--panel-all`; runs only
  when named by `--provider <label> --model <id>` or a `--require` that names the entry itself) and
  `reserve_percent: N` (1..50, a PROTECTED entry).
- **The hold** (`crates/c3/src/budget/mod.rs::reserve_hold`): on every long window (24 h or more,
  not stale, reset ahead) a protected entry is held when `used% >= min(elapsed% + N, 100 - N)`.
  Purposes `acceptance` and `core-contract` are exempt (`crates/c3-core/src/purpose.rs::is_standing`);
  `--spend-reserve` overrides. Held means: skipped by the bare default, not seated by a panel
  (skip kind `reserve`), a `--require` of it refuses the run, an explicit `--provider` is refused in
  preflight (`crates/c3/src/consult/orchestrate/mod.rs`).
- **The bare default depends on the purpose** (`orchestrate/mod.rs`, `crates/c3/src/panel/run.rs::route_single_default`,
  `crates/c3/src/providers.rs::walk_full_ctx`): acceptance / core-contract - the first available
  roster entry as before; every other purpose - one reviewer from the same routed draw a panel uses
  (weight = usefulness score x budget multiplier), seeded per consultation, reproducible with
  `--panel-seed`; no candidate -> the plain walk.
- **The ChatGPT windows are no longer forgotten** (`budget::openai_windows`,
  `crates/c3-core/src/quota.rs::account_window_within`): when the rollouts of today and yesterday
  give nothing, the newest observation of the last 8 days, then the recorded machine-health quota;
  only windows whose reset is still ahead.
- `c3 maintain`'s smoke never targets a protected or an on-request entry.

## CURRENT invariants claimed

- A roster with the two new `ext` keys loads in a binary that does not know them (per-entry `ext`
  is free-form; checked against the installed d7ce658 build).
- No budget data for an entry's long windows -> not held (the bridge cannot judge).
- The hold never applies on `acceptance` / `core-contract`, and never to an unprotected entry.
- An on-request entry is never chosen by any automatic path; naming it exactly always works.
- An explicit `--provider` run is never rerouted to another reviewer: it runs or is refused.
- `account_window_at` and the health verdicts keep their two-day rule; only the budget module
  reads older observations.

## Changed files

| File | Change |
|---|---|
| `crates/c3-core/src/{roster,roster_ext,purpose,quota}.rs` | the two ext settings, `is_standing`, `account_window_within` |
| `crates/c3/src/budget/{mod,tests}.rs` | `period_secs`, the hold, the openai fallback chain |
| `crates/c3/src/providers.rs`, `panel/{plan,run}.rs` | gates `request` / `reserve`, the routed single pick |
| `crates/c3/src/consult/{args,detach}.rs`, `orchestrate/{mod,member}.rs`, `cli/{consult,router}.rs` | `--spend-reserve`, the purpose-aware default, the preflight refusal, `router explain` marks |
| `crates/c3/src/maintain/{mod,smoke}.rs` | smoke never targets protected / on-request seats |
| `crates/c3-cli/tests/reviewer_protection.rs` | 4 integration tests |
| `CHANGELOG.md`, `docs/DESIGN.md`, `plugin/skills/consult/{SKILL,reference}.md` | documentation |

## Open findings

_(none open)_

## Evidence

- `cargo test -p c3-core` 282 passed; `cargo test -p c3 --lib` 639 passed; `--test reviewer_protection`
  4 passed. The full c3-cli suite is running while you review.
- Live (dry runs only, a scratch copy of the operator's roster): three bare `--purpose diff-review`
  runs picked GLM, Muse contributor / Gemini / Kimi - never the weighty first entry; a protected
  entry fed `97 % used at 57 % of the week` was refused with the reason and allowed with
  `--spend-reserve` and on `--purpose acceptance`.

## Questions

- **Q1.** The hold formula and its inputs: any window, clock or unit mistake (elapsed vs used,
  the 24-hour boundary, a stale snapshot, a reset window, two windows of one plan) that holds a
  reviewer wrongly or fails to hold it?
- **Q2.** The purpose-aware bare default: a path where the standing reviewer still takes a
  non-standing consultation, where an explicit `--provider` / `--thread` / `--mode resume|fork` run
  is rerouted, where a resumed task silently changes reviewer, or where no reviewer is found
  although one is available?
- **Q3.** `on_request`: any automatic path that still seats such an entry (panel, `--panel-all`,
  stand-ins for a light/weighty gap, the walk, `--require <label>`), or an exact request that fails?
- **Q4.** The openai fallback chain: can an old observation be taken for a current one in a way
  that matters (a window that reset, a different account, a future-dated record)?
- **Q5.** Compatibility: detached runs, panel members and `c3 work` started by an older binary, the
  ledger and status formats, the `--panel-seed` relaxation.
- **Q6.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 700 words.
