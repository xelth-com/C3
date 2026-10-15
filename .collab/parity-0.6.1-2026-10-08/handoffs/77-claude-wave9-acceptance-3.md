Write in English.

# Handoff 77 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, third pass (F76-1 answered)

Date: 2026-10-15. Base commit: `c82d97d` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `d271365..c82d97d` - wave 9e, the answer to your blocker F76-1 (reply 76); skip `.collab/`. Wave 9 as a
whole is `68774db..c82d97d` (handoffs 65, 67, 69, 71, 73, 75). The wave note: `docs/port/wave9-consilium-defects.md`,
paragraph "9e". You hold the acceptance.

## Delta (9e)

- **F76-1:** a hit is pair evidence while it lies inside the entry's CURRENT cap period. With a `reset_hint`, a hit
  counts iff `next_cap_reset(hint, hit) > now` - the reset following the hit has not passed; a pair lying entirely
  before the last reset is no evidence. Without a hint, the 9d rule stays: the hit counts iff it is younger than
  24 h. `until` is unchanged (hinted: `next_cap_reset(hint, second)`; unhinted: first + 24 h). The rule lives in
  `plan_cap_verdict` (the only production reader of `instant_429s`, so providers / preflight / `plan_verdict` agree);
  `endpoint_health` keeps id-carrying hits for 35 days (`CAP_PAIR_RETENTION_MINUTES`, the longest monthly period)
  instead of the former unconditional 24 h, no signature change. No-id rows still take no part in the pair; identity
  stays exact by id.
- Tests: `plan::the_cap_policy_is_inferred_exhaustion_with_a_24_hour_probe` now asserts your RC1 - the hinted entry
  (hits 2026-10-14 20:00Z / 20:20Z, `monthly:27T00:00+08:00`) is still unavailable at 2026-10-16T20:30:00Z and
  available after 2026-10-26T16:00:00Z, a hinted pair read after its reset yields no verdict, the unhinted entry keeps
  its 24 h behaviour; `plan::hits_are_counted_by_consultation_identity_not_by_time` asserts the verdict states
  (unhinted 25 h-old pair -> available; hinted 25 h-old pair with the reset ahead -> unavailable) instead of the
  retained-hit count.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| plan tests | `cargo test -j 2 -p c3-core plan::` | c82d97d | 0 | 11 passed | completed |
| workspace | `cargo test -j 2 --no-fail-fast --workspace` (default features) | c82d97d | 1 | c3-core 238/0, c3 424+/0; 20 failures confined to 4 c3-cli integration binaries driven by the fake claude launcher - every one a startup-probe timeout ("`claude --version` did not finish within 15 s") while a peer session's `cargo build --release` loaded the machine; the same tests pass alone (1/1) and 13/15 at two threads; none touches health/plan | completed, environmental failures |
| clippy / fmt | `--all-targets -D warnings`; `cargo fmt --check` | c82d97d | 0 | clean | completed |
| plugin harness | `harness-roster` | 6a462f0 | 0 | 125/0 at the 9d revision; NOT rerun at 9e (one harness at a time, the machine is loaded) | not rerun |

## Reading budget

No sub-agents; never print a whole file; `git diff d271365..c82d97d -- crates docs CHANGELOG.md` is two source files
and two docs - read the hunks of `crates/c3-core/src/plan.rs` and `crates/c3-core/src/health.rs`; at most 4 tool
calls; the verdict even on an incomplete read.

## Questions

- **Q1.** With the period rule: a path where the cap verdict fires on a usable entry, or stays silent on an
  exhausted one, beyond the documented residuals (pre-9b no-id rows; one consultation's repeats = one hit)?
- **Q2.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 150 words.
