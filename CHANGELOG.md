# Changelog

All notable changes to C3 are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning](https://semver.org/). The specification of every compatibility wave is the
PowerShell plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) at its
tag `v0.6.1`; the finding ids (Fnn-k, En, Dn) are those of the parity task's reviews and of the
plugin's own waves. Each wave's details are in `docs/port/wave*.md`.

## [Unreleased]

### Added

- **Wave 8 - quota windows** (`docs/port/wave8-quota.md`; the plugin's wave 30 / TECH_DEBT T15 of
  its `[Unreleased]` 0.6.2, `bade7cb..a162b8d`): after every codex run the ledger records `quota`
  `{primary, secondary, observed_at}`, `tokens` `{input, cached, output, thread_input, calls,
  max_input, context_window, subagents {count, input, calls}}`, `subagents_allowed` and `budget`
  after `commit_wait_ms`, read from the thread's rollout and its sub-agent rollouts
  (`c3_core::quota`); the endpoint health takes the newest observed window (ledgers, machine-wide
  records - which now carry `quota`, a run without an outcome record writing class `""` -, the
  codex home's newest rollout for the built-in openai endpoint): >= 98 % with its reset ahead is
  `unavailable (5-hour window 99 % used, resets <iso>)` like a usage limit, >= 80 % says `low
  quota: ...`, a past reset shows `5h reset (last 96% until 18:32)`; `c3 providers` has a QUOTA
  column and `--json` rows a `quota` object; `--budget <percent>` (env
  `CODEX_CONSULT_QUOTA_BUDGET`, default 90, 0 off) stops the main turn at the budget (partial kept,
  no continuation, class operator, telemetry class quota; detached runs and panel members the
  same); the replay guard `CODEX_CONSULT_FORK_MAX_KB` (default 512; warned from 128 KB) and the
  partial's `--mode new` hint; `-c features.multi_agent=false` on every codex turn unless the
  user's config names `features.multi_agent`; the reading discipline in the purpose paragraphs of
  acceptance, diff-review, checkpoint and core-contract; the dry run's `quota`, `budget`,
  `sub-agents` and `replay` lines; the consult shim forwards `-Budget`.

## [0.2.0] - 2026-10-14

The compatibility release: parity with claude-codex-consult 0.6.1, measured by the plugin's pinned
v0.6.1 harnesses through the shim (`docs/port/harness-shim.md`).

### Added

- Wave 1 (`wave1-parity.md`): the 0.6.1 rules of wave 28 - the time-only reset parse, the Codex
  app-server exclusion, the panel light, the re-read anchor.
- Wave 1b (`wave1b-plan.md`): the roster `plan` - quota propagation, the scheduling group and the
  machine-wide plan wait as shared code (F02-7).
- Wave 1c: F04-1..F04-3 - a Unicode-safe `.exe` strip, DST edges by a UTC round trip, the panel
  records from the final routing.
- Wave 2 (`wave2-telemetry.md`): 2a `consult_ref`, the rating judge and `rating_rev`, the backfill,
  the ledger order; 2b a durable telemetry outbox and the forget-me retry identity (F02-1, F02-3);
  2c the closed telemetry classes and the telemetry shim (F02-2, F02-4); C3 keeps its own app id,
  salt and outbox (decision P7).
- Wave 2b (`wave2b-compat.md`): the pre-0.6 gaps of the RC2 triage - the test-mode warning and
  the child scrub, the launcher quoting, the health journal, kick during a timeout continuation,
  the pending outcome, the store rename retry, the member step cap, the launch-mark hooks, STREAM
  D6, the hook's pointer line and `--explain`, `--require` on a single run, the role and topic
  validation, the shim fixes.
- Wave 3a (`wave3a-recovery.md`): the recovery hardening of the plugin's wave 28e (E1, E18, E19,
  E23, E25, E28) and F06-1.
- Wave 3b (`wave3b-notspooled.md`): one not-spooled file per producer, the fold that saves its
  record before it deletes, the legacy staging, the forgetting marker on pid and start ticks (E2,
  E3, E20, E24, E26; decision P8, the test-mode plugin-home switch).
- Wave 4 (`wave4-claude.md`): the `claude` engine - roster keys, the model table, the preflight,
  the turn, the panel seat, the endpoint mode; the not-spooled warning says "- dropped".
- Wave 5 (`wave5-leftovers.md`): `--brief-prefix` (`CODEX_CONSULT_BRIEF_PREFIX`, the plugin's
  wave 27c D6) with the reply-prefix and slug refusals and the dry run's `brief prefix:` line; the
  telemetry sender's intake handling - every batch of at most 100, the 429 `Retry-After` rule, the
  400 `events[i]` drop, the 413 halving, the 403 stop, the 60 s flush deadline and the 8 s request
  bound, the record's `rejected[]` and the plugin's result line; the sender lock's owner record
  (`flush.owner.json`) behind "sender busy" / "sender stuck" in the refusal, the notes and
  `--status`.

### Fixed

- Wave 2d: F09-1..F09-6 - the deletion transaction, the reread rule, closed legacy events, one
  coordinator resolver.
