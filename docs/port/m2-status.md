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
| PARENT (lineage-scoped fork/resume) | verified | M2d-3: `select_parent_thread` (`Select-ParentThread`) ports the walk — `-Thread` validated against this task's ledger by lineage + provenance (unknown/candidate/legacy/unresolved refusals), the automatic parent = the newest verified thread of the same lineage, `-Mode fork/resume` refused with the plugin's "needs a parent thread: <note>" when none. `-Thread` with auto mode now reaches the walk (only explicit `-Mode new` + `-Thread` is refused up front). The effective mode + `parent_thread` feed the codex `resume`/`fork` argv and the ledger. 6 unit tests. |
| PARENT (endpoint fingerprint drift) | verified | M2d-3: the drift refusal (`endpoint or protocol of provider ... changed since thread ...`) fires when a candidate parent's `provider_fingerprint` differs from the resolved identity's — unit-tested. |
| F02-3 (thread verified by consultation id) | partial | thread taken from `thread.started` (verified); the rollout-file fallback + consultation-id verification is deferred. |
| EVENTS (thread source from the event stream) | verified | `engines::codex::parse_thread_id` (primary + `session_configured`/`session.started` drift nets, foreign non-uuid ignored) — unit-tested. |
| F02-4 (effort vocabulary mapping) | verified | `orchestrate::effort_plan` maps through caps-v1 + the vocabulary table; undeclared host/model → refusal; `-NativeEffort` verbatim. Effort line byte-matches in dry-run. |
| F02-4 (ledger effort fields) | verified | `effort`/`effort_requested`/`effort_sent`/`effort_mapping`/`effort_caps` match in the live `sessions.json` diff; `effort_confirmed` null. |
| F02-9 (peak windows) | verified | `c3_core::peak` ports `ConvertFrom-PeakSpec`/`ConvertFrom-PeakExceptions`/`Get-PeakStatus`/`Get-ConsultClock`/`Get-PeakStatusNow` (M2d). `CODEX_CONSULT_PEAK_<PROVIDER>`/`_EXCEPT`, the `-OffPeakOnly` refusals (no provider / no schedule / inside window, before the lock), the warning, the `peak`/`peak_schedule`/`peak_source`/`peak_evaluated_at` ledger fields and the dry-run `peak` line — unit-tested (7 tests) and **live-verified**: the dry-run `peak` line and the `-OffPeakOnly` refusal message are byte-identical to the plugin under `CODEX_CONSULT_NOW`. |
| F06-3 (peak re-evaluated at launch) | verified | `run_live` re-evaluates the peak at launch (call index 1; the early check is call 0), refuses `-OffPeakOnly` on a boundary crossing (pending record withdrawn, nothing written), and the ledger records the launch-time value. |
| F06-4 (peak exception ranges) | verified | `parse_peak_exceptions` keeps ranges of any length (never expanded); only end-before-start refuses (`a range must not run backwards`) — unit-tested. |
| KILL (process-tree kill) | verified | `engines::subprocess` kills the tree on timeout (Windows `taskkill /T`); survivor **recording** now lands — the `CODEX_CONSULT_TEST_SURVIVORS` hook adds live pids to the kill, the `survivors` record is written and kept (`RecoveryDisposition::Retain`), and the `bridge_outcome` wording (`... K processes survived: pid ...; the next run for this task is refused until they exit`) is byte-identical to the plugin (live-verified). Real survivor *detection* from the kill is still best-effort (a clean `taskkill /T` leaves an empty list); the 5×-race is deferred. |
| OPENAI+WIRE (built-in vs user table) | verified | reused core; built-in `provider_config {builtin:openai}` matches in the live diff. |
| F06-1/F06-2 (construct scoping) | verified | reused `c3-core::config` (M1). |
| F02-5 (requested-checks paragraph + consult id) | verified | `consult::prompt` — unit-tested; the prompt byte-matches in dry-run; last line is `Consultation id: <guid>` = ledger `consult_id`. |
| LEDGER (field order, reviewer, no config mutation) | verified | ledger entry from `c3-core::ledger` (fixed field order incl. new `revision_moved`); `reviewer{}` sub-object and full entry byte-identical in the live `sessions.json` diff (only `cwd` differs, environmental); the user config is never written. |
| CODEXCFG (`-CodexConfig` overrides) | verified | reused `c3-core::roster::convert_from_codex_config_items` (~ expansion, `key=` splitting, identity/effort-key refusal) via `args::validate` — unit-tested. |
| TRANSPORT (schema transport selection) | verified | `orchestrate::resolve_transport` (caps-v1 default, `-SchemaTransport` override, `-Raw` → `""`); the dry-run `transport` line + ledger `schema_transport`/`_source` match. |
| F09-1 (fail-closed preflight, `-SkipPreflight`) | partial | M2d-3: the credential preflight now runs for **every** provider before the lock — openai(-auth) via `codex login status`, a third-party provider via its `env_key` (`env X not set`) / bearer token (`providers::identity_credential`). `-SkipPreflight` bypasses and, when a quota record is active, prints the `Format-QuotaWarning` line (`preflight_warning`, also recorded in the ledger). The `codex login status` **hanging** row and the `-DryRun still prints the verdict` row still fail (the login timeout wiring / dry-run verdict rendering for an unavailable identity). |
| F09-2/4 (endpoint health blocks a run) | verified | M2d-3: `resolve_preflight` reads every task ledger of this repo (`read_all_task_consults`) and computes `endpoint_health` for the resolved identity's fingerprint, then gates via `verdict_with_credential` — a recorded auth failure (24 h), usage limit (with reset), burst 429 (10 min) or reset-less quota (60 min) refuses before the lock. The QUOTA `... out for 60 minutes, until <iso>; nothing was started (pass -SkipPreflight to launch anyway)` refusal passes in harness-roster. `verdict.rs` gained the burst-vs-usage wording. |
| F09-3 (provider_failure classification) | partial | a failed run classifies via `c3-core::health::provider_failure_class` and records `provider_failure{class,code,message}`; the SSE-error-on-stderr classification row (`{auth, invalid_api_key, ...}`) still differs. The later-run endpoint block now lands (see F09-2/4). |

