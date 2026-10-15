Write in English.

# Handoff 79 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, fourth pass (F78-1, F78-2 answered)

Date: 2026-10-15. Base commit: `51dfc13` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `c82d97d..51dfc13` - wave 9f, the answer to your blockers F78-1 and F78-2 (reply 78); skip `.collab/`.
Wave 9 as a whole is `68774db..51dfc13` (handoffs 65 .. 77). The wave note: `docs/port/wave9-consilium-defects.md`,
paragraph "9f". You hold the acceptance.

## Delta (9f)

- **F78-1:** `plan_cap_verdict` no longer pairs with the newest hit only. The in-period distinct-id hits are sorted
  newest first and scanned: the first `second` that has an older partner within 60 min forms the pair (so the NEWEST
  qualifying pair drives `second` / `first` / `until` / `gap_min`); a later isolated 429 cannot hide an established
  pair. Your RC1 is the regression: hits 20:00Z / 20:20Z / 22:00Z (three distinct ids), read at 22:11Z -> still
  unavailable until 2026-10-26T16:00Z.
- **F78-2:** `update_machine_health` retention is `recent || still_out || evidence_kept`, where a row is cap
  evidence iff it is exactly what `endpoint_health` counts (`consult_id` non-empty, `class == "quota"`,
  `kind == "burst"` - `is_cap_evidence`) and `evidence_kept` holds for `CAP_PAIR_RETENTION_MINUTES` (35 days) from
  the row's instant regardless of an expired burst `until`. The 500-record cap (`cap_machine_endpoints`) evicts
  non-evidence rows first (front to back) and touches the oldest evidence rows only when the overflow exceeds the
  non-evidence count. The replay-guard comment is updated (evidence keys persist longer, which only strengthens the
  applied-mark dedup). Your RC2 is the regression through the production path: a pair seeded only in a scratch
  machine-health file with expired burst `until` and no quota window, an unrelated update at +48 h, then the
  conversion read without ledger rows -> the hinted refusal remains; plus the cap-ordering test (evidence kept when
  plain rows exist; the oldest evidence goes only when nothing else can).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| changed crates | `cargo test -j 2 -p c3-core -p c3 --lib` | 51dfc13 | 0 | c3-core 197/0 (plan:: and the two new health tests among them), c3 424/0 (incl. the providers cap-pair test) | completed |
| clippy / fmt | `--all-targets -D warnings`; `cargo fmt --check` | 51dfc13 | 0 | clean | completed |
| workspace | `cargo test -j 2 --no-fail-fast --workspace` | 51dfc13 | - | NOT run: a peer session's `cargo test --release` holds the machine (5 GB free); the full compile beside it was OOM-killed. The uncovered remainder is the c3-cli fake-claude integration binaries, untouched by 9e/9f and green alone. Will be rerun before the tag. | not run |
| plugin harness | `harness-roster` | 6a462f0 | 0 | 125/0 at 9d; not rerun at 9e/9f (same reason); rerun before the tag | not rerun |

## Reading budget

No sub-agents; never print a whole file; `git diff c82d97d..51dfc13 -- crates docs CHANGELOG.md` is two source files
and two docs; read the `plan_cap_verdict` hunk and the `is_cap_evidence` / `cap_machine_endpoints` / retention hunks
first; at most 4 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** A remaining path where the cap verdict fires on a usable entry, or stays silent on an exhausted one,
  beyond the documented residuals (pre-9b no-id rows; one consultation's repeats = one hit)?
- **Q2.** Verdict: ACCEPT (tag v0.2.2 after the workspace suite and the harness are rerun green), HOLD (blockers by
  id), or ADVISE.

Answer by number. Keep it under 150 words.
