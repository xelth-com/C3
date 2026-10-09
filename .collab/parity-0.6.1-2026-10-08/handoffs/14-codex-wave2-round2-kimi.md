# Handoff 14 - Codex: wave2-round2-kimi

Date: 2026-10-09 04:46 local. Author: Codex (model k3, effort high), Codex CLI 0.155.1.
Reviewer: kimi :: k3 (provider from -Provider, model from -Model; endpoint https://api.kimi.ai/coding/v1, wire_api: responses; provider fingerprint 8f7901d404b5; harness codex-cli 0.155.1).
Preflight: ok: env KIMI_API_KEY set.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 8 of 12 for -Provider kimi (nothing applied).
Effort: high sent (requested high, mapping kimi-v1, by caps-v1: api.kimi.ai, k3; not confirmed by the provider). Consultation id: fb034faf-625c-4cba-8eb1-ac3bda0f5247.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m k3 -c model_reasoning_effort="high" -c model_provider="kimi" -c model_context_window=256000 -c model_auto_compact_token_limit=204800 -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-1f35abc6f5ea4e8f84331168d3905930.md -` (prompt on stdin).
Parent thread: (none - new thread; no thread of lineage kimi :: k3 in this task's ledger; other lineage(s): openai :: gpt-6-astra). Result thread: `01a11e8d-d63f-72f2-945d-71e9a6088e76` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/12-claude-wave2-round2.md` (sha256 f3d531bef1f0). Reviewed: a624a3d, base a624a3d215ef2afc10a3e35777175f131b315e39, tree sha256 5e60e364af50, 0 changed files.
Bridge outcome: usable reply. Wall time: 1624.4 s. Tokens: in 3392137 (cached 3236224), out 21358, reasoning 14505.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `0d78ec0..bc8d605` - the range changes 11 files, 2137 lines (1888 insertions, 249 deletions).
Verdict: HOLD - All six F09 fixes are correctly implemented and adversarially tested, but the deletion transaction never writes its record on the local-only forget branch, so an interrupted local deletion leaves no blocker, no resume, and a false operator message. Findings: 1 blocker, 0 major, 1 minor, 0 note (F14-1..F14-2, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/14-codex-wave2-round2-kimi.reply.json`.
Raw event stream: `handoffs/14-codex-wave2-round2-kimi.events.jsonl`.
Verbatim reply follows.

---

## Q1 - Deletion transaction and two-lock rule

For every path that touches the intake, the design holds up under adversarial reading. The flush decides the deletion state under BOTH locks and keeps the sender lock across the POST (spool.rs:283-335), while every write of `forget-pending.json` happens inside a forget that holds both locks across the DELETE (complaint.rs:485-496). Because the state cannot change while the sender lock is held, the F09-2 interleaving is genuinely closed; RC2 part 1 proves it with the `after_check` barrier. Lock order is uniform (FLUSH then SPOOL; producers take SPOOL only), so no deadlock cycle. The record is written before the DELETE, moved `pending` -> `confirmed` -> `cleaning`, removed last; queued data is removed before the proof and the salt; resume works from either a flush or a forget, with the retained identity. RC2 parts 2-3 cover the crash shapes.

One hole, and it is a blocker (finding #1): a LOCAL-ONLY forget (no `-PublicRef`, no existing record) builds its transaction in memory with `phase: cleaning` and never writes it - `run_local_cleanup` only persists the record when `own_record && phase != cleaning` (complaint.rs:240), so the `None` branch (complaint.rs:542) leaves no record at all. A crash or a failed removal mid-cleanup then leaves no blocker and no resume: the next flush happily spools and sends, leftover `salt`/`refs.ndjson` survive, and the failure message (complaint.rs:673) falsely tells the operator a deletion record keeps the instance and blocks telemetry. The doc comments promise exactly the record this branch omits. Fix: write the cleaning record before `finish_cleanup` in that branch.

The lost-DELETE-answer limit (retry gets 404 forever, record stays `pending`) is acceptable as a documented operator decision for this track; a cheap follow-up is to treat a 404 `unknown public_ref` on a retry as confirmation rather than adding an `--abandon` command now. Not a blocker.

## Q2 - Per-event sanitiser

No leak found. `close_event_body` rebuilds every queued event field-by-field: `title` is the closed purpose/mark class, `tags[]` is `[provider class, model token]`, and every detail (`engine`, `outcome`, `topic_tag`, `judge`, versions, `os`, `consult_ref` as GUID only) passes a closed vocabulary - a legacy `customer-acme` becomes `other`. Non-`event` spool kinds are kept but never sent. Complaints are user-authored free text shown in full before sending (rule 3), so they are out of the sanitiser's scope by design. Note: the close-then-compare equality fast path preserves exact bytes, so any future constructor drift is caught by `a_current_event_closes_to_its_exact_bytes`.

## Q3 - Verdict

HOLD - one blocker, finding #1 (local-only forget writes no transaction record). Everything else in the range, including F09-1's key-split, F09-4's reread rule, F09-5's backlog closing and F09-6's shared resolver (minus a minor case-sensitivity doc mismatch, finding #2), is sound and well fixtured.

## Requested checks

- RC1 (finding #1): add a test calling `telemetry::forget_with` with `ForgetRequest{public_ref: None, local: true, yes: true}` and a `remove` that fails on `spool.ndjson`; observe `forget-pending.json` absent while the message claims it exists. Working dir `C:\Users\Dmytro\C3`, workspace-write (cargo builds), budget 15 min.
- RC2 (pending row in the brief): `cargo test --workspace -j 2` plus the 5 harnesses at `1c8c6f8`; observe exit 0 and the 586-test count. Working dir `C:\Users\Dmytro\C3`, workspace-write, budget 30 min.

---

### Findings

- **F14-1** [blocker] `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Trigger: c3 forget-me --local (or ForgetRequest{public_ref: None, local: true}) with no pending record, where a file removal fails (Windows file lock, permissions) or the process dies mid-cleanup. Evidence: read-code: The None arm builds PendingDeletion{phase: PHASE_CLEANING} and calls finish_cleanup(own_record=true) without any write_transaction.; read-code: run_local_cleanup writes the cleaning record only when txn.phase != PHASE_CLEANING; for the local-only caller the condition is false, and the final record removal tolerates NotFound.; read-code: On cleanup error with own_record the message says 'the deletion record ... keeps instance ... - nothing is spooled or sent until the next flush or c3 forget-me finishes it', which is false on this branch; out.pending is set from pending_deletion_in, which is None.; read-code: Docs state a local-only forget starts in the cleaning phase of the transaction record.; ran-command: No test exercises a failing local-only cleanup; RC2 tests all use a public_ref path. Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F14-2** [minor] `crates/c3-core/src/host.rs:297`, `crates/c3-core/src/host.rs:267` - The new coordinator resolver documents ordinal (case-insensitive) provider matching but compares with case-sensitive ==: provider == defaults.provider and the roster hits filter e.provider == provider. A CODEX_CONSULT_COORDINATOR label whose case differs from the config's model_provider or the roster entry ('OpenAI' vs 'openai') silently forgoes the configured-model inference the plugin would make. Trigger: CODEX_CONSULT_COORDINATOR='OpenAI' with model_provider = "openai" in config.toml (or the reverse), bare label form. Evidence: read-code: Doc comment says 'Test-ReviewerMatch ... ordinal' and '(defaults.provider, ordinal)'; the code uses String ==.; read-code: parse_reviewer_matcher_grammar lowercases the engine but keeps the provider as typed. Verify: Unit test: resolve_coordinator_match("OpenAI", None, &CodexDefaults{provider: "openai", model: "gpt-6-astra"}) should yield model Some("gpt-6-astra") if ordinal semantics are intended. Remedy: Use eq_ignore_ascii_case for the provider comparisons (and the engine comparison for symmetry), or correct the doc comments if case-sensitivity is deliberate.

### Prior findings

- F02-1 - fixed - Flush snapshot under spool lock, sender lock held across the POST, rewrite removes only delivered lines from the current file; RC2/RC3 exercise the interleavings.
- F02-3 - fixed - Failed DELETE keeps the transaction record with instance_id and public_ref; retries reuse the same identity.
- F02-4 - not-checked - Harness shim files untouched in 0d78ec0..bc8d605.
- F02-5 - not-checked - findings_tool/state.md not in this range.
- F02-6 - not-checked - Prompt anchor work not in this range.
- F02-7 - not-checked - roster.rs unchanged in this range.
- F02-8 - not-checked - Proposal finding, outside the diff.
- F04-1 - not-checked - liveness/proc.rs not in this range.
- F04-2 - not-checked - health.rs not in this range.
- F04-3 - not-checked - panel files not in this range.
- F06-1 - not-checked - health.rs not in this range.
- F09-1 - fixed - split_once('=') with trimmed case-insensitive key; RC1 test covers the Unicode item.
- F09-2 - fixed - Decision under both locks, sender lock held across the POST, every forget holds both locks across the DELETE; RC2 part 1.
- F09-3 - fixed - Transaction record before the DELETE, phase machine, record removed last, resume by flush or forget; but the local-only branch never writes a record - filed as finding #1.
- F09-4 - fixed - Reread error aborts the rewrite and returns the error; NotFound skipped under a documented invariant; RC3.
- F09-5 - fixed - Every queued event closed through classes::close_event_body; unclosable discarded with diagnostic; RC4.
- F09-6 - fixed - One shared resolve_coordinator_identity for the ledger and the rating actor, with roster-model and config-default inference; RC5. Minor case-sensitivity doc mismatch filed as finding #2.
- F11-1 - not-checked - Assigned to wave2e-fixes, outside this range.
- F11-2 - not-checked - Assigned to wave2e-fixes, outside this range.
- F11-3 - not-checked - Assigned to wave2e-fixes, outside this range.
- F11-4 - not-checked - Assigned to wave2e-fixes, outside this range.
- F11-5 - not-checked - Assigned to wave2e-fixes, outside this range.

## Verdict: HOLD

All six F09 fixes are correctly implemented and adversarially tested, but the deletion transaction never writes its record on the local-only forget branch, so an interrupted local deletion leaves no blocker, no resume, and a false operator message.

### Blockers

- **F14-1** `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.

### Unproven scenarios

- The merged main (1c8c6f8, 2b+2d) build/test/harness run is still pending per the brief; this review covered the wave 2d diff and the merged tree statically, not a fresh test run.
- The intake's actual 404 semantics on a retried DELETE (lost-answer case) are assumed from delete_instance's error mapping, not observed against a real server.
- Real OS-level crash behavior (power loss mid-cleanup, lock release on process death) is covered only by injected-failure tests and synthetic records.
- Whether a spool line of a kind other than 'event' (kept untouched forever) could accumulate unboundedly was not assessed.

### First-run checklist (observable)

- [ ] c3 telemetry --status names the transaction phase (pending/confirmed/cleaning) when a forget-pending.json exists.
- [ ] A flush overlapping a forget prints the skipped reason and posts nothing (check last-flush.json and the intake access log).
- [ ] A failed DELETE exits 3, leaves forget-pending.json with instance_id and public_ref, and blocks spooling on the next consultation.
- [ ] A flush over a legacy-queued event prints the discarded count locally and the intake receives only closed-class labels.
- [ ] A rating with a bare CODEX_CONSULT_COORDINATOR label records the roster's sole model in both the mark and the event.