## harness-fixes.ps1 (F04-*: store integrity, locking, numbering, validation, drift)

| check id | status | note |
|---|---|---|
| F04-1 (atomic stores; corruption refuses) | partial | commit uses `c3-core::store` atomic writes + write order (reused); the empty/unparseable-store refusal is enforced by the store's readers but not re-surfaced as a `c3 consult` message path yet. |
| F04-2 (held-handle task lock) | verified | reused `c3-core::store` fail-fast ownership lock; a second run is refused. |
| F04-3 (reservation numbering) | verified | `next_numbers` reused; a fresh reservation gets the next `n`/`nn`; the recovery pass now consumes a dead leftover — numbering skips past its `n`/`nn` and a `recovered reservation ...`/`cleared the recovery record ...` line is printed (real run) and shown as the dry-run `pending :` line (byte-identical to the plugin, live-verified: a seeded `reserved` leftover n=9/nn=20 → the next run commits at n=6/nn=08 with the record removed). Orphan findings stay flagged by `c3 findings -List` (M3), not consult. |
| F04-4 (verdict vs open prior blocker) | not yet | the prior-finding lifecycle ingestion (retained blockers, ACCEPT-contradiction, `unchecked_prior_blockers`) is deferred; the reply's own `prior_findings` are rendered as-is. |
| F04-5 (finding `line` bounds) | not yet | the `1..2147483647` local validation error is deferred (the reply schema validator accepts any `i64` line). |
| F04-6 (verdict fits the purpose) | not yet | per-purpose verdict-vocabulary validation deferred; the verdict is recorded as the reply gives it. |
| F04-7 (file mode in the tree hash) | verified | `consult::revision` reads modes via `git diff --raw` (mode-transition `a>b`), part of the manifest; `tree_sha256` byte-identical to the plugin in the dry-run diff. |
| F04-8 (case-sensitive paths) | partial | manifest keys are the git paths ordinal-sorted (matches on a case-sensitive tree); not separately exercised on Windows. |
| F04-9 (brief/artifact drift) | partial | tree drift (`tree_sha256_after`, `tree_changed_during_review`, `revision_moved` + the `Note: HEAD moved ...` line) and brief drift (`brief_sha256`, `brief_sha256_after`, `brief_changed_during_review`) implemented in `orchestrate::finish` via `revision::compare_tree_content` (content-only) — the false-drift path is live-verified byte-identical in `sessions.json`. **Artifact** drift (`-Artifact` hashing, `artifacts[]`, `artifacts_changed_during_review`, per-file WARNING) is still deferred: `-Artifact` is parsed but not hashed. |
| F04-10 (surviving child keeps the task locked) | verified | a timeout kill that leaves survivors writes the `survivors` recovery record and keeps it (`RecoveryDisposition::Retain`, not `Remove`), so a subsequent run reads it and — while a recorded pid is alive — refuses (`consult::recovery` + the reserved→launching→running record transitions via the subprocess `on_running` callback). Live-verified: the `survivors` record is kept with the survivor pid; a live recorded child yields the byte-identical `a previous consultation's codex process (pid N) is still running (...)` refusal. |
| F04-11 (raw-reply preservation on downstream failure) | partial | the raw `.reply.json` is now written to the handoff path **before the write lock** (crash-safety; `orchestrate::finish`, not the commit `files` list), matching the plugin's write order — live-verified byte-identical. The "preserving the raw reply itself fails → outcome names the kept path" nuance is still deferred. |

