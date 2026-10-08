# Running C3 under the codex-consult plugin's PowerShell harnesses

Source repo read for this: `C:\Users\Dmytro\claude-codex-consult` at commit `d592526`
(`tests/run-all.ps1`, `tests/harness-0.3.ps1`, `tests/harness-engines.ps1`,
`tests/harness-roster.ps1`, `tests/fake-codex3.ps1`/`.cmd`, `tests/README.md`,
`plugins/codex-consult/scripts/codex-providers.ps1`). Nothing there was modified.

## 1. How a harness locates and invokes a script

Every harness resolves paths purely from `$PSScriptRoot` — no PATH lookup, no
in-process dot-sourcing of the script under test:

```
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path      # harness-0.3.ps1:15
$scripts  = Join-Path $repoRoot 'plugins\codex-consult\scripts'      # harness-0.3.ps1:16
$providersPs = Join-Path $scripts 'codex-providers.ps1'              # harness-0.3.ps1:160
```
(identical pattern in `tests/harness-engines.ps1:18-21`).

Invocation is always an out-of-process file run, never `& $path @args` and never
dot-sourcing:

```
$out = & $psExe -NoProfile -ExecutionPolicy Bypass -File $providersPs @ArgList 2>&1   # harness-0.3.ps1:167
$code = $LASTEXITCODE
```
`$psExe = (Get-Process -Id $PID).Path` (harness-0.3.ps1:20) — the harness re-launches
whichever host it is itself running under (Windows PowerShell 5.1 or `pwsh`), so
`codex-providers.ps1` is required to be a `.ps1` that a `powershell.exe`/`pwsh -File`
invocation can run.

Per-run isolation, inside the `Providers` helper (harness-0.3.ps1:159-172):
- `Clear-TestEnv` first (clears the whole `codex-consult` env surface between cases).
- `$env:CODEX_HOME = $CodexHome` — a scratch Codex home directory (`config.toml` etc.)
  built per test case by `New-Home`.
- `$env:CODEX_CONSULT_EXE = $fake` — points the bridge's `codex` launcher resolution at
  `tests/fake-codex3.cmd` (harness-0.3.ps1:19), so no real `codex` CLI is ever invoked.
  `Resolve-CodexLauncher -Explicit $CodexExe` in `codex-providers.ps1` (its own
  `-CodexExe` parameter) is one override path; `CODEX_CONSULT_EXE` is the env override
  documented at `codex-providers.ps1` lines ~121-124.
- `$env:ZAI_KEY_A = 'test-key-a'` plus any case-specific `-Env` hashtable entries
  (empty string value removes the var) — credentials for the scratch `[model_providers]`
  tables.
- `Push-Location $Repo` — the scratch git repo under test becomes the CWD, because
  `-CollabDir` (default `.collab`) and repo-root path resolution in the real scripts are
  relative to CWD.
- `harness-roster.ps1` additionally sets `$env:CODEX_CONSULT_ROSTER` to a scratch roster
  JSON path, or the literal string `none` to mean "no roster" (harness-roster.ps1:78-96).
- `harness-engines.ps1` also sets `CODEX_CONSULT_AGY_EXE`/`FAKE_AGY_*` for the `agy`
  engine rows, and can point `CODEX_CONSULT_AGY_EXE`/`Path` at a scratch dir to simulate
  "launcher not found" (harness-engines.ps1 LISTING section).
- stdout+stderr are merged (`2>&1`) and re-joined into one text blob; `$LASTEXITCODE`
  is captured immediately after the call and is the sole exit-code signal used in every
  `Check`.
- With `-Json` on the args list, the harness parses the WHOLE captured text as JSON
  (`ConvertFrom-Json`), unwrapping a PS 5.1 single-object-array quirk
  (harness-0.3.ps1:164-166). This means `-Json` output must be nothing but the JSON on
  stdout — no banner lines, no trailing text.

## 2. Exact assertions on `codex-providers.ps1`

All from `harness-0.3.ps1` (section `PREFLIGHT`, lines ~1056-1080) and
`harness-engines.ps1` (section `LISTING`, ~632-660); `harness-0.3.ps1` also has one
provider check inside `F06-1` (~960-970) and one inside `F09-2/4` (~975-995). See
`docs/port/m1-acceptance.md` for the itemised checklist. Highlights:

