# Handoff 04 - Codex: core-contract-mimo

Date: 2026-09-26 21:37 local. Author: Codex (model mimo-v2.6-pro, effort high), Codex CLI 0.155.1.
Reviewer: mimo :: mimo-v2.6-pro (provider from roster, model from roster; endpoint https://token-plan-ams.xiaomimimo.com/v1, wire_api: responses; provider fingerprint 47cd6ee7e4ff; harness codex-cli 0.155.1).
Preflight: ok: env MIMO_API_KEY set.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 3 of 10, panel bf904728 member 3 of 8; skipped gemini :: gemini-3.8-flash-high [agy] (usage limit until 2026-09-28T21:30:50+02:00), gemini :: gemini-3.1-pro-high [agy] (usage limit until 2026-09-28T21:30:50+02:00).
Effort: high sent (requested xhigh, mapping mimo-v1, by caps-v1: token-plan-ams.xiaomimimo.com, mimo-v2.6-pro; not confirmed by the provider). Consultation id: 8211973d-3faf-4c13-bc76-ca5e25bca094.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: core-contract). Argv: `codex exec --sandbox read-only --color never --json -m mimo-v2.6-pro -c model_reasoning_effort="high" -c model_provider="mimo" -c model_catalog_json="C:/Users/Dmytro/.codex/model-catalogs.json" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-d7e5ed574fa74263a90e5762a44a38e9.md -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a0df38-b85a-7263-bcf9-c5c886bbad2f` (source: events).
Brief: `.collab/c3-core-contract/handoffs/01-claude-core-contract.md` (sha256 edbad56d93b8). Reviewed: e1acae1, base e1acae1d43db4fcdd28f296b1417c857d0361780, tree sha256 4396ed65da5c, 0 changed files.
Bridge outcome: usable reply. Wall time: 1287.8 s. Tokens: in 1203766 (cached 294912), out 31396, reasoning 20163.
Timeout: 2400 s (the default of purpose core-contract); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The interface shapes are usable, but locking, commit recovery and schema-extension behavior must change before dependent milestones rely on them. Findings: 2 blocker, 6 major, 4 minor, 0 note (F04-1..F04-12, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/04-codex-core-contract-mimo.reply.json`.
Raw event stream: `handoffs/04-codex-core-contract-mimo.events.jsonl`.
Verbatim reply follows.

---

The contracts are not ready to freeze: the record shapes are close, but commit/recovery, locking and schema extension need correction before M2c/M3.

### Q1
Use option (b): always write canonical PowerShell-5.1 bytes and accept any JSON on read. Matching the last host is impossible and a clean format causes churn. The reader is not lossless: `SessionsFile` and `FindingsFile` have no unknown-field carrier (`crates/c3-core/src/ledger.rs:47-143`, `crates/c3-core/src/findings.rs:56-75`), so a new plugin field is silently dropped on `to_bytes` (`ledger.rs:258-267`). Preserve unknown fields in order or version/reject them first.

### Q2
Keep `plan`/`run`, but make the seam turn-aware. Minimal change: `PromptDelivery` (stdin, NDJSON line, prompt file, HTTP body), `TurnKind {Initial, Continue, DenialRetry, TimeoutContinue, FormatRepair}`, `precheck(&Turn)` for muse, and `plan(&Turn) -> LaunchPlan::{Argv,Http}`. Current `continue_turn(&ConversationId,&str)` cannot carry request state, timeout, prior reply, pack or lineage (`crates/c3-core/src/engine.rs:285-304`), so it cannot implement http replay or distinguish the secondary mechanisms.

### Q3
Partly. The newtypes distinguish names only (`engine.rs:124-134`). Core can enforce sameness with `AttemptContext {consultation_id, attempt_id, lineage_key, input_digest, parent_attempt}`: retry requires equal `input_digest`, continuation requires equal `lineage_key`, and constructors reject violations. Runtime must still prove that the CLI thread belongs to that lineage and that retry redelivers the same inputs. Current types enforce none of these.

### Q4
Do not change the asserted ledger order (`ledger.rs:46-48`). Make the handoff a rendered commit artifact that is never regenerated. If regeneration is required, add a versioned `.handoff.meta.json` sidecar and render from ledger plus sidecar; `HandoffHeader` already depends on non-ledger provenance strings (`handoff.rs:13-21`).

### Q5
Keep `usage.total_tokens` optional and last (`ledger.rs:210-220`). Pin `denial_retry`, `timeout_continue`, `range` and `peak` now with golden fixtures from the plugin literals: typed envelopes plus an inner `extra` map. Raw pass-through is safe only inside that extension map; `effort_confirmed` is already `Option<bool>`. The evidence shows only null shapes.

### Q6
Confirm the no-new-column part. Put the `.pack.json` path and hash in the sidecar and `artifacts`; `reviewer.provider_config` may hold a stable pack reference, but pack bytes/hash must not change the endpoint compat string or fingerprint (`ledger.rs:155-158`, `lineage.rs:4-6`).

### Q7
Platform share modes and backoff may live in runtime, but the current split is unsafe: `FilesStore` presents `take_task_lock`/`commit` as enforcing serialization while `commit` takes no lock and returns `0` (`store.rs:330-348`, `358-401`). Add a `LockManager`/`CommitGuard`; `commit` must consume the write guard and return measured wait.

### Q8
Expensive later changes are unknown-field/schema versioning, worktree identity, cancellation ownership, Windows path normalization/containment and the unfrozen R12 status file. `base_commit`/`tree_sha256` are bare strings; `Engine` has no cancellation token or child handle; `PathBuf`/`to_string_lossy` does not define separators, case, non-UTF8 or traversal; and no R12 type exists. Evidence does not show non-null retry/range/peak literals, crash/concurrency behavior, OS lock behavior or the final R12 shape.

## State machines
**Finding status.** States: proposed, implemented, verified, rejected, wontfix, superseded. All ordered transitions, including self, are accepted. Failures: verified with empty evidence -> EvidenceRequired; rejected with empty note -> NoteRequiredReject; non-proposed -> proposed with empty note -> NoteRequiredReopen (`findings.rs:170-191`). Self-transition returns Ok although history is one event per change (`findings.rs:234-245`); history append and supersede linkage are not enforced.

**Pending recovery.** States: reserved, launching, running, survivors, committing (`store.rs:43-52`). Edges and failures: reserved -> launching, crash before child; launching -> running, crash around child pid/start time; running -> survivors, incomplete kill or survivor scan; survivors -> committing, crash before the mark; committing -> removed, crash after sessions rename before removal. No transition function, liveness rule or idempotent replay exists; the last edge can duplicate an entry because `add_entry` has no consult-id dedupe (`store.rs:269-283`).

**Commit sequence.** `RawReply -> TakeWriteLock -> ReReadStores -> HandoffMarkdown -> Findings -> Sessions -> RemoveRecoveryRecord -> ReleaseWriteLock` (`store.rs:112-145`). Failures leave a missing raw reply, no lock or timeout, stale reads after parse failure, orphan handoff, findings without a session commit, committed sessions with pending left, or a stale lock. `WriteStep` is only an array, not a state machine with typed failures.

---

### Findings

- **F04-1** [blocker] `crates/c3-core/src/store.rs:370`, `crates/c3-core/src/store.rs:389` - FilesStore::commit does not re-read findings.json and writes the caller-supplied FindingsFile wholesale, so one commit can erase another writer's finding changes despite the documented re-read-both-stores rule. Trigger: Two commits for one task, or a caller that built FindingsFile before another finding update, invoke commit. Evidence: read-code: Only sessions.json is read in commit; findings is accepted as a parameter and written.; read-code: The stated contract re-reads both stores under the lock and applies only the run's delta. Verify: Add a regression test that commits two runs with distinct findings and asserts both survive. Remedy: Re-read findings.json inside the locked commit and apply a delta, or change the API to a finding delta/merge.
- **F04-2** [blocker] `crates/c3-core/src/store.rs:330`, `crates/c3-core/src/store.rs:358` - take_task_lock and commit do not enforce mutual exclusion or write-lock acquisition; the lock file is merely opened, commit returns 0, and concurrent writers can bypass the documented order. Trigger: Two processes call take_task_lock or commit on the same task. Evidence: read-code: OpenOptions opens the existing lock without the required exclusive share mode; the comment defers share flags to M3.; read-code: commit performs writes without acquiring a lock and returns Ok(0). Verify: Run two concurrent commits under a real OS lock and observe that the second fails or waits. Remedy: Move platform open/share/backoff into a LockManager or CommitGuard; make commit consume the guard and return measured wait.
- **F04-3** [major] `crates/c3-core/src/store.rs:43`, `crates/c3-core/src/store.rs:269`, `crates/c3-core/src/store.rs:392` - Recovery after a crash between the sessions commit and pending removal can replay a commit and duplicate ledger entries; PendingState has Committing but no idempotence key or transition logic. Trigger: Crash after sessions.json rename succeeds and before .consult.pending.json is removed, then recover or retry. Evidence: read-code: Committing exists, add_entry inserts unconditionally, and pending removal follows the sessions write.; inferred: A retry can see the old pending record and the already-committed entry. Verify: Crash-inject at that boundary, recover, and assert exactly one entry per consult_id. Remedy: Make commit idempotent by consult_id or attempt_id; recovery should detect an already-committed consult and only remove or mark pending.
- **F04-4** [major] `crates/c3-core/src/ledger.rs:47`, `crates/c3-core/src/findings.rs:56`, `crates/c3-core/src/ledger.rs:258` - Typed store parsing silently drops unknown JSON fields, so plugin schema extensions are lost when C3 rewrites a store. Trigger: A writer adds a field not declared in the Rust structs, then C3 reads and writes sessions.json or findings.json. Evidence: read-code: Ordinary Deserialize structs have no unknown-field carrier or deny-and-version policy.; read-code: to_bytes serializes only the typed model. Verify: Round-trip a fixture containing an extra field and assert that it survives. Remedy: Capture unknown fields in ordered maps and serialize them, or reject unknown/versioned schemas explicitly.
- **F04-5** [major] `crates/c3-core/src/store.rs:267`, `crates/c3-core/src/store.rs:351`, `crates/c3-core/src/store.rs:396` - Panel recovery records are read but cannot be written or removed through EvidenceStore: write_pending always writes .consult.pending.json and commit deletes only that file. Trigger: A panel member creates .consult.pending-<NN>.json and later commits or recovers. Evidence: read-code: recover_pending reads panel member files.; read-code: write_pending and commit manage only the single pending filename. Verify: Test write, recover and commit for a PendingRecord with panel Some and assert the member file is managed. Remedy: Add record-name selection and remove the exact member record; return the record identity from write_pending.
- **F04-6** [major] `crates/c3-core/src/engine.rs:355` - Codex fork planning drops the thread id: Mode::Fork(String) emits only `fork`, contradicting the documented `fork|resume <thread>` argv. Trigger: SubprocessEngine::plan with EngineKind::Codex and Mode::Fork(`abc`). Evidence: read-code: The synopsis says fork|resume <thread>, but the Fork arm discards its String and pushes only fork.; read-code: Tests assert option ordering and shapes but never assert the fork thread argument. Verify: Add an exact argv unit test for fork and observe the thread id in the output. Remedy: Emit the thread argument for fork and pin the exact codex argv fixture.
- **F04-7** [major] `crates/c3-core/src/engine.rs:285`, `crates/c3-core/src/engine.rs:136` - The Engine seam cannot represent prompt delivery, per-turn prechecks or http replay: continue_turn receives only a conversation id and prompt, with no turn kind, prior reply, pack or attempt lineage. Trigger: M2b implements muse guard checks, denial retry, timeout continuation, format repair or http continuation. Evidence: read-code: The trait has capabilities, plan, run and a two-argument continue_turn only.; read-code: Request and Reply carry no TurnKind, prior_reply, pack or lineage relation. Verify: Implement a fake http continuation requiring pack plus prior reply and observe that the signature cannot supply them. Remedy: Add PromptDelivery, TurnKind, precheck and LaunchPlan::{Argv,Http}; pass a TurnContext containing prior reply/pack and attempt lineage.
- **F04-8** [major] `crates/c3-core/src/engine.rs:124`, `crates/c3-core/src/engine.rs:136` - ConsultationId, AttemptId and ConversationId are unvalidated strings and Request/Reply do not carry input digest or lineage key, so retry sameness and never-cross-lineage rules cannot be enforced at this layer. Trigger: Retry, fork, resume or http replay with altered inputs or a different endpoint fingerprint. Evidence: read-code: The ids are tuple structs over String and Request has no lineage/input digest.; read-code: Fingerprinting exists separately and is not attached to an attempt or continuation. Verify: Constructors should reject a continuation whose input_digest or lineage_key differs. Remedy: Add AttemptContext and validating constructors; runtime verifies the actual engine thread belongs to that lineage.
- **F04-9** [minor] `crates/c3-core/src/engine.rs:203` - StructuredReply uses plain strings for schema_version, verdict, severity and prior-finding status, so deny_unknown_fields does not enforce the v1 enum and const constraints. Trigger: Parse a reply with schema_version 2 or verdict MAYBE. Evidence: read-code: The schema mirror uses String fields and only rejects unknown fields. Verify: Serde tests with invalid enum or const values must fail after the fix. Remedy: Use enums or newtypes and explicit schema validation.
- **F04-10** [minor] `crates/c3-core/src/handoff.rs:18`, `crates/c3-core/src/handoff.rs:184` - HandoffHeader::extra is an untyped bag appended before the verdict line, so optional records cannot reproduce their specified interleaving and only the no-extra case is tested. Trigger: A reply with timeout continuation, denial retry, provider failure or format repair renders a handoff. Evidence: read-code: The module documents exact interleaving as future work and appends extras before verdict.; read-code: The acceptance render uses extra: vec![], so interleaving is untested. Verify: Add golden header fixtures for each optional record and compare rendered bytes. Remedy: Replace extra with ordered typed slots and pin exact order from the plugin.
- **F04-11** [minor] `crates/c3-core/src/ledger.rs:67`, `crates/c3-core/src/ledger.rs:92`, `crates/c3-core/src/ledger.rs:104` - Unexercised range, peak, denial_retry and timeout_continue are raw JSON with no fixture or envelope contract, so adapters can disagree on required fields and field order at first non-null use. Trigger: The first non-null value of any of those records is written or consumed. Evidence: read-code: These fields are Option<Value> or raw Value with shape explicitly unexercised.; read-code: The design evidence has only null values and leaves the literals unpinned. Verify: Add golden fixtures copied from the plugin literals and round-trip them byte-for-byte. Remedy: Pin minimal typed envelopes and allow an inner extra map; keep raw pass-through only inside that map.
- **F04-12** [minor] `crates/c3-core/src/store.rs:249`, `crates/c3-core/src/store.rs:381` - task_dir and commit file paths are joined without containment checks, so an absolute or parent-relative task or file path can escape the task directory. Trigger: task is ..\outside or files contains an absolute or parent path. Evidence: read-code: Paths are formed with join and no canonical containment check.; read-code: The path helper normalizes display strings but does not canonicalize or contain store paths. Verify: A traversal test should observe that escape paths are rejected before any write. Remedy: Validate task as one safe component and reject absolute or parent file paths after canonicalization.

### Prior findings

_(none)_

## Verdict: ADVISE

The interface shapes are usable, but locking, commit recovery and schema-extension behavior must change before dependent milestones rely on them.

### Blockers

- **F04-1** `crates/c3-core/src/store.rs:370`, `crates/c3-core/src/store.rs:389` - FilesStore::commit does not re-read findings.json and writes the caller-supplied FindingsFile wholesale, so one commit can erase another writer's finding changes despite the documented re-read-both-stores rule. Verify: Add a regression test that commits two runs with distinct findings and asserts both survive. Remedy: Re-read findings.json inside the locked commit and apply a delta, or change the API to a finding delta/merge.
- **F04-2** `crates/c3-core/src/store.rs:330`, `crates/c3-core/src/store.rs:358` - take_task_lock and commit do not enforce mutual exclusion or write-lock acquisition; the lock file is merely opened, commit returns 0, and concurrent writers can bypass the documented order. Verify: Run two concurrent commits under a real OS lock and observe that the second fails or waits. Remedy: Move platform open/share/backoff into a LockManager or CommitGuard; make commit consume the guard and return measured wait.

### Unproven scenarios

- Exact plugin literals and field order for denial_retry, timeout_continue, range and peak were not read; only null evidence is shown.
- No crash-recovery or concurrency run demonstrates behavior at each PendingState transition.
- No platform experiment demonstrates the intended Windows FileShare.Read and Unix fail-fast lock semantics.
- No fixture exercises handoff optional-record interleaving or the final R12 status-file shape.
- Worktree identity, cancellation ownership and Windows path normalization are specified only at the prose level.

### First-run checklist (observable)

_(none)_
