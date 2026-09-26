# C3 core contracts (M3) - map, invariants, review questions

The M3 contracts in `crates/c3-core` port the plugin's file formats and record shapes as
compilable, documented Rust, so a core-contract review can happen before any engine is
ported. No engine *runs* here: no subprocess launch, no network, no launcher discovery. The
format types are pure over their inputs; the one deliberate exception is `store.rs`, whose
`EvidenceStore` is the file-and-lock boundary - it takes real OS locks and writes files in
the commit order, because a lock guard that does not lock is not a contract (F02-1/F04-2).
Reference read at `claude-codex-consult` HEAD `9d79206` ("collab: wave 24b findings
implemented; re-acceptance brief"), with the lock/pending/argv/header literals re-read at
HEAD `aabc988`.

## Core-contract review: what the seams became (M2a)

The six-reviewer core-contract panel (findings `F02-*`..`F09-*`) settled the seams the M3
draft left open. The changes, by area:

- **Lock guards are real (A).** `EvidenceStore::take_task_lock` is fail-fast exclusive and
  `take_write_lock` backs off 50 ms doubling to a 1 s cap up to 60 s and returns a
  `WriteLock` guard carrying the measured `wait_ms`. On Windows both locks open with
  `share_mode(FILE_SHARE_READ)` (mirroring `Enter-TaskLock`/`Enter-WriteLock`, which pass
  `FileShare.Read` for *both*); on Unix, where there is no share mode, they take an `fs4`
  `flock`. Every store mutation - `commit`, `update_findings`, `update_pending`,
  `remove_pending` - takes `&WriteLock`, so the write order cannot be bypassed.
- **Findings are a delta, not a snapshot (B).** `commit` re-reads `findings.json` under the
  lock and applies a `FindingsDelta { new, status_changes, ratings }`; `update_findings` is
  the ledger-free transaction (status/rating). One commit can no longer erase another's
  findings.
- **Pending records are keyed and retained (C).** `PendingRef { task, nn }` picks
  `.consult.pending.json` vs `.consult.pending-<NN>.json`; `recover_pending` is path-bearing;
  `commit` takes a `RecoveryDisposition` and never deletes a `launching`/`survivors` record
  unless told `Remove`. The record models the recovery pointers `original`, `first_reply`,
  `reply_json`, `raw_reply`.
- **Schema tolerance (D).** Every plugin-mirrored struct has `#[serde(default)]` on omittable
  fields and a trailing `#[serde(flatten)] extra` map, so an older/fresher file parses and its
  unknown members survive a rewrite. `FindingStatus`/`PendingState` are tolerant tokens
  (known set + `Other(String)`). `peak` is confirmed a `boolean|null` scalar; `range`,
  `denial_retry`, `timeout_continue` are typed to their plugin literals; `provider_failure`
  gains omittable `kind`/`hint` so the five-field evidence stays byte-identical while a
  seven-field file round-trips.
- **Engine seam (E).** `Capabilities.prompt_delivery` (codex `Stdin`, agy `StdinNdjsonLine`,
  muse `PromptFile`); `TurnKind`; `Mode::Resume/Fork` carry a `Lineage` (cross-lineage
  refusable); `plan -> LaunchPlan { Subprocess | Http }`; `precheck` (muse per-token-billing
  guard); `TurnRequest`/`Continuation`; `AttemptOutcome` with `ConversationTrust`. Codex
  `fork` now pushes `fork <thread>`; agy/muse reject fork; `-c` values are TOML-escaped;
  `http` has a lineage row (`ALL_ENGINE_NAMES`/`engine_spec`).
- **Reply validation (F).** `RawReply` (tolerant) validates into `StructuredReply` via
  `try_from` with closed enums (`Verdict`/`Severity`/`EvidenceKind`/`PriorStatus`), a
  schema-version-first check and a required (present, may be null) `line`.
- **Cheap fixes (G).** `add_entry` refuses a duplicate `n`/`consult_id`; `commit` returns a
  `CommitReceipt { committed, cleanup_warning, wait_ms }`; a `TaskSlug` newtype and
  `contained_join` guard every joined path; the first-commit bootstrap takes `cwd`/`tool`;
  `Finding.status` is private behind `set_status`; `supersedes: Vec<String>`; `next_numbers`
  covers both halves of `Get-NextNumbers`; the handoff header has typed ordered optional
  slots at the plugin's exact positions.

## The contracts and where they come from

| Contract | Rust file | Plugin source |
|---|---|---|
| PowerShell-5.1 `ConvertTo-Json` formatter | `src/ps_json.rs` | `Write-JsonFile` / `Write-TextAtomic` (`codex-consult-common.ps1:132-181`) |
| Ledger entry + `sessions.json` | `src/ledger.rs` | `$entry = [pscustomobject]@{...}` (`codex-consult.ps1:3791`); field order asserted at `tests/harness-0.3.ps1:633` |
| Findings + lifecycle + ratings | `src/findings.rs` | stored record + status rules, `README.md` "Findings: ids, status, ratings" (~742); `codex-findings.ps1` |
| Handoff header | `src/handoff.rs` | `$headerLines` (`codex-consult.ps1:3675-3742`) |
| Evidence store, locks, pending, write order | `src/store.rs` | `Enter/Exit-TaskLock`, `Enter-WriteLock`, `Enter/Complete/Exit-StoreCommit`, `New-PendingRecord`, `Get-NextNumbers` (`codex-consult-common.ps1:5992-6340`); write order in `README.md` "Write order and atomic stores" (~794) |
| Engine trait + argv + v1 reply schema | `src/engine.rs` | `$script:Engines` (`codex-consult-common.ps1:3106`), `New-AgyArgv` (:3422), `New-MuseArgv` (:3825), codex synopsis (`codex-consult.ps1:1-13`), `schemas/consult-reply.schema.json`; DESIGN §4 |

## The JSON formatter (the acceptance mechanism)

The stores were written by **Windows PowerShell 5.1**'s `ConvertTo-Json`
(`System.Web.Script.Serialization.JavaScriptSerializer` + 5.1's indenter). `ps_json.rs`
reproduces it exactly, proven byte-for-byte against the real `sessions.json` (194,435 B)
and `findings.json` (183,726 B):

- **Column-anchored indentation.** A value is written after `"<key>":  ` (colon + two
  spaces) on the key's line; a container's members indent four columns past the *column of
  its opening bracket* (`bracket_col = key_indent + quoted_key.len() + 3`), the close sits
  at `bracket_col`. An empty array is `[`, a blank line, then `]`.
- **Escaping.** Only `"` and `\` take short escapes; `<`, `>`, `&`, `'` become `<`,
  `>`, `&`, `'`; control chars and `U+2028`/`U+2029` become `\uXXXX`
  (lowercase). All other characters, including non-ASCII, are raw UTF-8.
- **Number text.** A whole-valued double drops its `.0` (`277`, not `277.0`); a fractional
  double keeps its shortest form (`243.4`). This is why `wall_seconds` is on disk as an
  integer in some entries and a decimal in others, from one `[double]` field.

**Byte-identity result: YES**, for both stores, both through the formatter on a parsed
`Value` and through the typed `SessionsFile`/`FindingsFile` round-trip
(`tests/contracts.rs`). The typed model needed three fields widened to match reality:
`format_retry.events`, `provider_failure.retry_after` (both `null` in some entries) and
`usage.total_tokens` (present only for agy, always last).

The asserted ledger field-order string encoded verbatim in `LedgerEntry` (wave 24):

```
n,when,purpose,consult_id,reviewer,lineage,preflight,preflight_warning,roster,panel,
parent_thread,thread,thread_source,thread_candidate,mode,command,brief,range,prompt_chars,
reply,reply_json,events,partial_reply,model,effort,effort_requested,effort_sent,
effort_mapping,effort_caps,effort_confirmed,max_words,sandbox,timeout_sec,timeout_source,
continue_sec,extra_config,extra_config_source,peak,peak_schedule,peak_source,
peak_evaluated_at,structured,schema,schema_transport,schema_transport_source,
validation_error,format_retry,denial_retry,timeout_continue,base_commit,reviewed_revision,
tree_sha256,tree_sha256_after,tree_changed_during_review,changed_files,brief_sha256,
brief_sha256_after,brief_changed_during_review,fingerprint_note,artifacts,
artifacts_changed_during_review,bridge_outcome,provider_failure,warnings,verdict,
verdict_reason,findings,finding_ids,prior_findings,unchecked_prior_blockers,usage,
engine_run,wall_seconds,finished_at,commit_wait_ms
```

`reviewer`: `provider, provider_source, model, model_source, engine, harness,
provider_fingerprint, provider_config, identity_note`.

## Invariants encoded

- **Write order** (`store::COMMIT_WRITE_ORDER`, README ~794): `.reply.json` first (before
  the lock); then under `.consult.write.lock` - re-read both stores, apply this run's delta,
  write the handoff `.md`, then `findings.json`, then `sessions.json` (**the commit
  point**), then remove the recovery record, then release. Atomic replace = temp + flush +
  rename (`std::fs::rename`, which replaces on Windows and Unix, matching `Write-TextAtomic`).
- **Lock semantics** (`store::TaskLock`/`store::WriteLock`): a lock is a permanent file
  *owned by holding it open* under the plugin's share mode (Windows `FILE_SHARE_READ`, Unix
  `flock`); the record inside is informational (`{pid, start_time, host, task, started[,
  panel]}`, compact + `\n`). Releasing = dropping the guard. The write lock is waited for
  (backoff 50 ms→1 s cap, up to 60 s, `wait_ms` measured); the ownership lock is fail-fast.
- **Ledger sort by `n`** (`SessionsFile::add_entry`, D10): an entry lands after every entry
  whose `n` is not greater than its own, so the list stays sorted whatever order panel
  members commit in; a duplicate `n`/`consult_id` is refused so a replay cannot double it.
- **Finding transitions** (`findings::transition` + `Finding::set_status`): any status may
  follow any other; `verified` requires evidence, `rejected` and a reopen (`-> proposed`)
  require a note, `superseded` requires neither. The gate and the append-only `history[]`
  are bound together - `status` is private and moves only through `set_status`.
- **Lineage rule** (reused from `lineage.rs`): identity is `provider :: model [engine]` on
  an endpoint fingerprint; never fork or resume across lineages. Encoded in the engine
  capabilities (`resume`/`fork` per engine), the `Mode::Resume/Fork` `Lineage` key that
  `plan()` refuses to cross, and the reviewer record. `http` has its own lineage row
  (`ALL_ENGINE_NAMES`), fingerprinted per provider.
- **Verdict vs. outcome**: `verdict` is the reviewer's judgement (`ACCEPT/HOLD/REJECT/
  ADVISE`, from the reply, a closed `Verdict` enum); `bridge_outcome` is the bridge's own
  result (`usable reply`, `failed: timeout ...`). `Test-UsableOutcome` accepts exactly
  `usable reply` and `usable reply (after a timeout continuation)` - carried as a note for
  M2, not yet a type here.
- **http never receives tools** (DESIGN §3 invariant 2): `capabilities(Http).sandbox =
  false`, `threads = false`, and `plan()` returns `LaunchPlan::Http`, never a subprocess argv.

## Questions still open after the core-contract review

The review answered the questions on the unexercised record shapes (now typed to their
plugin literals, with `peak` confirmed a `boolean|null` scalar), the interleaving of the
optional header records (now typed ordered slots at the plugin's exact positions), the lock
share-mode fidelity and the write-lock acquisition/backoff (both now implemented in
`store.rs`, not deferred), and the engine-conditional `usage.total_tokens` (kept
engine-conditional to preserve byte-identity). These remain:

1. **The plugin's on-disk JSON format is host-dependent.** `Write-JsonFile` produces
   *different bytes* under PowerShell 5.1 (`:  `, brace-anchored indent, `<` escaping)
   vs. PowerShell 7 (`: `, plain 2-space indent, fewer escapes). "Byte-compatible with the
   plugin" (invariant 1) therefore has no single answer. C3 canonicalises to the 5.1 shape
   because that is the shape of the evidence on disk, with a reader that accepts any. Confirm
   this is the intended contract (option (b)) rather than a clean-format canonicaliser that
   re-churns a mixed repo on first write.

2. **The handoff header needs inputs the ledger entry does not carry.** Rendering the exact
   wording of the `Reviewer:`, `Preflight:`, `Roster:`, `Parent thread:/Result thread:`,
   `Brief:/Reviewed:`, `Timeout:` and `Verdict:` lines needs provenance phrases, the short
   fingerprint, the short brief and tree SHAs, the "other lineages" list, the roster
   path/counts, the effort *basis* phrase, the timeout *source* phrase and the severity
   counts + id-range wording - none of which live in `sessions.json`. `handoff.rs` holds
   those lines as composed `String`s and the optional records as typed ordered slots; the M2
   renderer derives them from the identity/lineage/revision layer, not the ledger alone.
   **Still open:** should the ledger entry gain these so a handoff is regenerable from
   `sessions.json` alone, or is regeneration explicitly not a goal?

3. **http pack provenance.** DESIGN §4 says the `http` engine writes `.pack.md`/`.pack.json`
   and its pack evidence uses `evidence.kind: read-code` with a `reference` naming the pack
   path and hash. The ledger entry has no pack-reference column and reviewer 15 rejected a
   reply-schema bump for it. `http` now has a core lineage row, but where pack provenance
   rides (`reviewer.provider_config` / a sidecar vs. a new ledger column) is an M7 decision.
