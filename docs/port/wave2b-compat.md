# Wave 2b of the compatibility track: the pre-0.6 parity gaps (2026-10-09)

Branch `wave2b-compat`. The specification is the plugin at `v0.6.1`; the gaps are the RC2 triage's
priorities 1-5 (`rc2-triage-2026-10-08.md`; its priority 2 - the ledger fields - went with wave 2a)
plus the launcher quoting left open by wave 2 (`wave2-telemetry.md`, "Not done") and the shim fixes
(`harness-shim.md`, "The shim fixes (wave 2b)"). Not here: the claude engine's wording (wave 4) and
the Z Code host checks (not applicable: C3 is Claude Code only - `harness-shim.md`).

## 1. Test mode said, and never handed to an engine child (fixes28b TESTMODE D10)

- `c3_core::test_hooks::TEST_MODE_WARNING` = `test mode is ON: test hooks are honoured` (the
  plugin's `$script:TestModeWarning`). With `CODEX_CONSULT_TEST_MODE` on, `build_context` adds it to
  the run's warnings right after the ignored-hooks warning (the plugin's order): ONCE in the ledger's
  `warnings[]` (a panel member's too - the member is the bridge), on the console of a real run
  (`WARNING: ...` with the run's output) and of a dry run (with its warnings), in the panel header
  (after the coordinator warnings), and on the `-Detach` foreground as the LINE AFTER the three
  detach lines (the first line stays `Detached <id8>: ...`). The single-run foreground now prints the
  run's warnings, the preflight's and the peak window's before its lines (`$detachWarnings`; it
  printed the preflight warning only). A refusal prints no such line.
- (found by engines RUN) the ledger's `warnings[]` and the handoff's `Warnings:` line hold the run's
  warnings for EVERY engine, then an agy/muse turn's own warnings (`foreach ($rw in $runWarnings) {
  $engineWarnings.Add($rw) }`); an agy or muse entry recorded its turn warnings only, so it lacked
  the test-mode line, a roster-ambiguity or coordinator warning.
- `engines::scrub_host_markers` also removes `CODEX_CONSULT_TEST_MODE` and every
  `CODEX_CONSULT_TEST_*` (`Hide-HostMarkers -TestVars`, `Remove-HostMarkersFromStartInfo`) from every
  engine child (codex/agy/muse, every turn) and every launcher probe (`--version`, `login status`,
  `models`). They are not listed in `child_env_scrubbed`. The panel member re-exec and the detached
  background keep them.

## 2. The launcher quoting (fixes28b CONTEXT, engines RUN argv)

Rust std quotes every argument of a `.cmd`/`.bat` launcher that holds a character outside
`[A-Za-z0-9#$*+-./:?@\_]` - so `-c "model_context_window=256000"` and `"-p="` - while the plugin's
`Start-Process -ArgumentList` (`ConvertTo-ProcArg`) passes a bare token as it is. The shared spawn
path (`engines::subprocess::apply_launcher_args`, every engine turn) now hands a batch launcher:

- a token of the plugin's bare set (`^[A-Za-z0-9_.\-:\\/=]+$`, or `-` alone) through
  `CommandExt::raw_arg`, unquoted;
- everything else through std's batch escaping: quoted, every `"` doubled - the plugin's form
  (`"foo=""a b"""`) - plus std's guards the plugin lacks (`%` cannot expand a variable, a line break
  is refused, a backslash before a quote is doubled).

An `.exe` launcher is unchanged. Tests: `batch_args_keep_plain_arguments_plain`,
`a_batch_launcher_sees_the_plugin_argv` (a real `.cmd` echoing `%*`: `-c
model_context_window=256000 -p= "a b" "k=""v w"""`), the end-to-end
`a_batch_launcher_gets_plain_arguments_as_they_are`, and `telemetry_parity`'s CONTEXT check no
longer strips the quotes.

## 3. The machine-wide health journal (fixes27c HEALTH D7/D8, fixes28b HEALTH D13, fixes28c JOURNAL D10, fixes26b HEALTHLOCK)

`c3-core/src/health.rs` ports `Add-MachineHealthJournal`, `Get-MachineHealthRecordKey`,
`Get-MachineHealthJournalNotes` and the journal half of `Update-MachineHealth`; the plugin and C3
share the health file AND its journal:

- `<health file>.journal`: append-only NDJSON, one endpoint record per line (the plugin's
  `ConvertTo-Json -Compress` shape - C3 reads the plugin's lines and the plugin C3's). Appended with
  an exclusive open retried for 2 s; a missing directory is named (`the directory X does not exist`).
