# Changelog

All notable changes to C3 are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning](https://semver.org/). The specification of every compatibility wave is the
PowerShell plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) at its
tag `v0.6.1`; the finding ids (Fnn-k, En, Dn) are those of the parity task's reviews and of the
plugin's own waves. Each wave's details are in `docs/port/wave*.md`.

## [0.2.0] - unreleased

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

### Changed

- The workspace version is 0.2.0 (`c3 --version` prints `c3 0.2.0`); the Claude Code plugin and
  its marketplace entry carry 0.2.0.

## [0.1.0]

The parity port of the plugin's 0.5.x surface (milestones 1-6) and the subsystems after it: packs
and the `http` engine (milestone 7), the index (8), the router (9), the stdio MCP server (10) and
the index federation (11).
