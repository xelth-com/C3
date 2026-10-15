Write in English.

# Handoff 69 - claude: wave 9 "consilium defects", third round (F68-1, F68-2 answered)

Date: 2026-10-15. Base commit: `891ea63` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `f2ff103..891ea63` - wave 9c; skip `.collab/`. Wave 9 as a whole is `68774db..891ea63` (handoffs 65, 67;
the reviews 66 and 68 by openai :: gpt-6-astra). The wave note: `docs/port/wave9-consilium-defects.md`, paragraph "9c".

## Context for a new reviewer

C3 is the Rust port of the PowerShell bridge `claude-codex-consult`; wave 9 fixes four defects seen while C3 ran
two full-roster panels: D1 a panel forked old threads by default (now `--mode new` unless asked), D2 C3 printed the
PowerShell command spellings (now its own), D3 detached single runs of one task serialised on the task lock (now the
lock covers only the bookkeeping; the reviewer turn runs unlocked; admission skips another run's record ONLY when
its owning bridge is proven alive and the record has no survivors / unverified / unconfirmed-kill evidence), D4 an
instant 429 on a plan with a monthly cap was a 10-minute burst (now two instant 429s of DISTINCT consultations
within an hour on an entry declaring `ext.c3.plan_caps` mark it out until `ext.c3.reset_hint`, else 24 h). The two
earlier rounds closed F66-1..6 (handoff 67 lists them) and F68-1/2.

## Delta (9c)

- **F68-1:** the hit deduplication of D4 was time-based in two places and could hide a real cap (fail-open). Now two
  rows are one hit ONLY when they carry the same non-empty `consult_id`; rows with different ids are distinct at any
  instant; a legacy machine row without an id is matched to a ledger row only when the endpoint, the failure text
  (which carries the provider's request id) and the instant to the second all agree - otherwise it is its own hit.
  Test `plan::hits_are_counted_by_consultation_identity_not_by_time` (same id mirrored = one; legacy row agreeing on
  endpoint/text/second = one; two ids at one instant = two -> the cap verdict; a legacy row 3 s away with another
  request id = two; same text at another second = two; two distinct consultations = two).
- **F68-2:** the admission unit test no longer shells out to PowerShell: the sleeping "bridge" is
  `ping 127.0.0.1 -n 60` on Windows and `sleep 60` elsewhere; the survivor's name comes from the liveness checker's
  own `process_info`. Run on Windows only; the Linux path is untested here (CI's ubuntu job runs it).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | 891ea63 | 0 | 829 passed, 0 failed; clean | completed |
| plugin harness through the shim | `harness-roster` (plugin HEAD tests, fresh binary, Windows PowerShell 5.1) | 891ea63 | 0 | 125/0 | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff f2ff103..891ea63 -- crates docs CHANGELOG.md` is three source
files and two docs - read the hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The identity rule: a pair of rows it still merges wrongly (two failures counted once) or splits wrongly in a
  way that blocks a usable endpoint?
- **Q2.** Anything in waves 9/9b/9c (the summary above, `docs/port/wave9-consilium-defects.md`) that you would hold a
  patch release on?
- **Q3.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 250 words.
