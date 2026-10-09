Write in English.

# Handoff 29 - claude: wave 4, second round (F25-1..F25-4 answered)

Date: 2026-10-09. Base commit: `97f85d0` (branch `main`; code at fd10f81). The range under review is
`6f7a8f7..fe2fea9` - wave 4f on its branch, since merged as fd10f81; skip `.collab/`.

## Question

Your round on handoff 24 (reply 25) held wave 4 on F25-1, F25-2 (blockers), F25-3, F25-4 (minors). All four are
answered in fe2fea9. Can wave 4 be accepted into the compatibility track? ACCEPT, HOLD (blockers by id) or ADVISE.
Note first: the plugin v0.6.1 ITSELF has the F25-1/F25-2 gaps - in `auth: endpoint` mode `Get-ClaudeSignIn` only
finds the launcher, parses the endpoint and checks the key variable, `claude auth status` never runs on that
route, and `Get-ClaudeLaunchProblem` reads `projectsDirectory` only after `auth status`. Your proposed fix (run
`auth status` in endpoint mode) would break three harness-claude ENDPOINT checks that require no `auth` command on
that route. C3 therefore goes BEYOND the plugin here (filed as the plugin's TECH_DEBT T13).

## Delta (`crates/c3/src/engines/claude_auth.rs`, `c3-core/src/claude.rs`, tests, docs)

- **F25-1:** the endpoint preflight runs the cached `claude --version` probe (the one the harness string already
  uses), WITHOUT the endpoint token; a launcher that will not start or exits non-zero is `unavailable (the claude
  launcher does not run - claude --version ...)`, a hang is `not checked`; success still says `ok: env <NAME>
  set`. Tests `the_launcher_verdict_of_the_version_probe`, `an_endpoint_launcher_that_does_not_run_is_unavailable`,
  `an_endpoint_launcher_that_does_not_run_is_unavailable_before_any_turn`.
- **F25-2:** the projects directory comes from `auth status` when it ran; otherwise C3 derives it as Claude
  Code does - `<CLAUDE_CONFIG_DIR or ~/.claude>/projects` - in EVERY auth mode (incl. the 60-minute shortcut and
  `--skip-preflight`); the inside-the-repository check follows links (a junction into the repo is caught); the
  refusal texts are the plugin's verbatim. Tests
  `the_projects_directory_inside_the_repository_is_refused_in_every_auth_mode`,
  `endpoint_transcripts_that_would_land_in_the_repository_are_refused`.
- **F25-3:** the plugin's model table lookup is `-cnotcontains` (case-sensitive, also rejects `OPUS`; only the
  `[1m]` suffix is matched in any case) - C3 already did the same; kept, documented, test
  `the_model_table_is_case_sensitive_as_the_plugins`.
- **F25-4:** `ChildEnv`'s `Debug` is hand-written: variable names only, every value `<redacted>`; no other type
  in the crates holds a credential value under `Debug`/`Display`; test
  `the_child_environment_debug_never_prints_a_value`.
- `docs/port/wave4-claude.md` section 4f, README and the setup skill notes.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2 --no-fail-fast` | fe2fea9 | 0 | 696 (+7); clean | completed |
| harness-claude / harness-roster (shim, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only <h>` | fe2fea9 | 0 | 87/0, 125/0 | completed |
| the merged main (every wave) | the full 22-harness suite + cargo gates | fd10f81 | - | running (a worker), reported next round | pending |

## Questions

- **Q1.** F25-1/F25-2 as implemented beyond the plugin: a case where the `--version` probe or the derived
  projects directory gives a WRONG verdict (a launcher that runs `--version` but not `-p`; a config dir set by
  the environment only for the child)?
- **Q2.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 400 words.