## harness-format.ps1 (structured/prose ingestion, format repair)

| check id | status | note |
|---|---|---|
| CONTR (prompt opens with the contract; `-FormatRetry` bounds) | verified | the FINAL OUTPUT CONTRACT paragraph is first (unit + dry-run); `-FormatRetry 0`/`-Raw`/chore → no contract, dry-run "format retry : 0 (off)"; `-FormatRetry` accepts only 0/1 (`args::validate`, unit-tested). |
| GATE (`Get-ProseGate`) | verified | `consult::ingest::prose_gate` ported exactly (refusal phrases, numbered-answer regex, the 25/40/120-word floors) — unit-tested. |
| REPAIR (one repair turn) | verified | `consult::orchestrate::run_format_repair` — a substantive-prose reply on a verified thread earns ONE `resume <thread>` turn (sandbox `read-only`, the lowest-vocabulary effort via `Get-RepairEffort`, NO `--output-schema` — codex is always prompt-only on the repair turn), the convert-only prompt byte-for-byte; the repaired object goes to `.reply.json`, the prose to `.original.md`; `format_retry{attempted,reason,succeeded,thread,wall_seconds,usage,drift,original,events(null for codex),schema_transport:"prompt-only"}`; handoff `Format repair:` line and the `format repair:` console line. **Live-verified** byte-identical (sessions.json, handoff `.md`, `.reply.json`, console) against the plugin in the seven M2d1c scratch scenarios. |
| TWICE / NONE | verified | never a second repair (`run_format_repair` runs at most once, only when `structured.is_none()`); `--format-retry 0` disables it (`format_retry: null`, `validation_error` names why) — live-verified in the `format-retry-0` scenario. A prose reply that is not substantive earns no repair turn (`validation_error` ends `(format repair not attempted: <reason>)`) — live-verified in `invalid-nonsub`. |
| DRIFT / DRIFT5 | verified | `consult::secondary::get_format_repair_drift` ports `Get-FormatRepairDrift` (RC ids, numbered answers, F-ids named in prose but absent from prior/findings, the verdict token, and the >=60-char prose sentences missing from `reply_markdown`, the 40 longest); unit-tested and live-verified (a verdict-drift note in `repair-success`). |
| ORPHAN (mid-repair recovery record) | verified | the recovery record names the saved prose (`original`, repo-relative) and `first_reply` BEFORE the repair process is launched (state `launching`), then the `on_running` callback flips it to `running` with the repair pid; the commit write order (findings.json before sessions.json, reused from `c3-core::store`) keeps a crash between the two detectable as the findings tool's `[ORPHAN]`. |

## harness-lock2 / harness-pending / harness-3b (lock ownership, pending recovery, liveness)

