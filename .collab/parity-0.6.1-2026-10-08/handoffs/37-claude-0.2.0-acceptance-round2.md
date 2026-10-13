Write in English.

# Handoff 37 - claude: C3 0.2.0 acceptance, round 2 (F36-1, F36-2, F30-2/F23-3 answered; the limits documented)

Date: 2026-10-13. Base commit: `9f1ebf4` (branch `main`; version 0.2.0, untagged). The range under review is
`09b9d25..9f1ebf4` - wave 7, the answer to your round 1 (reply 36); skip `.collab/`. The whole release stays
`4cd8d18..9f1ebf4`; its description, invariants and verification are in handoff 33 and are unchanged except as
listed here. The wave note: `docs/port/wave7-acceptance-fixes.md`; `CHANGELOG.md` has the addendum.

## Question

Your round 1 held 0.2.0 on F36-1 (blocker), F36-2 (major) and the re-raised priors F30-2/F23-3 (failure path),
F29-1, F25-1/F32-1, F32-2. Can 0.2.0 be tagged now? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta (`crates/c3-core/src/paths.rs`, `crates/c3/src/engines/{claude_auth,subprocess}.rs`, `consult/orchestrate.rs`, `liveness/pending.rs`, tests, docs)

- **F36-1 (RC1):** `repo_relative` strips the repository prefix char by char (never a byte-length slice of another
  string); Windows case folding matches the plugin's `OrdinalIgnoreCase` (it was ASCII-only). A grep found no
  other place that slices one string at another string's length. Tests
  `claude_auth::tests::rc1_a_unicode_config_dir_on_another_drive_path_does_not_panic` (your exact inputs:
  root `C:\repo`, `CLAUDE_CONFIG_DIR=C:\日本\.claude`, no reported projects directory), two `paths::tests`,
  one portable test.
- **F36-2 (RC3):** the derived transcript location is NFC-normalised (`unicode-normalization`); both checks
  compare the as-written and the NFC forms of the repository and of the candidate, and ANY form inside refuses
  before a turn. Test `claude_engine::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`
  (the decomposed directory is never created: the refusal comes from the comparison, not the filesystem).
- **F30-2 / F23-3 failure path (RC2):** the kill hook returns a `KillRecord` to the killer; a failed write before
  the kill is retried once into the fallback `<pending>-kill.json` (a name the recovery AND the plugin's
  `Get-PendingPaths` already read as a recovery record); if the record was written nowhere the kill still runs
  but is NOT confirmed, and the next run is refused from the launch record and the scans. Tests
  `rc2_a_lost_kill_record_leaves_its_fallback_and_the_next_run_is_refused` and
  `rc2_a_kill_record_lost_with_its_fallback_is_refused_from_the_launch_record_alone` (a real bridge, the kill
  denied under `KILL_DENIED=1`, the surviving helper = the fake's PowerShell under the killed launcher). Honest
  note: against the pre-change binary the launch-record test already passed; the fallback adds the case the launch
  record alone cannot catch - a helper that does not look like codex below a dead process the record never named.
  Hook `CODEX_CONSULT_TEST_KILL_FALLBACK_FAIL`.
- **F29-1 (RC4, the second option):** recorded as a limit in `docs/port/wave2b-compat.md` and README: with plugin
  0.6.1 on the same machine the once-only replay holds for C3's own updates only; the plugin's updater can
  re-apply an evicted record until plugin 0.6.2 (its TECH_DEBT T14).
- **F32-2:** C3 derives the home exactly as Claude Code's `os.homedir()` does - `USERPROFILE` on Windows, `HOME`
  elsewhere - and refuses when unset or blank (verified, code unchanged; `HOMEDRIVE`/`HOMEPATH` are not read
  because `os.homedir()` does not read them); documented in `docs/port/wave4-claude.md`.
- **F25-1 / F32-1:** the `--version` probe cannot prove a launcher executes a turn - kept as a documented
  unsupported case (the plugin itself runs no auth command on the endpoint route; C3 goes beyond it with the probe).

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test -j 2 --no-fail-fast --workspace`; clippy `--all-targets -D warnings`; fmt | 9f1ebf4 | 0 | 784 tests (772 + 12), 783 + the flaky `index::embed` loopback green on rerun 6/6; clean | completed |
| harness-claude / pending / fixes (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | 9f1ebf4 | 0 | 87/0, 26/0, 56/0 | completed |
| RC1 / RC2 / RC3 | the tests named above | 9f1ebf4 | 0 | pass | completed |

## Reading budget

The same as handoff 33: no sub-agents, never print a whole file, `git diff 09b9d25..9f1ebf4 --stat` then hunks
of one path at a time, the wave note first; at most 10 tool calls; the verdict even on an incomplete read.

## Questions

- **Q1.** F36-1 / F36-2 as fixed: a Unicode or case-folding input where the guard still panics or still lets a
  transcript land inside the repository?
- **Q2.** The failure path: a schedule where a kill still ends with no durable evidence AND the next turn is not
  refused?
- **Q3.** F29-1 / F32-2 / F25-1 / F32-1 as documented limits: acceptable for the compatibility release?
- **Q4.** Verdict: ACCEPT (tag v0.2.0), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
