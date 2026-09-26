# M1 acceptance checklist — codex-providers assertions

Derived from `C:\Users\Dmytro\claude-codex-consult` at commit `d592526`. Every row is a
single `Check` call the plugin's harnesses make against `codex-providers.ps1` (or the
`Providers` helper that wraps it). "Check id" is the harness's own tag string (first
argument of `Check`) plus a short disambiguator where a tag repeats. File:line points
at the `Check`/assertion, not the fixture setup above it.

A C3 `c3 providers` (or the `codex-providers.ps1` shim fronting it) is M1-complete when
every row below passes unmodified against the corresponding harness section
(`-Only PREFLIGHT`, `-Only F06-1`, `-Only F09-2/4`... on `harness-0.3.ps1`; `-Only
LISTING` on `harness-engines.ps1`).

| harness | section | check id | one-line expectation | exit code |
|---|---|---|---|---|
| harness-0.3.ps1 | F06-1 | F06-1 (providers, unreadable set) | `codex-providers -Provider openai` under an unreadable `[model_providers]` construct -> `unknown \(the providers could not be established: unsupported TOML construct at line 2: model_providers = ...\)` | 3 |
| harness-0.3.ps1 | F09-2/4 | F09-2/4 (auth, other alias) | auth failure 10 min ago under another alias in another task -> `-Provider ZAI` reports `unavailable (auth failed <iso>: 401 Unauthorized: invalid API key)` | 2 |
| harness-0.3.ps1 | F09-2/4 | F09-2/4 (capability, no effect) | a recorded `capability` failure (e.g. json_schema not supported) has no effect on availability, but the reason still shows in `-Provider ZAI` text as `capability: ... - json_schema not supported` | 0 |
| harness-0.3.ps1 | PREFLIGHT | PREFL (-Json parse) | `-Json` on a config with `openai` (built-in) + `ZAI` + `zeta` + `broken` parses into exactly 4 rows keyed by `name` | 0 |
| harness-0.3.ps1 | PREFLIGHT | PREFL (verdicts) | verdicts: `openai` available, `ZAI` available, `zeta` `unavailable (missing: env CC_TEST_KEY not set)`, `broken` matches `^unavailable \(table unusable: unsupported TOML construct at line 11: http_headers = \{` | n/a (no -Provider) |
| harness-0.3.ps1 | PREFLIGHT | PREFL (fields) | field values: `openai.credentials == 'ok: Logged in using ChatGPT'`, `ZAI.credentials == 'ok: env ZAI_KEY_A set'`, `ZAI.endpoint == 'https://api.z.ai/api/v1'`, `ZAI.effort_vocabulary == 'zai'`, `openai.last_limit.message` matches `usage limit`, `ZAI.last_limit == $null` | n/a |
| harness-0.3.ps1 | PREFLIGHT | PREFL (exit codes by provider) | `-Provider` exit codes: `openai`=0, `ZAI`=0, `zeta`=2, `broken`=2 | 0/0/2/2 |
| harness-0.3.ps1 | PREFLIGHT | PREFL (unknown/logged-out/fatal) | `-Provider nope` -> text contains `no provider 'nope'`, exit 1; `-Provider openai` with `FAKE_CODEX_LOGIN=out` -> text matches `unavailable \(missing: Not logged in\)`, exit 2; unreadable config (`this is not toml`) -> text matches `unknown \(config unreadable: `, exit 3 | 1 / 2 / 3 |
| harness-0.3.ps1 | PREFLIGHT | PREFL (console table) | no `-Provider`/`-Json`: an aligned table — header line matches `^VERDICT\s+PROVIDER\s+KIND\s+ENDPOINT\s+CREDENTIALS\s+EFFORT\s+LAST FAILURE`, exactly 5 lines total (header + 4 providers) each starting `available|unavailable|unknown `, and the PROVIDER column position aligns across all data rows | 0 |
| harness-0.3.ps1 | PREFLIGHT | PREFL (wrote nothing) | `codex-providers.ps1` (any invocation above) leaves `.collab` byte-identical (no file added/touched/resized) — no lock file, no `.consult.pending.json` | n/a |
| harness-engines.ps1 | LISTING | LISTING (json engine row) | `-Json` on a roster with two `gemini`/`agy` entries + one `openai` entry: exactly one `gemini` row (`engine == 'agy'`, `kind == 'engine agy'`, `endpoint == 'agy (<launcher>)'`, `table == 'n/a'`, `credentials == 'ok: signed in (3 models)'`, `effort_vocabulary == 'agy (tier in the model id)'`, `schema_transport == 'native'`, `roster_position == 1`, `roster_selected == $true`, `verdict == 'available'`); `openai` row has `engine == 'codex'`, `roster_position == 3` | 0 |
| harness-engines.ps1 | LISTING | LISTING (one agy models call) | one listing makes exactly one `agy models` call total (shared between the walk and the row) | n/a |
| harness-engines.ps1 | LISTING | LISTING (-NoNetwork) | `-Json -NoNetwork`: no `agy models` call at all; `gemini.credentials == 'not checked (launcher present; run codex-providers.ps1)'`, `gemini.verdict == 'unknown (sign-in not checked)'`, `gemini.roster_selected == $false`, `openai.roster_selected == $true` | 0 |
| harness-engines.ps1 | LISTING | LISTING (not signed in / no launcher) | agy not signed in -> verdict matches `^unavailable \(\`agy models\`: Error: you are not signed in`; no agy launcher on PATH and `CODEX_CONSULT_AGY_EXE` unset -> `verdict == 'unavailable (agy CLI not found on PATH)'`, `endpoint == 'agy (launcher not found)'` | n/a |
| harness-engines.ps1 | LISTING | LISTING (table + roster line) | plain table: a row matches `(?m)^available\s+gemini\s+1,2\s+engine agy\s+agy \(`; a trailing line matches `(?m)^roster: .* -> would select gemini :: gemini-3\.8-flash-high \[agy\]$` | 0 |
| harness-engines.ps1 | LISTING | LISTING (agy models hang) | `agy models` hangs past the (test-hook-shortened) timeout -> `-Provider gemini -Json` gives `verdict == 'unknown (\`agy models\` did not finish within 3 s)'`, exit 3 | 3 |

