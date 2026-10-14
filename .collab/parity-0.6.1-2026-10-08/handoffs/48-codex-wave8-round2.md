# Handoff 48 - Codex: wave8-round2

Date: 2026-10-14 06:56 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: f3baa0b3-efb7-4825-a184-2de402273ffd.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-9e26bd2202fb42f5b67eea7285831cc3.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a138c4-7ffb-7212-b3e4-362338d1846d` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/47-claude-wave8-round2.md` (sha256 30e9dad40800). Reviewed: e4690dc, base e4690dcc7d870fd8c069de0fdedf6d70a99be1ae, tree sha256 bf74d0ac3a15, 0 changed files.
Bridge outcome: usable reply. Wall time: 219 s. Tokens: in 237718 (cached 168320), out 4400, reasoning 650.
Quota (rollout): 5h 21% until 11:10; wk 39% until Oct 20; this run 237718 input tokens (168320 cached) in 6 calls, max 47777 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 70 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `bf1bd4a..c23869d` - the range changes 13 files, 1588 lines (1586 insertions, 2 deletions).
Verdict: HOLD - The new equal-time comparison can still discard a blocking weekly window because it prioritizes primary usage over secondary usage. Findings: 0 blocker, 1 major, 0 minor, 0 note (F48-1, tracked in `findings.json`). Structured reply: `handoffs/48-codex-wave8-round2.reply.json`.
Raw event stream: `handoffs/48-codex-wave8-round2.events.jsonl`.
Verbatim reply follows.

---

1. **Yes—finding #1.** At the same observation time, A=(primary 30%, secondary 99%) loses to B=(primary 31%, secondary 10%) in either recorded order. With future resets, A blocks and B does not. The same issue affects live replacement. Contrary to the handoff, `newer_wins` compares `max(primary, secondary)`; the new helper compares a lexicographic tuple. A blocking observation selected from sub-agents can therefore lose during endpoint selection.

   The original primary-only F46-1 example is fixed, but its broader blocking-window invariant remains incomplete. Both inspected tests omit competing secondary windows.

2. **HOLD; do not tag v0.2.1 yet.** Resolve finding #1 and run RC1. Two existing test-executable cases passed: the new recorded primary tie test and the existing `newer_wins` test. These were prebuilt binaries; their exact source revision was not established. No files were written.

Reading covered all three non-`.collab` changed paths and targeted aggregation/health context. `.collab` changes were skipped as instructed, apart from handoffs 47 and 45. The broader wave-8 implementation, external plugin sources/harnesses, and unrelated prior findings were not re-reviewed. Checkout HEAD was `e4690dc`, beyond the requested endpoint; the changed hunks were read from the explicit range.

## Requested checks

- **RC1:** From repository root, with **workspace-write** permission, add equal-time fixtures A=(30,99), B=(31,10), both resets future; exercise both recorded orders, both recorded/live assignments, and sub-agent aggregation feeding endpoint selection. Run `cargo test -p c3-core quota -j 2`. Every path must preserve weekly blocking. Budget: 30 minutes.

---

### Findings

