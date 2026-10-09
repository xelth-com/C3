# Handoff 09 - Codex: wave2-diff-review

Date: 2026-10-09 03:15 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 37164f6b-99a9-4f07-a0a9-056fc0919693.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-b16e6a3e6ac44be1b087a4a493016dcc.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json fork 01a11df0-6b43-79d0-a81d-74152fe1ebf0 -` (prompt on stdin).
Parent thread: `01a11df0-6b43-79d0-a81d-74152fe1ebf0`. Result thread: `01a11e3b-363e-7951-a94f-4c1f3fa0f187` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/07-claude-wave2-diff-review.md` (sha256 cad492cb2d6a). Reviewed: 254265a, base 254265a68d2495bf3fe4e93931951ccaff3018a9, tree sha256 a4156c9ef96a, 0 changed files.
Bridge outcome: usable reply. Wall time: 275 s. Tokens: in 6930387 (cached 6011648), out 28870, reasoning 5974.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `e508a76..3605395` - the range changes 29 files, 5766 lines (5207 insertions, 559 deletions).
Verdict: HOLD - Deletion interlocks, cleanup durability, fail-open spool rewriting, legacy payload privacy, and a Unicode panic require correction before acceptance. Findings: 0 blocker, 5 major, 1 minor, 0 note (F09-1..F09-6, tracked in `findings.json`). Structured reply: `handoffs/09-codex-wave2-diff-review.reply.json`.
Raw event stream: `handoffs/09-codex-wave2-diff-review.events.jsonl`.
Verbatim reply follows.

---

