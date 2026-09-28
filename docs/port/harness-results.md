# Harness results — C3 binary through the codex-consult plugin's PowerShell harnesses

Measurement run against `C:\Users\Dmytro\claude-codex-consult` (checkout at HEAD, wave 25) using
`tests/run-all.ps1`'s wave-25 `-ScriptsDir` override, pointed at a scratch scripts directory
containing:

- the five C3 shims from `C:\Users\Dmytro\c3\tests\shim\` (`codex-providers.ps1`,
  `codex-consult.ps1`, `codex-findings.ps1`, `codex-scoreboard.ps1`, `codex-consult-hook.ps1`),
- an unmodified copy of the plugin's own `codex-consult-common.ps1` (427 KB) — several harnesses
  dot-source this file directly for real PowerShell helper functions (`Enter-TaskLock`,
  `Enter-WriteLock`, `Read-PendingFile`, `Get-CollabSnapshot`, `Resolve-ReviewerIdentity`, etc.)
  that exercise generic lock/store/parsing primitives, not the `c3` binary; it is not one of the
  "five scripts" the C3 CLI replaces,
- an unmodified copy of `schemas/consult-reply.schema.json` next to `scripts/` (some harnesses read
  `Split-Path -Parent $ScriptsDir` + `schemas\consult-reply.schema.json`).

`$env:C3_EXE` was set to `C:\Users\Dmytro\c3\target\debug\c3.exe` (built fresh via `cargo build`;
only a test file was newer than the exe, so no rebuild was needed for the binary itself). Harnesses
were run ONE AT A TIME, never in parallel, per `tests/README.md`.

Skipped per the brief: `harness-3b.ps1`, `harness-fixes.ps1`, `harness-pending.ps1` (survivor scans
that fail spuriously with real codex.exe processes on this machine).

## Shim change made during this measurement

`tests/shim/codex-consult.ps1`: `-FormatRetry`/`-DenialRetry` were originally mapped to boolean
`--format-retry`/`--no-format-retry` and `--denial-retry`/`--no-denial-retry` flag pairs (that was
`docs/port/cli-surface.md`'s design proposal). The actual `c3 consult --help` shows both are
VALUE-taking flags (`--format-retry <FORMAT_RETRY>`, `--denial-retry <DENIAL_RETRY>`, 0|1, default
1) — the real binary kept the plugin's own int semantics rather than the proposed boolean-pair
design. The old mapping made every PARENT/CFG/TRANSP/etc. call in `harness-0.3.ps1` (which always
passes `-FormatRetry 1` implicitly through `Consult`'s helper) fail with `error: a value is
required for '--format-retry <FORMAT_RETRY>' but none was supplied`, which cascaded into a
downstream `Get-Content` on a `sessions.json` that was therefore never written, crashing the whole
harness with an unhandled exception after only the SCAN/FP sections. Fixed both mappings to always
pass `--format-retry <0|1>` / `--denial-retry <0|1>` by value. This let `harness-0.3.ps1` run to
its own completion (printed summary line) for the first time.

## Summary table

| harness | checks run | passed | failed | ran to its own summary line? |
|---|---:|---:|---:|---|
| harness-0.3 | 229 | 154 | 75 | yes |
| harness-engines | ~60 (partial, see below) | ~18 | ~18 | no — crashed twice (DRYRUN/ROSTER/TREE), re-run with `-Only` for LISTING/UNIT/SCOREBOARD/ROSTER/TREE to gather more |
| harness-format | 11 | 3 | 8 | no — crashed in REPAIR |
| harness-lock2 | 9 | 8 | 1 | yes |
| harness-muse | 33 | 19 | 13 | no — crashed in ENGINEEXE |
| harness-panel | 21 | 11 | 10 | no — crashed in RUN |
| harness-roster | 58 | 18 | 34 | no — crashed in PROV |
| harness-visibility | 33 | 33 | 0 | no — crashed calling `Invoke-EngineTurn` (see below) |
| **totals** | **~454** | **~264** | **~159** | |

No harness failed to run AT ALL (every one produced real PASS/FAIL lines against the live `c3`
binary); several stopped partway through because the harness script itself hit an unhandled
PowerShell exception once a section it depends on produced no usable data (see "Harnesses that did
not finish" below) — this is inherent to testing a Rust CLI through scripts written to assume a
PowerShell implementation with in-process helper functions, not a shim bug found in this run.

## PASSED checks, grouped by area

**providers** (harness-0.3 PREFL/OPENAI provider-table subset, harness-roster PROV, harness-engines
LISTING): config scanning of `[model_providers.*]` tables (`SCAN`, 17/17), fingerprinting
(`FP`, 16/16), `codex-providers.ps1 -Json`/`-Short`/table/exit-codes for openai/ZAI/zeta/broken
(11/11 PREFL rows that don't need the plugin's own "codex-consult:"-prefixed wording), the full
`agy` engine listing row end to end (`LISTING`, 7/7: JSON shape, one `agy models` call per listing,
`-NoNetwork`, not-signed-in/no-launcher wording, table + ROSTER columns, hang timeout, SessionStart
hook clause) — this is C3's most complete area.

**consult dry-run / plan** (harness-0.3 F02-4, F02-5, F02-9, F06-*, LEDGER, KILL, TRANSP subset):
caps-v1 effort-vocabulary refusals for undeclared hosts/models (F02-4, 17/17 incl. MiMo's 5
declared models), off-peak window handling incl. malformed spec / crossing-the-boundary cases
(F02-9, 34/34; F06-3 partial), event-drift nets (EVENTS, 5/6), timeout kill without false survivors
(KILL, 5/5), ledger field order (LEDGER, 4/4), inline-table/dotted-key/multi-line-string TOML
constructs marking `[model_providers.openai]` unusable (F06-1 partial), one alias resolving to the
same endpoint fingerprint (F09, F09-3 class samples).

**findings / scoreboard / hook**: not separately exercised as a dedicated section in the harnesses
run (harness-0.3/roster/engines cover `codex-findings.ps1 -Stats`/`-Rate` and
`codex-scoreboard.ps1` only as adjuncts inside PREFL/F09 rows); the standalone codex lineage row of
`codex-scoreboard.ps1 -Json` was exercised indirectly and produced correct output when a codex
(non-agy) consultation was rated.

**lock** (harness-lock2): live-holder refusal naming the pid, held-lock can't be overwritten or
deleted, dead-holder reopened with no heuristics, release-by-close semantics, lock record shape
`{pid, start_time, host, task, started}`, a real consult recording a finding, `-List` showing a
running pending record, clean finish leaving no recovery record — 8/9 checks pass end to end
against the live binary + real child process.

**roster** (harness-roster UNIT/FILE/WALK/LIVE/F12-2/F15-2 subsets): roster file validation helper
functions (`UNIT`, 8/8, these run inside `codex-consult-common.ps1` directly, not through `c3`),
one FILE check, one WALK check, 4 LIVE checks, the F12-2 classifier order sample, and the F15-2
ordinal-listing-cache unit sample all passed.

**operator visibility** (harness-visibility UNIT/UNIT24B/UNIT24C, 33/33): every check that ran was
an in-process PowerShell unit test of helper functions in `codex-consult-common.ps1`
(`Get-RangeStat`, `Get-KilledTurnFailure`, `Test-ContinuationReply`, the kimi/byteplus/ZAI-vs-zai
classifier cases, `Get-FailureHint`, `New-ListingCache`) — these do not call `c3` at all, so their
100% pass rate reflects the shared library being copied unmodified, not C3 behavior.

**engines (agy)**: the full `agy` provider-listing row (`LISTING`, above) and the generic MSP/event
unit helpers (`UNIT`, 9/9 in harness-engines, 19/19... in harness-muse's own UNIT section for the
muse adapter) pass; anything requiring `c3 consult --engine agy` or `--engine muse` to actually run
fails or is refused (see below) because M2d (agy/muse engines) is not implemented in `c3 consult`
yet.

## FAILED checks, grouped by reason

### Not implemented in C3 yet

- **Panel** (`--panel`, harness-panel DRY/RUN, 10 checks): `c3 consult: -Panel is a milestone 4
  feature; c3 consult runs one reviewer (drop -Panel).` — every panel dry-run/run/concurrency/lock
  check fails on this one refusal. `--panel-concurrency` itself is rejected by clap as an unknown
  argument in some invocation shapes even though `c3 consult --help` lists it (`error: unexpected
  argument '--panel-concurrency' found`) — worth a closer CLI-arg-grouping look once panel lands.
- **agy/muse engines** (`--engine agy`/`muse`, harness-engines DRYRUN/ROSTER/TREE, harness-muse
  DRYRUN/ENGINEEXE/ROSTER, ~29 checks combined): `c3 consult: -Engine agy lands in milestone 2d; c3
  consult currently runs the codex engine only.` (and the muse-equivalent wording); `-EngineExe`,
  `-MaxModelSteps`, `-DenialRetry` all refused the same way since they all require a non-codex
  engine. A roster containing an agy entry falls back silently to the next codex entry
  (`lineage : openai :: gpt-5.1` instead of the expected agy lineage) rather than reporting agy as
  unimplemented at the roster-walk level — worth flagging when M2d lands, since today it looks like
  a working fallback rather than an unimplemented engine.
- **Provider-history preflight gating** (harness-0.3 F09-2/4, F09-3, PREFL, ~9 checks): a prior
  auth/quota failure on an endpoint is recorded but does NOT yet block a later run on the same
  endpoint the way the plugin does — e.g. `... and the next run on that endpoint is refused by that
  auth failure` got `c3 consult: failed: quota - Error: credits exhausted for this token plan (wall
  0 s)` (the run was attempted and failed live instead of being refused up front from history), and
  several `-Provider` runs that should be refused for 60 minutes after a quota hit instead show
  `c3 consult: usable reply - ... (source: events)`.
- **`-Thread` fork/resume lineage walk** (harness-0.3 PARENT, F02-3, 14+8 checks): forking the
  newest thread of a lineage, refusing cross-lineage/legacy/unresolved/unverified `-Thread` values,
  and the rollout-verified-thread selection all fail; most give `c3 consult: -Thread needs -Mode
  fork or resume (-Mode new always starts a fresh thread).` where the plugin expects a lineage-aware
  walk to run or refuse with a specific reason. This looks like `-Thread`/`-Mode fork`/`-Mode
  resume` parent-selection logic (A8.1-era design) not yet ported.
- **Format repair turn** (harness-format REPAIR, 6 checks): the dry-run preview and refusal cases
  pass, but an actual prose-then-JSON-on-resume repair does not yet produce a populated
  `format_retry` record (`reason=` empty, `drift=` empty, handoff file not written at
  `repair-resume.log`, crashing the harness when it tries to read that log). Format-retry appears
  wired at the CLI/refusal level but not at the actual repair-turn level yet.

### C3 output differs

- **Error-message prefix**: C3 prints `c3 consult:` / `c3 providers:` where the plugin (and every
  harness assertion) expects `codex-consult:`. Verbatim: expected-vs-actual for
  `-FormatRetry 2 refused` — expected `codex-consult: -FormatRetry must be 0 or 1 (got 2): at most
  one format-repair turn per consultation.`, got `c3 consult: -FormatRetry must be 0 or 1 (got 2):
  at most one format-repair turn per consultation.` (message body is byte-identical past the
  prefix). This one substitution likely explains a meaningful fraction of the "differs" failures
  across every harness that does exact/regex `-match` on the refusal text, since the wording after
  the prefix is frequently otherwise correct.
- **Reviewer/endpoint line loses the full base_url**: `reply header carries the Reviewer line`
  expected `... endpoint https://api.z.ai/api/v1, wire_api: responses; ...` and got `... endpoint
  api.z.ai; ...` (bare host, no scheme, no path, and the `wire_api: responses` clause dropped
  entirely). Same pattern in OPENAI's "console names the endpoint" check (`endpoint proxy.example`
  vs. the full configured URL).
- **`wire_api: (default)` clause missing**: `WIRE console reviewer line says "wire_api: (default)"`
  — evidence shows the reviewer line C3 prints has no `wire_api` clause at all rather than the
  expected explicit `(default)` marker.
- **Telemetry consent banner on stdout pollutes exact-match/first-line checks**: several PREFL/F09-1
  checks that assert on the FIRST line of output instead observe `C3 telemetry is on. Installing C3
  means accepting these terms: one anonymous event per consultation goes to https://xelth.com/T/ -
  never your code, prompts, paths, or keys.` — this is a first-run consent banner c3 prints that the
  plugin's `codex-consult.ps1` has no equivalent of; it breaks every harness assertion that reads
  `$First`/`$d.First` as the refusal or preflight line.
- **Transport/format interaction on a short reply**: `TRANSP real run: codex gets no
  --output-schema; a fenced reply validates locally` — instead of a clean prompt-only validation, C3
  reports `Structured reply: INVALID (the reply is not a valid consult-reply v1 object (format
  repair not attempted: reply too short (3 words))) - raw text kept; no findings recorded.` on a
  case the harness expects to succeed.

### Shim mapping

- Fixed during this run (see "Shim change made" above): `-FormatRetry`/`-DenialRetry` boolean-pair
  mapping was wrong; both are now forwarded as `--format-retry <0|1>` / `--denial-retry <0|1>`.
- No other parameter-mapping bugs were found; `codex-providers.ps1`, `codex-findings.ps1`,
  `codex-scoreboard.ps1`, `codex-consult-hook.ps1` shims all matched their respective `c3 --help`
  output exactly on inspection.

### Environmental

- None specific to this measurement beyond the ones the brief already told us to skip
  (harness-3b/fixes/pending's survivor scans, which see real codex-like processes on this machine).

## Harnesses that did not finish, and why

- **harness-engines**: crashed twice. First (`-ScriptsDir` full run, no `-Only`): after the
  DRYRUN section's 9th agy-engine check (all correctly refused as "milestone 2d"), the harness
  tried `Last-Entry` on a task whose `sessions.json` was never created (because the DRYRUN cases
  that follow expect a REAL agy run to have written one) → `Es ist nicht möglich, einen Index auf
  ein NULL-Array anzuwenden` (indexing a null array). Re-run with `-Only 'LISTING,ROSTER,SCOREBOARD,
  UNIT,TREE'` got further but crashed again in TREE (`Remove-Item` on a `probe.txt` the agy-engine
  case never created, same root cause: agy engine unimplemented). A final `-Only 'LISTING,
  SCOREBOARD'` run completed cleanly (8 passed, 2 failed). Sections never reached at all: PANEL,
  SIGNIN, DENIAL, TIMEOUT, RESUME, RUN, PROSE, the harness's own `FAIL` section — all depend on a
  working `--engine agy` run.
