# Handoff 62 - Codex: wave8-round8

Date: 2026-10-14 17:15 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 5dcab3df-7429-4b23-bfc7-bfddfe60c1a1.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-a133f390d2524fba881455aae84c1f0f.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a13afb-efc3-7633-bfe1-aa523036e537` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/61-claude-wave8-round8.md` (sha256 20fc315307ce). Reviewed: eaca54f, base eaca54f89a8e24d8a9c980266639ec53d56c4a1a, tree sha256 22db371bd810, 0 changed files.
Bridge outcome: usable reply. Wall time: 210.5 s. Tokens: in 266356 (cached 205184), out 4718, reasoning 798.
Quota (rollout): 5h 58% until 21:23; wk 75% until Oct 20; this run 266356 input tokens (205184 cached) in 6 calls, max 60258 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `f4dd195..a0404b1` - the range changes 11 files, 1560 lines (1546 insertions, 14 deletions).
Verdict: HOLD - Timestamp precision loss can still hide a blocking recorded window, and the legacy-record branch explicitly retains F60-1. Findings: 0 blocker, 1 major, 0 minor, 0 note (F62-1, tracked in `findings.json`). Structured reply: `handoffs/62-codex-wave8-round8.reply.json`.
Raw event stream: `handoffs/62-codex-wave8-round8.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** The new fields fix the described persistence failure for newly written records with sufficiently separated observation times. Ledger and health serialization preserve them. However, finding #1 identifies another persistence failure: the fields truncate fractional seconds, while live comparisons retain them. An older live 30% observation can therefore replace a newer recorded 99% observation.

   **F60-1 also remains for legacy records.** The new legacy test explicitly expects the promoted 30% window to beat the genuinely newer 99% window. That documents compatibility behavior; it does not establish the blocking invariant across upgrades.

2. **Q2: HOLD** on finding #1 and the legacy remainder of F60-1; do not tag v0.2.1 yet.

I read all source and documentation diff hunks, plus targeted parser, selection, persistence, and test ranges. I skipped `.collab/` changes as instructed, older handoffs, and unrelated implementation. I did not run tests or independently verify the reported suite results. Targeted context reads used the checkout, whose HEAD was `eaca54f`, rather than the reviewed endpoint `a0404b1`.

## Requested checks

- **RC1:** Repository root; workspace-write; extend the persistence fixture with recorded primary=99 at `12:00:00.900Z` and live primary=30 at `12:00:00.100Z`, both resets future. Require 99% and an active block after serialization; budget: one focused test, 10 minutes.
- **RC2:** Repository root; workspace-write; extend the existing legacy fixture to exercise endpoint health with its promoted aggregate and newer 99% observation. Require an active block under the chosen migration/conservative policy; budget: one fixture, 10 minutes.

---

### Findings

- **F62-1** [major] `crates/c3-core/src/quota.rs:79`, `crates/c3-core/src/quota.rs:200`, `crates/c3-core/src/quota.rs:1196`, `crates/c3-core/src/quota.rs:1237`, `crates/c3-core/src/quota.rs:2456` - The new per-window persistence fields discard fractional seconds, but live observation timestamps retain them. Persistence can reverse the relative ordering of a recorded blocking window and an older live nonblocking window, causing endpoint selection to suppress an active quota block. Trigger: For the same endpoint, persist primary=99 observed at 12:00:00.900Z through Observation::to_quota. Compare it against a live primary=30 observation at 12:00:00.100Z, with both reset times in the future and no independent failure record. The recorded timestamp becomes 12:00:00Z, so the older live observation is treated as strictly newer. The live source can lack the newer rollout because its raw cache predates that observation or its bounded rollout scan excludes it. Evidence: read-code: Both to_quota and stamp_window_instants serialize window instants through iso, whose format contains seconds but no fractional component.; read-code: Rollout timestamps are parsed as RFC3339, account windows retain their in-memory timestamps, and live selection compares those timestamps directly against recorded timestamps. Strictly newer wins without comparing usage.; read-code: The persistence fixtures use whole-second timestamps separated by minutes and check Unix seconds, so they do not exercise precision loss.; inferred: After serialization, 12:00:00.100Z compares greater than the recorded 12:00:00Z even though it precedes the original 12:00:00.900Z observation; selection therefore replaces 99% with 30%. Verify: Run RC1 through the real serialization and recorded/live endpoint selection, asserting exact timestamp preservation and a surviving active 99% block. Remedy: Serialize the new per-window timestamps losslessly, including fractional seconds, in both to_quota and stamp_window_instants. Add subsecond round-trip and recorded/live regression coverage; handle previously truncated records conservatively where their ordering is ambiguous.

### Prior findings

- F02-1 - not-checked - Outside the reviewed delta.
- F02-3 - not-checked - Outside the reviewed delta.
- F02-4 - not-checked - Harness coverage was not audited.
- F02-5 - not-checked - Outside the reviewed delta.
- F02-6 - not-checked - Outside the reviewed delta.
- F02-7 - not-checked - Outside the reviewed delta.
- F02-8 - not-checked - No benchmark evidence reviewed.
- F04-1 - not-checked - Outside the reviewed delta.
- F04-2 - not-checked - Windows boundary behavior was not exercised.
- F04-3 - not-checked - Outside the reviewed delta.
- F06-1 - not-checked - Historical Windows timezone behavior was not reviewed.
- F09-1 - not-checked - Outside the reviewed delta.
- F09-2 - not-checked - Outside the reviewed delta.
- F09-3 - not-checked - Outside the reviewed delta.
- F09-4 - not-checked - Outside the reviewed delta.
- F09-5 - not-checked - Outside the reviewed delta.
- F09-6 - not-checked - Outside the reviewed delta.
- F11-1 - not-checked - Journal recovery was not reviewed.
- F11-2 - not-checked - Outside the reviewed delta.
- F11-3 - not-checked - Shared-plan availability was not reviewed.
- F11-4 - not-checked - Outside the reviewed delta.
- F11-5 - not-checked - Outside the reviewed delta.
- F14-1 - not-checked - Outside the reviewed delta.
- F19-1 - not-checked - Outside the reviewed delta.
- F19-2 - not-checked - Outside the reviewed delta.
- F19-3 - not-checked - Outside the reviewed delta.
- F22-1 - not-checked - Journal replay and retention were not reviewed.
- F23-1 - not-checked - Outside the reviewed delta.
- F23-2 - not-checked - Outside the reviewed delta.
- F23-3 - not-checked - Outside the reviewed delta.
- F23-4 - not-checked - Recovery fixtures were not reviewed.
- F23-5 - not-checked - Production Samoa behavior was not exercised.
- F24-1 - not-checked - Outside the reviewed delta.
- F24-2 - not-checked - Outside the reviewed delta.
- F24-3 - not-checked - Outside the reviewed delta.
- F24-4 - not-checked - Outside the reviewed delta.
- F24-5 - not-checked - Oracle adapter behavior was not reviewed.
- F24-6 - not-checked - Outside the reviewed delta.
- F25-2 - not-checked - Outside the reviewed delta.
- F25-3 - not-checked - Outside the reviewed delta.
- F25-4 - not-checked - Outside the reviewed delta.
- F30-1 - not-checked - Outside the reviewed delta.
- F30-2 - not-checked - Outside the reviewed delta.
- F30-3 - not-checked - Kill durability fixtures were not reviewed.
- F31-1 - not-checked - Outside the reviewed delta.
- F36-1 - not-checked - Outside the reviewed delta.
- F36-2 - not-checked - Outside the reviewed delta.
- F38-1 - not-checked - Outside the reviewed delta.
- F40-1 - not-checked - Outside the reviewed delta.
- F40-2 - not-checked - Outside the reviewed delta.
- F42-1 - not-checked - Outside the reviewed delta.
- F46-1 - fixed - Code inspection: equal-time selection compares window ranks rather than retaining the first observation; tests not rerun.
- F48-1 - fixed - Code inspection: primary and secondary are selected independently; tests not rerun.
- F50-1 - fixed - Code inspection: equal-time ranking includes reset presence and future/past status; tests not rerun.
- F52-1 - fixed - Inspected aggregation paths receive one comparison instant; controlled-clock tests not rerun.
- F52-2 - fixed - Recorded selection rejects non-finite and out-of-range percentages through finite_percent; tests not rerun.
- F55-1 - fixed - Account cache retains raw observations and reruns selection at the requested instant; tests not rerun.
- F55-2 - fixed - Recorded selection validates each candidate window before replacement, preserving older valid windows; tests not rerun.
- F58-1 - fixed - In-memory selection compares each window's own timestamp. Persistence limitations are tracked under F60-1 and finding #1.
- F60-1 - still-open - Fixed for newly serialized whole-second records, but legacy records still reconstruct both timestamps from the aggregate. The new legacy regression explicitly expects primary=30 to defeat the newer primary=99 observation.

## Verdict: HOLD

Timestamp precision loss can still hide a blocking recorded window, and the legacy-record branch explicitly retains F60-1.

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

- No Rust tests, builds, or fault-injection procedures were executed; reported passing results were not independently verified.
- The subsecond failure is established by code inspection and comparison semantics, not an executed production regression.
- The complete codex_run_quota-to-ledger-to-health runtime flow was not driven.
- Historical handoffs, .collab changes other than the requested brief, and unrelated implementation were not read.
- Targeted context reads were from checkout HEAD eaca54f89a8e24d8a9c980266639ec53d56c4a1a; their equivalence to a0404b1 outside the inspected diff was not independently established.
- Plugin compatibility and a safe upgrade policy for legacy aggregates remain unverified.

### First-run checklist (observable)

_(none)_
