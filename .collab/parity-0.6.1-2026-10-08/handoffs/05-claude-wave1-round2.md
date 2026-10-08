Write in English.

# Handoff 05 - claude: wave 1, second round (F04-1..F04-3 answered)

Date: 2026-10-08. Base commit: `add198b` (branch `main`, fast-forwarded from `wave1c-fixes`; the range under
review is `bd96c44..add198b`; skip `.collab/`).

## Question

Your round on handoff 03 held wave 1 on F04-1..F04-3. All three are fixed in add198b with your fixtures RC1-RC3.
Can wave 1 be accepted into the compatibility track? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta since the last review

- **F04-1:** `strip_exe` checks the last four bytes with `str::get` before comparing to `.exe`; `日本` no longer
  panics. Tests `strip_exe_is_boundary_safe`, `unicode_name_codex_match_completes_without_a_match`,
  `unicode_name_scan_completes_without_a_match_or_an_exclusion` (RC1: no panic, no match, no exclusion).
- **F04-2:** `ResetZone::wall_offsets` keeps an offset only when converting `wall - offset` back to local time
  yields that offset again (a UTC round trip): the false Ambiguous at 03:00 on the fall-back day is dropped, the
  spring 02:00 reads as a gap. RC2 `windows_local_reset_boundaries` ran for real on this machine (W. Europe
  Standard Time): autumn time-only 03:00 at reference 2026-10-25T02:00+01:00 -> 2026-10-25T03:00:00+01:00;
  autumn dated 03:00 -> +01:00; spring dated 02:00 -> 2026-03-29T02:00:00+02:00; the six expected instants
  were produced by the plugin's own `Get-RetryAfter` on that zone. Plus
  `wall_offsets_are_normalised_against_a_utc_round_trip` and
  `reset_boundaries_follow_the_plugin_in_every_zone_implementation` (IANA fixtures kept).
- **F04-3:** `panel.members` and the skipped record (which feeds each child's member spec and
  `roster.skipped`) are built from the routed states, joined by position: a required light reviewer is recorded
  `run` with an empty reason, an unseated one `not-picked`. RC3 as a real fake-CLI run:
  `crates/c3-cli/tests/panel_light_required.rs` (Windows only - the fake codex is a `.cmd` wrapper); on the
  wave-1 `run.rs` it fails with the light reviewer `skipped`. Beyond the brief: the panel's `Timeout:` line lists
  the seated members' roster timeout exceptions in seat order as the plugin does (TIMEOUT category 6/0).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests | `cargo test --workspace -j 2` | add198b | 0 | 542 (535 + 7) | completed |
| clippy / fmt | `-D warnings` / `--check` | add198b | 0 | clean | completed |
| harness-panel (shim v0.6.1) | `run-all.ps1 -ScriptsDir ... -Only harness-panel` | add198b | 1 | 60/2 (the two pre-existing SPEC rows) | completed |
| harness-roster | the same | add198b | 1 | 121/4 (FILE, WALK, RATE x2 - later waves) | completed |
| harness-fixes E27 | the same | add198b | 1 | 9/2 (E28 unknown-tree, wave 3) | completed |

## Open findings

F04-1..F04-3 `implemented` (add198b); F02-6 `implemented`; F02-1..F02-4, F02-7 assigned to waves 1b-4
(1b runs now: the roster `plan` key and the machine-wide plan records).

## Questions

- **Q1.** F04-2: a zone or boundary where the round-trip rule drops a genuine candidate or keeps a false one?
- **Q2.** F04-3: a path that still serialises the pre-routing states (a panel member's own ledger entry, a
  detached panel's status file, `--status`)?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