1. **The original lost-update race is fixed, but the outbox is not yet sound.** Locked removal from the current file preserves concurrent appends. However, a sender can pass the deletion check before forgetting starts and subsequently send while deletion is pending (finding #2). Successful remote deletion clears its recovery record before local cleanup finishes, permitting old events to reappear after interrupted cleanup (#3). A failed reread becomes an empty replacement and can erase unsent events (#4). New envelopes correctly measure retention from queue time; legacy lines infer it from `client_time`. The existing sequential forgetting test does not establish these interleavings or interruption guarantees.

2. **New event construction closes the identified label leaks.** Reviewer and judge values use closed classes; the OpenRouter exception checks both vendor and model. `consult_ref` is independently generated and GUID-filtered. But pre-upgrade queued events bypass these constructors and retain private labels (#5), so F02-2 is only partially resolved. Rating attribution also misses the plugin’s bare-label model inference (#6). `ps_version: "unknown"` is consistent with the plugin’s fallback convention; deployed intake acceptance remains unproven. Saved judge/revision/time and commit-before-spool ordering are materially improved.

3. **Keep C3’s app identifier and physical outbox separate for now.** Sharing a directory requires compatible lock names, envelope versions, acknowledgment rules, deletion transactions, retention and sender response handling; this wave does not establish that contract. Separate transport storage and logical identity are independent choices. If alternating bridges must update one consultation’s rating history, introduce a shared logical identity protocol or persist the consultation’s telemetry owner and route later ratings accordingly. Specify how the single `telemetry_sent` marker works across owners and how forgetting covers both bridges.

   To choose, obtain the intake’s actual correlation/replacement key, app allowlist, deletion scope and the intended mixed-bridge workflow. Separate paths do **not** make the plugin harness unusable as an oracle: adapt storage/app fixtures while preserving semantic assertions and distinguishing C3 execution from copied helpers. The coverage map and telemetry shim address F02-4’s original visibility gap.

4. **HOLD for findings #1–#5.** Finding #1 additionally aborts context-budgeted consultations for valid Unicode configuration values. Finding #6 is a smaller attribution parity defect. The declared 429/400/413 and sender-deadline omissions remain exclusions from full sender parity. This was a source review with read-only inspection, not an independent rerun of the reported test suite.

## Requested checks

- **RC1:** Cwd repository root; workspace-write; add and run a `context_window_config` fixture with positive context tokens and `notify=["日本日本日本日本"]`. It must return normally and add both defaults (finding #1). Budget: one fixture, 10 minutes.
- **RC2:** Cwd repository root; workspace-write; add a barrier-controlled forgetting test: pause flush after its initial check, fail DELETE, resume flush, then retry deletion with cleanup interrupted after salt removal. Require no POST while pending and recoverable blocked cleanup after restart (#2–#3). Budget: two schedules, 25 minutes.
- **RC3:** Cwd repository root; workspace-write; inject a current-spool read failure after successful POST and a concurrent append. Require an error and unchanged queued bytes (#4). Budget: one fixture, 15 minutes.
- **RC4:** Cwd repository root; workspace-write; seed a fresh legacy event containing `customer-acme`, capture `flush_with` output, and require that label never leaves (#5). Budget: one fixture, 10 minutes.
- **RC5:** Cwd repository root; workspace-write; repeat the rating parity fixture using bare `JudgeLabel-Kimi`, with its sole model `k3`. Require saved and emitted judge `moonshot/k3/rating_actor` (#6). Budget: one fixture, 10 minutes.

---

### Findings

- **F09-1** [major] `crates/c3/src/consult/orchestrate.rs:105`, `crates/c3-core/src/roster.rs:155` - The new context-window configuration helper can panic on a valid configuration item containing Unicode because it slices arbitrary UTF-8 at the byte length of a different configuration key. Trigger: A Codex reviewer has positive context_tokens and an extra configuration item such as notify=["日本日本日本日本"]. Evidence: read-code: The helper checks byte length and then evaluates t[..key.len()] without checking a UTF-8 character boundary.; read-code: The configuration parser accepts the ASCII notify key and preserves the Unicode array literal.; ran-command: The input has 35 UTF-8 bytes; model_context_window has 20 bytes; input byte 20 is a continuation byte, so the Rust slice boundary is invalid. Verify: Call context_window_config with positive context tokens, is_codex=true, and the stated Unicode item; assert normal return and both generated default items. Remedy: Split each item at '=' and compare its trimmed key, or use boundary-safe string access; add Unicode configuration coverage.
- **F09-2** [major] `crates/c3/src/telemetry/spool.rs:228`, `crates/c3/src/telemetry/complaint.rs:280` - A flush can send retained events while deletion is pending because it checks the pending-deletion marker before acquiring the lock that serializes it with local forgetting. Trigger: Flush observes no pending record and pauses before acquiring flush.lock; forget --local acquires the locks, receives a DELETE failure, persists forget-pending.json and releases the locks; flush resumes and posts the retained events. Evidence: read-code: FORGET_PENDING is checked before flush.lock acquisition and is not rechecked under the sender or snapshot lock.; read-code: Local forgetting holds both locks and records pending deletion on failure, then returns with the spool retained.; read-code: The test starts flush only after pending deletion exists, so it does not exercise the check-before-lock interleaving. Verify: Use a barrier after the initial pending check, complete a failed local forget, then resume flush and assert the injected sender is never called. Remedy: Check deletion state after acquiring the sender lock and under the appropriate spool interlock; synchronize all operations that establish or clear the deletion state with those same locks.
- **F09-3** [major] `crates/c3/src/telemetry/complaint.rs:314`, `crates/c3/src/telemetry/complaint.rs:353` - Confirmed remote deletion is not followed by recoverable local cleanup: the pending record is removed before cleanup, and the salt is deleted before the spool, allowing surviving old-instance events to be sent again without retained retry identity. Trigger: After DELETE succeeds, cleanup removes the pending record and salt, then spool removal fails or the process terminates before removing the spool. Evidence: read-code: Successful DELETE immediately removes FORGET_PENDING.; read-code: Cleanup deletes salt before SPOOL_FILE and returns on a removal failure without creating a recovery marker.; inferred: A later flush sees no pending marker and can post surviving event bodies containing the deleted instance identifier. Verify: Confirm DELETE, interrupt cleanup after salt removal while preserving the spool, then restart and assert no old-instance POST occurs and cleanup can resume using retained identity. Remedy: Persist a deletion transaction before the remote operation, retain its identity and confirmed phase through cleanup, remove queued data before identity material, and clear the transaction only after cleanup completes.
- **F09-4** [major] `crates/c3/src/telemetry/spool.rs:307`, `crates/c3/src/telemetry/spool.rs:326` - An error rereading the current spool is treated as an empty file and can erase newly appended, undelivered events during atomic replacement. Trigger: Flush snapshots event A, a producer appends B during POST, POST succeeds, and the current-file read fails while replacement remains possible, such as a read-permission failure with directory replacement permitted. Evidence: read-code: read_to_string(...).unwrap_or_default() converts every read error to empty content, which is then passed to replace_atomic.; inferred: A successful POST triggers this rewrite even though the current file may contain events absent from the delivered snapshot. Verify: Inject a reread failure after appending B during a successful send of A; assert flush reports failure and the spool bytes remain intact. Remedy: Propagate reread errors and skip replacement whenever current contents cannot be established; handle NotFound separately only with an explicit justified invariant.
- **F09-5** [major] `crates/c3/src/telemetry/spool.rs:279`, `crates/c3/src/telemetry/spool.rs:401` - The privacy fix does not cover the upgrade backlog: legacy queued events containing private provider or model labels are still transmitted unchanged. Trigger: Before upgrading, an unsuccessful flush leaves a recent event whose provider or model is a syntactically valid private label such as customer-acme; the upgraded sender later flushes it. Evidence: ran-command: The previous consultation and rating constructors copied provider/model through label_or and safe_label rather than a closed classifier.; read-code: Legacy events are accepted using client_time and retained verbatim as the outbound body.; read-code: Batch construction directly concatenates stored bodies without applying the new privacy classes. Verify: Seed a recent pre-wave-2 raw event containing customer-acme and capture flush_with output; assert the private label is absent. Remedy: Validate and sanitize legacy payloads before delivery, conservatively mapping unresolvable labels to closed fallback classes, or explicitly discard unsafe legacy events with a local diagnostic. Supersedes: F02-2.
- **F09-6** [minor] `crates/c3/src/telemetry/classes.rs:685`, `crates/c3-core/src/host.rs:190`, `crates/c3-cli/tests/telemetry_parity.rs:508` - Rating actor resolution omits the plugin's model inference for bare provider labels, permanently saving and transmitting judge.model='other' when the roster identifies a unique known model. Trigger: CODEX_CONSULT_COORDINATOR is JudgeLabel-Kimi, its sole roster entry has model k3, and its provider endpoint is api.kimi.ai; C3 produces moonshot/other/rating_actor instead of moonshot/k3/rating_actor. Evidence: read-code: rating_actor uses parse_coordinator_matcher and build_coordinator; a bare label leaves the model unset without unique-roster-model inference.; ran-command: Get-TelemetryRatingActor uses Resolve-CoordinatorIdentity with Codex defaults; that resolver infers a unique roster model and supplies applicable configured defaults.; read-code: The test described as a roster-label case supplies JudgeLabel-Kimi :: k3, bypassing missing-model inference. Verify: Repeat the rating parity fixture with the bare provider label and require moonshot/k3/rating_actor in both the saved mark and event. Remedy: Use a full shared coordinator resolver matching the pinned plugin; cover bare labels, omitted models with configured defaults, and ambiguous multiple-model labels.

### Prior findings

- F02-1 - fixed - The original concurrent-append overwrite is repaired by locked removal from the current file. Finding #4 concerns a separate reread-error path.
- F02-2 - still-open - New event construction is fixed; finding #5 supersedes the broad claim with the remaining legacy-backlog exposure.
- F02-3 - fixed - Failed DELETE now preserves retry identity and proof. Findings #2 and #3 concern separate synchronization and post-confirmation cleanup defects.
- F02-4 - fixed - The oracle coverage map distinguishes C3, helper, documentation and unshimmed checks, and codex-telemetry.ps1 now forwards to C3. This does not establish that all semantic assertions pass.
- F02-5 - fixed - The state document acknowledges existing commit-before-spool ordering; the revised rating path also commits before enqueueing.
- F02-6 - not-checked - The earlier anchor implementation was not revalidated in this telemetry review.
- F02-7 - not-checked - The current roster accepts plan on any engine, but cross-engine and cross-repository quota behavior was not audited here.
- F02-8 - fixed - State plan P4 explicitly treats native Messages as a benchmark-gated candidate and distinguishes the plugin's measured route; the benchmark result remains unproven.
- F04-1 - not-checked - Outside this review range; the recorded earlier closure was not independently revalidated.
- F04-2 - not-checked - Outside this review range; no Windows local-zone boundary tests were rerun.
- F04-3 - not-checked - Required-light ledger metadata was not revalidated; this range's panel change forwards the telemetry switch.
- F06-1 - still-open - The state document retains the historical Samoa case for wave 3; this range contains no health-parser correction.

## Verdict: HOLD

Deletion interlocks, cleanup durability, fail-open spool rewriting, legacy payload privacy, and a Unicode panic require correction before acceptance.

### Blockers

_(none)_

### Unproven scenarios

- No build, test suite, live intake request, or filesystem mutation was performed during this review.
- The deletion schedules, cleanup interruption and reread failure are established from source; their deterministic runtime reproductions remain requested.
- Deployed intake acceptance of app_id c3, ps_version unknown and the HTTP/OpenRouter extensions was not verified.
- Mixed-bridge correlation, replacement and deletion semantics remain unspecified and untested.
- The reported 576 passing tests and harness results were read as supplied evidence, not independently reproduced.

### First-run checklist (observable)

_(none)_