## Open questions

- The harnesses hard-code `$scripts = <repoRoot>\plugins\codex-consult\scripts`
  (`tests/harness-0.3.ps1:15-16`, `tests/harness-engines.ps1:18-19`) with no
  `-ScriptsDir` parameter or env override. To run these exact harness files against a
  C3 checkout without copying/symlinking a shim into that path inside a
  `claude-codex-consult` clone, the plugin side would need such an override added.
  Flagging for the supervisor to decide whether that's an M1-side ask or out of scope.
- The `-Provider openai` / `requires_openai_auth` rows depend on `codex login status`
  via the fake `codex` launcher (`CODEX_CONSULT_EXE`/`-CodexExe`). The M1 checklist
  above includes those rows because the harnesses assert on them, but whether C3's
  `c3 providers` re-implements that check itself or shells out to the configured codex
  launcher is a design decision for whoever implements `c3 providers` (out of scope for
  this shim/design task).
- `c3 providers` itself does not exist yet (being implemented in parallel), so none of
  the above has been run end-to-end against C3; this checklist is unverified against a
  live binary.

## Wave 24b additions (verified against `6b88cc8`, by the c3 port)

The port's base was already at `6b88cc8`'s `codex-providers.ps1` / `codex-consult-common.ps1`
content, so these three wave-24b facts are implemented and verified with a byte/field
parity diff of `c3 providers` against the `6b88cc8` script on this machine (clock frozen
via `CODEX_CONSULT_NOW`).

| harness | section | check id | one-line expectation | exit code |
|---|---|---|---|---|
| harness-0.3.ps1 | PREFLIGHT | PREFL (roster_positions) | every `-Json` row carries `roster_positions` — an array of *every* roster position of that `(provider, engine)` label, in roster order, `[]` when none — beside the scalar `roster_position` (the first, or `null`). Verified: `gemini` `[4,5]`, single-entry labels `[6]`, engine rows filtered by their own engine. | n/a |
| harness-0.3.ps1 | F09-2/4 | F09 (reset-unknown quota, direct -Provider) | a usage limit that names **no** reset time, hit < 60 min ago, refuses a direct `-Provider X` run too (F08-7), not only the roster walk: `-Provider X` -> `unavailable (usage limit hit <iso>, reset unknown; retry after <hit + 60 min iso>)`, exit 2; after 60 min it clears (available again) while still showing as the last failure for 24 h. `-Json` `verdict`/`last_limit`/`last_failure` mirror it. Verified byte-identical (verdict text + exit 2). | 2 |
| n/a (classification) | — | Test-UsableOutcome (exactly two) | a ledger `bridge_outcome` counts as a usable reply only when it is exactly `usable reply` or `usable reply (after a timeout continuation)`; any other string (a future `usable reply (<x>)` included) is not usable — so it neither clears the endpoint health nor evidences a sign-in. Mirrored in `c3_core::health::is_usable_outcome`. | n/a |

Note on the F08-7 refusal wording: `codex-providers.ps1` builds its rows with
`Get-PreflightVerdict -RosterWalk`, so the row `verdict` string and exit code for the
reset-unknown case are the same whether or not the caller is a roster walk; the only
`-RosterWalk`-dependent field is the `Refusal` message (the direct-run form appends
`(pass -SkipPreflight to launch anyway)`), which `codex-providers.ps1` never prints. C3
reproduces both refusal forms (unit-tested) even though the listing output does not emit
the refusal.