- **F48-1** [major] `crates/c3-core/src/quota.rs:992`, `crates/c3-core/src/quota.rs:1036`, `crates/c3-core/src/quota.rs:1052`, `crates/c3-core/src/quota.rs:619`, `docs/port/wave8-quota.md:91` - The replacement equal-time rule can hide a blocking secondary window: lexicographic comparison prioritizes any higher primary percentage over a 99% secondary percentage. It also differs from the existing rollout/sub-agent rule, which compares the maximum usage across both windows. Consequently, an endpoint can remain available despite an equally recent blocking weekly observation. Trigger: For the same endpoint and observed_at, observation A has primary=30 and secondary=99, while B has primary=31 and secondary=10; all reset times are in the future. Recorded selection chooses B in either order. Recorded B also rejects live A, and live B replaces recorded A. Evidence: read-code: window_obs_wins compares (primary used_percent, secondary used_percent) with tuple > and is used by both recorded selection and live replacement.; read-code: Observation::fullest returns max(primary, secondary); newer_wins uses that value for equal timestamps in sub-agent and account-rollout selection.; read-code: Either window blocks at usage >=98 with a future reset, but health evaluates only the observation retained by endpoint_window.; inferred: The tuple comparison always selects B, which has no blocking window, although A has a blocking weekly window.; ran-command: Both tests passed. Their inspected fixtures do not test competing secondary usage, so these passes do not establish the requested invariant; binary revision was not verified. Verify: Run RC1 with both input orders and both recorded/live assignments; require the resulting endpoint quota to retain the 99% weekly block. Remedy: Use one consistent equal-time selection policy across recorded, live, and sub-agent aggregation that preserves the blocking secondary observation; align the endpoint helper with the existing fullest-window rule for this case and add cross-window regression tests. Correct the documentation's assertion that the current rules are equivalent. Supersedes: F46-1.

### Prior findings

- F02-1 - not-checked - Outside this review's quota delta.
- F02-3 - not-checked - Outside this review's quota delta.
- F02-4 - not-checked - Harness coverage map not audited.
- F02-5 - not-checked - Rating transaction not inspected.
- F02-6 - not-checked - Prompt generation not inspected.
- F02-7 - not-checked - Plan compatibility not inspected.
- F02-8 - not-checked - No three-route benchmark reviewed.
- F04-1 - not-checked - Process classification not inspected.
- F04-2 - not-checked - Windows timezone behavior not tested.
- F04-3 - not-checked - Panel metadata not inspected.
- F06-1 - not-checked - Historical timezone behavior not inspected.
- F09-1 - not-checked - Context configuration not inspected.
- F09-2 - not-checked - Telemetry locking not inspected.
- F09-3 - not-checked - Deletion cleanup not inspected.
- F09-4 - not-checked - Spool reread handling not inspected.
- F09-5 - not-checked - Legacy telemetry sanitization not inspected.
- F09-6 - not-checked - Rating actor inference not inspected.
- F11-1 - not-checked - Health journal I/O not inspected.
- F11-2 - not-checked - Required-reviewer launcher resolution not inspected.
- F11-3 - not-checked - Shared-plan required-reviewer filtering not inspected.
- F11-4 - not-checked - Hook command execution not inspected.
- F11-5 - not-checked - AGY stall handling not inspected.
- F14-1 - not-checked - Local deletion transactions not inspected.
- F19-1 - not-checked - Complaint/deletion synchronization not inspected.
- F19-2 - not-checked - Salt cleanup errors not inspected.
- F19-3 - not-checked - Duplicate-key sanitization not inspected.
- F22-1 - not-checked - Health journal replay retention not inspected.
- F23-1 - not-checked - Process metadata failure handling not inspected.
- F23-2 - not-checked - Unknown creation-time recovery not inspected.
- F23-3 - not-checked - Kill-evidence durability not inspected.
- F23-4 - not-checked - Recovery fixtures not inspected.
- F23-5 - not-checked - Production Samoa behavior not tested.
- F24-1 - not-checked - Not-spooled fallback accounting not inspected.
- F24-2 - not-checked - Unterminated suffix accounting not inspected.
- F24-3 - not-checked - Concurrent cleanup enumeration not inspected.
- F24-4 - not-checked - Not-spooled status baseline not inspected.
- F24-5 - not-checked - Plugin-home adapter coverage not inspected.
- F24-6 - not-checked - Fold record rewrite handling not inspected.
- F25-2 - not-checked - Endpoint transcript guard not inspected.
- F25-3 - not-checked - Claude model case handling not inspected.
- F25-4 - not-checked - Child environment debug formatting not inspected.
- F30-1 - not-checked - Unknown-tree helper recovery not inspected.
- F30-2 - not-checked - Pre-kill persistence not inspected.
- F30-3 - not-checked - Kill-site durability coverage not inspected.
- F31-1 - not-checked - Live-producer fallback accounting not inspected.
- F36-1 - not-checked - Unicode path handling not inspected.
- F36-2 - not-checked - Transcript path normalization not inspected.
- F38-1 - not-checked - Recovery persistence fallback not inspected.
- F40-1 - not-checked - Recovery-copy enumeration errors not inspected.
- F40-2 - not-checked - Kick acknowledgement failure handling not inspected.
- F42-1 - not-checked - Recovery-copy metadata errors not inspected.
- F46-1 - still-open - The primary-only 30%/99% example is fixed. Finding #1 supersedes the broader claim with the remaining secondary-window failure and inconsistent aggregation rules.

