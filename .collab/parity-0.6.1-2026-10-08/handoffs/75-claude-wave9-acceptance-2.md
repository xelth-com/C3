Write in English.

# Handoff 75 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, second pass (F72-1, F72-2 answered)

Date: 2026-10-15. Base commit: `d271365` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT; the code is
`6a462f0`, the commit on top is one doc paragraph). The range under review is `891ea63..d271365` - wave 9d; skip
`.collab/`. Wave 9 as a whole is `68774db..d271365` (handoffs 65, 67, 69, 71, 73). The wave note:
`docs/port/wave9-consilium-defects.md`, paragraph "9d". Your acceptance (reply 72) held on F72-1 and F72-2, both
in the legacy no-id fallback of 9c. Round 5 (handoff 73) went to ZAI-claude :: glm-5.3: ACCEPT with two notes
(F74-1, F74-2, below). You hold the acceptance. The ChatGPT weekly window is at 94 %: read economically.

## Delta (9d)

- **F72-1 / F72-2 - the legacy fallback is REMOVED, not refined.** Rows without a `consult_id` take no part in the
  cap pair at all - they keep the plain 10-minute burst exactly as before wave 9. The pair is built only from
  id-carrying rows (`MachineEndpoint.consult_id`, the ledger's own id), counted by exact id (same id = one hit;
  different ids = two, whatever their instants or texts - no text comparison remains), and only from hits younger
  than 24 h at READ time (`CAP_PAIR_MAX_AGE_MINUTES`, applied inside `endpoint_health`, so `plan_cap_verdict` and
  every provider / preflight read apply it themselves, independent of retention). Your RC1 (a legacy mirror read 48 h
  later) cannot block: a no-id row never enters the pair, and an id-carrying hit older than 24 h is dropped at read
  time. Your RC2 (same 100-character prefix, different request ids) cannot merge: identity is the id, never the text.
- Consequence: without a `reset_hint` the entry is out until 24 h after the FIRST hit (9b said the second), because
  the read-time bound dissolves the pair at that instant (policy test, code comment, README updated).
- Residual, documented: machine rows written before 9b lack the id, so for 24 h after an upgrade the cap verdict can
  only come from new runs; it ends by itself.
- GLM's notes, recorded in the wave note as design consequences, no code change: **F74-1** all instant 429s of ONE
  consultation share its id and count once, so exhaustion seen only inside a single run stays on the burst;
  **F74-2** the `until` fallback for an unresolvable hint (second hit + 24 h, where the pair dissolves at first hit
  + 24 h) is unreachable - the roster validator accepts a `reset_hint` only when `next_cap_reset` resolves it at load.
- Test `plan::hits_are_counted_by_consultation_identity_not_by_time`: a no-id legacy mirror of a ledger 429 = one hit,
  never the cap verdict; a no-id row never completes a pair even when it differs from the ledger failure only in its
  request id; two different ids at one instant -> the cap verdict; two id-carrying hits 10 min apart -> the cap
  verdict; two hits 25 h old -> none at read time although retention did not prune them. The F66-6 policy test, the
  earlier cap-verdict test and the providers test carry `consult_id` in their fixtures.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | 6a462f0 | 0 | 828 passed; the known flaky `index::embed` loopback green alone; clean | completed |
| plugin harness through the shim | `harness-roster` (plugin HEAD tests, fresh binary, Windows PowerShell 5.1) | 6a462f0 | 0 | 125/0 | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff 891ea63..d271365 -- crates docs README.md CHANGELOG.md` is three
source files and three docs - read the hunks of `crates/c3-core/src/health.rs` and `crates/c3-core/src/plan.rs`
first; at most 4 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With the fallback gone and the bound applied at read time: a path where the cap verdict still fires on a
  usable entry, or stays silent on an exhausted one, beyond the documented residuals?
- **Q2.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 150 words.