- **Exit codes** (`-Provider <name>` given): `0` available, `2` unavailable, `3`
  unknown, `1` usage error (e.g. unknown provider name). Without `-Provider`: always
  `0` (harness-0.3.ps1:1073-1075, docstring in `codex-providers.ps1` "Exit codes:"
  section).
- **Stdout line patterns without `-Provider`/`-Json`** (a table): first line matches
  `^VERDICT\s+PROVIDER\s+KIND\s+ENDPOINT\s+CREDENTIALS\s+EFFORT\s+LAST FAILURE`, one
  data line per provider matching `^(available|unavailable|unknown)\s+`, and the
  PROVIDER column must be reachable by fixed-column alignment (harness-0.3.ps1:1078,
  the `$aligned` computation two lines above).
- **`-Provider <name>` stdout**, single-provider text form used with regex `-match`
  against the whole captured text, e.g. `unknown \(the providers could not be
  established: ...\)` (harness-0.3.ps1:971), `unavailable \(auth failed <iso>: ...\)`
  (harness-0.3.ps1:977), `unavailable \(missing: Not logged in\)`
  (harness-0.3.ps1:1074).
- **`-Json`**: an array of objects, one per provider name, fields asserted by name:
  `name`, `verdict` (e.g. `'unavailable (missing: env CC_TEST_KEY not set)'`),
  `credentials`, `endpoint`, `effort_vocabulary`, `last_limit` (object or `$null`)
  (harness-0.3.ps1:1069-1070). Engine rows (harness-engines.ps1 LISTING) add `engine`,
  `kind` (`"engine agy"`), `schema_transport`, `roster_position`, `roster_positions`,
  `roster_selected` (bool).
- **`-Short`**: one line starting `codex-consult: ` — `"all N reviewers available"` or
  `"out - <label> :: <model> (until <local>, in <relative>); K of N reviewers
  available"`; `-Short -Json` returns `{line, health_source, total, available, out,
  not_checked, roster, entries[...]}` (docstring `-Short` section,
  `codex-providers.ps1`).
- **"wrote nothing" check**: a full byte-for-byte snapshot (path, length, LastWriteTime
  ticks of every file under `.collab`) taken before the `-Json` call and compared equal
  after (harness-0.3.ps1:1067-1068, 1080) — no lock file, no `.consult.pending.json`,
  nothing under `.collab` may be created, touched, or its mtime bumped.
- Env-var handling matches `codex-providers.ps1`'s own doc comments (credentials,
  endpoint, effort, last-failure wording) rather than being independently invented by
  the harness — the harness strings are lifted from that script's `.DESCRIPTION` block
  (`codex-providers.ps1:24-90`).

## 3. Can a thin shim satisfy this?

