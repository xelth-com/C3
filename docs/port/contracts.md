# C3 core contracts (M3) - map, invariants, review questions

The M3 contracts in `crates/c3-core` port the plugin's file formats and record shapes as
compilable, documented Rust, so a core-contract review can happen before any engine is
ported. No engine runs here: no subprocess, no network, no launcher discovery. Every type
is pure over its inputs. Reference read at `claude-codex-consult` HEAD `9d79206`
("collab: wave 24b findings implemented; re-acceptance brief").

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
- **Lock semantics** (`store::TaskLock`): a lock is a permanent file *owned by holding it
  open*; the record inside is informational (`{pid, start_time, host, task, started[,
  panel]}`, compact + `\n`). Releasing = dropping the handle. The write lock is waited for
  (backoff up to 60 s); the ownership lock is not.
- **Ledger sort by `n`** (`SessionsFile::add_entry`, D10): an entry lands after every entry
  whose `n` is not greater than its own, so the list stays sorted whatever order panel
  members commit in.
- **Finding transitions** (`findings::transition`): any status may follow any other;
  `verified` requires evidence, `rejected` and a reopen (`-> proposed`) require a note,
  `superseded` requires neither; `history[]` is append-only.
- **Lineage rule** (reused from `lineage.rs`): identity is `provider :: model [engine]` on
  an endpoint fingerprint; never fork or resume across lineages. Encoded in the engine
  capabilities (`resume`/`fork` per engine) and the reviewer record.
- **Verdict vs. outcome**: `verdict` is the reviewer's judgement (`ACCEPT/HOLD/REJECT/
  ADVISE`, from the reply); `bridge_outcome` is the bridge's own result (`usable reply`,
  `failed: timeout ...`). `Test-UsableOutcome` accepts exactly `usable reply` and `usable
  reply (after a timeout continuation)` - carried as a note for M2, not yet a type here.
- **http never receives tools** (DESIGN §3 invariant 2): `capabilities(Http).sandbox =
  false`, `threads = false`, and `plan()` returns `NoArgv`.

## Questions for the core-contract review

1. **The plugin's on-disk JSON format is host-dependent.** `Write-JsonFile` produces
   *different bytes* under PowerShell 5.1 (`:  `, brace-anchored indent, `<` escaping)
   vs. PowerShell 7 (`: `, plain 2-space indent, fewer escapes). "Byte-compatible with the
   plugin" (invariant 1) therefore has no single answer. C3 currently canonicalises to the
   5.1 shape because that is the shape of the evidence on disk. **Decision needed:** is the
   contract (a) match whatever host wrote a given file (impossible to know on write), (b)
   canonicalise to 5.1 always (current), or (c) canonicalise to a clean format and accept
   that a mixed repo re-churns on first write? Interop only needs *reading* both; churn-free
   coexistence needs a fixed writer. I recommend (b) documented, with a reader that accepts
   any.

2. **`usage` shape varies by engine** (`{...4}` for codex, `{...5}` with `total_tokens` for
   agy). Modelled with an optional last field. Is `total_tokens` intended to be engine-
   conditional, or should C3 normalise all engines to one shape (would break byte-identity
   with existing agy entries)?

3. **`denial_retry`, `timeout_continue`, `range`, `peak`, `effort_confirmed` shapes are
   unexercised** in the design evidence (all `null` there). They are kept as raw
   `serde_json::Value`/`Option` to preserve byte-identity, but their field order is not
   pinned by a fixture. The review should confirm their exact literals (from
   `codex-consult.ps1`) before M2 fills them, or accept raw-JSON pass-through as the
   contract.

4. **The handoff header needs inputs the ledger entry does not carry.** Rendering the exact
   wording of the `Reviewer:`, `Preflight:`, `Roster:`, `Parent thread:/Result thread:`,
   `Brief:/Reviewed:`, `Timeout:` and `Verdict:` lines needs: the provenance phrases
   (`provider from -Provider, model from -Model`), the short fingerprint, the short brief and
   tree SHAs, the "other lineages" list, the roster path/counts, the effort *basis* phrase
   (`caps-v1: builtin:openai, any model`), the timeout *source* phrase, and the severity
   counts + id-range wording - none of which live in `sessions.json`. `handoff.rs` holds
   these lines as composed `String`s and documents them; the M2 renderer must derive them
   from the identity/lineage/revision layer, not the ledger alone. **Question:** should the
   ledger entry gain these (e.g. `effort_basis`, `timeout_source` already exists) so a
   handoff is regenerable from `sessions.json` alone, or is regeneration explicitly not a
   goal?

5. **The optional header records interleave** around the timeout/verdict lines (engine
   turns, warnings, denial retry, timeout continuation, partial reply, provider failure,
   format repair). `HandoffHeader::extra` currently appends them before the verdict line;
   M2 must place each at its exact point. The review should confirm the exact ordering from
   `codex-consult.ps1:3692-3737` as the spec.

6. **http needs a field the ledger lacks.** DESIGN §4 says the `http` engine writes
   `.pack.md`/`.pack.json` and its pack evidence uses `evidence.kind: read-code` with a
   `reference` naming the pack path and hash. The ledger entry has no pack reference field;
   the reviewer 15 explicitly contradicted the idea that this needs a reply-schema bump
   (handoff 15, Q8). Confirm: pack provenance rides in `reviewer.provider_config` / a new
   sidecar, not a new ledger column.

7. **Lock share-mode fidelity is deferred to M3-runtime.** `FilesStore::take_task_lock`
   holds the handle open (the contract) but does not yet reproduce the plugin's
   `FileShare.Read` (Windows) / `FileShare.None` (Unix) fail-fast open, which needs a
   platform open call. The pure-contract layer cannot express it; confirm it belongs in the
   `c3` runtime crate, not `c3-core`.

8. **`commit()` implements the write *order* but not the lock acquisition/backoff.** The
   ordered atomic writes are real and encoded in `COMMIT_WRITE_ORDER`; taking
   `.consult.write.lock` with 60 s backoff and returning `commit_wait_ms` is an M3-runtime
   concern (it needs the same platform lock). Confirm this split.
