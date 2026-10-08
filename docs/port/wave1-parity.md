# Wave 1 of the 0.6.1 parity (2026-10-08)

The plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) moved from
the 0.5 era C3 follows (Run 17 of `harness-results.md`) to **v0.6.1**. This wave ports four
self-contained behaviours from `CHANGELOG.md` `## [0.6.1]` and `## [0.6.0]` and the plugin's
`plugins/codex-consult/scripts/codex-consult-common.ps1` / `codex-consult.ps1` at the tag, with
unit tests lifted from the plugin's own harness samples, and measures them through the shims
(`harness-shim.md` section 4).

## A. A reset time without a date (0.6.1 "Fixed": `Get-RetryAfter` wording 1b)

Module `crates/c3-core/src/health.rs` (`Get-RetryAfter`, `Select-TimeOnlyReset`,
`Get-WallClockCandidates`, `Get-TimeOnlyZone`).

- **The wording.** `try again at 9:43 PM.` / `try again at 21:43`, also after `resets at`,
  `available at`, `until`: tried after the month-name and the ISO forms, before the durations.
  An AM/PM hour outside 1..12, an hour over 23, minutes or seconds over 59: not a clock time
  (the next form is tried; none matches -> no reset time, the default hold).
- **Which day.** The clock time on the day before, the day of and the day after the reference's
  date (in the reference zone; with a qualifier, in that offset); the EARLIEST candidate not
  before (reference - 5 min) wins (`TIME_ONLY_LATE_MINUTES`, F06-3): today unless today's is more
  than 5 minutes past, then tomorrow; a reset that has just passed stays passed (the hold ends at
  once); just after midnight yesterday's when it passed within the 5 minutes.
- **Daylight saving (F06-4).** A wall time in the repeated hour of a fall-back night has BOTH
  instants as candidates (earliest first); a wall time inside the spring-forward gap means the
  first valid minute after the gap, at the offset after the transition.
- **The round trip (wave 1c, F04-2).** Which offsets a wall time has in the zone is never taken
  from the zone's own classification alone: `ResetZone::wall_offsets` keeps an offset `o` only
  when the zone's offset at the instant `wall - o` is `o` again (UTC-to-local round trip; the
  candidates are the reported offsets and the zone's offsets a day before and after). Two
  genuine offsets = the repeated hour, one = a plain time, none = a gap. chrono's Windows
  `Local` (0.4.45) reports the exact END of a repeated hour (03:00 on the Berlin fall-back day)
  as ambiguous and the exact START of a gap (02:00 on the spring-forward day) as a single time
  at the earlier offset; .NET and the zone database do not. Without the round trip `try again
  at 3:00 AM` at `2026-10-25T02:00:00+01:00` picked 03:00+02:00 (= the reference, already
  reached) instead of 03:00+01:00, and the dated spring 02:00 read +01:00 instead of the
  plugin's +02:00.
- **The qualifier (F06-5, F08-2),** read as a WHOLE token after the clock (spaces or an opening
  parenthesis between; up to a space, `,` `;` a bracket, a quote, `!` `?` `|` `\`; a
  sentence-ending period stripped): `UTC` / `GMT` (any case) / `Z` = UTC; a COMPLETE numeric
  offset `+H`, `+HH`, `+H:MM`, `+HH:MM`, `+HHMM` (also after `UTC`/`GMT`, also spaced:
  `UTC +02:00`, `(GMT+2)`) = that offset, shown in the zone. A token that starts a qualifier but is
  not complete (`UTC+05:3`, `+02:000`, `UTC+oops`, `UTC+`, `Z+02`, `+020`, `+15:00`, `+02:60`), a
  lone sign followed by a number (`21:43 + 2`), or another zone-like word (two to five capitals
  other than AND / OR: PST, CET, BST) = the wording does NOT parse. A trailing comma, a sentence
  period (`9:43 PM.` - the period of `p.m.` is the abbreviation's), `and`, a dash between words
  or the end of the message = no qualifier (the zone's local time).
- **The two modes.** `retry_after_ref` (read time, `-ReferenceOffset`: the reference's offset
  and day) and the new `retry_after_in(message, reference, zone)` (write time: a `ResetZone`,
  every chrono `TimeZone` - `chrono::Local` in production). The write time now also gives the
  dated forms the plugin's F15-1 zone rules (a nonexistent wall time takes the offset after the
  transition, an ambiguous one the offset before it; ISO instants and durations are shown in the
  zone at that instant) - before this wave C3 read every reset time in the reference's fixed
  offset at write time too. The month-name regex gained the plugin's `\b` after the day and the
  year (`until jun 21:43` is no "Jun 2 1:43").
- **The moment of parsing** (`New-ProviderFailure`, 0.6.1): the consult clock -
  `CODEX_CONSULT_NOW` when set (test hook), else the system clock
  (`orchestrate::parse_reference`, both the codex and the engine failure paths); `when` stays
  the system's.
- **`codex-providers` LAST FAILURE.** The table already printed `quota until <iso>: <when> -
  <message>` when a quota failure carries a `retry_after` (both scripts' format strings match);
  what printed a bare `quota: ...` was a time-only wording that parsed to no reset time. With the
  parse ported, such a failure records `retry_after` and the line reads `quota until <iso>: ...`.

Tests: `health::retry_after_tests` - EVERY sample of `harness-roster.ps1`'s UNIT
`Get-RetryAfter` check (all 69, generated from the v0.6.1 harness: the dated, ISO, duration and
DST cases, the time-only ones, F06-3/F06-4, the F06-5 and F08-2 qualifier cases) with the same
expected ISO instants, the Berlin cases in `Europe/Berlin` by id (`chrono-tz`, a dev-dependency
only); the TIMEONLY section's three recorded resets; qualifier-token cases; the `\b` regression.
Wave 1c: six transition-edge cases (`BOUNDARIES`, the expected instants from the plugin's own
`Get-RetryAfter` with `W. Europe Standard Time`) in three zone implementations -
`reset_boundaries_follow_the_plugin_in_every_zone_implementation` (`Europe/Berlin` and a
synthetic zone with chrono's Windows boundary classification, so every machine proves the
round trip), `wall_offsets_are_normalised_against_a_utc_round_trip`, and (RC2)
`windows_local_reset_boundaries` with production `chrono::Local` (`#[cfg(windows)]`; it skips
itself with a printed note unless the machine zone has W. Europe Standard Time's 2026 offsets
and transition instants).

