# Handoff 01 - Claude: core contracts of C3 (ledger, findings, handoff, store, engine)

Date: 2026-09-26. Base commit: `e1acae1` (clean tree except this `.collab/c3-core-contract/` directory).

## Question

Are the core contracts in `crates/c3-core` safe to build the engines (M2c), the ledger runtime (M3) and the `http`
path (M7) on? Every dependent milestone hangs on these types and traits; changing them later means rewriting the
engine adapters and the store. Review the shapes and invariants, not the prose.

## Delta since the last review

Follows: the framing panel of task `c3-design` (`.collab/c3-design/handoffs/01..15`, decisions in
`.collab/c3-design/state.md`, design of record `docs/DESIGN.md`). This is the first review of the code.

- Milestone 1 landed: `c3 providers` reproduces `codex-providers.ps1` byte for byte on this machine.
- Milestone 2a landed (`e1acae1`): the contracts below, with byte-identical round-trips of the real stores.
- The plugin session (working in `../claude-codex-consult`, now at 9d79206) confirmed: the ledger field order comes
  from the `$entry` literal and the harness `$order` string; roster gets an `ext` object for C3 data (wave 26);
  R12 status-file shape is decided, not frozen.

## CURRENT invariants claimed

- Files are the record; `.collab/<task>/{sessions.json, findings.json, handoffs/}` are byte-compatible with the
  plugin (`ps_json.rs` reproduces Windows PowerShell 5.1 `ConvertTo-Json`; the real c3-design stores, 194 KB and
  184 KB, round-trip byte-identically through the typed `SessionsFile` / `FindingsFile`).
- `LedgerEntry` carries every field of the wave-24 order string, in that order; `SessionsFile::add_entry` keeps
  `consults` sorted by `n`.
- Write order is `COMMIT_WRITE_ORDER`: `.reply.json` before the lock; under `.consult.write.lock`: re-read both
  stores, apply the delta, write the handoff `.md`, then `findings.json`, then `sessions.json` (commit point),
  then remove the pending record, then release. Atomic replace = temp + flush + rename.
- A lock is a permanent file owned by holding it open; the record inside is informational; releasing = dropping
  the handle. Lock share-mode fidelity and write-lock backoff are NOT in `c3-core` (see Q7).
- Finding transitions: any status may follow any other; `verified` needs evidence, `rejected` and a reopen need a
  note, `superseded` neither; `history[]` append-only.
- Lineage = `provider :: model [engine]` on an endpoint fingerprint; never fork or resume across lineages.
- `Engine` for `http`: no tools, no sandbox, no threads (`capabilities(Http)`), `plan()` returns `NoArgv`.
- `verdict` (reviewer) and `bridge_outcome` (bridge) are distinct fields; usable outcomes are exactly two strings.
- C3 never commits to git. Reviewers are read-only.

## Changed files

Base commit `e1acae1`, fingerprint not computed.

| File | Change |
|---|---|
| `crates/c3-core/src/ledger.rs` | `SessionsFile`, `LedgerEntry` (asserted order), nested records, `read`/`to_bytes`/`add_entry` |
| `crates/c3-core/src/findings.rs` | `FindingsFile`, `Finding`, statuses, `HistoryEvent`, `Rating`, `finding_id`, pure `transition` |
| `crates/c3-core/src/handoff.rs` | `HandoffHeader`, `render()`; header lines held as composed strings |
| `crates/c3-core/src/store.rs` | `EvidenceStore` trait, `FilesStore`, `LockRecord`, `PendingRecord`, `TaskLock`, `COMMIT_WRITE_ORDER` |
| `crates/c3-core/src/engine.rs` | `Engine` trait, `EngineKind`, `Capabilities`, `Request`, `Argv`, `Reply`, `StructuredReply` (deny_unknown_fields), ids, `SubprocessEngine::plan` (exact codex/agy/muse argv) |
| `crates/c3-core/src/ps_json.rs` | the PS 5.1 formatter and its rules |
| `crates/c3-core/tests/contracts.rs` | byte-identity and header-render acceptance tests |
| `docs/port/contracts.md` | contract map, sources, invariants, the 8 questions below in full |
| `docs/port/cli-surface.md`, `m2-acceptance.md`, `m3-acceptance.md`, `fake-clis.md` | the port surface the engines will be measured against |