- Wave 2e: F11-1..F11-5 (the wave 2b review).
- Wave 2f: F14-1 - the local-only forget records its cleaning transaction before the first
  removal (F14-2 stays as the plugin: the provider match is case-sensitive).
- Wave 2g: F19-1..F19-3 - the complaint joins the deletion protocol, an unreadable salt never ends
  a cleanup, every outbound event is re-serialised.
- Wave 2h: F22-1 - a kept health journal lists its applied record keys, so a replay after an
  eviction never applies a record twice.
- Wave 3c: F23-1..F23-5 - the recovery invariant fail-closed (a failed lookup is never "gone", an
  unreadable start time never skips a scan row, the kept evidence is written at the kill).
- Wave 3d: F24-1..F24-6 (the wave 3b review).
- Wave 4f: F25-1..F25-4 - the endpoint launcher probe, the transcript guard in every auth mode, the
  plugin's model case rule, a redacted child-environment debug print.
- Wave 5: `CODEX_CONSULT_COORDINATOR` is refused with the plugin's character rule
  (`Get-IdentityStringProblem`: "the provider 'open::ai' must not contain '::'") instead of a
  provider-label pattern the plugin does not have; interior blanks are accepted as the plugin does.
  The coordinator's dry-run line and the "not in the roster" console line follow the plugin's
  `Format-CoordinatorText` / `Format-CoordinatorId` (no ` [codex]` suffix, "(model not named)", an
  unresolved `#n` shown as such).
- Wave 6 (`wave6-live-diffs.md`, the RC3 live matrix): the ledger's `coordinator` record carries
  `host_by`, `in_roster` and `unresolved` in the plugin's order, and a rewrite of `sessions.json`
  keeps every entry it read verbatim (key order, explicit nulls) apart from what C3 changed in it.
- Wave 6: the `c3 providers` header counts this repository's consultations only, not the
  machine-wide health records.
- Wave 6: the agy and muse engines' reviewer line (dry run, reply header) is `engine <name>
  (<launcher>)` as the plugin's, not the codex endpoint text.
- Wave 6: telemetry is sent by a detached `c3 telemetry --flush` started right after a
  consultation's, a panel's, a rating's or a backfill's commit (not waited for, allow-listed
  environment), not at the start of the next run.
- Wave 7 (`wave7-acceptance-fixes.md`), acceptance round 1 fixes: F36-1, F36-2, F30-2 failure
  path, F29-1/F32-2 documented. F36-1 - a repository-relative path strips the root char by char
  (a Unicode `CLAUDE_CONFIG_DIR` outside the repository no longer panics the Claude transcript
  guard; Windows folds every letter as `OrdinalIgnoreCase`). F36-2 - the guard compares the NFC
  form Claude Code writes under as well as the path as written, the root likewise; any form inside
  refuses. F30-2 - the write before a kill reports its result to the killer; a failed one is
  retried once into `.consult.pending-kill.json`, which the recovery reads as a record; both
  failing, the kill still runs, not confirmed, and the next run is refused from the launch record
  and its fail-closed scans. F29-1 (the shared journal with plugin 0.6.1) and F25-1/F32-1/F32-2
  (what the endpoint preflight and the transcript guard cannot see) are stated as known limits.
- Wave 7b (`wave7-acceptance-fixes.md`, "7b"), acceptance round 2 (F38-1): no kill without a
  record - the record before a kill goes to the recovery record, else its fallback, else a third
  location outside the collab directory, `<codex home>/c3/pending-kill/<task>-<run id>.json`, which
  the recovery reads and consumes as the fallback (and the end of a run that writes the record
  clean removes); all three failing, the tree is left alive, the run ends failed naming its root
  and the three errors, and the next run is refused from the launch record while it lives.
- Wave 7c (`wave7-acceptance-fixes.md`, "7c"), acceptance round 3 (F40-1, F40-2): the codex home's
  kill records that cannot be listed (the directory cannot be opened, an entry cannot be read) are
  an inspection failure, never "no records" - the consult and panel recovery, the dry run, the
  findings refusal and `-List` and the kick refuse naming the directory and the error (only an
  absent directory is empty); a kick whose kill did not happen (its record written nowhere) is
  acknowledged `not killed` - the kick names the live root and why, says the tree is left alive and
  the task refused, and exits 1.
- Wave 7d (`wave7-acceptance-fixes.md`, "7d"), acceptance round 4 (F42-1) and a sweep of the
  family: a recovery record - the record, its fallback, a codex home copy, the running record -
  whose metadata lookup, read or parse fails is an inspection failure, never an absent record (only
  `NotFound` is): every reader refuses naming the file and the error (`the recovery record <path>
  cannot be read (<error>): the task is refused until it can be read`; `-List` prints it as
  `pending:`, the kick exits 1); the task directory's own listing that fails refuses the same way;
  the record before a kill never replaces a record that cannot be read.

### Changed

- The workspace version is 0.2.0 (`c3 --version` prints `c3 0.2.0`); the Claude Code plugin and
  its marketplace entry carry 0.2.0.

## [0.1.0]

The parity port of the plugin's 0.5.x surface (milestones 1-6) and the subsystems after it: packs
and the `http` engine (milestone 7), the index (8), the router (9), the stdio MCP server (10) and
the index federation (11).