## Verdict: HOLD

The new equal-time comparison can still discard a blocking weekly window because it prioritizes primary usage over secondary usage.

### Blockers

- **F14-1** (prior, not-checked) `crates/c3/src/telemetry/complaint.rs:542`, `crates/c3/src/telemetry/complaint.rs:240`, `crates/c3/src/telemetry/complaint.rs:673` - A local-only forget (no public_ref, no existing transaction record) never writes forget-pending.json: the transaction is constructed in memory with phase 'cleaning' and run_local_cleanup skips its write because its guard requires phase != cleaning. An interrupted local-only cleanup (crash or failed removal) therefore leaves no record: nothing blocks spooling or sending, nothing resumes the cleanup, identity files (salt, refs.ndjson) may survive, and the failure message falsely claims a deletion record keeps the instance and blocks telemetry until the next flush or forget finishes it - the next flush sends instead. This contradicts the wave's documented invariant ('a local-only deletion starts here', 'an interruption leaves a record that blocks spooling and sending'). Verify: Add a test: forget_with with ForgetRequest{public_ref: None, local: true, yes: true} and an injected remove failing on spool.ndjson; assert whether forget-pending.json exists and whether a subsequent flush posts the retained events. Remedy: In the None arm, persist the transaction (phase cleaning) via write_transaction before calling finish_cleanup (or pass phase confirmed so run_local_cleanup writes it), so an interruption leaves the blocking, resumable record the docs and the CLI message promise.
- **F22-1** (prior, not-checked) `crates/c3-core/src/health.rs:1563`, `crates/c3-core/src/health.rs:1915`, `crates/c3-core/src/health.rs:1942`, `crates/c3-core/src/health.rs:1960`, `crates/c3-core/src/health.rs:2028` - After failed `.bad` archival, the retained journal is replayed against mutable endpoint state whose record keys can be removed by the 24-hour or 500-record retention rules. A journal record can therefore be applied, evicted, and re-applied with a different final effect, while a record already stale on a replay is parsed but never becomes visible. The record-key dedup is not durable, so the documented harmless/once replay invariant is false. Verify: Add a deterministic test with an unwritable `.bad` path, one recent journal record, and 500 newer endpoint records; run two updates and assert the journal record is neither resurrected nor allowed to displace another endpoint. Remedy: Persist a monotonic journal applied-sequence marker or equivalent durable consumed state, and truncate the journal by that sequence independently of endpoint retention; do not use the mutable endpoint set as the only replay ledger.
- **F23-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:97`, `crates/c3/src/liveness/proc.rs:100`, `crates/c3/src/liveness/pending.rs:283`, `crates/c3/src/liveness/pending.rs:344` - E19 can drop a live task process: `process_info` collapses a failed name/parent lookup to `None`, which the pending re-check treats as `gone`, even when `process_start_iso` proved the pid exists with a readable start time. Verify: Inject `name_and_parent -> None` while `process_start_iso -> Some(start)` and require `test_unverified_process`/`test_recorded_process` to return running or an explicit inspection failure. Remedy: Distinguish process absence from inspection failure; return an unknown/running verdict when the pid exists but metadata cannot be read.
- **F23-2** (prior, not-checked) `crates/c3/src/liveness/proc.rs:753`, `crates/c3/src/liveness/proc.rs:775`, `crates/c3/src/liveness/proc.rs:827`, `crates/c3/src/liveness/pending.rs:790` - The unknown-tree release rule is fail-open for live processes with unknown creation/start time: both by-parent and machine-wide scans skip `created: None`, so `kill_unconfirmed` can be released while such a process still runs. Verify: Run `test_pending_active` on a `kill_unconfirmed` record with a live scan row whose `created` is `None`; require refusal/failed verification, never release. Remedy: Treat unknown creation/start as blocking evidence or fail the scan; do not silently filter those rows.
- **F23-3** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:5105`, `crates/c3/src/consult/orchestrate.rs:5143`, `docs/port/wave3a-recovery.md:59` - The kept recovery evidence is not durable at the kill: C3 writes `survivors`, `unverified`, and `kill_unconfirmed` only once at the end of the run, so a crash or forced bridge termination in that interval leaves the pending record without the evidence describing the surviving tree. Verify: Add a pause between the kill and the end-of-run pending write, terminate the bridge there, and inspect the recovery record for the expected `unverified`/`kill_unconfirmed` fields. Remedy: Persist the kill disposition at each kill site, or write a durable kill journal before continuing the run.
- **F24-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:640`, `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:704` - The spool-lock fallback marks all non-legacy not-spooled lines as seen without folding or retaining per-file provenance; a later fold counts the same gone-producer files again, so the accounting can double-count and temporarily hide the count. Verify: Hold `spool.lock` for over one second during flush 1 with N gone-producer lines, then run flush 2 and compare both records; require exactly one accounting of N. Remedy: In the fallback, retain per-file folded entries or mark only positively kept files seen; never advance the global baseline for files that remain foldable.
- **F25-2** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:357`, `crates/c3/src/engines/claude_auth.rs:374` - Endpoint mode cannot enforce the transcript-location safety guard because it never populates `projectsDirectory`; transcripts may land inside the repository and alter the tree being reviewed. Verify: Make endpoint-mode `auth status` report an in-repository projects directory and require refusal before spawn. Remedy: Probe the status facts for endpoint mode too, without reading or logging the token, and reject unsafe transcript locations.
- **F30-1** (prior, not-checked) `crates/c3/src/liveness/proc.rs:815`, `crates/c3/src/liveness/pending.rs:297`, `crates/c3/src/liveness/pending.rs:902` - Unknown-tree release remains fail-open for a live reviewer helper whose creation time is unreadable and whose name or command line does not match the codex rule, especially when it is a grandchild below an unrecorded intermediate process. Verify: Run `test_pending_active_with` with the wrapper/helper synthetic table and require the record to remain active; repeat with a readable non-codex command line. Remedy: Scan full descendant relationships and treat unknown-start or command-line-unreadable/generic-runtime rows as blocking unless positive evidence proves they are unrelated.
- **F30-2** (prior, not-checked) `crates/c3/src/engines/subprocess.rs:536`, `crates/c3/src/consult/orchestrate.rs:587`, `crates/c3/src/consult/orchestrate.rs:4927`, `crates/c3/src/consult/orchestrate.rs:5674` - Kill evidence is not durable at the actual kill: the processes are killed before the kept record is written, and write failures are ignored while execution continues. Verify: Add a pause between `kill_tree_checked` and `KillSite::write`, terminate there, and separately inject `write_pending` errors before continuation or commit. Remedy: Persist the kill record synchronously from the kill path or an append-only sidecar before returning, and fail closed when that persistence fails.
- **F31-1** (prior, not-checked) `crates/c3/src/telemetry/spool.rs:668`, `crates/c3/src/telemetry/notspooled.rs:886`, `crates/c3/src/telemetry/notspooled.rs:950` - The lock-busy fallback records live producers' existing lines as seen without recording per-file provenance; when such a producer later exits, the next fold counts its entire file again as a gone producer, double-counting those lines across the record history. Verify: Run the RC1 schedule and compare the first record’s `not_spooled_seen`, the second record’s fold note, and the total accounting for the N lines. Remedy: Persist per-file seen byte/line prefixes or carry the fallback’s kept-file provenance into the next fold, and subtract those already-seen lines when folding a formerly-live producer.
- **F36-1** (prior, not-checked) `crates/c3/src/engines/claude_auth.rs:425`, `crates/c3/src/engines/claude_auth.rs:540`, `crates/c3-core/src/paths.rs:41` - The new Claude transcript guard can panic on a valid Unicode configuration path because repo_relative slices the candidate string at the byte length of an unrelated repository prefix without checking a UTF-8 boundary. Verify: Run RC1 and assert the guard returns normally for the stated paths. Remedy: Use boundary-safe prefix comparison, such as str::get before comparing, or an appropriate path-component comparison; add the Unicode regression.
- **F38-1** (prior, not-checked) `crates/c3/src/consult/orchestrate.rs:687`, `crates/c3/src/engines/subprocess.rs:912`, `crates/c3/src/liveness/pending.rs:1026`, `crates/c3/src/liveness/proc.rs:878`, `crates/c3-cli/tests/kill_record_durable.rs:589` - The fallback narrows but does not close the recovery durability failure: if both pre-kill writes fail and the bridge dies after killing an unrecorded intermediate, recovery can release the task while that intermediate's non-Codex helper remains alive. The surviving launch record contains insufficient ancestry, and the in-memory KillRecord::Lost result does not survive the bridge. Verify: Run RC1 from Requested checks with an isolated process table or controlled Windows tree, proving the helper remains alive and requiring the restarted task to refuse before spawning another engine turn. Remedy: Make an interrupted launch remain conservatively blocked when complete kill evidence cannot be persisted. Establish that recovery obligation durably before spawning, or require explicit operator release of an unresolved launch record instead of treating these empty scans as proof of termination. Add the missing dead-intermediate regression.
- **F40-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:170`, `crates/c3/src/liveness/pending.rs:173`, `crates/c3/src/consult/recovery.rs:71` - Recovery silently treats failure to enumerate the Codex-home kill-copy directory, or errors while iterating it, as absence of recovery copies. A durable copy naming a live helper can therefore be omitted and the task released. Verify: Extend the existing RC1 fixture to inject PermissionDenied and an entry-iteration error during kill-copy discovery; require recovery to refuse before writing the engine-turn marker while the helper remains alive. Remedy: Return discovery errors to all recovery consumers and refuse on non-NotFound directory or entry errors. Distinguish a genuinely absent copy directory from failed inspection; do not flatten away iteration errors.
- **F42-1** (prior, not-checked) `crates/c3/src/liveness/pending.rs:306`, `crates/c3/src/consult/recovery.rs:88`, `crates/c3/src/liveness/pending.rs:380` - A metadata inspection error on an enumerated recovery copy is treated as absence. read_pending_file uses Path::exists(), which converts metadata errors to false, and returns exists=false with no error. Recovery and findings checks then skip the copy, potentially releasing a task while a helper named only in that durable copy remains alive. Verify: Extend the existing RC1 integration fixture with a metadata-error injection after successful copy enumeration; require restart refusal before any engine-turn marker while the helper remains alive. Remedy: Replace the boolean existence precheck with an error-preserving read or metadata operation. Treat only a confirmed NotFound as absence; propagate other errors as unusable-record inspection failures. Add the copy-file failure fixture alongside the directory-error tests.

### Unproven scenarios

- RC1's cross-window recorded/live/sub-agent cases were inferred from code, not executed.
- Equal-time observations with differing reset times, expired windows, or missing windows were not exercised.
- The two passing prebuilt test cases were not established to come from c23869d.
- The full workspace suite and external plugin harness results were not independently rerun.
- The broader a7f2626..bf1bd4a wave-8 implementation and unrelated prior fixes were not re-reviewed.
- The .collab diff artifacts were skipped as requested; external plugin sources and harness scripts were not read.

### First-run checklist (observable)

_(none)_