## Open findings

_(none open in this task; the c3-design findings are design-level and closed by DESIGN.md)_

## Requested checks run

_(first review of this task)_

## Evidence

- `crates/c3-core/tests/contracts.rs` - `cargo test`: 42 pass; the two stores re-serialise byte-identically.
- `docs/port/contracts.md` - the contract map with plugin line references and the invariants above.
- `docs/port/cli-surface.md` "Open questions" - prompt delivery differs per engine (codex/agy on stdin, agy as one
  NDJSON line; muse via `--prompt-file` with empty stdin); the muse oauth/billing guard is re-checked before every
  secondary turn; timeout continuation, denial retry and format repair are three mechanisms with distinct gating.
- `crates/c3-core/src/engine.rs:286-310` - the `Engine` trait: `capabilities()`, `plan(&Request) -> Argv`,
  `run(&Request) -> Reply`, `continue_turn(...)`.
- `crates/c3-core/src/store.rs:158-190` - the `EvidenceStore` trait: `task_dir`, `read_sessions`, `read_findings`,
  `recover_pending`, `next_handoff_number`, `take_task_lock`, `write_pending`, `commit`.

## Questions

- **Q1.** On-disk JSON is host-dependent in the plugin (PS 5.1 vs PS 7 bytes differ). C3 canonicalises to the 5.1
  shape on write and reads any. Is that the right contract for a repository where the plugin (on any host) and C3
  both append to one task, or should C3 match the shape of the file it re-reads, or write a clean format and accept
  one-time churn?
- **Q2.** `Engine` trait shape: is `plan -> Argv` plus `run` / `continue_turn` the right seam when (a) prompt
  delivery differs per engine (stdin vs NDJSON line vs `--prompt-file`), (b) `http` has no argv, (c) muse re-checks
  its guard before every turn, (d) continuation, denial retry and format repair are three distinct secondary-turn
  mechanisms? Propose the minimal change (e.g. a `PromptDelivery` in `Capabilities`, a `Turn` enum, a per-turn
  `precheck` hook) rather than a redesign.
- **Q3.** Identity: `ConsultationId`, `AttemptId`, `ConversationId` are newtypes over strings. Are the rules
  (a retry is a new attempt with the same inputs and no redraw; `http` continuation is replay of pack + prior
  reply; lineage never crosses) expressible and enforceable at this layer, or do they belong to the runtime?
- **Q4.** The handoff header needs inputs the ledger lacks (provenance phrases, short SHAs, other-lineages list,
  roster path and counts, effort basis, severity counts). Should the ledger entry gain fields so a handoff is
  regenerable from `sessions.json` alone (breaking the asserted order string), or is a header derived once at write
  time and never regenerated the contract?
- **Q5.** `usage` is engine-conditional (`total_tokens` only for agy) and `denial_retry`, `timeout_continue`,
  `range`, `peak`, `effort_confirmed` are only `null` in the evidence, kept as raw JSON. Accept raw pass-through for
  unexercised shapes as the contract until a fixture pins them, or pin them now from the plugin literals?
- **Q6.** `http` pack provenance: DESIGN says the pack path and hash ride in `reviewer.provider_config` and a
  `.pack.json` sidecar, not a new ledger column (the asserted order string would break). Confirm or contradict.
- **Q7.** `EvidenceStore`: lock share-mode fidelity (FileShare.Read on Windows, None on Unix, fail-fast) and the
  write-lock backoff with `commit_wait_ms` are deferred to the runtime crate; `c3-core` encodes only the write
  order. Right split, or must the trait expose the lock acquisition so the order cannot be bypassed?
- **Q8.** What in these contracts will be expensive to change after M2c/M3 build on them and is not covered above
  (schema-extension compatibility, worktree identity, cancellation ownership, Windows paths, the R12 status file)?

Answer by number. Keep it under 900 words. Cite files by path and line; mark "assumed" where you did not read.
