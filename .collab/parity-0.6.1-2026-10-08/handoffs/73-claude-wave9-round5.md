Write in English.

# Handoff 73 - claude: wave 9 "consilium defects", fifth round (F72-1, F72-2 answered: the legacy fallback is gone)

Date: 2026-10-15. Base commit: `6a462f0` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `891ea63..6a462f0` - wave 9d; skip `.collab/`. Wave 9 as a whole is `68774db..6a462f0` (handoffs 65, 67,
69, 71). The wave note: `docs/port/wave9-consilium-defects.md`, paragraph "9d". Your round 3 (reply 70) accepted 9c
with the note F70-1; Astra's acceptance (reply 72) then held on two majors in the same legacy no-id fallback:
F72-1 (the 24 h age bound was enforced only at retention, not when health is read) and F72-2 (the match compared a
100-character message prefix, merging independent same-second failures).

## Delta (9d)

- The legacy fallback is REMOVED, not refined: rows without a `consult_id` take no part in the cap pair at all -
  they keep the plain 10-minute burst exactly as before wave 9. The pair is built only from id-carrying rows, counted
  by exact id (same id = one hit; different ids = two, whatever their instants or texts), and only from hits younger
  than 24 h at READ time (`CAP_PAIR_MAX_AGE_MINUTES`, applied in `endpoint_health`, so `plan_verdict` and every
  provider/preflight read apply it themselves, independent of retention). The 100-character comparison and the
  legacy matching are gone from the pair logic.
- One consequence: without a `reset_hint` the entry is out until the pair is 24 h old (24 h after its FIRST hit),
  where 9b said 24 h after the second - the read-time bound expires the pair at that point, so a later instant would
  have been wrong (policy test, code comment and README updated).
- Residual, documented: machine rows written before 9b lack the id, so for the 24 h after an upgrade the cap verdict
  can only come from new runs; it ends by itself.
- Test `plan::hits_are_counted_by_consultation_identity_not_by_time`: a no-id legacy mirror of a ledger 429 = one hit,
  never the cap verdict; a no-id row never completes a pair even when it differs from the ledger failure only in its
  request id; two different ids at one instant -> the cap verdict; two id-carrying hits 10 min apart -> the cap
  verdict; two hits 25 h old -> none at read time although retention did not prune them. The F66-6 policy test, the
  earlier cap-verdict test and the providers test got `consult_id` in their fixtures.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | 6a462f0 | 0 | 828 passed; the known flaky `index::embed` loopback green alone; clean | completed |
| plugin harness through the shim | `harness-roster` (plugin HEAD tests, fresh binary, Windows PowerShell 5.1) | 6a462f0 | 0 | 125/0 | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff 891ea63..6a462f0 -- crates docs README.md CHANGELOG.md` is three
source files and three docs - read the hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With the fallback gone: a path where the cap verdict still fires on a usable entry, or stays silent on an
  exhausted one, beyond the documented 24 h upgrade residual?
- **Q2.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