- **harness-format**: crashed in REPAIR reading `repair-resume.log`, which the format-repair turn
  never wrote (see "Format repair turn" above). Everything after the first REPAIR case (prose
  twice, drift notes, a different thread, panel-member repair, wave 15's prose-gate/drift-check-5,
  the orphaned-original recovery record) was not reached.
- **harness-muse**: crashed the same way as harness-engines' first run
  (`Last-Entry`/`NullArray`) once the ENGINEEXE section's real-run cases needed a muse consultation
  to have written `sessions.json`; muse is unimplemented the same way agy is. Everything past
  ENGINEEXE (the MSP parser/turn rules end to end, caps-v1 `muse-v1`, billing guard, sign-in states,
  a full run's ledger, prompt/launcher path handling, D12's forced `permission` class, resume, wave
  23b's format repair via the muse adapter) was not reached.
- **harness-panel**: crashed in RUN reading `findings.json`, never written because the panel run
  itself is refused up front (`-Panel is a milestone 4 feature`) so no member ever produces a
  finding. Everything past the first RUN case (wall-clock-below-sum, roster-order sorting, the
  commit write-lock contention cases, kill-mid-panel, `-PanelConcurrency` semantics, `-Rate` refused
  by an active record) was not reached.
- **harness-roster**: crashed in PROV reading a `sessions.json` after an "unusable roster" case
  where `c3 providers` correctly refused (exit 1) but with the `c3 providers:`-prefixed message
  (see "C3 output differs" above) rather than the exact wording the harness pattern-matches;
  downstream of that the harness assumed a normal run had populated the ledger for `Last-Entry`.
  Everything past PROV (the review panel roster-related cases, DST reset-time edge cases beyond
  QUOTA, more of F12-2's classifier order, `-SchemaTransport` on a roster entry) was not reached.
- **harness-visibility**: crashed calling `Invoke-EngineTurn` — a PowerShell function name pulled
  directly from the AST of the REAL `codex-consult.ps1` (F15-6: "taken from the AST of
  codex-consult.ps1"). Because the scripts-under-test directory holds C3's thin `.ps1` shim instead
  of the real multi-thousand-line plugin script, that function simply does not exist in this
  directory for the harness to introspect or call. This is a structural limitation of testing a
  Rust binary through a PowerShell-source-level test (AST reflection, not argv/stdout black-box
  testing) — no shim can satisfy it short of vendoring the real script's internal functions. All 33
  checks that ran before this point (UNIT/UNIT24B/UNIT24C) are pure `codex-consult-common.ps1` unit
  tests and do not exercise `c3` at all; everything from GATES/GATES24C/BURST onward (the actual
  end-to-end continuation/visibility behavior of a `c3 consult` run) was not reached.

## Open questions

- Should `c3`'s error-message prefix be changed from `c3 consult:`/`c3 providers:` to
  `codex-consult:` to match the plugin's own wording (and every harness assertion), or should the
  harnesses be considered out of scope for that particular string? This single substitution is
  implicated in a large fraction of the "C3 output differs" failures across every harness run here.
- Should the first-run telemetry consent banner be suppressed (or sent to stderr only, never
  stdout) in a scripted/harness context, since it currently corrupts every check that reads the
  first line of `c3`'s output as the refusal/preflight message?
- `harness-visibility`'s F15-6 case (and any other AST-introspection case elsewhere) cannot be
  satisfied by a thin shim in principle — is that acceptable as a permanent gap in this comparison,
  or does it call for a stub `codex-consult-common.ps1`-shaped file that at least defines
  `Invoke-EngineTurn` (and whatever else is introspected) as a thin forwarder too?

---

# Run 2 — after the single-consultation parity pass (M2d-3)

Same procedure as Run 1: the scratch scripts directory holds the five C3 shims from
`tests/shim/`, plus the plugin's own `codex-consult-common.ps1` **and** `codex-consult-detached.ps1`
(the common lib dot-sources the latter) and a sibling `schemas/consult-reply.schema.json`.
`$env:C3_EXE = target\debug\c3.exe` (rebuilt this pass); each harness run ONE AT A TIME through the
wave-25 `-ScriptsDir` override. Only the two harnesses in scope were re-run: `harness-0.3.ps1` and
`harness-roster.ps1`.

## Summary table (Run 1 → Run 2)

| harness | Run 1 pass/fail | Run 2 pass/fail | ran to its own summary line? | regressions |
|---|---|---|---|---|
| harness-0.3 | 154 / 75 | **189 / 40** | yes (both runs) | none (every Run-2 failure was already failing in Run 1) |
| harness-roster | 18 / 34 (crashed in PROV) | **19 / 25** (crashed in QUOTA — reached further) | no (both runs) | none |

Net: **+35** checks now pass in harness-0.3, **+1** in harness-roster (and it now advances past the
PROV/FILE/WALK sections into QUOTA before the harness's own `Last-Entry`/null-array crash — that
crash is inherent to running a Rust CLI through a PowerShell-source harness once a section it needs
produced no ledger, unchanged from Run 1). A Run-1-vs-Run-2 diff of the FAIL sets shows **zero new
failures** in either harness.

## What the parity pass fixed (the Run-1 "C3 output differs" list, now closed)

- **Error-message prefix** — `codex-consult:` / `codex-providers:` everywhere (was `c3 consult:` /
  `c3 providers:`). This alone flipped a large fraction of the exact/`-match` refusal assertions.
- **Telemetry consent banner** — no longer printed to STDOUT before the run; it now prints to
  STDERR after the summary/refusal, once per installation, with the marker under the real home
  (`~/.codex/c3/telemetry/notice-shown`, ignoring `CODEX_HOME`) so scratch homes never trigger it.
  The first-line PREFL/F09-1 assertions are no longer corrupted.
- **Reviewer line** — the full canonical base_url and the `wire_api: <x>` / `wire_api: (default)`
  clause (`endpoint https://api.z.ai/api/v1, wire_api: responses`, `endpoint (default), wire_api:
  (default)`) in the dry-run line, the handoff header and the console reviewer line.
- **TRANSP fenced reply** — a fenced short JSON object now validates locally (the fence is stripped
  before the structured parse), so `Structured reply (prompt-only transport): ...` renders instead
  of `reply too short (3 words)`.
- **`-Thread` with auto mode** — `-Thread` without `-Mode` now reaches the parent-thread walk (only
  an explicit `-Mode new` + `-Thread` is refused), so the PARENT cross-lineage / unknown / legacy /
  unresolved / candidate refusals fire from `Select-ParentThread`.
- **Endpoint-health preflight** — a recorded auth/usage/burst/quota-unknown failure on the endpoint
  now blocks a later run before the lock (the QUOTA `... out for 60 minutes, until <iso>; nothing
  was started (pass -SkipPreflight to launch anyway)` refusal passes), and `-SkipPreflight` prints
  the `Format-QuotaWarning` line.

## Remaining "C3 output differs" / failing checks (verbatim, top 10)

These are the largest remaining clusters; all were already failing in Run 1 (no regressions).

1. `FAIL WALK env key of entry 1 missing -> entry 2 selected: reviewer mimo from roster ... ledger roster {path, position 2, skipped [ZAI missing], applied [model]}` — the **roster walk** (`Select-RosterReviewer`, decision 5a) is not implemented.
2. `FAIL WALK no entry available -> refusal listing every entry with its reason` — roster walk.
3. `FAIL WALK console and handoff header carry the Roster line` — roster walk (the `roster{}` record + `Roster:` line).
4. `FAIL FILE unusable roster files refused (-DryRun too), message names the path: corrupt, version2, ...` — roster file validation (roster walk).
5. `FAIL QUOTA future provider_failure.retry_after in another task -> openai skipped ..., ZAI selected` — roster-walk quota-skip (endpoint-health gating exists, but the walk to the next entry does not).
6. `FAIL F02-3 no thread.started + a rollout containing the consultation id -> thread verified` — the rollout-file thread verification / `thread_candidate` fallback is deferred (M2c note).
7. `FAIL F02-1 config model_provider=ZAI, no -Provider: preview records ZAI (source config), lineage ZAI :: glm-5.3` — pre-existing identity path (dry-run preview of a config-sourced identity); not in this pass's decisions.
8. `FAIL F02-1 unreadable config (fatal line) -> provider unknown, lineage unknown :: gpt-5.1, fingerprint empty, mode new with a note` — pre-existing: an unresolved identity makes `effort_plan` error and `build_context` returns that error before the dry-run plan is rendered (an ordering issue in the base identity path, not a decision here).
9. `FAIL WIRE reply header says "wire_api: (default)"; ledger provider_config has no wire_api` — the reviewer line is now correct, but `reviewer.provider_config` still carries `wire_api: "(default)"`; the plugin echoes only the table's declared keys, so it omits it. (Left alone to avoid disturbing the M2c LEDGER byte-parity; `build_reviewer` synthesizes `{base_url,name,wire_api}` rather than echoing the raw table.)
10. `FAIL F04-* / prior-finding lifecycle` (not reached in these two harnesses' passing subset) and `FAIL CFG ~/ is expanded ...` — the `-CodexConfig` `~` expansion / comma-split argv rows and the prior-finding ingestion (decision 6) remain.

## Not implemented this pass (the two largest subsystems)

- **Roster walk** (`Select-RosterReviewer`, decision 5a): entry selection, skip reasons, the
  `roster{}` ledger record, the `Roster:` header/dry-run line, `--provider X` taking the entry's
  model, `codex_config` from the entry. This is the bulk of the remaining `harness-roster` WALK/FILE
  failures and the roster-related QUOTA rows. The **parent-thread walk** half of decision 5 (5b) IS
  implemented and unit-tested.
- **Prior-finding lifecycle** (decision 6): the open-findings prompt snapshot already exists; the
  reply-side ingestion (the ACCEPT-vs-still-open-blocker contradiction, `unchecked_prior_blockers`,
  `supersedes`/`superseded_by`, `reviewer_checks`) is not yet wired into the commit path.

---

# Run 3 — the roster walk wired into `c3 consult` (M2d-4)

Same procedure as Run 2 (the five C3 shims from `tests/shim/`, the plugin's own
`codex-consult-common.ps1` + `codex-consult-detached.ps1`, a sibling
`schemas/consult-reply.schema.json`; `$env:C3_EXE = target\debug\c3.exe`, rebuilt this pass;
each harness run ONE AT A TIME through the wave-25 `-ScriptsDir` override). Only the two
harnesses in scope were re-run: `harness-0.3.ps1` and `harness-roster.ps1`.

## Summary table (Run 2 → Run 3)

| harness | Run 2 pass/fail | Run 3 pass/fail | ran to its own summary line? | regressions |
|---|---|---|---|---|
| harness-0.3 | 189 / 40 | **202 / 26** | yes (both runs) | none (a Run-2-vs-Run-3 FAIL-set diff shows **zero** new failures; 14 Run-2 failures now pass) |
| harness-roster | 19 / 25 (crashed in QUOTA) | **56 / 1** (ran to completion) | no in Run 2, **yes** in Run 3 | none |

Net: **+13** in harness-0.3, **+37** in harness-roster. harness-roster now advances all the way
through PROV/FILE/WALK/QUOTA/RULE1/RULE2/AUTO to its own summary line for the first time (the
Run-1/Run-2 `Last-Entry`/null-array crash is gone because every section it depends on now
produces a ledger). A Run-2-vs-Run-3 diff of the harness-0.3 FAIL sets shows **zero** new
failures; the 14 flipped-to-pass checks are the base-identity/preflight dry-run rows the roster
wiring unblocked (F02-1 preview identity, F06-1 unresolved-identity dry-run plan, F09-1/PREFL/OPENAI
`-DryRun still prints the verdict`).

## What this pass landed (item 1 of the brief — the roster walk, codex)

- **Roster read + fail-closed validation, wired into consult** — `c3 consult` now reads the
  reviewer roster (`providers::read_reviewer_roster`) before planning; a `CODEX_CONSULT_ROSTER`
  file that does not exist, or an unusable roster, refuses on a real run **and a dry run** (FILE).
  `CODEX_CONSULT_ROSTER=none` and a missing default file resolve silently to no roster.
- **`Select-RosterReviewer` (rule 3, the walk)** — `providers::Ctx::walk_full`: the first entry
  whose preflight verdict is available, `-Model` narrowing, `-SkipPreflight` (first entry
  unchecked), the skip records, the "no entry available" refusal listing every entry with its
  reason, and the "-Model … no entry resolves" refusal. Quota-skip across tasks rides the existing
  `endpoint_health` (a future `provider_failure.retry_after` in another task skips that endpoint).
- **`-Provider` rule / `-Thread` rule** — `find_roster_entry` supplies the entry's model (when
  `-Model` is empty) and `codex_config` (when `-CodexConfig` is empty); the `-Thread` rule sources
  the reviewer from the thread's ledger entry and names the roster's alternative when that
  endpoint is out.
- **The `roster{}` ledger record + `Roster:` line** — `{path, position (null when no entry),
  skipped[{provider,model,engine,reason}], applied[]}`; the `Roster: …` line on the console (real
  run), the dry-run block and the handoff header. `extra_config_source` (`""`/`-CodexConfig`/`roster`)
  and the `provider_source`/`model_source` overrides (`roster`/`-Thread`) are recorded.
- **Dry-run preview completeness** — the `sessions.json entry preview` now carries `reviewer`,
  `preflight`, `preflight_warning`, `roster`, `panel` (null), `extra_config`, `extra_config_source`,
  and the `preflight :` dry-run line now shows the verdict **label** (matching the plugin), which
  flipped the F09-1/F06-1/OPENAI/PREFL dry-run verdict rows to pass.

## Remaining differing checks (verbatim, top 10)

1. `FAIL WALK env key of entry 1 missing -> entry 2 selected: reviewer mimo ... argv -c model_provider="mimo"` — the **only** harness-roster failure. The `roster{}` record, `Roster:` line, reviewer and sources are all correct and the ledger `command` contains `-c model_provider="mimo"` byte-for-byte; the failing conjunct is the FAKE_CODEX `.cmd` log's `%*`, which expects PowerShell's `model_provider=""mimo""` quote-doubling. Rust's `std::process` escapes the embedded quotes as `\"` for the Windows command line, so cmd's `%*` logs `\"` not `""`. Environmental (the delivered argument value and the recorded command are identical), same class as the documented `cwd`/schema-path diffs.
2. `FAIL F02-3 no thread.started + a rollout containing the consultation id -> thread verified` (+7 more F02-3 rows) — the rollout-file thread verification (brief item 6) is not done this pass.
3. `FAIL CFG ~/ is expanded ... passed as -c after the bridge's own -c options, before -o; ledger extra_config` (+3 more CFG rows) — `-CodexConfig` `~` expansion / comma-split / argv-order (brief item 5) not done this pass; note the roster's `codex_config` `~` expansion (RULE1) DOES pass, since it runs through `convert_from_codex_config_items` at validation time.
4. `FAIL WIRE reply header says "wire_api: (default)"; ledger provider_config has no wire_api` — `reviewer.provider_config` still synthesizes `{base_url,name,wire_api}` rather than echoing the raw table keys (brief item 4) not done this pass.
5. `FAIL F02-1 unreadable config (fatal line) -> provider unknown, lineage unknown :: gpt-5.1, fingerprint empty, mode new with a note` (+4 more F02-1 rows) — the pre-existing base-identity dry-run ordering (an unresolved identity makes `effort_plan` error before the plan renders); not a decision of this pass.
6. `FAIL OPENAI no table + OPENAI_BASE_URL in the environment: preview records it, console names it` — pre-existing base-identity path.
7. `FAIL F09-1 codex login status hanging past 15 s -> refused ...` — the login-timeout wiring (pre-existing, partial in M2d-3).
8. `FAIL F09-3 a failed run with an SSE error on stderr -> ledger provider_failure {auth, invalid_api_key, ...}` — the SSE-error-on-stderr classification (pre-existing).
9. `FAIL TRANSP undeclared host -> prompt-only by default, and the dry run says so` / `FAIL F02-4 undeclared host -> refused` — the undeclared-`localhost` host rows (pre-existing; the effort/transport refusal wording differs).
10. `FAIL PARENT no thread of lineage ZAI :: glm-4.6 -> mode new with a note naming legacy/other/unresolved/candidates` — the parent-walk note enumeration (pre-existing).

## Not implemented this pass (brief items 2–8)

Items 2 (prior-finding lifecycle), 3 (main-turn `failed: codex exit N`), 4 (`provider_config` raw
echo), 5 (`-CodexConfig` `~`/argv-order — the roster half is done, the `-CodexConfig` half is not),
6 (rollout-file thread verification), 7 (`--artifact` hashing/drift), 8 (the full summary console)
were **not** implemented this pass. This pass was scoped to the roster walk (item 1), the single
largest remaining harness-roster cluster; it closes all of harness-roster except the one
Windows-argv-quoting check above.

---

# Run 4 — the remaining codex-engine rows (M2d-5)

Same procedure as Run 3 (the five C3 shims from `tests/shim/`, the plugin's own
`codex-consult-common.ps1` + `codex-consult-detached.ps1`, a sibling
`schemas/consult-reply.schema.json`; `$env:C3_EXE = target\debug\c3.exe`, rebuilt this pass;
run through the reusable wave-25 `-ScriptsDir` override). Only `harness-0.3.ps1` was re-run:
`harness-fixes.ps1` was **skipped** because a real `codex.exe` (pid 15256) is running on the
machine (its survivor scans fail spuriously), per the brief. The c3.exe used is a
`--no-default-features` build of `c3-cli` (the SurrealDB-backed index is not exercised by the
consult/providers harness, and the shared `target` disk is at 100% while the index worker's
`surrealdb` dependency compiles); the consult/providers surface is identical either way.

## Summary table (Run 3 → Run 4)

| harness | Run 3 pass/fail | Run 4 pass/fail | ran to its own summary line? | regressions |
|---|---|---|---|---|
| harness-0.3 | 202 / 26 | **226 / 3** | yes (both runs) | none (a Run-3-vs-Run-4 FAIL-set diff shows **zero** new failures; 23 Run-3 failures now pass) |

Net: **+24** in harness-0.3. All three remaining failures are environmental (below); zero
genuine "C3 output differs" failures remain.

## What this pass landed (brief items 2–8, plus item 1's ledger core)

- **Main-turn `failed: codex exit N - <detail>`** (item 2) — the failed-main-turn outcome is
  now `failed: codex exit N` with ` - <event error | last stderr line>`, built after the
  evidence is read (`codex-consult.ps1:3840`). The classified `provider_failure` is (re)built
  from that evidence by `codex_failure_pf` (so `when`/`kind`/`hint` are stamped like
  `New-ProviderFailure`), which fixed **F09-3** (`{auth, invalid_api_key, message, when}` + the
  handoff `Provider failure:` line). A new c3-core test pins the `Get-EndpointHealth` legacy
  parse of that literal.
- **`reviewer.provider_config` raw echo** (item 3, WIRE) — `Get-ProviderEndpoint`'s `Config`
  is now echoed (declared table keys, ordinal-sorted, secrets dropped, `base_url` audited,
  booleans/integers typed), so an absent `wire_api` leaves no key; the built-in openai shape is
  `{builtin:"openai"[, base_url, base_url_source:"OPENAI_BASE_URL"]}`. WIRE passes.
- **`-CodexConfig` `~` expansion** (item 4, CFG) — `args::validate` now expands `~` against the
  real user home (`$HOME`/`$USERPROFILE`), not `CODEX_HOME` (the argv order + `extra_config_source`
  were already correct). Both `~`-expansion CFG rows pass.
- **Rollout-file thread verification** (item 5, F02-3) — `engines::codex::find_thread_in_rollouts`
  ports `Find-ThreadInRollouts`; a rollout under `<CODEX_HOME>/sessions/<y>/<m>/<d>/rollout-*.jsonl`
  holding the consultation id verifies the thread (`source: rollout (verified by consultation
  id)`), else the newest uuid is an unverified `thread_candidate` (console + handoff + ledger,
  never a thread or parent). All F02-3/EVENTS rollout rows pass; the PARENT candidate-count note
  now fires.
- **`--artifact` hashing** (item 6, F04-9) — `-Artifact` paths are resolved (missing refuses),
  hashed pre-run and re-hashed after (`artifacts[]` = `{path,sha256,sha256_after}`,
  `artifacts_changed_during_review`, the `WARNING: artifact(s) changed ...` drift line, and the
  timeout-continuation files-changed gate). (Exercised in harness-fixes, skipped this run;
  unit-covered.)
- **Summary console completeness** (item 7) — the rollout-candidate `thread      :` line, the
  `prior      :`/`unknown ids:`/`supersedes :` lines and the `verdict    : (invalid: ...)` form.
- **Pre-existing rows** (item 8): the localhost effort refusal now lists the declared caps-v1
  hosts (F02-4); `OPENAI_BASE_URL` with no table records `{builtin, base_url, base_url_source}`
  and the console names `endpoint builtin:openai via OPENAI_BASE_URL <url>` (OPENAI); the
  `codex login status` timeout is 15 s (F09-1); the SSE-on-stderr classification (F09-3); the
  undeclared-host transport basis wording (TRANSP); the parent-walk note (PARENT). An unresolved
  identity no longer lets `effort_plan` mask the identity/preflight refusal, and an explicit
  `-Provider` with an unusable table refuses up front with the scanner's line number (F02-1,
  OPENAI); the dry-run `parent      :` line renders the parent note; the fork/resume-unresolved
  refusal ends `pass -Provider and -Model explicitly, or use -Mode new`.
- **Prior-finding lifecycle ledger core** (item 1, F04-4/6) — `consult::semantics`
  (`Test-ReplySemantics`): the verdict-vs-purpose gate (F04-6) and the ACCEPT-vs-new-blocker /
  ACCEPT-vs-still-open-prior-blocker contradictions (F04-4) blank the verdict and set
  `validation_error`/`unchecked_prior_blockers`; `prior_findings` reports are ingested onto the
  stored findings' `reviewer_checks[]`, and each new finding's `supersedes` folds into the old
  finding's `superseded_by[]` (`FindingsDelta` extended). (Exercised in harness-fixes, skipped
  this run; unit-covered.)

## Remaining failing checks (verbatim — all environmental)

1. `FAIL F02-1 real run: codex received model_provider=ZAI ... ARGS ... -c model_provider=\"ZAI\"` —
   the documented Windows argv quote-doubling diff: Rust's `std::process` escapes the embedded
   quotes as `\"` for the Windows command line, so the FAKE codex `.cmd`'s `%*` logs `\"` where
   the harness pattern expects PowerShell's `""`. The delivered argument value and the recorded
   `command` are byte-identical (same class as the one harness-roster failure).
2. `FAIL CFG real run: codex receives the expanded override before -o ... -c model_catalog_json=\"C:/Users/Dmytro/.codex/model-catalogs.json\"` —
   same Windows `\"` vs `""` quoting diff; the `~` expansion is now correct.
3. `FAIL CFG one comma-separated string: split only where the next key= starts` — c3's dry-run
   output is byte-correct (verified directly: `extra_config` = `["model_catalog_json=\"<home>/a.json\"",
   "tools.web_search=[1,2]", "model_reasoning_summary=\"detailed\"", "hide_agent_reasoning=true"]`,
   exit 0); the harness reported empty evidence, i.e. its `$b.Preview` extraction returned nothing
   for the multi-item comma+quotes `-CodexConfig` value — a harness/shim arg-passing artifact, not
   a c3 output difference (the two single-item `~` CFG rows pass).

# Run 5 — agy/muse engines wired into `c3 consult`, dry-run path (M2d pass 1)

This pass wires the agy and muse engines into `c3 consult` for **everything that does not launch
a live engine turn**: engine selection (`-Engine`/roster/`-Thread`/walk, with the `engine_from`
label), the reviewer identity + `provider_config` (`{engine, launcher}` for agy;
`{engine, launcher, credential_mechanism: "oauth"}` for muse), the effort/transport resolution
(model-tier for agy, `muse-v1` for muse; `native` default with the `-SchemaTransport` overrides
and the codex/engine refusals), the capability refusals (fork / sandbox / `-CodexConfig` /
transport / `-MaxModelSteps` / `-EngineExe` binding / muse billing guard), the engine file-name
prefix (`NN-agy-*` / `NN-muse-*`), the ledger/preview `reviewer` (engine, harness, provider_config)
and `engine_run`, and the full per-engine dry-run block (the `engine`/`harness`/`denial retry`/
`max steps`/`sandbox`/`stdin`/`prompt file`/`reply source` lines and the engine `Tools:` prompt
line). A **non-dry** `--engine agy|muse` run is still refused with
`codex-consult: the <engine> engine runs at milestone 2d pass 2; use --dry-run` (pass 2 owns the
live turns: denial retry, tree check, secondary turns).

Same shim setup as Runs 2–4 (the scratch scripts directory with the C3 shims + the plugin's own
`codex-consult-common.ps1` + a sibling `schemas/consult-reply.schema.json`; `$env:C3_EXE` = a fresh
`cargo build` of `target\debug\c3.exe`). `harness-engines.ps1` and `harness-muse.ps1` were run ONE
AT A TIME. Both were run with **`-Only` restricted to the pass-1 sections**
(`UNIT,ROSTER,DRYRUN,LISTING` for engines; `UNIT,ROSTER,DRYRUN,ENGINEEXE` for muse): a full run
crashes at the first live-run section (`RUN` → `Last-Entry` on an empty ledger) because pass 1
refuses live engine turns — that crash is expected and is pass-2 work, not a regression.

Local gates before the harness: `cargo build` (clean), `cargo test` (**256 passed, 0 failed** —
unchanged), `cargo clippy --all-targets -- -D warnings` (clean), `rustfmt` on the changed files.

## Summary table (Run 1 → Run 5, pass-1 sections)

| harness | Run 1 pass/fail | Run 5 pass/fail (pass-1 sections) | notes |
|---|---|---|---|
| harness-engines | ~18 / ~18 (partial, crashed twice) | **31 / 3** (`UNIT,ROSTER,DRYRUN,LISTING`) | all 3 fails = the schema-path exact-command match (below) |
| harness-muse | 19 / 13 (crashed in ENGINEEXE) | **30 / 3** (`UNIT,ROSTER,DRYRUN,ENGINEEXE`) | 1 N/A-to-c3, 1 schema-path, 1 pass-2 |

All ROSTER checks (engine-field validation, the walk selecting the engine, `roster.applied`
`[engine, model]`, the `-Engine`/`-Provider` contradiction refusal, the not-signed-in skip, the
F10-3 label warning) and the identity/effort/transport/refusal/preflight/file-naming/`engine_run`
DRYRUN checks pass for both engines. The muse LISTING/SIGNIN and the engine RUN/RESUME/DENIAL/FAIL
sections are pass 2 (live turns) and were not measured.

## Remaining differing checks (verbatim), grouped as pass-1 vs pass-2

### Pass-1 differences (a real c3 design divergence, not fixable in this pass)

- `FAIL DRYRUN argv: ... --json-schema <schema> ...` (harness-engines) and
  `FAIL DRYRUN argv pinned to the real flags: muse exec --json --prompt-file ... --output-schema <schema> ...`
  (harness-muse), plus the two agy rows that fold the full command in
  (`FAIL DRYRUN -NativeEffort high -> --effort high at the end`,
  `FAIL DRYRUN -Mode resume -> --conversation ...`).
  **One diagnosis for all four:** the harness compares the whole command against its own
  `$schemaPath` (`<parent of scripts>/schemas/consult-reply.schema.json`, the plugin's on-disk
  schema). c3 materializes its **own** schema under `CODEX_HOME` and passes
  `<CODEX_HOME>/c3/schemas/consult-reply.v1.json` — a different path *and* filename. This is the
  same divergence codex has; codex passes only because harness-0.3 checks `--output-schema`
  *presence*, whereas the agy/muse harnesses hard-code the exact path. The flag **structure** and
  order are still confirmed by the passing sibling checks (`-SchemaTransport prompt-only` drops the
  flag, `-Purpose chore` drops it, `native` is present, `-MaxModelSteps 40` appends
  `--max-model-steps 40` as a suffix, `-Thread` appends `--conversation <t0>` as a suffix,
  `effort_sent`/`effort_mapping` are correct). Not fixable without changing where c3 ships its
  schema (out of scope; would affect codex byte-identity elsewhere).

### Not applicable to c3

- `FAIL UNIT D2 (every engine): codex-consult.ps1 names no engine's parser, outcome or argv
  function ...` (harness-muse) — this UNIT check reads `[IO.File]::ReadAllText($consultPs)` and
  counts `& $engineSpec.Adapter.Argv -Turn` / `.Events` / `.Outcome` call sites in the script under
  test. `$consultPs` is the **C3 shim** (a thin forwarder), so it has zero such call sites. The
  check asserts an internal structural property of the PowerShell orchestrator; it cannot pass
  against a shim and is not a c3 behavior.

### Pass-2 (live-turn) checks that surfaced in a pass-1 section

- `FAIL ENGINEEXE D3: ... with none at all a real run is refused ("muse CLI not found")`
  (harness-muse) — the check's `$noLauncher` leg is a **non-dry** muse run with no launcher; it
  expects the live "muse CLI not found on PATH" refusal, but pass 1 refuses every non-dry engine
  run first (`the muse engine runs at milestone 2d pass 2; use --dry-run`). The dry-run legs of the
  same section (`-EngineExe` binding, ambiguity, `-Engine codex` → `-CodexExe`, no-roster/no-Engine,
  a launcher that does not exist, the vendor install location) pass. Lands in pass 2 with the live
  path.

# Run 6 — agy/muse live turns wired into `c3 consult` (M2d pass 2)

This pass drives the agy and muse engines through their live turns: the primary turn per engine
(`run_primary_turn` -> `AgyEngine`/`MuseEngine`), the resolved parent-thread resume
(`--conversation` / `--session-id`), the agy denial-retry turn (F11), the read-only tree check
(wave 26b **D9**), the timeout continuation and format-repair turns through the engine adapters, and
the per-engine ledger/handoff/summary lines (`engine_run`, `denial_retry`, the muse
`Tokens: not reported by muse.` line, the usage mapping, the tree-check warnings). The pass-2
refusal is gone; a non-dry `--engine agy|muse` run now executes.

Same shim setup as Runs 2-5 (the C3 shims from `tests/shim/`, the plugin's own
`codex-consult-common.ps1`, a sibling `schemas/consult-reply.schema.json`; `$env:C3_EXE` a fresh
`--no-default-features` build of `target\debug\c3.exe`). Both harnesses were run **FULL** (no
`-Only`), ONE AT A TIME (`run6-engines.ps1` / `run6-muse.ps1`).

Local gates: `cargo build` (clean), `cargo test -p c3 -p c3-core --no-default-features` (**all
green**, +6 new unit tests for the engine gates, the tree-check naming, and the collab
snapshot/compare), `cargo clippy -p c3 -p c3-core --no-default-features --all-targets -- -D
warnings` (**clean**), `rustfmt` on the changed files.

## Summary table (Run 1 / Run 5 -> Run 6)

| harness | Run 1 | Run 5 (pass-1 `-Only`) | Run 6 (FULL) | ran to its own summary line? |
|---|---|---|---|---|
| harness-engines | ~18 / ~18 (crashed) | 31 / 3 | **70 / 8** | no - crashes in PANEL (`$e1.panel.id` null; panel is M4, refused) |
| harness-muse | 19 / 13 (crashed) | 30 / 3 | **63 / 11** | **yes** (first full completion) |

harness-muse ran to its own summary line for the first time (`harness-muse (...): 63 passed, 11
failure(s).`). harness-engines runs every section through SCOREBOARD and crashes only in PANEL (an
`$e1.panel.id.Substring(0,8)` on a null panel id, because `--panel` is refused as a milestone-4
feature - expected; pass 2 does not own panel).

## What this pass landed

- **Live turns per engine** - `run_primary_turn` builds `AgyEngine`/`MuseEngine`/`CodexEngine`; the
  primary turn uses the resolved parent-thread mode (RESUME resumes `--conversation`/`--session-id`).
  All engine RUN/RESUME/FAIL rows pass (usable/thread/usage/command/reviewer/harness, the failure
  classes, the malformed/no-terminal/no-result/model-drift/session-mismatch cases, a timeout kill's
  candidate-only thread, the reply kept on a failure that still produced one).
- **agy denial retry (F11)** - the gate, the `min(timeout,300)` cap, the `.denial-retry.events.jsonl`
  file, the exact retry prompt (the denied tool + permission wording + schema lines; prompt-only
  re-sends the schema), the success path (`usable reply` + the `denial notice (...)` warning) and the
  failure path (` (denial retry failed: ...)`). DENIAL rows pass except the entry field-order
  conjunct (below).
- **Read-only tree check (wave 26b D9)** - discovered this pass (the reference sweep had the older
  `Get-EngineTreeProblem`). A write-disabled engine (muse) now **WARNS** (`tree_check.outcome=warned`,
  the reply stays usable, `warnings[]` = "the working tree/collab directory changed during the run (N
  file: ...) - muse ran write-disabled, the change is not the reviewer's"); **agy** keeps the
  **failure** (forced class `permission`, "... - agy's sandbox does not block writes", the reply
  discarded). The collab-directory snapshot excludes `.consult.*` and ignores this run's own handoff
  prefix. All TREE rows pass.
- **Timeout continuation + format repair through the engine adapters** - the resume forms, native
  schema where the engine has it (prompt-only drops it and re-sends the schema in the prompt), the
  engine repair keeps its `.repair.events.jsonl`, effort `low` for muse, a fresh `--prompt-file` per
  muse turn. PROSE/REPAIR rows pass except the schema path (below).
- **Ledger/handoff/summary per engine** - `engine_run{turns, max_model_steps, msp_schema_version}`,
  the `denial_retry{}` record, agy usage mapping (`cache_read->cached_input`,
  `thinking->reasoning_output`, `total_tokens`), muse `usage: null` + the `Tokens: not reported by
  muse.` handoff line, the `Engine turns:` header line, engine warnings as `warning    :` summary
  lines and in ledger `warnings`, the `Meta Muse (muse)`/`Gemini (agy)` title + author lines, the
  `[agy]`/`[muse]` reviewer lineage, the muse `--prompt-file` prompt-via clause, and the retry_after
  parse of a quota message.
- **`--max-model-steps`** - appended after `--approval-mode never` for muse, into
  `engine_run.max_model_steps` and the header; refused for agy/codex.
- **`<engine> CLI not found on PATH`** refusal (non-dry, no launcher) and the F02-14 cmd.exe
  `%`-argument hazard (dry-run `launch      :` line + the real-run "run is refused before launch").

## Remaining differing checks, grouped

### Blocked on c3-core ledger fields the wave-26 order requires (NOT owned by this task)

`FAIL RUN ledger fields in the same order as a codex entry` (engines + muse) and
`FAIL DENIAL ... denial_retry {...} right after format_retry` (engines). The wave-26 harness
`$order` inserts three fields c3-core's `LedgerEntry` does not have: **`mode_fallback`** (after
`mode`), **`stall`** (after `timeout_continue`) and **`tree_check`** (after
`artifacts_changed_during_review`). The behaviour is correct (the evidence is `usable reply`, the
`denial_retry` record and `engine_run` are right); only the entry field-order conjunct fails because
those named fields are missing. This task populates `tree_check` through the flattened `extra` map
(so the muse TREE checks that read `$e.tree_check.outcome` pass), but that serializes at the entry's
end, not in order. **Reported to the supervisor:** c3-core `ledger.rs` (the sync worker's file)
needs named `mode_fallback`, `stall`, `tree_check` fields at those positions.

### Environmental - the documented schema-path divergence

`FAIL DRYRUN argv ...` (x3 agy), `FAIL RUN D1` (muse, `[5]=$schemaPath`), `FAIL RUN F02-14/D15`
(muse), `FAIL DRYRUN argv pinned` (muse), `FAIL REPAIR D1` (muse, the repair `--output-schema
<path>`). Each compares c3's `<CODEX_HOME>/c3/schemas/consult-reply.v1.json` against the plugin's
on-disk schema path. Same divergence codex has; the flag structure/order and the fresh per-turn
`--prompt-file` are confirmed by the sibling passing checks.

### A shim / PowerShell argument artifact

`FAIL RUN A8: the ask with ", \, %APPDATA%, a newline and non-ASCII arrives byte for byte`
(engines). Direct inspection of the delivered NDJSON stdin (a preserved sample) shows the ask
arrives and is stored **byte-for-byte** (the sibling `A16` check - the same content's length and
boundaries - passes every run); the failure is the embedded newline surviving the harness's
`Start-Process` -> shim -> native-exe argument passing on Windows, not a c3 logic bug (the same class
as the documented `\"` vs `""` argv-quoting divergences).

### Not applicable / out of scope

- `FAIL UNIT D2` (muse) - reads the C3 shim's text for `$engineSpec.Adapter` call sites; a shim has
  none (same as Run 5).
- `FAIL PANEL ...` (x1 engines, x3 muse) and `FAIL BILLING ... a panel member ...` (x2 muse) -
  `--panel` is a milestone-4 feature (refused); these are panel rows.
- `FAIL RECOVER A18` (engines) - the recovery-record note spans `codex-findings.ps1 -List` (the
  `findings_tool`, not owned here) plus the dry-run/next-run recovery lines. The engine pending
  record itself is correct (`RECOVER #1` passes). Deferred.

## Hand-off

- c3-core `LedgerEntry` needs the wave-26 named fields `mode_fallback`, `stall` and `tree_check`
  (`{outcome, files[]}`) at their order positions; once present, this task's `tree_check`-via-`extra`
  write moves to the named field and the RUN/DENIAL field-order rows pass.
- `RECOVER A18`'s note wording lives partly in `findings_tool` (not owned here).

---

# Run 7 (panel dry-run / CLI) — 2026-09-28

M4 chunk 1: the member-side path of `c3 consult` (`--panel-spec`), the panel CLI surface and its
refusals, and the panel **plan / dry-run block** (a non-dry `--panel` run refuses — the scheduler
lands in chunk 2). Same shim setup as the earlier runs (scratch `scripts/` = the five C3 shims +
the plugin's own `codex-consult-common.ps1`); `$env:C3_EXE` pointed at a freshly built
`target/debug/c3.exe`. The shim (`tests/shim/codex-consult.ps1`) gained `-PanelSize`,
`-PanelOrder`, `-PanelSeed`, `-Require`, `-Role`, `-Roles`, `-Topic` forwarding, and
`-PanelConcurrency`/`-PanelSize` now map to `Option<i64>` so a negative value reaches the
validator instead of being swallowed by a sentinel. Harnesses run one at a time.

## Summary

| harness | section filter | checks | passed | failed |
|---|---|---:|---:|---:|
| harness-panel | `-Only DRY,SPEC` | 9 | 6 | 3 |
| harness-companions | `-Only SIZE,REQUIRE,ROSTER,ROUTE,ROUTED,ROLE` | 13 | 8 | 5 |

## What passed (c3, chunk 1)

- **DRY** plan text: the header `Panel <id8> (dry run - nothing is executed or written): N of M
  roster entries would run, <planText> (roster <path>; panel id <guid>)`; the per-entry
  `  #k <lineage> - member, n=N, handoff NN` lines with the pre-assigned numbers (past an
  inactive leftover member record); the `Concurrency:` line with endpoint groups and
  `-PanelConcurrency 0 (no cap)`; the `pending     : ... the real run recovers it` line for the
  inactive leftover.
- **DRY** the plan-text variants: `-PanelConcurrency 1` → `one after another`; `-PanelSize 2` of 3
  → `at most 2 at a time`; two entries of one label → `ZAI x2 one after another`; roster
  `parallel: {"ZAI": 2}` → `ZAI x2 at once`.
- **DRY** `-PanelConcurrency` without `-Panel` refused, and a negative value refused
  (`-PanelConcurrency must be 0 (no cap) or a positive number (got -1).`).
- **SPEC** a member whose parent is dead at accept refuses after rewriting its record
  (`... is gone; this panel member was not started - nothing was started and its recovery record
  '...' was withdrawn.`); a missing record refuses (`does not exist`); a record naming another `n`
  refuses naming the mismatch (`n 7 in the record, 1 in the spec`).
- **companions**: 8 checks (`REQUIRE`/`ROUTED`/`SIZE`/`ROSTER` sub-checks that exercise the c3
  plan and refusals, and the common.ps1 unit checks that happen to be version-aligned).

## Remaining differing checks (verbatim) and diagnosis

- **FAIL DRY** `each member's own dry-run plan shows its assigned numbers ... summary "planned";
  nothing written` — the panel dry-run in the plugin launches each seated member as a child
  `-PanelSpec` process **with `dry_run=true`**, captures its single-run dry-run block, prints it,
  then a `  <lineage>   planned` summary. That launch-and-collect IS the scheduler → **chunk 2**.
  Chunk 1 emits the plan block only.
- **FAIL SPEC** `... dies during its preflight -> it stops right before launching: "... this member
  stopped before starting codex ..."` and **FAIL SPEC** `F07-1/F11-6: the parent killed between the
  member's rewrite and its parent check ...` — both need (a) the member to rewrite its reserved
  record **before** its preflight (c3 rewrites it in `run_live`, after `build_context`'s preflight,
  so the harness's kill-after-rewrite window is inverted) and (b) the `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS`
  test hook. Both are member-lifecycle-ordering concerns of the runtime → **chunk 2**. The
  pre-launch parent-alive check itself is implemented and fires; only the ordering/timing differs.
- **FAIL SIZE** `D6 end to end ... one member plan each` and **FAIL SIZE** `D6 floor end to end ...
  the member's ledger warnings[]` — the first needs the per-member dry-run recursion (chunk 2); the
  second needs a real member run's committed ledger (chunk 2). The header, the `not picked: panel
  size k` lines and the `WARNING: panel floor: ...` / `panel size reduced` console warnings the
  plan itself emits are correct.
- **FAIL ROUTE** `D6 floor ...` and **FAIL ROUTE** `D8 role files ...` — these call the plugin's
  **own** `Select-PanelRouting`, `Resolve-RoleFile` and `Select-RoleAssignment` (dot-sourced from
  `codex-consult-common.ps1`), not the c3 binary; they fail on the scratch common.ps1 version and
  the absent plugin `templates/` dir, not on c3.
- **FAIL ROSTER** `fail-closed ...` — roster-file validation lives in c3-core `roster.rs` (not owned
  by this task); its unknown-key message did not match a sub-assertion. Pre-existing, out of scope.

---

# Run 8 (panel parent scheduler — M4 chunk 2) — 2026-09-28

The parent scheduler that runs a panel end to end: `panel::run` takes the task lock, reserves every
seat's `.consult.pending-<NN>.json`, launches each seat as a child `c3 consult --task <t>
--panel-spec <b64>` (its console to `<temp>/codex-consult-panel-<id>/<NN>.out|.err`), schedules by
endpoint group (parallel across groups up to the caps, sequential within a group, `-PanelConcurrency`
caps the total, F15-3), collects each member's outcome (its last `codex-consult: ` line + its
committed ledger entry), patches every member's `panel.started`/`panel.usable`, prints the
byte-identical summary block, and returns the exit code. A dry run takes the SAME path with
`dry_run=true` members and no lock/records (the per-member dry-run recursion), marking each seat
`planned`/`refused`. Also landed: the routed-draw rating reader (`read_all_task_ratings`),
`Select-RoleAssignment` (`-Roles`, Kuhn matching), the member lifecycle reorder (record rewrite +
`CODEX_CONSULT_TEST_MEMBER_PAUSE_MS` pause + parent check BEFORE the preflight), the seeded-draw
parity with `tests/reference-draw.py` (35d4a32), and the c3-core ledger fields the wave-26b order
needs (`panel.started`/`usable` explicit `null`, `panel.roles_note`, `routing.size_asked`,
`routing.reserve`).

Shim setup as Runs 2-7 (the five C3 shims from `tests/shim/` + the plugin's own
`codex-consult-common.ps1`/`codex-consult-detached.ps1` **refreshed to 35d4a32 / wave 26b** + a
sibling `consult-reply.schema.json`); `$env:C3_EXE` a fresh `--no-default-features`
`target/debug/c3.exe`. Harnesses run ONE AT A TIME. Every harness sets `CODEX_CONSULT_HEALTH=none`
(the machine-wide health file is not in this chunk).

Local gates: `cargo build -p c3 -p c3-core --no-default-features` (clean); `cargo test -p c3 -p
c3-core --no-default-features` (all green; new tests: the `reference-draw.py` seed/uniform/draw
parity, the summary column layout against the real panel-01 rows, the guard budget, the role
assignment, `format_summary_rows`/`format_prior_counts`/`provider_slug`); `cargo clippy -p c3 -p
c3-core --no-default-features --all-targets -- -D warnings` (clean); `rustfmt` on the changed files.

## Summary (Run 7 chunk-1 -> Run 8)

| harness | section filter | Run 7 | Run 8 | ran to its own summary line? |
|---|---|---|---|---|
| harness-panel | full | 11/10 (crashed in RUN, 21 reached) | 32-33 / 14-20 (46-53 reached, runs to completion) | yes |
| harness-companions | full | 8/5 (subset) | 24 / 1 | yes |
| harness-engines | `-Only PANEL` | (crashed before PANEL) | 2 / 2 | yes |
| harness-muse | `-Only PANEL,BILLING` | (crashed before PANEL) | 9 / 2 | yes |

harness-panel is now non-crashing and runs every section; its RUN-time counts vary run to run
(one section) because the members are real child processes and several checks are timing-sensitive
(the fullest observed run reached 53 checks, 33 pass). All of UNIT, DRY, RUN, NOLOSS, SEQ pass
every run.

## What passed (the scheduler, chunk 2)

- **DRY** (all): the plan block header/member lines/routing/concurrency/timeout/pending byte-identical;
  the per-member dry-run recursion (each seat's child prints its own single-run dry-run block with
  its pre-assigned numbers, summary `planned`); the plan-text variants; the `-PanelConcurrency`
  refusals.
- **RUN** (all): three members run concurrently (last start before first finish - the ledger `when`
  is now the run START, not the commit); the panel wall clock below the sum of member walls; the
  ledger sorted by `n` in `panel.routing.picked` order; findings.json in id order; the per-member
  progress lines; the `panel{}` record (concurrency, per-label limits, same id/members in every
  entry); no recovery record left after success; `codex-findings -Rate` through the commit.
- **NOLOSS** (all): three concurrent commits on the re-read stores lose no findings
  (`Add-ReplyFindings` id-ordered insertion ported to `apply_findings_delta`); the seed finding
  carries all three reviewer checks; the commits genuinely contend (`commit_wait_ms > 0`, the
  `write lock : waited N ms` summary line, `CODEX_CONSULT_TEST_COMMIT_PAUSE_MS`).
- **SEQ**: `-PanelConcurrency 1` strictly one after another; two entries of one endpoint serialize
  while the other endpoint runs beside them.
- **companions** (24/1): DRAW (the length-prefixed seed golden, the draw sequences, the exploration
  bound - matching `reference-draw.py`), ROUTE floor, SIZE, REQUIRE, ROSTER, ROUTED.

## Remaining differing checks, grouped with diagnosis

### Environmental - the documented Windows argv `\"`-vs-`""` quoting (not a scheduler bug)

- **TIMEOUT** (4), **GUARD** (the kill leg): the fake codex's `FAKE_CODEX_HANG_ON =
  'model_provider=""ZAI""'` matches the PowerShell `""`-doubled argument; Rust's `std::process`
  escapes the embedded quotes as `\"` on Windows (the same divergence documented in Runs 3/4/6), so
  the fake never hangs, the member never times out / never outlives its guard, and it commits a
  usable reply instead. The scheduler's timeout collection, survivor handling and guard-kill
  (`kill_tree`, `CODEX_CONSULT_TEST_PANEL_GUARD_SEC`) are implemented and unit-tested; only the
  fake's trigger does not fire under c3's argv quoting.

### Environmental - the documented schema-path divergence

- **muse PANEL** (2): the panel plan and the `--max-model-steps 40` suffix are correct; the checks
  compare the whole command including `<CODEX_HOME>/c3/schemas/consult-reply.v1.json` against the
  plugin's on-disk schema path (the divergence documented in Runs 5/6).

### Out of scope for this task (findings_tool / roster.rs, c3-core)

- **INFLIGHT**: the single-run refusal now names the panel holder (`held open by pid N on HOST since
  T (review panel <short>)`, via `format_task_lock_refusal`); the check also requires
  `codex-findings -Status` to emit the same (that lives in `findings_tool`), and the member records'
  `start_time` conjunct (below).
- **companions ROUTE D8 role files**: needs the plugin's `templates/role-*.md`; C3 ships no plugin
  templates (only `<collab>/roles/*.md` resolve). Pre-existing chunk-1 limitation.

### Member-side behaviours for the detached follow-on (not the parent scheduler)

- **BLOCKED** (3): a member whose commit cannot take the write lock within the test window must keep
  its reply, mark its record `committing`, and report `commit blocked: ...` (D3) - c3 currently
  refuses with a plain "could not take the write lock" and no `committing` record. The
  commit-blocked path is unimplemented in the member commit (`orchestrate::finish`).
- **ORPHAN / MEMBERKILL** (when reached): a bridge/member killed inside its commit (between
  findings.json and sessions.json) leaves an ORPHAN finding + a `committing` record the next run
  recovers - the member-commit-interruption recovery is unimplemented.
- **SPEC** (2) / **PARENT** #1 / **INFLIGHT** #1: the member rewrites its reserved record with its
  own pid + start time before its preflight (implemented, `member_early_accept`, with the
  `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS` pause), but the record's writer-liveness during the
  pause/parent-death window is judged so that a concurrent run reads the member as gone rather than
  active (the member appears to exit before the 15 s pause window is observed by the sibling run).
  Needs a closer look at the member-record `start_time` write and `test_pending_active`'s writer
  rule for a live member whose parent has died.
- **AGY PANEL** (2): the agy member runs in a panel (engine from its roster entry) but the summary
  `[agy]` line / the "agy fails, codex still usable" outcome differ; agy-in-panel specifics
  (tree-check sibling exclusion, the forced failure) need the agy path exercised under the scheduler.

## Hand-off for the detached chunk

- **Commit-blocked + commit-interruption recovery** (member side, `orchestrate::finish` + `store`):
  the D3 `commit blocked` outcome (keep reply, record `committing`), and the ORPHAN/MEMBERKILL
  recovery of a `committing` record. Closes BLOCKED/ORPHAN/MEMBERKILL.
- **Member-record writer-liveness during the parent-death/pause window**: reconcile
  `member_early_accept`'s `start_time` write with `liveness::pending::test_pending_active` so a
  sibling run sees a paused/alive member as active. Closes SPEC/PARENT#1/INFLIGHT#1.
- **agy-in-panel**: exercise the agy member end to end under the scheduler. Closes AGY.
- **Reported to the supervisor (not owned here)**: (a) `findings_tool` `-Status`/`-Rate` must emit
  the panel-holder lock refusal and judge member records like the panel does, for INFLIGHT/TIMEOUT's
  codex-findings legs; (b) c3-core `roster.rs` must refuse a provider/model/engine containing `::`,
  `[`, `]`, `|`, `,`, `#` or edge whitespace (wave-26b string validation) - the one remaining ROSTER
  fail-closed sub-assertion; (c) the wave-26b `partial_reply`-on-any-failure rule (README ledger
  list) is a member render/ledger change, not done this chunk.