## B. The Codex desktop app's processes are no reviewer run (0.6.0, E27 / E28)

Module `crates/c3/src/liveness/proc.rs` (`Get-CodexRule`, `Get-CodexMatch`,
`Get-CodexServerExclusion`, `Split-CommandLineTokens`, `Test-ExecWord`, `Get-CommandLineGap`).

- **Excluded** (E27): a codex-named process (name without `.exe` starting with `codex`) whose
  first non-option argument (a global option's value such as `-c key=value` skipped with it) is
  `app-server`, `exec-server`, `mcp-server`, `login` or `app` ("codex app-server"), a
  `codex-computer-use*` executable ("codex-computer-use-swift helper", also with an unreadable
  command line), or `--parent-pid` without exec ("codex helper (--parent-pid, no exec)").
- **Never excluded** (E28 / F37-1): a command line holding the WORD exec anywhere - on the raw
  text and on every argument as the program sees it (a hyphen belongs to the word:
  `exec-server` and `--exec` are not exec; `e"x"ec` is); an unreadable command line (empty or
  ps's `[name]`: fail-closed, "name codex"); an ambiguous one (unbalanced quoting: "command line
  ambiguous - counted as codex"). The command line is split with the Windows rules of the Rust
  std / MSVC runtime (program name: quotes toggle; arguments: `2n` backslashes + quote -> `n`
  backslashes and a toggle, `2n+1` -> a literal quote, `""` inside quotes a literal quote).
- **Process names are Unicode (wave 1c, F04-1).** The `.exe` suffix is cut only through a
  boundary-safe check (`str::get` on the last four bytes, ASCII-case-insensitive compare): a
  name whose last four bytes start inside a character (a Linux `/proc/<pid>/comm` such as
  `日本`) stays whole - never a slice panic that aborts the machine-wide recovery scan.
- **The scan** (`find_codex_processes`, the name branch) reads the command line of every recent
  candidate now - a codex-named one too - and names what it left out in its check text:
  `...; not the Codex app's servers and helpers; started at or after <t>; excluded: pid N
  codex.exe [codex app-server], ...)` (at most 6, then `(+k more)`); `ScanOutcome.excluded`
  lists them. The test hook `CODEX_CONSULT_TEST_CMDLINE_UNREADABLE=<pid>[,<pid>]` (test mode
  only) reads those pids with an empty command line, in the scan and in the bare-pid check.

Tests: `liveness::proc::tests` - all 17 E27 and 9 E28 samples of `harness-fixes.ps1` (the plugin
keeps them there; `harness-fixes28e.ps1` has none), the two `Split-CommandLineTokens` checks,
`Test-ExecWord`, `Get-CommandLineGap`, and the scan's exclusion text. Wave 1c (RC1):
`strip_exe_is_boundary_safe`, `unicode_name_codex_match_completes_without_a_match`
(`codex_match("日本", "unrelated", "")`) and
`unicode_name_scan_completes_without_a_match_or_an_exclusion` (a recent unrelated `日本` process
in both scan flavours).

## C. Roster `panel: light` (0.6.0, 2026-10-07)

Modules `crates/c3-core/src/roster.rs` (validator), `crates/c3/src/providers.rs`
(`Select-PanelMembers`), `crates/c3/src/panel/{run,plan}.rs`.

- The roster validator accepts `"light"`: `entry N: panel must be "always", "weighty" or
  "light" (got ...)`.
- A light entry joins a panel on the light purposes (checkpoint, diff-review, chore, none). On a
  weighty purpose (framing, decision, core-contract, acceptance, stuck) without `-PanelAll` it is
  held back AFTER every other check - `light reviewer; purpose <p> is weighty - it stands in only
  when no entry of its label runs` (skip kind `light`) - and after the whole roster is judged it
  STANDS IN when no other entry of its provider label runs: `stands in for #<p> (<that sibling's
  skip reason>)`, `stands in (no other entry of label <label>)` or `stands in (no other entry of
  label <label> runs)`; in roster order, so a second light entry of the label sees the first one
  run. The plan line of a stand-in reads `member, n=1, handoff 01, stands in for #1 (...)`; the
  ledger's `panel.members` records it `run` with that reason.
- `-Require #p` lifts the light gate like the weighty one (the entry takes a seat first; it is
  no outage); `-PanelAll` takes every entry; the single-reviewer walk ignores the weight.
- **What is recorded is the FINAL routing (wave 1c, F04-3).** The plugin's `Select-PanelRouting`
  mutates the shared member objects (a required weighty/light entry revived to `run` with no
  reason, an eligible one without a seat `not-picked`) before the members / skipped records are
  built. C3 builds `panel.members`, the skipped record (each member's `roster.skipped`) and so
  every child's member spec from the routed states joined to the roster identity by position -
  before wave 1c they came from the earlier selection, so a required light reviewer that ran was
  recorded `skipped` (light gate) and an unseated one `run`. The `Timeout:` line's roster
  exceptions likewise list the SEATED members in seat order (the plugin's slots).
- The D16 context skip moved INTO `panel_members` (before the light gate, as in the plugin), so a
  weighty sibling skipped for its context window is what a light entry stands in for, and a
  panel whose every entry is context-skipped refuses with the plugin's "no reviewer ... is
  available" listing.

Tests: `providers::roster_walk_tests::panel_light_*` (the harness-panel LIGHT roster: checkpoint,
acceptance, the context stand-in, `-PanelAll`; the no-sibling stand-in and two light siblings)
and `roster::tests::panel_light_is_the_third_weight`. Wave 1c (RC3): the integration test
`c3-cli/tests/panel_light_required.rs` `required_light_reviewer_is_recorded_run_in_every_member_record`:
the LIGHT roster's `-Require #2` on acceptance as a REAL panel run against a fake codex
(`.cmd` + PowerShell, Windows only), asserting the plan line, three usable members, and in EVERY
member's ledger entry `panel.members` = all three `run` with an empty reason and an empty
`roster.skipped` (harness-panel's own LIGHT `-Require` check is a dry run).

## D. The re-read anchor (0.6.0 wave 28e E4; with 28c D11 and 28d D7)

Module `crates/c3/src/consult/prompt.rs` (`reread_line`). C3 had no re-read line at all, so the
whole chain was ported: a reviewer with a context window (`context_tokens` > 0) gets, as the
LAST line before the consultation id, `Before you answer, re-read the brief: \`<brief>\`.`
(D11) or, without a brief file, `Before you answer, re-read the ask: <ask>` (D7). E4: the ask's
FIRST non-blank line whole (whitespace folded), cut at 300 characters with `... (cut here: the
whole ask is at the top of this prompt)`, then ` (+<n> more line|lines)` for the remaining
non-blank lines. The line takes no part in the fork/resume context estimate (D8 - C3 estimates
before assembling the prompt).

Tests: `consult::prompt::tests::reread_anchor_keeps_the_first_line_and_counts_the_rest` (the
four `harness-fixes28e.ps1` ANCHOR asks, the brief form, the position before the id).

## Measurements

Scripts directory: the five shims + the plugin's `codex-consult-common.ps1`,
`codex-consult-detached.ps1` and `schemas/consult-reply.schema.json` from tag `v0.6.1`
(`harness-shim.md` section 4); harnesses from the plugin checkout (`v0.6.1` + one `.collab`-only
commit), each through `tests/run-all.ps1 -ScriptsDir <dir> -Only <harness>` (the two category
runs call the harness file with `-Only <category>` and run-all's environment), ONE AT A TIME,
holding `%TEMP%\codex-consult-tests\HARNESS.lock`. "main" is `cdbc614` (its code is the branch
point `4cd8d18`'s: the two commits since touch `.collab` only), measured the same way, so every remaining failure is classed by evidence.

| Harness (0.6.1) | main | wave 1 | Fixed by wave 1 | Still failing - all pre-existing (identical on main), outside wave 1 |
|---|---|---|---|---|
| harness-roster | 115 / 10 | **121 / 4** | TIMEONLY x4, WEIGHT x2 (`panel: light` accepted, the new validator text) | FILE (`claudeauth`, `claudemodel`: the 0.6.0 claude engine's roster keys); WALK (ledger order wants the 0.6.x `context_window` field after `extra_config_source`); RATE x2 (0.6.1 F06-1/F06-2: `rating_rev` and `judge` in a rating) |
| harness-panel | 52 / 10 | **60 / 2** | LIGHT x8 | SPEC x2 (a member whose panel run dies during its preflight; F07-1/F11-6 the parent killed between the member's rewrite and its parent check) |
| harness-lock2 | 11 / 0 (Run 17) | **11 / 0** | - | - |
| harness-fixes `-Only E27` (extra) | 4 / 7 | **9 / 2** | E27 (a)(b) proceed with the app's servers named as excluded, (c) x2, (d) x2 | E28 x2: the unknown-tree recovery of a panel member's record after an unconfirmed kill (C3 has no unknown-tree rule yet; the scan rows it relies on pass) |
| harness-fixes28e `-Only ANCHOR` (extra) | 0 / 4 | **4 / 0** | all four | - |

The harness-roster UNIT `Get-RetryAfter` check (69 samples) runs the plugin's own dot-sourced
function, so it is green for both binaries; C3's equivalent is the Rust test over the same 69
samples. Run 17's 119/0 (roster) and 54/0 (panel) were the 0.5-era harnesses; the 0.6.1 ones have
125 and 62 checks. A manual check through the shims (`CODEX_CONSULT_NOW` 2026-10-07 21:00 local,
the fake codex failing with the time-only wording) recorded `retry_after
2026-10-07T21:43:00+02:00` and `c3 providers` printed `quota until 2026-10-07T21:43:00+02:00:
<when> - You've hit your usage limit...`.

`cargo test --workspace -j 2`: 535 passed, 0 failed (522 before + 13 new). `cargo clippy
--workspace --all-targets -- -D warnings`: clean. `cargo fmt --all -- --check`: clean.

**Wave 1c** (Astra's diff review, handoff 04: F04-1..F04-3 with RC1-RC3), measured the same way
with its own binary: harness-panel **60 / 2**, harness-roster **121 / 4**, harness-fixes `-Only
E27` **9 / 2** - unchanged, the same pre-existing rows as above; harness-fixes26b `-Only TIMEOUT`
(extra, the `Timeout:` line's roster exceptions now come from the seated members) **6 / 0**.
`cargo test --workspace -j 2`: 542 passed, 0 failed (535 + 7 new); RC2's
`windows_local_reset_boundaries` ran for real here (machine zone W. Europe Standard Time:
chrono reports 2026-10-25 03:00 as `Ambiguous(+02:00, +01:00)` and 2026-03-29 02:00 as
`Single(+01:00)`; normalised `Single(+01:00)` and `None`). The RC3 integration test fails on
the wave-1 `run.rs` (`glm-5.3-flash` recorded `skipped` with the light-gate reason) and passes
with the fix. Clippy and fmt clean.

## What differs from the plugin, and why

- **The zone database.** Production reads the machine's zone through `chrono::Local` (the OS
  rules, as `[TimeZoneInfo]::Local`); the unit tests read Berlin by its IANA id through
  `chrono-tz` (a dev-dependency only), where the plugin's harness uses `W. Europe Standard Time`.
  Same rules for 2026. chrono's own boundary classification is not trusted: every wall time is
  normalised against the UTC-to-local round trip (section A, wave 1c), so the exact edges of a
  transition read as .NET reads them.
- **Character counts.** The 300-character cut of the re-read anchor counts Unicode scalar values
  (the plugin: UTF-16 code units); the exec word's neighbours are "letter, digit, `_` or `-`"
  (`char::is_alphanumeric`; .NET `\w` also counts connector punctuation and marks). Both differ
  only for characters outside the common range.
- **The parse clock** takes the first `CODEX_CONSULT_NOW` token (`get_consult_clock_peek`); the
  plugin's `-Peek` takes the token at its current call count - different only with a
  comma-separated multi-token test clock.
- **The bare-pid liveness check** (`pending.rs`, a recorded pid without a start time) gained the
  exclusion and the unreadable-command-line hook but still lacks the plugin's wider E19 evidence
  rules (an unreadable command line of a NON-codex process, a child of a recorded pid, counted as
  running) - wave 28e E19, outside this wave.
- **Not ported here:** the unknown-tree recovery behind the two E28 harness rows; the
  `retryDelay` fallback on a gRPC failure's raw payload in `New-ProviderFailure`; the
  claude-engine roster keys, the `context_window` ledger field and the F06-1/F06-2 rating fields
  (the remaining harness-roster rows).
