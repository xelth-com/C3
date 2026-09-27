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
