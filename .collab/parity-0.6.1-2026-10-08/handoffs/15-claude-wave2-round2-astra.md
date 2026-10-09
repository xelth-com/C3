Write in English.

# Handoff 15 - claude: wave 2, second round for the acceptance reviewer (F09-1..F09-6 and F14-1 answered)

Date: 2026-10-09. Base commit: `fd3a59e` (branch `main`; code at 4597ec5). The range under review is
`0d78ec0..bc8d605` (wave 2d on its branch) PLUS the single commit `5c9f8a0` (wave 2f; `git show 5c9f8a0`); skip
`.collab/`. Your earlier attempt on this round (n=7) failed on the usage limit; meanwhile kimi :: k3 gave a second
opinion (handoff 14) that found one more blocker, fixed in 2f.

## Question

Your round on handoff 07 held wave 2 on F09-1..F09-5 (F09-6 minor). All six are fixed in bc8d605 with your
fixtures RC1-RC5; kimi's F14-1 (the local-only forget wrote no transaction record) is fixed in 5c9f8a0; F14-2
(case-insensitive provider match) is `wontfix` - the plugin's `Test-ReviewerMatch` and
`Resolve-CoordinatorIdentity` compare provider and model with `-ceq`, only the engine with `-eq`; C3 now does the
same and says so. Can wave 2 be accepted into the compatibility track? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta since your last review (07)

- **F09-1:** each `-c` item is split at its first `=` and the trimmed key compared; RC1
  `context_window_config_survives_a_unicode_operator_item`.
- **F09-2:** the flush decides on the deletion state only after taking BOTH the sender lock and the spool lock;
  every forget holds both locks across the DELETE; RC2 part 1
  `a_forget_failing_while_a_flush_waits_never_lets_the_flush_post_rc2`.
- **F09-3:** `forget-pending.json` is a transaction: `pending` before the DELETE, `confirmed`, `cleaning`, removed
  last; queued data goes before the references and the salt; the next flush or forget resumes with the saved
  identity (`--status` shows the phase); RC2 part 2
  `a_confirmed_delete_with_an_interrupted_cleanup_resumes_and_never_posts_the_old_instance_rc2`,
  `a_confirmed_transaction_is_finished_by_the_next_forget`. **F14-1 (2f):** the LOCAL-only forget now writes its
  `cleaning` record BEFORE the first removal; if the record cannot be written nothing is removed (exit 1); the
  failure message names the record only when it exists; tests
  `a_local_only_forget_interrupted_keeps_its_record_and_resumes_rc1` (resumed once by a flush, once by a forget),
  `a_local_only_forget_leaves_no_record_behind`, `a_local_only_forget_that_cannot_record_removes_nothing`.
  Documented limit (operator decision): a DELETE the intake carried out but whose answer was lost stays
  `pending` (404 on retry) - no `--abandon` yet; kimi suggests treating a 404 `unknown public_ref` on a retry as
  confirmation.
- **F09-4:** a failed reread returns the error and replaces nothing; a missing spool is skipped under a
  documented invariant; RC3 `outbox_reread_failure_after_a_send_keeps_the_spool_bytes_rc3`.
- **F09-5:** EVERY queued event body is rebuilt through the closed classes before sending (`close_event_body`:
  title = the purpose/mark class, `tags[]` = [provider class, model token], every detail through a closed
  vocabulary, `consult_ref` as GUID only); an event without a valid instance id or time is discarded with a
  diagnostic; RC4 `legacy_queued_events_never_send_a_private_label_rc4`, `a_current_event_closes_to_its_exact_bytes`.
- **F09-6:** one shared `resolve_coordinator_identity` (`c3-core/src/host.rs`) for the ledger coordinator and
  the rating actor: a bare label infers its sole roster model and the configured codex defaults; RC5
  `rating_actor_of_a_bare_label_infers_its_sole_roster_model_rc5`; the engine comparison case-insensitive, the
  provider case-sensitive (2f, `coordinator_provider_is_case_sensitive_the_engine_is_not`). Visible change: the
  ledger coordinator for `openai :: gpt-6-astra` now has `engine: "codex"`.
- `docs/port/wave2-telemetry.md` section "2d" (+ two 2f entries).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests | `cargo test --workspace -j 2` | bc8d605 / 5c9f8a0 | 0 | 586 / 614 | completed |
| the old bugs put back by hand | RC2-RC4; the three 2f tests | - | - | all fail as they should | completed |
| the merged main (2b + 2d) | build, tests, clippy, fmt, 6 harnesses | 1c8c6f8 | 0 | 610 tests; 0.3 229/0, companions 42/0, panel 62/0, roster 124/1 (FILE), telemetry 46/97 (by design, P7), host 55/10 | completed |

## Open findings

F09-1..F09-6 `implemented`; F14-1 `implemented`; F14-2 `wontfix` (parity). Wave 2b's F11-1..F11-5 are
`implemented` on 4b51226 (handoff 16, a separate round).

## Questions

- **Q1.** The deletion transaction with 2f: a schedule that still posts after a forget began, or loses the
  identity before cleanup completed? The "lost DELETE answer stays pending" limit - acceptable as documented, or
  does the 404-as-confirmation rule need to ship?
- **Q2.** The per-event sanitiser: a way a private label still leaves?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
