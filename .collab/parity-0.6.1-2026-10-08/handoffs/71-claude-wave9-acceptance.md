Write in English.

# Handoff 71 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2 (F68-1, F68-2 answered; GLM round 3 ACCEPT)

Date: 2026-10-15. Base commit: `891ea63` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `f2ff103..891ea63` - wave 9c (your round 2, reply 68, held on F68-1 and F68-2); skip `.collab/`. Wave 9
as a whole is `68774db..891ea63` (handoffs 65, 67, 69). The wave note: `docs/port/wave9-consilium-defects.md`,
paragraph "9c". Round 3 (handoff 69) went to ZAI-claude :: glm-5.3 because of the ChatGPT weekly window: ACCEPT with
one note, F70-1 (below). You hold the acceptance.

## Delta (9c)

- **F68-1:** hits are counted by consultation IDENTITY only. Two rows are one hit ONLY when they carry the same
  non-empty `consult_id`; rows with different ids are distinct at any instant; a legacy machine row without an id is
  matched to a ledger row only when the endpoint, the failure text (which carries the provider's request id) and the
  instant to the second all agree - otherwise it is its own hit. The 5 s window is gone (time proves nothing about
  identity). Test `plan::hits_are_counted_by_consultation_identity_not_by_time` (same id mirrored = one; legacy row
  agreeing on endpoint/text/second = one; two ids at one instant = two -> the cap verdict; a legacy row 3 s away with
  another request id = two; same text at another second = two; two distinct consultations = two).
- **F68-2:** the admission unit test no longer shells out to PowerShell: the sleeping "bridge" is
  `ping 127.0.0.1 -n 60` on Windows and `sleep 60` elsewhere; the survivor's name comes from the liveness checker's
  own `process_info`. Run on Windows here; CI's ubuntu job runs the Linux path.
- **F70-1 (GLM's note, not fixed - a documented upgrade-window residual):** the legacy no-id fallback can still merge
  a different consultation's machine row into a ledger hit when the failure text carries no request id and both fall
  in the same second (hides the cap), and can split a genuine mirror whose machine instant differs by one second
  (double-counts one failure -> the cap verdict on a usable entry until its hint). Only pre-9b machine rows lack the
  id; they age out of the health record (24 h / 500 rows). Our reading: acceptable for a patch release, noted in the
  wave note; say if you disagree.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace` (default features); clippy `--all-targets -D warnings`; fmt | 891ea63 | 0 | 829 passed, 0 failed; clean | completed |
| plugin harness through the shim | `harness-roster` (plugin HEAD tests, fresh binary, Windows PowerShell 5.1) | 891ea63 | 0 | 125/0 (0.3 229/0, quota 57/0, detach 49/2 pre-existing at f2ff103) | completed |

## Reading budget

No sub-agents; never print a whole file; `git diff f2ff103..891ea63 -- crates docs CHANGELOG.md` is three source
files and two docs - read the hunks; at most 5 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** The identity rule and the F70-1 residual: a case you would NOT ship in a patch release?
- **Q2.** Verdict: ACCEPT (tag v0.2.2), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
