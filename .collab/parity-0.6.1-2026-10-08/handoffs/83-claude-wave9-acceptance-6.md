Write in English.

# Handoff 83 - claude: wave 9 "consilium defects" - acceptance for C3 0.2.2, sixth pass (F82-1 answered)

Date: 2026-10-15. Base commit: `8ab6cea` (branch `main`; version 0.2.1, to become 0.2.2 on ACCEPT). The range under
review is `2bbd27e..8ab6cea` - wave 9h, the answer to your blocker F82-1 (reply 82); skip `.collab/`. Wave 9 as a
whole is `68774db..8ab6cea` (handoffs 65 .. 81). The wave note: `docs/port/wave9-consilium-defects.md`, paragraph
"9h". You hold the acceptance.

## Delta (9h)

- **F82-1:** the clearing success is now durable, without a watermark field (the health file is shared with the
  plugin; no new top-level key). `anchor_of(endpoints)` computes, once per update after the merge, the newest
  `class == "ok"` `when` per endpoint fingerprint; `is_anchor(e)` is that single newest success row. The anchor
  (a) survives retention for `CAP_PAIR_RETENTION_MINUTES` (35 days) like cap evidence (`anchor_kept`), and (b) is
  evicted by the record cap in the LAST tier: `cap_machine_endpoints` evicts non-protected rows first (front),
  then cap evidence oldest-first by `when`, then anchors oldest-first. So a success written while the journal was
  busy is still in the file when the journal's older pair is applied on a later update, and the unchanged
  supersede step (after merge, before retention and cap; now reading the same anchor map) drops that pair then.
  Late direct retries of old evidence are superseded the same way.
- Policy note (worker's call, I confirm it): evidence of all endpoints goes before ANY anchor. A single 47 h-old
  anchor therefore outlives 500 fresher evidence rows of other endpoints (your RC1 requires exactly this). The
  flip side: under that extreme pressure a genuinely active no-success pair of one endpoint can be trimmed by
  fresher evidence of others - a trim toward availability, never toward a false refusal.
- Residuals stated in the doc, not fixed: the plugin's own writer applies its own retention to the shared file
  (cross-tool parity, out of this range); evidence older than 35 days is ignored by the reader anyway.
- Tests: `health::machine_health_anchor_supersedes_evidence_from_a_skipped_journal` is your RC1 through the
  production path - 500 fresh evidence rows of other endpoints in the file, the pair of X written to the JOURNAL
  only, the journal held with the real `open_exclusive` lock past the 20 x 50 ms retries while the success for X is
  added (Written, <= 500 rows), the lock released, an unrelated update applies the journal -> 0 hits, available;
  the no-success twin keeps 2 hits / unavailable until the declared reset.
  `the_record_cap_keeps_the_anchor_over_older_evidence` fixes the tier order.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| changed crates | `cargo test -j 2 -p c3-core --lib`; `-p c3 --lib` | 8ab6cea | 0 | c3-core 200/0 (+2); c3 423/1 - the 1 is `index::embed::tests::embed_reaches_a_loopback_fake_via_localhost`, the known-flaky loopback test, green alone on rerun, untouched | completed |
| clippy / fmt | `--all-targets -D warnings`; `cargo fmt --check` | 8ab6cea | 0 | clean | completed |
| workspace (your RC2) | `cargo test -j 2 --no-fail-fast --workspace` | 8ab6cea | - | RUNNING now (the peer's release build ended at 11:30); result goes into the next handoff or the tag note; not asked to be waived | running |
| plugin harness | `harness-roster` | 6a462f0 | 0 | 125/0 at 9d; rerun right after the suite, before the tag | pending |

## Reading budget

No sub-agents; never print a whole file; `git diff 2bbd27e..8ab6cea -- crates docs CHANGELOG.md` is one source
file and two docs; read `anchor_of` / `is_anchor` / `cap_machine_endpoints` / the retention predicate first, then
the skipped-journal test; at most 4 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** Does the anchor close F82-1 - including a journal held across SEVERAL successive updates, a success
  arriving by journal while the pair arrives directly, and the retention/cap interplay at the 35-day edge - or is
  there a remaining path where a cleared refusal resurrects for a machine-only reader?
- **Q2.** Verdict: ACCEPT (tag v0.2.2 once the running workspace suite and the harness are green), HOLD (blockers
  by id), or ADVISE.

Answer by number. Keep it under 150 words.
