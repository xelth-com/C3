Write in English.

# Handoff 85 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, seventh pass (F84-1 answered)

Date: 2026-10-15. Base commit: `298ed07` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `8ab6cea..298ed07` - wave 9i, the answer to your blocker F84-1 (reply 84); skip `.collab/`. Wave 9 as a
whole is `68774db..298ed07` (handoffs 65 .. 83). The wave note: `docs/port/wave9-consilium-defects.md`, paragraph
"9i". You hold the acceptance.

## Delta (9i)

- **F84-1:** exactly one anchor per endpoint. `anchor_of` now maps endpoint -> `(when, machine_record_key)`: the
  newest `class == "ok"` row, ties on `when` broken by the greater record key (lexicographic, so deterministic across
  writers; the key is endpoint|class|kind|repo|when|mark, and two rows with an identical key are already one row by
  the key dedup). `is_anchor` matches BOTH fields, so redundant equal-time successes are ordinary rows (cap tier 1)
  and cannot consume the protected tier. The supersede step reads the anchor `when` only; retention and the cap are
  unchanged otherwise.
- Test `health::redundant_equal_time_successes_do_not_evict_another_endpoints_anchor` is your schedule through the
  production path with the real `open_exclusive` journal lock: 497 distinct recent anchors + three equal-`when` Y
  successes (distinct repos, hence keys); X's pair in the journal only; journal held; X's success added (when after
  the pair, before the fillers) -> X's success persisted, rows <= 500; still held, a newer Y success -> X's success
  still persisted; released; an unrelated update applies the journal -> X 0 hits / available.
- Residual, documented not fixed: more than 500 distinct endpoints each with a live anchor still evict the oldest
  anchors (the 500-row file is the plugin-shared format limit).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| changed crate | `cargo test -j 2 -p c3-core --lib` | 298ed07 | 0 | 201/0 (+1) | completed |
| clippy / fmt | `--all-targets -D warnings`; `cargo fmt --check` | 298ed07 | 0 | clean | completed |
| workspace (your RC2) | `cargo test -j 2 --no-fail-fast --workspace` | 8ab6cea | 101 | 827 passed / 7 failed: the known-flaky `index::embed` loopback test, plus 6 in three c3-cli fake-claude integration binaries (`claude_engine`, `compat_wave2b`, `kill_record_durable`) that ran while another cargo compile loaded the machine (these binaries spawn a PowerShell fake `claude` with a 15 s start budget and are known to fail only under load; 9e-9i touch none of them). | completed, not clean |
| workspace rerun | same | 298ed07 | - | RUNNING now with nothing else on the machine; the tag waits for it and for `harness-roster` after it | running |

## Reading budget

No sub-agents; never print a whole file; `git diff 8ab6cea..298ed07 -- crates docs CHANGELOG.md` is one source file
and two docs; read `anchor_of` / `is_anchor` first, then the new test; at most 3 tool calls; the verdict even on an
incomplete read.

## Questions

- **Q1.** Does the single deterministic anchor close F84-1, or is there a remaining path (short of > 500 distinct live
  anchors) where a cleared refusal resurrects for a machine-only reader?
- **Q2.** Verdict: ACCEPT (tag v0.2.2 once the clean workspace rerun and the harness are green), HOLD (blockers by
  id), or ADVISE.

Answer by number. Keep it under 150 words.