Yes, structurally: the harness only ever (a) resolves `$providersPs` as a plain file
path under `plugins\codex-consult\scripts\`, (b) launches it via
`powershell -File`/`pwsh -File` with an argument list, (c) reads merged stdout+stderr
text and `$LASTEXITCODE`. None of that requires the target to be a PowerShell script's
*logic* — only that the file at that path is a `.ps1` a PowerShell host can execute,
that it accepts the same named parameters, prints the same stdout shapes, and exits
with the same codes. A `.ps1` shim that shells out to the `c3` binary and forwards
stdout/stderr/exit code verbatim satisfies every assertion above, provided:

- The shim is placed at (or replaces) exactly
  `plugins\codex-consult\scripts\codex-providers.ps1` relative to the harness's
  `$scripts` — i.e. to run the *existing, unmodified* harnesses against C3, the C3
  checkout would need to present a `plugins\codex-consult\scripts\codex-providers.ps1`
  file (a copy or symlink of the shim) inside a directory tree shaped like the
  plugin repo, OR the harness needs a new `-ScriptsDir`/env-override parameter on the
  plugin side to point `$scripts` elsewhere. **No such override parameter exists
  today** — `$scripts` is hard-coded to `..\plugins\codex-consult\scripts` relative to
  `$PSScriptRoot` (harness-0.3.ps1:15-16, harness-engines.ps1:18-19). This is the one
  harness-side change worth requesting if C3's binary should sit outside that path:
  add an optional `-ScriptsDir` param (default preserved) to each harness, or an
  `CODEX_CONSULT_TEST_SCRIPTS_DIR` env override read before computing `$scripts`.
  Until then, the practical path is to drop/symlink the shim file into that exact
  location inside a checkout of `claude-codex-consult`.
- The shim must consume `$env:CODEX_CONSULT_EXE` (fake codex launcher path) the same
  way `Resolve-CodexLauncher` does, since PREFLIGHT/F09 cases assert on codex-login
  text produced by the fake — this only matters for provider *rows that need `codex
  login status`* (the built-in `openai` row and any `requires_openai_auth` table); a
  first C3 port that only implements `-Provider`/`-Json`/`-Short` for non-codex-auth
  providers can defer this by forwarding `-CodexExe`/`CODEX_CONSULT_EXE` straight to
  `c3 providers` and letting the Rust side shell out to the fake, unchanged.
- Nothing observed requires PowerShell-only objects on stdout — all assertions are
  string `-match`/`ConvertFrom-Json` against captured text, so a Rust binary's stdout
  is indistinguishable from a script's `Write-Output` as long as line endings/encoding
  match (harness expects UTF-8 text; PS 5.1's `ConvertFrom-Json` on the merged
  `2>&1` text needs the JSON to be the *only* content when `-Json` is passed).
  `Write-Host`/color codes must not leak into stdout the JSON parse depends on.
  `c3 providers --json` must therefore write JSON to stdout and nothing else, with
  any diagnostics on stderr (already merged in by the harness, but never inside the
  `-Json` payload region — see `codex-providers.ps1`'s own separation of stdout table
  vs stderr chatter, which the shim must preserve since the harness's `-Json` parse
  path takes the *entire* joined `$text`, not just a captured region).
- The "wrote nothing" snapshot check has no PowerShell dependency at all — it just
  needs the binary invocation to genuinely not touch `.collab`.

No blocker was found that rules out a shim; the one open item is the missing
`-ScriptsDir`/env override on the harness side (see `docs/port/m1-acceptance.md`
"Open questions").

## 4. The scripts directory for a 0.6.x plugin (wave 1 of the 0.6.1 parity, 2026-10-08)

The harnesses no longer need the shim inside a plugin checkout: `tests/run-all.ps1 -ScriptsDir
<dir>` (wave 25; else `CODEX_CONSULT_SCRIPTS_DIR`) points every harness at a scratch scripts
directory, and the one-harness form is `run-all.ps1 -ScriptsDir <dir> -Only <harness>`. For the
plugin at **v0.6.1** that directory holds:

- the five C3 shims from `tests/shim/` under the plugin's script names (`codex-providers.ps1`,
  `codex-consult.ps1`, `codex-findings.ps1`, `codex-scoreboard.ps1`, `codex-consult-hook.ps1`);
- an unmodified copy of the plugin's `codex-consult-common.ps1` (several harnesses dot-source it
  for their in-process UNIT checks - e.g. harness-roster's `Get-RetryAfter` samples run against the
  plugin's own function, not through `c3`);
- an unmodified copy of the plugin's **`codex-consult-detached.ps1`: the 0.6.x common library
  dot-sources it at load time** (`. (Join-Path $PSScriptRoot 'codex-consult-detached.ps1')`), so a
  scripts directory without it fails every harness that loads the common library;
- a sibling `schemas/consult-reply.schema.json` (`<scripts dir>/../schemas/`), unmodified.

`C3_EXE` names the binary the shims run (a copy of `target\debug\c3.exe` keeps a concurrent
`cargo test` from replacing it mid-run). Run the harnesses ONE AT A TIME. The plugin's
`codex-telemetry.ps1` (0.6.x) has no C3 shim; no harness in the wave-1 set calls it. Copy the
library files from the tag (`git show v0.6.1:plugins/codex-consult/scripts/<file>`) rather than
checking anything out in the plugin repository.