- EVERY update (a registration, an outcome, the retry, an unregistration) first applies the journal
  under the health lock, then writes the file, then empties the journal by exactly the prefix that
  was applied or moved. Idempotent: the record key is endpoint, class, kind, repository and the
  INSTANT of `when` (+ a quota mark's `until`), so the same record from the journal and directly is
  one record.
- A line that does not parse (a torn append) is moved to `<journal>.bad` as `<local time><TAB><the
  exact bytes>` and counted: `health journal: <n> unreadable line(s) kept in <journal>.bad`. When the
  `.bad` cannot be written the line and everything after it STAY in the journal (`... could not be
  moved to <bad> - kept in <journal>`).
- The run (`orchestrate`): the record is built ONCE (`new_machine_health_record`, its `when` fixed)
  and written BEFORE the write lock with the full budget (3 x 5 s; it was one bounded attempt inside
  the lock). Its failure (any cause) puts the record into the journal INSIDE the write lock (a local
  append) and the entry's warning says so; the full retry runs after the lock is released and its
  outcome is a summary line (the console and a detached run's status record):

| moment | text |
|---|---|
| `warnings[]` at the commit | `machine-wide health not updated at the commit (<cause>); the record is kept in the journal; a retry follows the commit` |
| ... the journal not written | `machine-wide health not updated at the commit (<cause>); the journal could not be written (<why>); a retry follows the commit` |
| summary, the retry worked | `health     : machine-wide health updated by the retry after the commit (the journal applied)` |
| summary, it failed | `warning    : machine-wide health not updated by the retry after the commit (<cause>) - the record waits in <journal> for the next run` (no tail when the journal was not written) |
| `warnings[]` + summary | `health journal: <n> unreadable line(s) kept in <journal>.bad` (the updates before the commit); after it: a `warning    :` line |

- The test hook `CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST=1` (test mode only): the process's first
  update that adds an endpoint record fails with `lock timeout (test hook
  CODEX_CONSULT_TEST_HEALTH_FAIL_FIRST)`.
- A write failure is now `write failed: <why>` (the plugin's text; was `the health file could not be
  written`).

## 4. The small compat bugs

- **KICK D2** - a kick during the timeout continuation was never delivered: only the format repair
  watched the kick file. The continuation watches it too; found there it cancels the continuation,
  the run keeps the main turn's timeout outcome and its salvage, no `operator` failure, and
  `warnings[]` says `kick: the operator stopped the timeout continuation (-Kick); the timeout outcome
  and its salvage stay` (the handler existed).
- **pending (e)** - the `failed: could not register the codex process (<why>); codex was stopped`
  outcome was overwritten by the later `failed: codex exit 1` framing of the stopped child; the
  framing now skips a registration failure.
- **muse TREE** - `store::write_text_atomic` retries the rename 8 times 250 ms apart (the plugin's
  `Write-TextAtomic`): a reader holding the destination open without `FILE_SHARE_DELETE` (a harness
  polling the recovery record) made the registration fail with `os error 5`.
- **muse PANEL** - a panel passes `-MaxModelSteps` to every member; a member whose engine has no step
  cap (codex) now drops it (`if ($panelMember) { $MaxModelSteps = 0 }`) instead of refusing; a single
  codex run still refuses it.
- **panel SPEC D1** - the hooks `CODEX_CONSULT_TEST_MEMBER_LAUNCH_MARK=<file>` (written with the
  member's pid when it reaches its launch-time parent check) and
  `CODEX_CONSULT_TEST_MEMBER_LAUNCH_PAUSE_MS=<ms>` (a pause there), test mode only.
- **STREAM D6** (wave 28b D12) - the stall cut names the open tool call: `failed: stalled after N s
  without an event - no output for S s (a tool call open for M s: codex command_execution item_9)
  (process tree killed)`. The tool-call tracker is a set by key with labels (`Update-ToolFlight`:
  codex `codex <type> <id>`, closed by id else the first open call of its type; muse `muse <kind>
  <task id>`, opened by `proposed`, closed by `completed|failed|cancelled|canceled|rejected` whatever
  the kind field) - it was a +1/-1 count that a muse `proposed` + `started` pair left at 1. The
  suspension bound is the plugin's 2 x stall without growth, no 1800 s floor (the test hook
  `CODEX_CONSULT_TEST_TOOL_CAP_SEC` unchanged).
- **POINTER D13 / host EXPLAIN, HOOK** - `c3 hook` prints a SECOND line, always:
  `codex-consult: coordinator rules - skill codex-consult:coordinate (or <command>); telemetry:
  on|off` (`<command>`: `--explain-command`, else `"<this c3>" consult --explain coordinate`; C3's
  plugin hook wrappers print it too). `c3 consult --explain coordinate|consult|providers` prints the
  plugin's skill (`<plugin root>/skills/<coordinate|consult-codex|setup-providers>/SKILL.md`, C3's
  own `consult` for `consult-codex`) without its front matter after the head line `codex-consult
  -Explain <name>: the <skill> skill, <path> - ${CLAUDE_PLUGIN_ROOT} in it is replaced by the plugin
  directory <root>`, every `${CLAUDE_PLUGIN_ROOT}` replaced; refusals `-Explain takes coordinate,
  consult or providers (got '<x>').` and `-Explain takes no other parameter (got -Task).`.
  `--task` is no longer required by clap: every other form without it is refused with the plugin's
  `-Task <id> is required (a slug: the task directory <CollabDir>/<id>/); the one form without it is
  -Explain coordinate|consult|providers.` (exit 1, before anything else).

## 5. `--require` on a single run, the role and topic slugs (companions REQUIRE, ROLE D8, ROUTED D8)

- `--require` without `--panel` and without `--provider` (a roster walk) is refused: `-Require goes
  with -Panel, or with -Provider (a single run of a chosen reviewer); a roster walk takes whichever
  reviewer is available.` (exit 1). With `--provider` every required reviewer is judged by the
  roster walk's verdict (`panel_members` over the required entries, `-PanelAll` semantics, the
  consult clock): one that is out refuses before anything starts, exit 5 - `required reviewer[s] not
  available (-Require, judged like the roster walk): #<n> <lineage> (<reason>[; back <local>,
  <relative>]); nothing was started - wait for it|them, or run without -Require (exit 5).`; no
  roster, `none` combined with a reviewer, a matcher that names no entry: the plugin's exit-1 texts.
  The dry run says `required    : #<n> available (-Require)`. `Format-RequiredOutage`'s `; back
  <local>, <relative>` is now in the panel's outage text too (`PanelMemberRow.until`).
- `--topic`, `--role`, `--roles` are lower-cased and slug-checked in `args::validate`
  (`ConvertTo-SlugList`) and carried on canonical: `-Topic 'a/b' is not a slug (...).`, `-Role: role
  '../brief' is not a slug (...).`, `-Role takes one role (...)`, `-Roles: ...`, `-Role and -Roles
  exclude each other: ...`.
- The role file (`<collab>/roles/<name>.md`, else `<plugin root>/templates/role-<name>.md`) is
  resolved for a single run as for a member: ANY error refuses (`-Role: unknown role 'nope' (...).`;
  a member adds `; this panel member was not started`) - when the plugin root is known. Without one
  (no `CLAUDE_PLUGIN_ROOT`, no scripts dir, no `plugin/templates` beside the binary - the new
  fallback finds C3's own `plugin/` in its repository layout) only a safety problem refuses, as
  before. The ledger's `role` is a single run's role too (it was a member's only); the dry run says
  `topics      : a, b` and `role        : <name> (<source>: <path>) - in the prompt after the ask`.

## 6. The shim fixes

`harness-shim.md` "The shim fixes (wave 2b)": `-Task` optional, `-Explain` forwarded, the plugin
root (`CLAUDE_PLUGIN_ROOT`) and the schema path (`C3_SCHEMA_FILE`, honoured by c3 only when the file
holds the embedded schema) set by the consult shim, `-CodexConfig` whole, `-By` in the scoreboard
shim, the hook's pointer line; the staging recipe stages the whole plugin tree. Found by engines RUN
A8 and the hook's pointer line: Windows PowerShell 5.1 hands a native command an argument with an
embedded `"` unescaped (it only wraps one with whitespace in quotes), so `a "b" c` reached c3 as
`a b c`; the consult and hook shims pre-escape every argument by the C runtime's rules on a legacy
host (5.1, pwsh before 7.3, or `$PSNativeCommandArgumentPassing` Legacy) - `Get-NativeArgs`.

## Tests

Rust: `crates/c3-cli/tests/compat_wave2b.rs` (the real binary against a fake codex `.cmd`: TESTMODE
console/warnings/dry run/env dumps of the exec turn and the probes; the batch-launcher argv; the
journal at a failed commit, an orphan replayed with a torn line moved aside, a missing directory;
the registration outcome; a kick during the continuation; the stall naming the tool call; REQUIRE on
a single run; ROLE/TOPIC; EXPLAIN and `--task`; the hook's two lines), unit tests in
`engines::subprocess` (batch args, a real `.cmd` echo, the tool-call tracker, the 2 x stall bound),
`c3_core::health` (the journal applied once, torn lines and an unwritable `.bad`, the record key),
`c3_core::store` (the rename waits for a reader), `c3_core::test_hooks`, `consult::explain`,
`hook`, `orchestrate` (a codex member drops the step cap).

## Harnesses through the shim (pinned v0.6.1, 2026-10-09)

Staging as in `harness-shim.md` section 4 (wave 2b: the whole plugin tree of the tag, the six
shims over its scripts), `C3_EXE` a copy of this branch's build, one harness at a time under
`HARNESS.lock`, host **Windows PowerShell 5.1**. "Before" is the brief's baseline (RC2, or wave 2's
rerun where it ran).

| harness | before | after | went green |
|---|---|---|---|
| fixes28b | 13 / 7 | 20 / 0 | TESTMODE x2, HEALTH D13 x3, CONTEXT x2 (the launcher quoting), DOCS (the plugin tree) |
| fixes28c | 13 / 2 | 14 / 1 | JOURNAL D10 |
| fixes27c | 25 / 11 | 34 / 2 | STREAM D6, HEALTH D7/D8, POINTER D13, KICK D2, DOCS D17/D18/D23/D24 (the plugin tree) |
| fixes26b | 49 / 2 | 51 / 0 | HEALTHLOCK 26c D2, ROLEFILE D1 (the plugin's templates) |
| pending | 25 / 1 | 26 / 0 | (e) the registration outcome |
| detach | 49 / 2 | 50 / 1 | SINGLE (the test-mode line 4) |
| panel | 60 / 2 | 62 / 0 | SPEC D1 and F07-1/F11-6 (the launch-mark hooks) |
| companions | 35 / 7 | 42 / 0 | REQUIRE x2, ROLE D8, ROUTED D8, ROUTE D8, SIZE D6, RATE `-By topic` |
| engines | 87 / 10 | 95 / 2 | ROSTER F10-3, DRYRUN x4 (warnings[]; argv, -NativeEffort, -Mode resume: the schema path), RUN thread/usage (warnings[] of an agy run), RUN argv (`-p=`), RUN A8 (the shim's 5.1 argument escaping) |
| muse | 62 / 12 | 69 / 5 | DRYRUN argv, RUN D1/D2/F02-14, REPAIR D1 (the schema path; warnings[] of a muse run), TREE (the rename retry), PANEL dry run and real panel (-MaxModelSteps on a codex member) |
| 0.3 | 227 / 2 | 229 / 0 | LEDGER reviewer fields (the test-mode line), CFG (`-CodexConfig` whole) |
| format | 37 / 0 | 37 / 0 | no regression |
| roster | 124 / 1 | 124 / 1 | no regression |
| host | hung (RC2) | 55 / 10, 72 s | finishes (optional `-Task`); TESTLINE D12, HOOK D5, EXPLAIN x3, WARN D3 "no marker", the DOCS/SKILL/README/AGENTS rows (the plugin tree), GREP D6 (the shims' wording) |

### Every remaining failure, classified

- **Wave 4 (the claude engine's wording and roster keys):** engines ROSTER "refused: unknown,
  nomodel, cfg, auth, twoengines" and DRYRUN "refused, one message each ... -SchemaTransport native"
  (the engine list "codex, agy, muse, claude", "agy, muse and claude engines"); muse ROSTER, DRYRUN
  (steps with agy/codex: "... muse --max-model-steps, claude --max-turns"), ENGINEEXE (two engines),
  PANEL (-MaxModelSteps with no muse "(or claude)" member); roster FILE (`claudeauth`,
  `claudemodel`).
- **Wave 3 (recovery):** fixes28c IDENTITY D8 (the unverified-descendant kill text; the check reads
  `Get-KillMayRunPids` from the script source - a shim artifact over a real wave-3 gap).
- **Shim artifacts (by design):** fixes27c POINTER D15 (a regex over the script source: `$explainOut`
  flushed and disposed - C3 writes the bytes and flushes; the shim has no such stream), muse UNIT D2
  (a regex over codex-consult.ps1 for the engine adapters), detach CARRY F11-2 (the background's args
  are C3 JSON, not CLIXML).
- **Not applicable (C3 is Claude Code only):** fixes27c ZCODE D20/D21 (`harness-shim.md`).
- **Still open - pre-0.6 gaps outside this wave:** host REFUSE D3 (an unparseable
  `CODEX_CONSULT_COORDINATOR` is refused, but with C3's own wording - `the provider 'open::ai' is not
  a provider label (...)` - where the plugin says `must not contain '::'`); host WARN D3 real run and
  the `engine codex` half of WARN D3/F04-10 (wave 27c D9: a coordinator given as a label resolves
  its model from the config - `gpt-5.1` - and its engine; C3 records null); host PREFIX D6 x2 (the
  brief prefix of wave 27 R13 D6 - `-BriefPrefix`, `CODEX_CONSULT_BRIEF_PREFIX`, the `brief prefix:`
  dry-run line, the reply-prefix refusal - is not ported; the shim has no `-BriefPrefix` either).
- **Harness environment:** none left (roster TIMEONLY was pwsh 7 only; this run is 5.1).

Totals: the 13 harnesses with a baseline 806 / 59 -> 853 / 12; harness-host from hung to 55 / 10
(72 s).
