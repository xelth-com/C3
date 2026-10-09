# Handoff 19 - Codex: wave2-round2-astra

Date: 2026-10-09 08:16 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from roster; endpoint builtin:openai; provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:/Users/Dmytro/.codex/codex-consult-roster-0.6.json - entry 1 of 12 for -Provider openai (model applied).
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 964dfe6b-4b1d-4f9e-bf87-eb100f3bdca5.
Invocation: `codex-consult.ps1` (mode: fork, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-4b1d4947300c46748308478821c36bd0.md --output-schema C:\Users\Dmytro\claude-codex-consult\plugins\codex-consult\schemas\consult-reply.schema.json fork 01a11e7e-03ad-7971-9a5f-3576ef761311 -` (prompt on stdin).
Parent thread: `01a11e7e-03ad-7971-9a5f-3576ef761311`. Result thread: `01a11f4e-c543-7333-acce-00c506ea18c3` (source: events).
Brief: `.collab/parity-0.6.1-2026-10-08/handoffs/15-claude-wave2-round2-astra.md` (sha256 854d2a7fd749). Reviewed: a2059bf, base a2059bf8b8cd709fea3a4a1505089a5e589d2ac1, tree sha256 bac511c8e7e2, 0 changed files.
Bridge outcome: usable reply. Wall time: 265.3 s. Tokens: in 11063557 (cached 9504512), out 46816, reasoning 11120.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s. Range: `0d78ec0..bc8d605` - the range changes 11 files, 2137 lines (1888 insertions, 249 deletions).
Verdict: HOLD - The original fixes are present, but complaints bypass the deletion interlock and cleanup can clear its transaction while the old salt remains unreadable. Findings: 0 blocker, 2 major, 1 minor, 0 note (F19-1..F19-3, tracked in `findings.json`). Structured reply: `handoffs/19-codex-wave2-round2-astra.reply.json`.
Raw event stream: `handoffs/19-codex-wave2-round2-astra.events.jsonl`.
Verbatim reply follows.

---

1. **The original F09 deletion schedules and F14-1 are addressed, but the deletion contract still has gaps.** Flush checks deletion state under both locks; confirmed cleanup retains its record until completion; `5c9f8a0` persists local-only cleanup before removing anything. Reread failures now preserve the spool.

   However, `complain_to` neither acquires those locks nor checks deletion state. A complaint prepared before forgetting can POST the deleted identity afterward, or send while deletion is pending (finding #1). Cleanup also treats an unreadable salt as a different instance, retains it, and clears the transaction; restoring readability permits reuse of the deleted identity (#2).

   An already-running flush may finish before forget acquires its locks; that ordering is coherent because DELETE follows it. The unresolved lost-DELETE-response case can remain a documented fail-closed limitation. **Do not equate “unknown public_ref” with confirmed deletion** unless the intake explicitly guarantees that meaning for this authenticated instance. An abandonment command is not required for this wave’s acceptance and must not imply remote deletion succeeded.

2. **Ordinary legacy provider/model labels, titles and tags are now rebuilt through closed classes.** The shared coordinator resolver and Unicode key-splitting fix address F09-6 and F09-1. Complaint text remains intentionally user-approved free text; its CLI-generated last-run summary uses classes and numbers.

   There is one sanitizer edge: duplicate JSON keys can hide a private value during parsing, after which the equality fast path returns the original bytes containing that value (#3). This requires a crafted or externally produced queued body; I found no normal constructor that emits duplicate keys.

3. **HOLD for findings #1–#2; also close the sanitizer edge before claiming every queued body is sanitized.** The original F09-1–F09-6 triggers and F14-1 can close with the qualifications recorded below. F11 fixes belong to their separate review. I inspected source and tests at the requested revisions; I did not rerun the reported suites or runtime fault scenarios.

## Requested checks

- **RC1:** Cwd repository root; workspace-write; use the complaint confirmation callback as a barrier, complete a successful remote-plus-local forget, then release confirmation. Require no POST using the old identity; repeat with a pending deletion (#1). Budget: two schedules, 20 minutes.
- **RC2:** Cwd repository root; workspace-write; resume confirmed cleanup while Windows denies reading the existing salt but permits metadata access. Require failure with the transaction retained; release the handle and require cleanup to remove the old salt before unblocking telemetry (#2). Budget: one fixture, 20 minutes.
- **RC3:** Cwd repository root; workspace-write; take a current valid event and insert `"title":"customer-acme",` immediately before its existing title key. Capture `flush_with` output and require the private string to be absent or the event rejected (#3). Budget: one fixture, 10 minutes.

---

### Findings

- **F19-1** [major] `crates/c3/src/telemetry/complaint.rs:56`, `crates/c3/src/telemetry/complaint.rs:74`, `crates/c3/src/telemetry/complaint.rs:88` - Complaint submission bypasses the deletion transaction and its locks, allowing a complaint to recreate data under a deleted identity or transmit while deletion is pending. Trigger: A complaint captures instance A and waits for confirmation; another process successfully forgets A and removes its local identity; the complaint then receives confirmation and POSTs its already-built payload. A complaint started while a pending transaction exists also sends without checking it. Evidence: read-code: complain_to builds the identity before confirmation, sends directly, and stores the returned reference without acquiring either deletion lock or consulting deletion_state.; read-code: Forget serializes its transaction using the sender and spool locks, but those locks cannot exclude the direct complaint sender.; inferred: The retained complaint body can POST the old instance after cleanup, and its returned reference can be written into the now-cleared local directory. Verify: Pause a complaint in its confirmation callback, complete remote-plus-local forgetting, then resume it and assert no old-instance POST occurs; also test submission while pending. Remedy: Make complaint identity selection, sending and reference persistence participate in the deletion protocol. After user confirmation, acquire the required locks, reject pending cleanup, and verify the approved payload still names the current identity; refuse or request fresh confirmation if it changed.
- **F19-2** [major] `crates/c3/src/telemetry/complaint.rs:294`, `crates/c3/src/telemetry/complaint.rs:305`, `crates/c3/src/telemetry/complaint.rs:322`, `crates/c3/src/telemetry/mod.rs:313` - Cleanup treats failure to read an existing salt as evidence that it belongs to another instance, keeps that salt, and removes the deletion transaction, allowing the old identity to be reused when readability returns. Trigger: A confirmed or resumed cleanup has a saved nonempty instance ID, but reading the existing salt fails transiently while its existence remains visible, for example because another Windows handle denies sharing. Evidence: read-code: instance_id_if_exists_in(...).unwrap_or_default() turns a read failure into an empty identity. The mismatch branch keeps the salt as another instance, explicitly labels the empty result unreadable, and proceeds to remove FORGET_PENDING.; read-code: The identity reader maps every salt read failure to None, without distinguishing an unreadable existing salt from absence.; inferred: The unchanged salt again computes the old identity, while the removed transaction no longer blocks producers or senders. Verify: Resume confirmed cleanup with an existing but temporarily unreadable salt; require an error and retained transaction, then restore access and require complete cleanup. Remedy: Distinguish missing, unreadable and successfully read identities. Preserve the transaction and return failure when identity cannot be established; retain a different salt only after positively reading and comparing its identity.
- **F19-3** [minor] `crates/c3/src/telemetry/classes.rs:839`, `crates/c3/src/telemetry/spool.rs:358` - The sanitizer's raw-byte fast path can forward private strings hidden in duplicate JSON keys, even when the parsed event equals its sanitized reconstruction. Trigger: An otherwise valid queued event contains an earlier duplicate such as "title":"customer-acme" followed by its normal allowed title. Parsing retains the latter value, but the original body still contains both. Evidence: read-code: When the reconstructed Value equals the parsed Value, close_event_body returns the original input bytes rather than the reconstruction.; read-code: Object deserialization inserts successive key/value pairs into a map, replacing earlier values for duplicate keys.; read-code: The returned body is added directly to the outbound batch. Verify: Insert a private earlier duplicate title into a current valid event, capture the sanitized outbound bytes, and assert the private string is absent or the event is rejected. Remedy: Serialize the sanitized reconstruction for every outbound event, or reject duplicate keys before allowing any original-byte fast path.

### Prior findings

- F02-1 - fixed - Locked removal from the current spool preserves concurrent appends; reread failure now also avoids replacement.
- F02-3 - fixed - Failed DELETE retains the saved identity and proof in the pending transaction.
- F02-4 - fixed - Retains the earlier closure for the implementation coverage map and telemetry shim; aggregate harness passes are not full behavioral certification.
- F02-5 - fixed - Retains the earlier correction acknowledging commit-before-spool ordering.
- F02-6 - not-checked - The inline re-read anchor was not revalidated in this review.
- F02-7 - not-checked - Full cross-engine and cross-repository plan behavior is outside this review.
- F02-8 - fixed - The plan continues to treat native Messages as a separately benchmarked candidate rather than inheriting the measured savings.
- F04-1 - not-checked - The process-name Unicode fix was not revalidated.
- F04-2 - not-checked - Windows local-zone boundary parsing was not revalidated.
- F04-3 - not-checked - Required-light execution and ledger metadata were not revalidated.
- F06-1 - not-checked - The reported historical Samoa fix belongs to another change and was not inspected here.
- F09-1 - fixed - Configuration items are split at '=' before comparing keys; the exact Unicode regression fixture is present.
- F09-2 - fixed - Flush rechecks deletion state under both locks, and forget holds both locks. Finding #1 concerns the separate complaint sender.
- F09-3 - fixed - The original premature-marker-removal and salt-before-spool ordering are repaired. Finding #2 identifies a separate identity-read failure during cleanup.
- F09-4 - fixed - A failed current-spool reread returns an error without replacing the file; NotFound also performs no replacement.
- F09-5 - fixed - Ordinary legacy event labels are rebuilt through closed classes. Finding #3 concerns duplicate-key raw bytes, which normal constructors do not emit.
- F09-6 - fixed - Ledger coordinators and rating actors use the shared resolver with unique-model inference and Codex defaults; targeted fixtures cover the reported cases.
- F11-1 - not-checked - The brief assigns the implemented journal fix to the separate wave-2b follow-up review.
- F11-2 - not-checked - The implemented requirement-launcher fix is outside the requested patch set.
- F11-3 - not-checked - The implemented requirement-plan fix is outside the requested patch set.
- F11-4 - not-checked - The implemented PowerShell pointer fix is outside the requested patch set.
- F11-5 - not-checked - The implemented AGY tool-flight fix is outside the requested patch set.
- F14-1 - fixed - Commit 5c9f8a0 writes the fresh local-only cleaning transaction before cleanup and removes nothing if that write fails; interruption and write-failure fixtures are present.

## Verdict: HOLD

The original fixes are present, but complaints bypass the deletion interlock and cleanup can clear its transaction while the old salt remains unreadable.

### Blockers

_(none)_

### Unproven scenarios

- No builds, test suites, network requests or filesystem mutations were performed during this review.
- Complaint/deletion concurrency and the unreadable-salt scenario were established from source rather than executed.
- The duplicate-key sanitizer case is source-supported but was not run through a compiled fixture; no normal event constructor was found to produce it.
- The intake's authenticated 404 and deletion-idempotency semantics were not verified.
- Reported mutation-test failures and passing suite totals were not independently reproduced.
- Unrelated later changes on the moving main branch were excluded from this review.

### First-run checklist (observable)

_(none)_