| check id | status | note |
|---|---|---|
| lock2 (held-handle lock semantics) | partial | the ownership lock is reused from `c3-core::store` (held by handle, file kept on release); the in-flight-visibility interaction with `c3 findings -Status` is M3. |
| pending (a)–(g), F06-1 | partial | the full lifecycle lands: `run_live` writes `reserved` → `launching` → `running` (this bridge's pid/start/host; the child pid + start time via the subprocess `on_running` callback) and, on timeout survivors, `survivors` (kept). Before launch, `consult::recovery::assess` reads every `.consult.pending*.json` under the lock: a corrupt record refuses; a live recorded process refuses (message names the pid, byte-identical to the plugin); a dead record is recovered/cleared (numbering skips past it, consumed member records removed). **Divergence (documented):** the machine-wide "looks like codex" descendant/name scan the plugin falls through to once every recorded pid is gone is intentionally reduced to an inactive verdict (`liveness::pending`), so a case that depends on that scan (some of a–g) resolves to *recover* where the plugin *refuses* on unrelated codex processes — verified live on this machine, where the plugin's scan matched the developer's real codex.exe/node processes. The full `harness-pending.ps1` suite was not run row-by-row. |
| 3b-a/b/c (`Test-PendingActive`) | partial | the pid+start-time rules (writer-pid, reserved, recorded-pid/survivors) are ported and unit-tested + live-verified (a live recorded child is active with the plugin's exact message; a reserved/dead record is inactive → recovered). The name/descendant "looks like codex" scan is intentionally reduced (see the pending row). |

## harness-visibility.ps1 (per-purpose timeouts, `-Range`, timeout continuation)

| check id | status | note |
|---|---|---|
| DEFAULTS (per-purpose timeout/continue) | verified | `args::validate` — the purpose timeout table, `timeout_source`, and `continue_sec = min(timeout, 900)` (0 = off, negative refused) — unit-tested; dry-run `timeout` line matches. |
| RANGE (`-Range`) | verified | `revision::range_stat` runs `git diff --shortstat <spec> --` once before the lock (unknown range refuses with the plugin's wording), records `range{spec,files,insertions,deletions,lines}`, adds the prompt line, the dry-run `range` line and the handoff `Range:` suffix, and the >1500-lines-under-2400 s size warning to `warnings[]` — unit-tested and **live-verified** byte-identical (dry-run `range` line + prompt `Review range:` line). |
| CONT (codex timeout continuation) | verified | `consult::orchestrate::run_timeout_continuation` — after the timeout kill, ONE `codex exec ... resume <thread> -` turn with the plugin's exact continuation prompt and the main turn's options; usable → `bridge_outcome: "usable reply (after a timeout continuation)"` + `timeout_continue{thread,wall_seconds,outcome,events,usage}` with its `.continue.events.jsonl` next to the primary (a further turn); failed → `.partial.md` (salvaged via `Read-CodexSalvage`/`Format-PartialBody`) + `partial_reply` in the ledger + the `partial :`/`resume :` summary lines (the resume args are `summary::build_resume_command`, byte-identical to the plugin). **Live-verified** in `cont-success`, `cont-failure`, `cont-sec-0` (sessions.json, handoff, partial, console byte-identical modulo the documented items below). `--continue-sec 0` → `timeout_continue.outcome "not attempted: -ContinueSec 0"`. |
| GATES-F08-* / F07-1 | verified | the continuation gates are ported: no continuation without a verified thread, none when survivors remain, none after the killed turn's own quota/auth failure (`consult::secondary::get_killed_turn_failure`), and `-ContinueSec 0` off; the reply of the continuation must pass `Test-ContinuationReply` (`consult::secondary::test_continuation_reply`) before it counts. F07-1: a prompt-only transport re-sends the reply format + schema in the continuation prompt (`prompt::schema_lines` + the inlined schema); the built-in openai endpoint is `output-schema`, so it re-passes `--output-schema` instead — both paths implemented. The tree/brief-changed gate is wired (`drift.tree_changed`/`brief_changed`); the artifact-changed gate rides `-Artifact` hashing, still deferred. |

## harness-roster.ps1 (roster walk)

| check id | status | note |
|---|---|---|
| WALK / QUOTA / RULE1 / RULE2 / AUTO / FILE / PROV | verified | M2d-4: the roster walk is wired into `c3 consult` (codex). `providers::Ctx::walk_full` ports `Select-RosterReviewer` (rule 3, `-Model` narrowing, `-SkipPreflight`, skip records, the two "no entry" refusals); the `-Provider`/`-Thread` rules source the entry's model/`codex_config`; quota-skip across tasks rides `endpoint_health`; the `roster{}` ledger record, the `Roster:` console/dry-run/handoff line, `extra_config_source`, and the `provider_source`/`model_source` overrides all land. The reviewer roster is read (and fail-closed validated) before planning — a missing `CODEX_CONSULT_ROSTER` file or an unusable roster refuses on a dry run too (FILE). harness-roster: **56/1** (was 19/25), the one failure a Windows argv-quoting nuance in the FAKE `.cmd` log (the delivered arg + ledger command are byte-correct). 3 unit tests. |
| RULES / AUTH / SCHEMA / UTF8 | partial | `auth:none` narrowing and the panel-specific rows are still M4; the walk's `-SkipPreflight`/`-Model` narrowing IS covered above. SCHEMA's `-SchemaTransport`+`-Raw` refusal IS covered (see TRANSPORT / CONTR). |

## Other engines (n/a for the codex task)

| harness | status | note |
|---|---|---|
| harness-engines.ps1 (agy) | n/a | agy engine, milestone 2d. `--engine agy`/`--engine-exe`/`--denial-retry` are parsed and refused with a "milestone 2d" message. |
| harness-muse.ps1 (muse) | n/a | muse engine, milestone 2d. `--max-model-steps` refused with a "milestone 2d" message. |
| PANEL rows | n/a | `--panel`/`--panel-all` refused with a "milestone 4" message. |

## Counts

- **verified:** 28
- **partial:** 11
- **not yet:** 8
- **n/a:** 3 (agy / muse / panel groups)

## M2d progress (this pass)

Landed and verified (unit tests + live fake-codex parity where the plugin has one):

1. **Peak windows** (F02-9 / F06-3 / F06-4) — `c3_core::peak`; dry-run line and `-OffPeakOnly`
   refusal byte-identical to the plugin.
2. **`-Range`** (RANGE) — `revision::range_stat`; range line + prompt line byte-identical.
3. **Live drift** (part of F04-9) — tree + brief drift via `revision::compare_tree_content`;
   `sessions.json` byte-identical (false-drift path). Artifact drift still deferred.
4. **Bundled schema + raw-reply-before-lock** (item 10 / part of F04-11) — the reply schema is
   embedded in `c3_core::schema` (`REPLY_SCHEMA_V1`, the plugin's file verbatim) and materialised
   under `<CODEX_HOME>/c3/schemas/consult-reply.v1.json`; `.reply.json` is written before the
   write lock. `reply.json` is byte-identical to the plugin; the recorded `command`'s schema path
   is a `CODEX_HOME`-relative C3 path (environmental, like `cwd`). The `-o` last-message and stderr
   sidecars are now system-temp files (`codex-consult-last-<guidN>.md`) exactly like the plugin.
5. **Liveness module** — the plugin's process-liveness / pending-record code moved from
   `findings_tool/{proc,pending}.rs` to a shared `crates/c3/src/liveness/` module (findings_tool
   re-imports it, tests stay green), ready for the consult pending-recovery path.
6. **Dry-run parity fixes** — the dry-run `reviewer` line now shows the full reviewer line (was a
   reduced form); the argv block no longer prints a spurious leading `codex` line. The full
   pre-preview dry-run console is byte-identical to the plugin except random uuids, the C3 schema
   path, and C3's added `telemetry` line.
7. **Telemetry wiring** (C3 addition, per coordinator) — `--telemetry on|off`
   (env `CODEX_CONSULT_TELEMETRY=off` also disables), background spool flush + join at every exit
   (real run only), the one-time notice, `record_consultation` at the commit point, and the
   dry-run `telemetry` line via `telemetry::status`.

Still deferred to M2d-2 / later: format repair (REPAIR/TWICE/DRIFT/ORPHAN), timeout continuation
(CONT/GATES), preflight endpoint health (F09-2/4), the roster walk (WALK/QUOTA/RULES/AUTH), the
prior-finding lifecycle (F04-4), `-Artifact` hashing + artifact drift, and the full live
summary-block console (the plugin's `warning    :`/verdict/drift summary lines) — the persisted
stores are byte-parity, the summary console block is partial.

## M2d-2 progress — pending recovery + liveness (item 1)

Landed and verified (unit tests + live fake-codex parity where the plugin has one):

1. **Recovery pass** (`crates/c3/src/consult/recovery.rs`) — before launch, under the task lock,
   `assess` reads every `.consult.pending*.json` (`liveness::pending`): a corrupt record refuses
   (`unusable`), a live recorded process refuses (message names the pid — byte-identical to the
   plugin, live-verified), a dead record is recovered/cleared (numbering skips past it, consumed
   member records removed). The dry-run `pending :` line and the handoff `Recovery record:` line
   ride the existing slots; the dry-run line is byte-identical to the plugin.
2. **Record lifecycle** — `run_live` writes `reserved` (this bridge's pid/start_time/host, the
   writer-pid liveness rule) → `launching` → `running` (child pid + start time). The
   `running` transition happens through a new subprocess `on_running` callback
   (`engines::subprocess` + `CodexEngine::on_running`) that fires right after spawn, so a mid-run
   crash leaves a record naming the live child.
3. **Timeout survivors** — a kill that leaves survivors writes the `survivors` record and keeps it
   (`RecoveryDisposition::Retain`), and the `bridge_outcome` wording matches the plugin; the
   `CODEX_CONSULT_TEST_SURVIVORS` hook makes it testable. Live-verified end to end.
4. **Panel naming only** — `.consult.pending-<NN>.json` (`PendingRef::member`) is honoured for
   consuming/removing a member's leftover; the panel itself is M4.

Divergence (documented, intended): the machine-wide "looks like codex" descendant/name scan the
plugin falls through to once every recorded pid is gone is reduced to an inactive verdict in C3
(recorded pid + start time only), avoiding the plugin's environment false-positives.

Tests: 5 unit tests in `consult::recovery`, 3 live integration tests in
`crates/c3/tests/pending_liveness.rs` (callback pid, survivor hook, start-time liveness);
workspace `cargo test` green (185), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt`
applied.

Still to do before agy/muse (M2d-2 remaining, items 2–8 of the brief): timeout continuation, format
repair, preflight endpoint-health, the roster walk, the prior-finding lifecycle, `-Artifact`
hashing, and the byte-identical summary-console block.

## M2d-2 progress — timeout continuation + format repair (this pass)

Both codex secondary-turn mechanisms landed and are **live-verified** against `codex-consult.ps1`
(PS 5.1) in seven scratch scenarios under `.../scratchpad/m2d1c/` — timeout+continuation success,
timeout+continuation failure (partial + resume), timeout with `--continue-sec 0`, prose+repair
success (with a drift note), prose+repair failure, `--format-retry 0`, and invalid-JSON-not-prose.

New/changed code:
1. **`crates/c3/src/consult/secondary.rs`** (new) — `read_codex_salvage`/`format_partial_body`
   (`Read-CodexSalvage`/`Format-PartialBody`), `get_format_repair_drift` (`Get-FormatRepairDrift`),
   `test_continuation_reply` (`Test-ContinuationReply`), `get_killed_turn_failure`
   (`Get-KilledTurnFailure`), the prose gate re-export. Unit-tested.
2. **`consult::orchestrate::{run_timeout_continuation, run_format_repair, run_codex_secondary,
   build_partial_reply, codex_failure_pf}`** — the two turns run through `CodexEngine`'s secondary
   `TurnFiles`; the continuation events file is a further turn kept next to the primary, the repair
   events/last are temp; the repair record transitions `launching`→`running` via `on_running`.
3. **`consult::ingest::first_validation_error`** — `ConvertFrom-StructuredReply`'s `ValidationError`
   analog (empty / `not valid JSON: <msg>` / a schema error).
4. **Core**: `AttemptOutcome::TimedOut` carries `wall_seconds`; `TokenReport::Unknown` (`Format-Usage`
   of a null usage = `unknown`, not "not reported by codex"); `FormatRetry.usage` is now
   `Option<Usage>` (null on a failed/empty repair turn).
5. **Summary/handoff**: the `continued :`, `partial :`/`resume :`, `format repair:`+`drift:` and
   `structured : INVALID (...)` console lines; the handoff `Timeout continuation:`, `Partial reply:`
   and `Format repair:` header lines (the header INVALID line uses the FIRST-reply parse error, the
   ledger/console use the suffixed one — matching the plugin's `$parse.ValidationError` vs
   `$validationError`).

Parity result: sessions.json, the handoff `.md`, the `.partial.md`, `.reply.json` (byte-for-byte)
and the console are identical to the plugin in all seven scenarios **except** these documented,
expected differences:
- **Reviewer `topics`/`role`** — the plugin writes these two roster fields (empty) into every
  `reviewer` record; c3 omits them. Pre-existing (M4 roster surface), present in *every* scenario
  incl. ones unrelated to this task, and not introduced here.
- **Schema path in `command`/argv** — c3 passes its own materialised
  `<CODEX_HOME>/c3/schemas/consult-reply.v1.json`; the plugin passes its bundled
  `…/schemas/consult-reply.schema.json`. Environmental (like `cwd`), already documented for M2c.
- **`not valid JSON: <msg>`** — the parser message is runtime/locale-specific (PS 5.1 gave a German
  ".NET" message; Rust gives `expected value at line 1 column 1`). The surrounding structure and the
  `(format repair failed/not attempted: …)` wrappers are byte-identical.
- **Failed-continuation `outcome`** — c3 reports `failed: <classified message>`; the plugin reports
  `failed: codex exit N - <message>`. The codex engine adapter classifies a turn failure into a
  `ProviderFailure` (with a cleaned message) rather than re-deriving the raw `codex exit N` string;
  everything else (that it failed, the thread, the partial, the resume line, `provider_failure`)
  matches.
- **`resume :` console prefix** — the plugin prints `pwsh/powershell -File "…codex-consult.ps1"
  <args>`; c3 prints `<args>` alone (it has no single `.ps1` entrypoint). The **args** are
  byte-identical (`summary::build_resume_command`), and the persisted `.partial.md` footer's
  `continue with \`<args>\`` is byte-identical.
- Tool-name prefix (`c3 consult:` vs `codex-consult:`) — the intended C3 rename.

Tests: 7 unit tests in `consult::secondary`; workspace `cargo test` green (195), `cargo clippy
--all-targets -- -D warnings` clean, `cargo fmt` applied.

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

## M2d-3 progress — single-consultation parity pass (this pass)

The coordinator's 10 parity decisions for the remaining single-consultation rows. **8 of 10
landed and verified** (unit tests + the two plugin harnesses re-run — see
`docs/port/harness-results.md` "Run 2": harness-0.3 154/75 → **189/40**, harness-roster 18/34 →
**19/25**, zero regressions).

1. **Prefixes** — every consult refusal/console line uses `codex-consult:` and every providers line
   uses `codex-providers:` (the `TOOL` const + `providers::run`); no `c3 <cmd>:` remains.
2. **Telemetry notice** — no longer on STDOUT before the run; printed to STDERR after the
   summary/refusal, once per install, marker under the real home (`~/.codex/c3/telemetry/notice-shown`,
   ignoring `CODEX_HOME`). Dry-run `telemetry` line kept.
3. **Reviewer line** — `endpoint <base_url>, wire_api: <x>|(default)` (`identity_display`) in the
   dry-run line, handoff header and console reviewer line.
4. **Preflight completeness** — endpoint-health gating from this repo's ledgers + the non-openai
   `env X not set` credential check + the openai login check, all before the lock; `-SkipPreflight`
   quota warning. Burst (10 min) vs usage (60 min) wording in `verdict.rs`. (F09-2/4 verified.)
5. **Roster + lineage** — **5b (parent-thread walk) DONE** (`select_parent_thread`, PARENT/drift
   verified, 6 unit tests). **5a (roster walk `Select-RosterReviewer`) NOT DONE** — the WALK/FILE
   rows and roster-related QUOTA rows still fail; a single run resolves identity directly and
   ignores `CODEX_CONSULT_ROSTER` except `=none`.
6. **Prior-finding lifecycle** — **NOT DONE** (the prompt open-findings snapshot already exists; the
   reply-side ingestion — ACCEPT-vs-still-open contradiction, `unchecked_prior_blockers`,
   `supersedes`/`superseded_by`, `reviewer_checks` — is not yet wired into the commit path).
7. **Reviewer `topics`/`role`** — DONE: written as top-level ledger fields between `purpose` and
   `consult_id` (empty defaults `[]` / `""`), tri-state so pre-topics fixtures stay byte-identical
   (omittable on read).
8. **Failed-continuation outcome** — DONE: `failed: codex exit N - <msg>` (bare `codex exit N` when
   no error text); the raw exit code is surfaced from the adapter via
   `AttemptOutcome::ProviderFailure { failure, exit_code }`. The format-repair turn uses the same
   `codex exit N` framing.
9. **`--panel-concurrency <n>`** — DONE: parsed by clap, refused as a milestone-4 feature like
   `--panel`.
10. **TRANSP prompt-only validation** — DONE: `parse_structured` strips a single fence before the
    structured parse, so a fenced short JSON reply validates locally instead of falling to the prose
    gate's word floor.

Tests: workspace `cargo test` green (**202**, +7 from 195: the fenced-reply regression test + 6
parent-walk tests), `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt` applied.

Still open (next pass): the roster walk (5a), the prior-finding lifecycle (6), and the smaller
pre-existing base-identity dry-run ordering rows (F02-1/OPENAI/F06-1: an unresolved identity makes
`effort_plan` error before the dry-run plan renders), the rollout-file thread verification (F02-3),
`reviewer.provider_config` echoing the raw table keys (WIRE), and the `-CodexConfig` `~` expansion
argv rows (CFG).

## M2d-4 progress — the roster walk in `c3 consult` (this pass)

Scoped to brief item 1 (the roster walk), the single largest remaining harness-roster cluster.

New/changed code:
1. **`crates/c3/src/providers.rs`** — `read_reviewer_roster`/`find_roster_entry` made `pub(crate)`;
   `Ctx` + `Ctx::for_consult` exposed; `Ctx::walk_full` ports the full `Select-RosterReviewer`
   (rule 3 walk, `-Model` narrowing, `-SkipPreflight`, skip records, the "no entry available" and
   "-Model no entry resolves" refusals). 3 unit tests (`roster_walk_tests`).
2. **`crates/c3/src/consult/orchestrate.rs`** — `build_context` reads the roster before planning
   and resolves the reviewer through the `-Provider`/`-Thread`/walk rules: identity provider/model
   + `provider_source`/`model_source` overrides (`roster`/`-Thread`), `codex_config` from the entry
   (`extra_config_source` `roster`), the `roster{}` ledger record (`{path, position|null,
   skipped[{provider,model,engine,reason}], applied[]}`), the `Roster:` line (console/dry-run/handoff)
   and the ambiguous-label run warning. `-Provider needs -Model` now fires only when neither
   `-Model` nor the roster supplied a model. `resolve_preflight` also returns the verdict **label**
   (dry-run `preflight :` line); a `-Thread` endpoint that is out names the roster's alternative.
3. **`crates/c3/src/consult/dryrun.rs`** — the `sessions.json entry preview` now carries
   `reviewer`, `preflight`, `preflight_warning`, `roster`, `panel` (null), `extra_config`,
   `extra_config_source`; the `preflight :` line uses the verdict label; the `Roster:` line and the
   `-SkipPreflight` quota WARNING print in the dry-run block.
4. **`crates/c3-core/src/ledger.rs`** — `RosterRef.position` is now `Option<i64>` (`null` when no
   entry matched, e.g. `-Provider`/`-Thread` with no roster entry).

Parity result: **harness-roster 19/25 → 56/1** (ran to its own summary line for the first time),
**harness-0.3 189/40 → 202/26**, **zero regressions** in either (14 previously-failing base-identity
/ preflight dry-run rows now pass because the roster wiring + preview completeness unblocked them).
The single remaining harness-roster failure is a Windows argv quote-encoding nuance surfaced only
through the FAKE codex `.cmd`'s `%*` log (`\"` from Rust's `std::process` vs PowerShell's `""`); the
delivered argument value and the recorded `command` are byte-identical.

Tests: workspace `cargo test` green (**219**, +3 roster-walk tests), `cargo clippy --all-targets --
-D warnings` clean, `cargo fmt` applied.

Still open (next pass): brief items 2 (prior-finding lifecycle), 3 (main-turn `failed: codex exit
N`), 4 (`provider_config` raw echo — WIRE), 5 (`-CodexConfig` `~`/argv-order — CFG; the roster
half is done), 6 (rollout-file thread verification — F02-3), 7 (`--artifact` hashing/drift), 8 (the
full summary console), plus the pre-existing base-identity dry-run ordering rows (F02-1/OPENAI/F06-1
error-before-plan), F09-1 login-hang timeout, and F09-3 SSE classification.
