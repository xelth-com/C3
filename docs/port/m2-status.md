# M2 status — `c3 consult` codex engine (milestone 2c)

Status of each **codex-related** row of `docs/port/m2-acceptance.md` after the M2c port of
`c3 consult` for the codex engine. Legend:

- **verified** — implemented and checked (a unit test, and/or the dry-run/live parity diffs
  in this report against `codex-consult.ps1` driven by the plugin's `fake-codex3.cmd`).
- **partial** — the core path is implemented and exercised, but a documented sub-case is
  deferred (named in the note).
- **not yet** — deferred to a later milestone (M2c+ hardening, M2d agy/muse, M3 findings, M4
  panel, or the R12 detach surface); the reason is in the note.
- **n/a** — not the codex engine (agy/muse = M2d; panel = M4).

Reference read at `claude-codex-consult` HEAD `c4cb428` (wave 24c, past `50dbddd`).

## harness-0.3.ps1 (identity, lineage, timing, transport, preflight)

| check id | status | note |
|---|---|---|
| SCAN (config scanner) | verified | reused `c3-core::config` (M1); identity resolves in every live run. |
| SCAN (identity from a real config) | verified | live runs resolve `openai :: gpt-5.1` from the scratch config; `-Provider ZAI -Model glm-5.3` resolves in dry-run parity. |
| FP (fingerprint stability) | verified | reused `c3-core::lineage`/`health` (M1); `provider_fingerprint` byte-identical in the live `sessions.json` diff. |
| F02-1 (identity resolution, refused paths) | partial | happy path, `-Provider` without `-Model` refusal, and fork/resume-on-unresolved refusal implemented; the `-Mode new` records-`unknown` path is implemented; full roster-sourced identity is M4. |
| F02-1 (unresolved forecloses fork/resume) | verified | `build_context` refuses `-Mode fork/resume` on an unresolved identity. |
| PARENT (lineage-scoped fork/resume) | not yet | thread-from-events works; resolving/validating a parent thread from the ledger by lineage is deferred (M2c+). `--mode resume --thread` sends `resume <id>` but does not yet cross-check the ledger lineage. |
| PARENT (endpoint fingerprint drift) | not yet | deferred with parent-thread resolution. |
| F02-3 (thread verified by consultation id) | partial | thread taken from `thread.started` (verified); the rollout-file fallback + consultation-id verification is deferred. |
| EVENTS (thread source from the event stream) | verified | `engines::codex::parse_thread_id` (primary + `session_configured`/`session.started` drift nets, foreign non-uuid ignored) — unit-tested. |
| F02-4 (effort vocabulary mapping) | verified | `orchestrate::effort_plan` maps through caps-v1 + the vocabulary table; undeclared host/model → refusal; `-NativeEffort` verbatim. Effort line byte-matches in dry-run. |
| F02-4 (ledger effort fields) | verified | `effort`/`effort_requested`/`effort_sent`/`effort_mapping`/`effort_caps` match in the live `sessions.json` diff; `effort_confirmed` null. |
| F02-9 (peak windows) | not yet | peak evaluation deferred; ledger records `peak: null, peak_source: "none"`. |
| F06-3 (peak re-evaluated at launch) | not yet | deferred with peak. |
| F06-4 (peak exception ranges) | not yet | deferred with peak. |
| KILL (process-tree kill) | partial | `engines::subprocess` kills the tree on timeout (Windows `taskkill /T`, verified: a 60 s hang is killed at the 3 s / 2 s timeout, exit 1); survivor **detection** is best-effort (empty list), the 5×-race and survivor-recording are deferred. |
| OPENAI+WIRE (built-in vs user table) | verified | reused core; built-in `provider_config {builtin:openai}` matches in the live diff. |
| F06-1/F06-2 (construct scoping) | verified | reused `c3-core::config` (M1). |
| F02-5 (requested-checks paragraph + consult id) | verified | `consult::prompt` — unit-tested; the prompt byte-matches in dry-run; last line is `Consultation id: <guid>` = ledger `consult_id`. |
| LEDGER (field order, reviewer, no config mutation) | verified | ledger entry from `c3-core::ledger` (fixed field order incl. new `revision_moved`); `reviewer{}` sub-object and full entry byte-identical in the live `sessions.json` diff (only `cwd` differs, environmental); the user config is never written. |
| CODEXCFG (`-CodexConfig` overrides) | verified | reused `c3-core::roster::convert_from_codex_config_items` (~ expansion, `key=` splitting, identity/effort-key refusal) via `args::validate` — unit-tested. |
| TRANSPORT (schema transport selection) | verified | `orchestrate::resolve_transport` (caps-v1 default, `-SchemaTransport` override, `-Raw` → `""`); the dry-run `transport` line + ledger `schema_transport`/`_source` match. |
| F09-1 (fail-closed preflight, `-SkipPreflight`) | partial | credential preflight for the built-in openai endpoint (`codex login status`) refuses before the lock (exit 1, nothing written — verified against the plugin's byte-identical refusal message); `-SkipPreflight` bypasses (ledger `preflight: "skipped"`); the hanging-check / non-openai-credential paths are deferred. |
| F09-2/4 (endpoint health blocks a run) | not yet | the recorded 24 h auth block / usage-limit block reads no task ledgers yet (endpoint-health preflight deferred; `verdict_with_credential` is called with `health: None`). |
| F09-3 (provider_failure classification) | partial | a failed run classifies via `c3-core::health::provider_failure_class` and records `provider_failure{class,code,message}`; the later-run endpoint block is deferred (see F09-2/4). |

## harness-fixes.ps1 (F04-*: store integrity, locking, numbering, validation, drift)

| check id | status | note |
|---|---|---|
| F04-1 (atomic stores; corruption refuses) | partial | commit uses `c3-core::store` atomic writes + write order (reused); the empty/unparseable-store refusal is enforced by the store's readers but not re-surfaced as a `c3 consult` message path yet. |
| F04-2 (held-handle task lock) | verified | reused `c3-core::store` fail-fast ownership lock; a second run is refused. |
| F04-3 (reservation numbering) | partial | `next_numbers` reused; a fresh reservation gets the next `n`/`nn`; orphan flagging + dry-run recovery-preview are deferred (recovery is M2c+). |
| F04-4 (verdict vs open prior blocker) | not yet | the prior-finding lifecycle ingestion (retained blockers, ACCEPT-contradiction, `unchecked_prior_blockers`) is deferred; the reply's own `prior_findings` are rendered as-is. |
| F04-5 (finding `line` bounds) | not yet | the `1..2147483647` local validation error is deferred (the reply schema validator accepts any `i64` line). |
| F04-6 (verdict fits the purpose) | not yet | per-purpose verdict-vocabulary validation deferred; the verdict is recorded as the reply gives it. |
| F04-7 (file mode in the tree hash) | verified | `consult::revision` reads modes via `git diff --raw` (mode-transition `a>b`), part of the manifest; `tree_sha256` byte-identical to the plugin in the dry-run diff. |
| F04-8 (case-sensitive paths) | partial | manifest keys are the git paths ordinal-sorted (matches on a case-sensitive tree); not separately exercised on Windows. |
| F04-9 (brief/artifact drift) | not yet | the after-run re-hash of the brief/artifacts and the drift flags are deferred; `brief_sha256` (before) is recorded. |
| F04-10 (surviving child keeps the task locked) | not yet | pending-record liveness/recovery is deferred (M2c+). |
| F04-11 (raw-reply preservation on downstream failure) | not yet | the "raw reply before the lock" crash-safety path is deferred; M2c writes `.reply.json` via the commit `files` list instead of `store::write_raw_reply` (documented in `orchestrate::finish`). |

## harness-format.ps1 (structured/prose ingestion, format repair)

| check id | status | note |
|---|---|---|
| CONTR (prompt opens with the contract; `-FormatRetry` bounds) | verified | the FINAL OUTPUT CONTRACT paragraph is first (unit + dry-run); `-FormatRetry 0`/`-Raw`/chore → no contract, dry-run "format retry : 0 (off)"; `-FormatRetry` accepts only 0/1 (`args::validate`, unit-tested). |
| GATE (`Get-ProseGate`) | verified | `consult::ingest::prose_gate` ported exactly (refusal phrases, numbered-answer regex, the 25/40/120-word floors) — unit-tested. |
| REPAIR (one repair turn) | not yet | the format-repair turn (resume the thread, convert-only prompt, `.original.md`, `format_retry` record, drift notes) is deferred (M2c+). A substantive-prose reply is kept as the reply of record with a `validation_error`. |
| TWICE / NONE | not yet | deferred with REPAIR. |
| DRIFT / DRIFT5 | not yet | `Get-FormatRepairDrift` is not ported yet (deferred with REPAIR). |
| ORPHAN (mid-repair recovery record) | not yet | deferred with REPAIR + pending recovery. |

## harness-lock2 / harness-pending / harness-3b (lock ownership, pending recovery, liveness)

| check id | status | note |
|---|---|---|
| lock2 (held-handle lock semantics) | partial | the ownership lock is reused from `c3-core::store` (held by handle, file kept on release); the in-flight-visibility interaction with `c3 findings -Status` is M3. |
| pending (a)–(g), F06-1 | not yet | the reserved/launching/running/survivors recovery records and their process-liveness checks are deferred (M2c+); a run reserves and removes its own record but does not recover an interrupted one. |
| 3b-a/b/c (`Test-PendingActive`) | not yet | deferred with pending recovery. |

## harness-visibility.ps1 (per-purpose timeouts, `-Range`, timeout continuation)

| check id | status | note |
|---|---|---|
| DEFAULTS (per-purpose timeout/continue) | verified | `args::validate` — the purpose timeout table, `timeout_source`, and `continue_sec = min(timeout, 900)` (0 = off, negative refused) — unit-tested; dry-run `timeout` line matches. |
| RANGE (`-Range`) | partial | the `-Range` refusals (purpose gate, single-revision) are in `args::validate` (unit-tested); the `git diff --shortstat` measurement, the `range{}` ledger record, the prompt/handoff range line and the size warning are deferred. |
| CONT (codex timeout continuation) | not yet | the continuation turn, `timeout_continue` record, `.partial.md` salvage and the printed resume command are deferred; a killed main turn is a failed run (exit 1) with `bridge_outcome: "failed: timeout ..."`. |
| GATES-F08-* / F07-1 | not yet | the continuation gates (tree/quota/survivor/short-reply) and prompt-only re-send are deferred with CONT. |

## harness-roster.ps1 (roster walk)

| check id | status | note |
|---|---|---|
| WALK / QUOTA / RULES / AUTH / SCHEMA / UTF8 | not yet | the roster walk (rule 1/2/3, quota-skip, `auth:none`, `-SkipPreflight`/`-Model` narrowing) is a panel/roster concern deferred to M4; a single `c3 consult` run resolves identity directly. `CODEX_CONSULT_ROSTER=none` is honoured. SCHEMA's `-SchemaTransport`+`-Raw` refusal IS covered (see TRANSPORT / CONTR). |

## Other engines (n/a for the codex task)

| harness | status | note |
|---|---|---|
| harness-engines.ps1 (agy) | n/a | agy engine, milestone 2d. `--engine agy`/`--engine-exe`/`--denial-retry` are parsed and refused with a "milestone 2d" message. |
| harness-muse.ps1 (muse) | n/a | muse engine, milestone 2d. `--max-model-steps` refused with a "milestone 2d" message. |
| PANEL rows | n/a | `--panel`/`--panel-all` refused with a "milestone 4" message. |

## Counts

- **verified:** 15
- **partial:** 10
- **not yet:** 22
- **n/a:** 3 (agy / muse / panel groups)

## Wave-24c items (coordinator course-correction) — where verified

1. **`revision_moved` ledger field** — added to `c3-core::ledger::LedgerEntry` between
   `tree_changed_during_review` and `changed_files`, tri-state (`Option<Option<String>>`) so
   pre-24c fixtures stay byte-identical (skipped when absent) and a real entry writes `null`.
   Verified: the byte-identity contract tests stay green; a live entry writes `revision_moved: null`.
2. **`provider_failure.kind = "burst"` + 10-minute out-window** — `c3-core::health::failure_kind`
   (burst-text vs quota-window patterns) + `BURST_OUT_MINUTES`, wired into `endpoint_health`
   (`Record.kind`, `until = hit + out_minutes`). Verified with a new unit test
   (`health_burst_429_clears_after_10_minutes`) and the reworked F08-7 test; the `kind` field
   already sits after `class` in `ProviderFailure`. **Not** exercised against the fake codex
   (no live burst scenario run — the endpoint-health preflight that reads it is itself deferred).
3. **Handoff `Note: HEAD moved ...` line** — confirmed this rides the existing `drift_lines`
   slot (`handoff.rs` item 12, ref line 3732→3754), so no new field was added; the composed
   string (`consult::revision::revision_moved_note`) is unit-tested. Not exercised live (needs a
   mid-run commit, deferred with drift detection).
4. **Content-only tree check** — `consult::revision::{revision_info, compare_tree_content}`
   computes `content_sha256` from `git ls-files -s` + worktree state; a moved HEAD with identical
   contents is `revision_moved`, not a change. Unit-tested (`compare_detects_move_vs_change`).
   Not exercised live (the after-run comparison is deferred with drift detection).
5. **Resume command carries `-ReplyName`/`-SkipPreflight`** — `consult::summary::build_resume_command`
   ports the full `Format-ResumeCommand` option list incl. `-ReplyName` and `-SkipPreflight` —
   unit-tested. Not exercised live (the resume command is only printed on a timeout salvage,
   which is deferred with CONT).

Verified against the fake codex: **none of the five** ran through a live fake-codex scenario —
items 1–5 are verified by unit tests and byte-identity contract tests, because each rides a
subsystem (peak/endpoint-health, drift detection, timeout continuation) that is itself deferred
in M2c. They are correct in isolation and ready for those subsystems to land.
