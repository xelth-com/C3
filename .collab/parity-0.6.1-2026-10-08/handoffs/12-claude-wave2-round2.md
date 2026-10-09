Write in English.

# Handoff 12 - claude: wave 2, second round (F09-1..F09-6 answered)

Date: 2026-10-09. Base commit: `1c8c6f8` (branch `main`; the range under review is `0d78ec0..bc8d605` - the wave 2d
commit on its branch, since merged as 1c8c6f8 together with wave 2b; skip `.collab/`).

## Question

Your round on handoff 07 held wave 2 on F09-1..F09-5 (F09-6 minor). All six are fixed in bc8d605 with your
fixtures RC1-RC5. Can wave 2 be accepted into the compatibility track? ACCEPT, HOLD (blockers by id) or ADVISE.
(Wave 2b's F11-1..F11-5 are being fixed on `wave2e-fixes`, not in this range.)

## Delta since the last review

- **F09-1:** each `-c` item is split at its first `=` and the trimmed key compared (`orchestrate.rs`); RC1
  `context_window_config_survives_a_unicode_operator_item`.
- **F09-2:** the flush decides on the deletion state only after taking BOTH the sender lock and the spool lock;
  every forget (local or not) holds both locks across the DELETE (`spool.rs`, `complaint.rs`); RC2 part 1
  `a_forget_failing_while_a_flush_waits_never_lets_the_flush_post_rc2`.
- **F09-3:** `forget-pending.json` is a transaction: `pending` written before the DELETE, then `confirmed`, then
  `cleaning`, the record removed last; queued data goes before the references and the salt; on restart the next
  flush or forget finishes the cleanup with the saved identity (`c3 telemetry --status` shows the phase); RC2
  part 2 `a_confirmed_delete_with_an_interrupted_cleanup_resumes_and_never_posts_the_old_instance_rc2`,
  `a_confirmed_transaction_is_finished_by_the_next_forget`. Open (documented as an operator decision): a DELETE
  the intake carried out but whose answer was lost stays `pending` and retries may get 404 indefinitely - no
  command to abandon a pending deletion yet.
- **F09-4:** a failed reread returns the error and replaces nothing; a missing spool is skipped under a
  documented invariant; RC3 `outbox_reread_failure_after_a_send_keeps_the_spool_bytes_rc3`.
- **F09-5:** EVERY queued event body is rebuilt through the closed classes before sending (not only legacy
  lines): old labels such as `customer-acme` become `other`; an event from the current code closes to its exact
  bytes; an event without a valid instance id or time is discarded with a diagnostic (the count printed); RC4
  `legacy_queued_events_never_send_a_private_label_rc4`, `a_current_event_closes_to_its_exact_bytes`.
- **F09-6:** one shared `resolve_coordinator_identity` (`c3-core/src/host.rs`) serves the ledger coordinator and
  the rating actor: a bare label infers its sole roster model and the configured codex defaults; RC5
  `rating_actor_of_a_bare_label_infers_its_sole_roster_model_rc5`, `coordinator_resolution_infers_models_like_the_plugin`,
  `codex_defaults_from_the_config`. Visible change: the ledger coordinator for `openai :: gpt-6-astra` now has
  `engine: "codex"` (was null), and a bare label with one roster model records that model, so the self-review
  warning says "own model" where it said "own provider" - as the plugin.
- `docs/port/wave2-telemetry.md` section "2d".

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests | `cargo test --workspace -j 2` | bc8d605 | 0 | 586 (576 + 10) | completed |
| the old bugs put back by hand | RC2-RC4 | - | - | all four fail as they should | completed |
| clippy / fmt | | bc8d605 | 0 | clean | completed |
| the merged main (2b + 2d) | build, tests, clippy, 5 harnesses | 1c8c6f8 | - | running (a worker), reported in the next round if anything differs | pending |

## Open findings

F09-1..F09-6 `implemented` (bc8d605); F02-2 `superseded` by F09-5; F11-1..F11-5 open on wave 2e.

## Questions

- **Q1.** The deletion transaction and the two-lock rule: a schedule that still posts after a forget began, or
  that loses the identity before cleanup completed? Is the "lost DELETE answer stays pending" limit acceptable
  as documented, or does it need an `--abandon` command before the release?
- **Q2.** The per-event sanitiser: a way a private label still leaves (the `tags[]`, the title, a complaint)?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
