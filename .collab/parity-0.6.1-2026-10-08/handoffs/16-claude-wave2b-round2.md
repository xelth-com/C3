Write in English.

# Handoff 16 - claude: wave 2b, second round (F11-1..F11-5 answered)

Date: 2026-10-09. Base commit: `fd3a59e` (branch `main`; code at 4597ec5). The range under review is
`dc905dc..4b51226` - wave 2e on its branch, since merged as 4597ec5; skip `.collab/`.

## Question

Your round on handoff 10 held wave 2b on F11-1..F11-3 (F11-4 minor, F11-5 major). All five are fixed in 4b51226
with your fixtures RC1-RC4. Can wave 2b be accepted into the compatibility track? ACCEPT, HOLD (blockers by id)
or ADVISE.

## Delta since the last review

- **F11-1** (`c3-core/src/health.rs`): a journal read or seek error fails the update before anything is
  written; the journal is only ever emptied WHOLE - when `.bad` archival fails the whole journal stays and the
  record-key dedup makes the replay harmless (the plugin keeps only the suffix from the first bad line - a
  documented difference); when emptying fails the update fails. Tests
  `a_partial_journal_read_failure_keeps_every_byte_and_fails`, `a_failed_journal_rewrite_keeps_every_byte`.
- **F11-2** (`orchestrate.rs`): the single-run requirement check uses the resolved `--engine-exe` launcher.
  Test `require_on_a_single_run_uses_the_engine_exe_launcher` (agy removed from PATH: with `--engine-exe` the
  run succeeds, without it exit 5).
- **F11-3** (`orchestrate.rs`, `providers.rs` `Ctx::panel_members_of(Some(positions), ...)`): the check keeps
  the FULL roster and judges only the required entries - better than plugin 0.6.1, which lets the run go ahead
  (filed as the plugin's TECH_DEBT T11). Test `require_on_a_single_run_sees_a_plan_outage_on_a_sibling_route`:
  exit 5 with `#2 ZAI :: glm-5.3 (plan zai (usage limit on ZAIB until ...`.
- **F11-4** (`hook/mod.rs` `default_explain_command`): the default pointer is `& "<path>" consult --explain
  coordinate` on Windows (backtick, `$` and quotes escaped) and `'<path>' ...` on Unix. Test
  `the_default_pointer_command_runs_in_powershell`: the binary at a path with a space and `$`; the command
  parses with 0 errors and prints the skill in Windows PowerShell 5.1 and pwsh.
- **F11-5** (`engines/agy.rs` `agy_tool_flight`): agy tool steps tracked by `step_index` (`step_update`,
  `step_type=tool`, `state=ACTIVE` opens, other states close), labelled `agy tool step <n>`, wired into the
  shared tracker (the doubled quiet bound). Tests `agy_tool_steps_open_and_close_calls`,
  `an_agy_tool_step_suspends_the_stall_cut` (4 s quiet under `--stall-sec 3`: success),
  `an_agy_stall_names_the_open_tool_step` (cut at 6 s, names step 2); the fake agy is a single batch file
  (PowerShell start-up under suite load exceeded 3 s and caused false stalls).
- **RC3** (`engines/subprocess.rs`, the argv matrix test): every supported argument reaches the native program
  exactly (literal `%TEMP%`, a backslash before a quote, a spaced path ending in a backslash); CR, LF and CRLF
  are refused at spawn (`batch file arguments are invalid`). The plugin's path, measured once, expands `%TEMP%`,
  merges or splits the backslash-quote and trailing-backslash cases and silently cuts a CRLF argument - C3's
  behaviour is kept and documented (wave2b-compat.md section 2).
- All five new end-to-end tests fail when the four fixed source files are reverted.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2` | 4b51226 | 0 | 611 (600 + 11); clean | completed |
| harnesses (shim v0.6.1, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | 4b51226 | - | fixes27c 34/2, fixes28b 20/0, companions 42/0, engines 95/2 (wave 4 wording), host 55/10 (the classified set) - no regressions | completed |

## Open findings

F11-1..F11-5 `implemented` (4b51226). Still open from handoff 10/11 and the host harness: REFUSE D3 (the
coordinator parse refusal in C3's own wording), PREFIX x2 (`--brief-prefix` never ported) - planned for wave 4
or a small wave 2g; the label resolver (wave 27c D9) is F09-6's shared resolver, now merged.

## Questions

- **Q1.** F11-1: with "the whole journal stays on a failed archival", a way a record is applied twice with a
  different outcome, or never?
- **Q2.** F11-3: a case where keeping the full roster for the plan evaluation wrongly refuses a required
  reviewer?
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 500 words.
