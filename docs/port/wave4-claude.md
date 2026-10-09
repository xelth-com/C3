# Wave 4 of the compatibility track: the `claude` engine (2026-10-09)

Branch `wave4-claude`. The specification is the plugin at `v0.6.1`: CHANGELOG `[0.6.0]` (the
`claude` engine, wave 29; its endpoint mode, wave 29b E1-E7; the acceptance decisions E12-E17),
README "Engines (wave 29)" with "Endpoint mode", "Reviewer roster and panel", "Reviewer identity
and lineage"; `codex-consult-common.ps1` (`$script:ClaudeModels`, `Get-ClaudeModelProblem`,
`ConvertFrom-ClaudeEndpointValue`, `Get-ClaudeChildEnvironment`, `Invoke-ClaudeProbe`,
`Get-ClaudeSignIn`, `Test-ClaudeLocalCredential`, `Get-ClaudeIdentityConfig`,
`Get-ClaudeLaunchProblem`, `Get-ClaudeHarness`, `Get-ClaudeArgs`, `ConvertTo-CrtArg`,
`Read-ClaudeEvents`, `Get-ClaudeInitProblem`, `Get-ClaudeServedModelProblem`,
`Get-ClaudeTurnOutcome`, `Read-ClaudeSalvage`, `Resolve-EngineIdentity`, `Get-CoordinatorMatch`,
`Get-EndpointGroups -Scheduling`, `New-MachineHealthRecord -QuotaMark`), `codex-consult.ps1` (the
engine dispatch, the dry run, the killed-turn proof, the engine_run record, the quota mark) and
`setup-providers` 3g; the oracle is `tests/harness-claude.ps1` with its fake CLI through the shim.
Decision P4 of the parity task holds: Claude Code is SPAWNED, exactly as the plugin spawns it; a
native Messages API route is an improvement candidate for later, not this wave.

## 4a - the roster, the preflight, the model table

`c3_core::claude` (new): the closed model table (`CLAUDE_MODELS`, 16 names with `claude-haiku-5-5`,
`[1m]` allowed), the family rule (`model_family`, `model_match` - an alias matches an id of its
family; `exact` for an endpoint route), `model_problem` (the plugin's texts; auth `endpoint`: the
open id pattern and E11 - never an Anthropic id), the endpoint object (`endpoint_from_value`,
`endpoint_problem`: an absolute https URL without credentials, query or fragment, `env_key` the
NAME `^[A-Z][A-Z0-9_]{2,}$`, `timeout_ms` 60000-7200000 default 3000000; no value echoed), the
ALLOW-listed child environment (`child_environment`, pure over a variable list), the argv
(`claude_args`, `crt_arg`, `schema_text`, `format_argv`) and `lab_of_host`.

- **Roster** (`roster.rs`): `engine: "claude"` with `auth` `subscription` (default) | `api-key` |
  `endpoint` (`entry <n>: auth of engine claude must be "subscription" (the claude.ai login, the
  default) or "api-key" (ANTHROPIC_API_KEY) or "endpoint" (...)`), the model check (`entry <n>: the
  claude model '<m>' is not in the claude engine's model table (...)`), the `endpoint` block
  required with auth `endpoint` and refused elsewhere (`... (this entry: engine claude, auth
  subscription)`), the `[1m]` suffix exempt from the D3 delimiter rule; `RosterEntry.endpoint`.
  The engine list of every message is `codex, agy, muse, claude` (`lineage::ENGINE_NAMES`).
- **The engine row** (`lineage::engine_spec("claude")`): label `Claude (claude)`, reply prefix
  `claudecode`, `CODEX_CONSULT_CLAUDE_EXE`, launchers `claude.exe` `claude.cmd` `claude` and
  `%USERPROFILE%\.local\bin\claude.exe`, modes new/resume/fork, native|prompt-only, `--max-turns`,
  a denial retry, 1 MiB prompt bound, the engine scheduling scope; caps-v1 `engine:claude`
  (vocabulary claude, mapping `claude-v1`).
- **Identity** (`resolve_reviewer_identity_auth`): `auth` and `endpoint` on the identity;
  fingerprint `cc-engine-v1|claude|<auth>|<family>`, an endpoint ROUTE
  `cc-engine-v1|claude|endpoint|<canonical base_url>|<env_key>`; `provider_config` `{engine,
  launcher, credential_mechanism, auth_method, api_provider}` (the last two from `claude auth
  status`, filled after the preflight) or, auth endpoint, `{engine, launcher, credential_mechanism
  endpoint, base_url, env_key, plan}`.
- **Preflight** (`engines::claude_auth`): no launcher - `claude CLI not found on PATH`; the LOCAL
  check first (api-key: `ANTHROPIC_API_KEY is not set (roster auth api-key)`; endpoint: `env <NAME>
  not set`), auth endpoint `ok: env <NAME> set` with no `claude auth status` (4f: the launcher's
  `--version` must run); a usable reply within 60 minutes;
  `-NoNetwork` not checked; else `claude auth status` in the child environment (15 s, the JSON read
  before the exit code): `ok: signed in (claude.ai subscription)`, `not signed in (...)`, another
  `authMethod` or `apiProvider`, no JSON / a hang - not checked. One probe per launcher and auth per
  process; only authMethod, apiProvider and projectsDirectory are kept. The harness string is
  `claude-cli <version>` from `--version` in the child environment.
- **`c3 providers`**: the claude row (`engine claude`, endpoint `claude (<launcher>)` or `claude
  endpoint <base_url> (<launcher>)`, effort vocabulary `claude`, transport native); every roster
  identity resolves with the entry's auth and endpoint (`Ctx::entry_identity`).

## 4b - the turn

`engines::claude` + the orchestrator: the argv as sent (copied from `Get-ClaudeArgs`):

```
claude -p --output-format stream-json --verbose --restricted --strict-mcp-config
  --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model <m>
  [--effort <e>] [--json-schema <the schema TEXT, one line>] [--max-turns <n>] [--add-dir <dir>...]
  (--session-id <minted uuid> | --resume <thread> [--fork-session])
```

The prompt goes on stdin, the child starts from the repository root with THE child environment
(`SpawnRequest.env`: the inherited one cleared), and a batch launcher gets every argument as
`ConvertTo-CrtArg` writes it (`SpawnRequest.crt_quote`). A NEW thread's id is minted before the
first byte; resume and fork send the parent's `engine_run.model_resolved`; every secondary turn
(denial retry, timeout continuation, format repair) resumes the session with the id the main turn's
init resolved. The rules (`claude_turn_outcome`): the init proof (E14 fields present; tools within
Read/Grep/Glob/StructuredOutput, no MCP server, `dontAsk` - class permission; apiKeySource per auth
- class auth), the exit code with the quota/auth wording, a malformed stream (transport), the
session checks (a minted id, a resume on its thread, a fork on a NEW session - class unknown), the
result's subtype (`error_max_turns` capability), the model proof (D4 drift, E13 every assistant
message by the init's model), denials (`DeniedEmpty` - the denial retry), a rejecting
`rate_limit_event` (quota with its reset; E15 beside a success: usable + warning +
`engine_run.quota_mark`). E12: a killed turn's init and model are judged too - `failed: <problem>
(the turn was also stopped: timeout after N s (process tree killed))`, no continuation (`not
attempted: the killed turn failed its proof (class <c>: <problem>)`), the salvage kept; a killed
turn after a rejecting event is class quota and not continued. The strict tree check fails a run
that changed the tree (`... - claude ran with --restricted and read tools only, but managed
settings and their hooks still apply`). The ledger `engine_run` adds `auth, init_tools,
mcp_servers, permission_mode, api_key_source, model_resolved, other_models, permission_denials,
denied_tools, rate_limit, quota_mark, cost_usd, child_env_allowed, switched_off`; the machine-wide
`ok` record carries the quota mark (`new_machine_health_record_marked`). Telemetry needs nothing
new: the closed classes of wave 2 already map engine claude to `anthropic` and an endpoint's host
to its vendor (`zai`, `xiaomi`, `moonshot`, `minimax`).

## 4c - the plan wait and the panel seat

The claude members of a panel share ONE scheduling group (`Runner.scope = engine:claude`; D5) -
`"parallel": {"<label>": n}` raises it; a `plan` joins the plan's codex and claude routes in one
group (wave 1b's `plan:<slug>` key) and a member waits for a run of its plan elsewhere on the
machine (E16, the machinery of wave 1b - the claude endpoint entries now name the plan). The
coordinator rule (item 9, `coordinator_reviewer_warning_auth`): a claude reviewer is compared with
`anthropic` (any case) whatever its label, the models after normalising (`[1m]` stripped, an alias
equal to any id of its family); an endpoint entry is compared as a codex entry; after the run the
resolved model is compared again. Lab: `claude`/`opus`/`sonnet`/`haiku`/`fable` -> anthropic, an
endpoint entry by its base URL's host.

## 4d - tests and docs

`crates/c3-cli/tests/claude_engine.rs` (9 tests) drives the real binary against the plugin's own
fake CLI (`tests/fixtures/fake-claude.{cmd,ps1}`, copied from v0.6.1) with every real `claude` off
PATH; unit tests in `c3_core::claude`, `engines::claude` and `engine.rs`. README "The `claude`
engine", `plugin/skills/setup-providers/SKILL.md` section 4c.

## 4f - the diff-review fixes (F25-1..F25-4, 2026-10-09)

Branch `wave4f-claude-fixes`. MiMo's diff review of wave 4 (handoff 25 of
`parity-0.6.1-2026-10-08`, HOLD: two blockers, two minors). On F25-1 and F25-2 the plugin at v0.6.1
has the gap the reviewer names: its `Get-ClaudeSignIn` for auth endpoint (E3) checks only that the
launcher was FOUND, the endpoint parses and the token variable is set ("NO `claude auth status` (it
reads the local login and ignores the base URL - P12) and no live request"), and
`Get-ClaudeLaunchProblem` reads the projectsDirectory only from the sign-in cache, which auth
endpoint never fills. The reviewer's literal remedy - `claude auth status` on the endpoint route -
would break E3/P12 and three harness-claude ENDPOINT checks (the dry run, the end-to-end run and
the listing require NO `auth` start on that route), so C3 closes both without it:

- **F25-1 (blocker) - the endpoint preflight proves that the launcher runs.** After the local
  checks, auth endpoint takes the launcher's `--version` probe - the one `Get-ClaudeHarness` runs
  for the harness string anyway (`version_probe`, cached per launcher: a dry run still starts
  exactly one `version` process; in the child environment of auth subscription, as the plugin's
  harness probe, so no token reaches a launcher not yet proven). Not started -> ``missing: the claude
  launcher does not run - `claude --version` could not be started (<os error>)``; a non-zero exit
  -> `missing: ... exited <n>`; no exit within 15 s -> ``unknown: not checked - `claude --version`
  did not finish within 15 s`` (as a hung `claude auth status`). Success keeps the plugin's `ok: env
  <NAME> set`; `-NoNetwork` (the SessionStart hook) starts nothing and keeps the local answer. A
  nonexistent `--engine-exe` was already refused at the resolution (`... is not a file and not an
  application on PATH.`). Tests: `engines::claude_auth::tests::the_launcher_verdict_of_the_version_probe`,
  `engines::claude_auth::tests::an_endpoint_launcher_that_does_not_run_is_unavailable` (a text file
  named `.exe`, a launcher whose `--version` exits 3, a runnable one; `-NoNetwork`) and
  `claude_engine::an_endpoint_launcher_that_does_not_run_is_unavailable_before_any_turn` (the real
  binary: `--engine-exe` naming a file that is no program -> `preflight   : unavailable (the claude
  launcher does not run - ...) - a real run is refused`; the roster walk -> `no reviewer of the
  roster ... is available; nothing was started: #1 ZAI-claude :: glm-5.3 [claude] (missing: ...)`,
  exit 1, the fake never started; `CODEX_CONSULT_CLAUDE_EXE` a `.cmd` exiting 3 -> unavailable; the
  fake -> available, one `version` start, no `auth` start).
- **F25-2 (blocker) - the transcript guard in every auth mode.** `projects_directory`: the
  projectsDirectory `claude auth status` reported when it ran, else the one Claude Code derives -
  `<CLAUDE_CONFIG_DIR>/projects`, or `<home>/.claude/projects` (home `USERPROFILE`, else `HOME`, as
  node's `os.homedir()`) - so auth endpoint, a preflight answered by a usable reply within 60
  minutes and `--skip-preflight` are guarded too. `path_inside` decides lexically first (as
  `Get-RepoRelativePath`), then with the links resolved (the deepest existing ancestor and the root
  canonicalised), so a junction or a symbolic link into the repository is caught. The refusals are
  the plugin's texts verbatim (`the claude projectsDirectory (<dir>) lies inside the repository
  under review: the engine's transcripts would change the tree`; `CLAUDE_CONFIG_DIR (<dir>) lies
  inside ...; point it elsewhere`). Tests:
  `engines::claude_auth::tests::the_projects_directory_inside_the_repository_is_refused_in_every_auth_mode`
  (reported; derived from the home; `CLAUDE_CONFIG_DIR` outside with `projects` a junction inside;
  outside allowed) and `claude_engine::endpoint_transcripts_that_would_land_in_the_repository_are_refused`
  (the real binary on the endpoint route: `CLAUDE_CONFIG_DIR` outside, its `projects` a junction
  into the repository -> the dry run's `a real run is refused: <the refusal>`, the run's `the claude
  engine is refused: <the refusal>; nothing was started.`, exit 1, no turn, no `auth` start; a
  plain `CLAUDE_CONFIG_DIR` outside -> a usable run).
- **F25-3 (minor) - the model table's case: the plugin's rule, kept.** `Get-ClaudeModelProblem`
  looks the model up with `-cnotcontains` (case-sensitive) after `-replace '\[1m\]$'`
  (case-insensitive): the plugin refuses `OPUS` and `CLAUDE-HAIKU-5-5` too and takes `opus[1M]`;
  the lower-casing helpers (`ConvertTo-ClaudeModelBase`, the family and match rules) compare what
  the CLI served. C3 already did exactly this; the rule is now written on `model_problem`, in the
  README and in the setup skill (`spelled exactly so`). Test:
  `c3_core::claude::tests::the_model_table_is_case_sensitive_as_the_plugins` (with the endpoint side:
  `GLM-5.3` taken, `CLAUDE-HAIKU-5-5` refused as an Anthropic id, `glm-5.3[1M]` refused by the
  case-sensitive pattern).
- **F25-4 (minor) - `ChildEnv`'s `Debug` redacts.** `Debug` by hand: every variable by name, every
  value `<redacted>` (`auth`, `names`, `removed` and `problem` hold names only and print as they
  are). Test: `c3_core::claude::tests::the_child_environment_debug_never_prints_a_value` (`{:?}` and
  `{:#?}` of an endpoint environment carry neither the token nor any other value). No other type of
  the crates holds a credential value under `Debug`/`Display`: `SpawnRequest` (which borrows the
  environment) derives neither, `HttpConfig` holds the key's variable NAME and never an
  `Authorization` header, the endpoint object holds `env_key` (a name).

Verification (2026-10-09, the 4f binary): `cargo test --workspace -j 2 --no-fail-fast` 696 passed
(7 new: 3 in `engines::claude_auth`, 2 in `c3_core::claude`, 2 in `claude_engine`; two index tests -
`embed_reaches_a_loopback_fake_via_localhost`, `rebuild_then_embed_reproduces_the_vector_count` -
failed once under a concurrent build's load and passed on the rerun; untouched code), `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo fmt --check` clean. Through the shim (pinned
v0.6.1, staged per `harness-shim.md` section 4, one at a time under `HARNESS.lock`, Windows
PowerShell 5.1):

| harness | wave 4 | 4f | note |
|---|---|---|---|
| claude | 87 / 0 | **87 / 0** | ENDPOINT green: still no `auth` start on the route, one `version` start in the dry run |
| roster | 125 / 0 | **125 / 0** | the claude model texts unchanged (the case rule was already the plugin's) |

## 4g - the second-round fixes (F32-1, F32-2, 2026-10-09)

Branch `wave4g-claude-r2`. MiMo's second round on wave 4 (handoff 32 of `parity-0.6.1-2026-10-08`,
HOLD: one blocker, one major; F25-1..F25-4 fixed). Both narrow the 4f fixes; neither touches the
plugin's E3 rule - still no `claude auth status` on the endpoint route.

- **F32-1 (major) - the endpoint preflight probes the launcher as the endpoint turn starts it.**
  The `--version` probe of an auth endpoint entry runs in the endpoint TURN's child environment
  minus its token (`c3_core::claude::probe_environment`, `engines::claude_auth::probe_env`).
  Passed: the allow list, `DISABLE_AUTOUPDATER=1`, the roster's `ANTHROPIC_BASE_URL` and
  `API_TIMEOUT_MS`, exactly as the turn gets them - the base URL is what makes the turn an endpoint
  turn, so a wrapper that dispatches on it must answer under it. NOT passed: `ANTHROPIC_AUTH_TOKEN`
  (for a probe the token variable is not even read - no credential reaches a launcher not yet
  proven) and, as in every child, nothing outside the allow list (no parent `ANTHROPIC_*`). The
  other auth modes keep the plugin's harness probe in auth subscription's environment (so no
  `ANTHROPIC_API_KEY` either). The probe starts the launcher the turn starts, by the turn's
  program-and-argument path (`subprocess::apply_launcher_args_quoted`, CRT quoting for a batch
  file). A launcher whose probe fails there is unavailable (``missing: the claude launcher does not
  run - `claude --version` exited <n>``); one that answers only there is available. The probe is
  cached per launcher AND probe environment (`endpoint|<base_url>|<timeout_ms>` or
  `subscription`), and an entry's harness string comes from its own probe environment
  (`providers::engine_harness` takes the entry's auth and endpoint; `orchestrate` computes the
  harness after the auth), so the endpoint preflight and the harness string share ONE `--version`
  start - the E3 dry run still logs exactly one. Not covered, deliberately: a launcher that answers
  `--version` but rejects the `-p` turn (proving the turn interface takes a turn, i.e. a request -
  such a launcher fails its first turn, as in the plugin); a launcher whose `--version` needs the
  token is classed unavailable (a probe never carries one). Tests:
  `c3_core::claude::tests::the_probe_environment_is_the_endpoint_turns_without_its_token` (the
  route's two variables, not the parent's; no token, no parent token, no `ANTHROPIC_API_KEY`; the
  turn's names minus `ANTHROPIC_AUTH_TOKEN`; subscription and api-key probe in auth subscription's
  environment), `engines::claude_auth::tests::the_endpoint_launcher_is_probed_in_the_endpoint_environment`
  (a launcher answering only WITHOUT `ANTHROPIC_BASE_URL` -> `exited 4` on the endpoint route,
  which auth subscription's environment - the 4f probe's - passes; one answering only WITH the
  route's variables and WITHOUT a token -> available, harness `claude-cli 2.1.0-w4g`, which auth
  subscription's environment rejects with `exited 5`) and
  `claude_engine::the_endpoint_preflight_probes_the_launcher_in_the_endpoint_turns_environment` (the
  real binary: the first launcher -> ``preflight   : unavailable (the claude launcher does not run -
  `claude --version` exited 4) - a real run is refused``, the walk's ``... [claude] (missing: ...
  exited 4)``, exit 1, only `--version` starts; the second -> available, `harness     : claude-cli
  2.1.0-w4g`, one start; the fake's version start carries `base_url` = the roster's,
  `api_timeout_ms` 3000000 and `has_token` false although the parent sets `ANTHROPIC_BASE_URL` and
  `ANTHROPIC_AUTH_TOKEN`).
- **F32-2 (blocker) - the transcript location as Claude Code derives it, fail-closed.** Claude
  Code (its 2.1.29x bundle) writes under `(CLAUDE_CONFIG_DIR ?? join(os.homedir(),
  ".claude")).normalize("NFC")` + `/projects`. `os.homedir()` is libuv's `uv_os_homedir` under
  node and Bun alike: on Windows `USERPROFILE`, else the account's profile directory from the
  system - `HOME`, `HOMEDRIVE` and `HOMEPATH` are NOT read, and a blank `USERPROFILE` fails the
  lookup (checked on this machine with node 24.13 and Bun 1.4: `USERPROFILE` unset with `HOME`,
  `HOMEDRIVE`, `HOMEPATH` pointing elsewhere -> `C:\Users\<name>`; blank -> `ENOENT,
  uv_os_homedir`); elsewhere `HOME`, else the user database. The plugin assumes nothing here
  (`Get-ClaudeLaunchProblem` checks `CLAUDE_CONFIG_DIR` and the projectsDirectory `claude auth
  status` reported, no home). C3's `projects_directory`: the reported projectsDirectory when `claude
  auth status` ran; else, from the CHILD's environment (the allow list passes `CLAUDE_CONFIG_DIR`,
  `USERPROFILE` and `HOME` unchanged in every auth mode), `CLAUDE_CONFIG_DIR` when set, else
  `<home>/.claude` with the home variable of the platform (`home_var`: `USERPROFILE` on Windows,
  `HOME` elsewhere - the 4f fallback to the other one is gone, Claude Code never reads it); a
  relative value against the child's working directory (the repository root), `.` and `..` folded as
  `path.join` folds them. It cannot be established - and the run is refused with `the transcript
  location cannot be established (<why>); set CLAUDE_CONFIG_DIR to a directory outside the
  repository` (the dry run: `a real run is refused: ...`; the run: `the claude engine is refused:
  ...; nothing was started.`) - when `CLAUDE_CONFIG_DIR` is set but blank (`??` takes it as it is:
  relative to the repository), when neither it nor the home variable is set (`neither
  CLAUDE_CONFIG_DIR nor USERPROFILE is set: Claude Code would fall back to the account's profile
  directory from the system, which c3 does not resolve`; elsewhere `... nor HOME ... the account's
  home from the user database ...`), or when the home variable is blank. The home inside the
  repository keeps the plugin's text (`the claude projectsDirectory (<home>\.claude\projects) lies
  inside ...`). Not covered: a wrapper launcher that sets `CLAUDE_CONFIG_DIR` inside itself - C3
  cannot see into it; the strict tree check fails such a run after its turn. Tests:
  `engines::claude_auth::tests::the_transcript_location_that_cannot_be_established_is_refused` (the
  Windows rule: no `CLAUDE_CONFIG_DIR`/`USERPROFILE` -> refused whatever `HOME`, `HOMEDRIVE`,
  `HOMEPATH` say, inside the repository or not; the Unix rule: no `HOME` -> refused whatever
  `USERPROFILE` says; blank `USERPROFILE`, blank `CLAUDE_CONFIG_DIR` -> refused; `USERPROFILE` and
  `UserProfile` (Windows) and `HOME` (Unix) inside -> the plugin's text; the variable the platform
  does not read changes nothing; `CLAUDE_CONFIG_DIR` set or a reported projectsDirectory needs no
  home; a relative `CLAUDE_CONFIG_DIR` against the repository root, `..` folded),
  `engines::claude_auth::tests::the_projects_directory_inside_the_repository_is_refused_in_every_auth_mode`
  (4f, on the new inputs; the junction kept),
  `claude_engine::a_transcript_location_that_cannot_be_established_refuses_the_endpoint_run` (the
  real binary, RC2: `CLAUDE_CONFIG_DIR`, `USERPROFILE` and `HOME` removed, `HOMEDRIVE`/`HOMEPATH`
  pointing into the repository -> the dry run and the run refused with the text above, exit 1, no
  turn, no `auth` start; `USERPROFILE` inside -> the plugin's text; `USERPROFILE` outside with `HOME`
  inside -> a usable run) and `claude_engine::endpoint_transcripts_that_would_land_in_the_repository_are_refused`
  (4f, the junction, unchanged).

Besides `claude_auth.rs` and `c3-core`'s `claude.rs`, the shared probe needs its two call sites:
`providers::engine_harness` (the entry's auth and endpoint) and `consult::orchestrate` (the
harness computed after the auth).

Verification (2026-10-09, the 4g binary): `cargo test --workspace -j 2 --no-fail-fast` 720 passed,
1 failed - `compat_wave2b::a_stall_cut_names_the_open_tool_call` (a 3 s stall cut under the load of
concurrent builds; untouched code), green on the rerun; 5 new tests (1 in `c3_core::claude`, 2 in
`engines::claude_auth`, 2 in `claude_engine`). An earlier full run had the two new `claude_engine`
tests fail against ANOTHER worktree's `target\debug\c3.exe` (the shared target directory: that
binary carried none of the 4g texts) and the timing test `notspooled_parity::f24_1_...` skip on a
busy spool lock; both files 14/14 and 18/18 on this binary. `cargo clippy --workspace --all-targets
-- -D warnings` and `cargo fmt --check` clean. Through the shim (pinned v0.6.1, staged per
`harness-shim.md` section 4, under `HARNESS.lock`, Windows PowerShell 5.1 with `PSModulePath`
reset):

| harness | 4f | 4g | note |
|---|---|---|---|
| claude | 87 / 0 | **87 / 0** | ENDPOINT green: no `auth` start on the route; the E3 dry run's one `version` start is now the endpoint-environment probe the harness string shares |

## Harnesses through the shim (pinned v0.6.1)

Plugin pinned at v0.6.1, the tree staged per `harness-shim.md` section 4, one harness at a time
under `%TEMP%\codex-consult-tests\HARNESS.lock`, Windows PowerShell 5.1 (2026-10-09). The claude,
roster, engines, muse and panel rows ran on the wave 4b binary (`1ca23d6`); claude and telemetry ran
again on the binary with main merged (`115f015`, wave 3b and 2g in), with the same results.

| harness | before (RC2 / last recorded) | after | note |
|---|---|---|---|
| claude | 17 / 5, crash at `unknown engine 'claude'` | **87 / 0** | every category green: UNIT, ROSTER, DRYRUN, ENGINEEXE, RUN, BILLING, PREFLIGHT, FAIL, TOOLSET, TREE, RESUME, REPAIR, TIMEOUT, STALL, PANEL, LISTING, ENDPOINT, ACCEPT |
| roster | 124 / 1 | **125 / 0** | FILE green (the claude auth and model texts, the engine list naming claude) |
| engines | 95 / 2 | **97 / 0** | ROSTER refused and DRYRUN refused name claude |
| muse | 69 / 5 | **73 / 1** | the claude-wording rows green (ROSTER, DRYRUN one message each, ENGINEEXE two engines, PANEL -MaxModelSteps) |
| panel | 62 / 0 | 62 / 0 | no regression |
| telemetry | 47 / 96 (main after wave 3b) | 47 / 96 | the same 96 checks; FORGET D3 below |

Remaining failures, classified:

- **muse UNIT D2 (every engine)** - shim artifact: a code grep over `codex-consult.ps1`'s source
  (`$engineSpec.Adapter`, the Argv/Events/Outcome calls), which is the C3 shim here. C3's
  equivalent is the engine dispatch in `consult/orchestrate.rs` (`engine_turn` and the `match ctx.engine` arms).
- **telemetry FORGET D3** - shim artifact: the consult shim forces `CODEX_CONSULT_TELEMETRY=off`
  when the caller leaves it unset (its safety net), so the run spools nothing and prints no
  telemetry line (`run exit 0 nosalt True | | forget exit 0`); the reason text also names
  `c3 telemetry --forget --local` (wave 3b's decision), not `codex-telemetry.ps1 -Forget -Local`.
  The wording this wave owed - the run's line ends `- dropped`, no longer `- counted (c3
  telemetry --status)` - is proven by `claude_engine::an_event_refused_by_a_living_forget_is_said_dropped`
  (a living marker of the test process: exit 0, the entry committed, the marker kept).
- **telemetry, the other 95** - unchanged from main (wave 3b's table: plugin path by design (P7),
  COMPLAIN x10, DOCS, DRYRUN).

Live smoke (the real Claude Code 2.1.294 on the claude.ai subscription, a scratch repository and a
scratch roster `[{"provider":"anthropic","engine":"claude","model":"haiku","auth":"subscription"}]`,
telemetry off, a scratch machine-health file): the dry run showed `preflight   : available (ok:
signed in (claude.ai subscription))`, `harness     : claude-cli 2.1.294`, the allow-listed child
environment and `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config
--disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model haiku --effort low
--session-id <uuid>` (chore is raw: no `--json-schema`). One real `--purpose chore --max-words 50`
consultation: usable reply in 6.3 s (8.9 s with the process start), `Engine turns: 1 (claude -p, auth
subscription; model claude-haiku-5-5; init tools Glob, Grep, Read; permission denials 0)`, a seven-day
`allowed_warning` rate-limit warning (the plugin's wording), `cost_usd` 0.0008; the ledger's
reviewer `{provider anthropic, model haiku, engine claude, harness claude-cli 2.1.294,
provider_config {engine claude, launcher, credential_mechanism subscription, auth_method claude.ai,
api_provider firstParty}}` and `engine_run {auth subscription, model_resolved claude-haiku-5-5,
mcp_servers 0, permission_mode dontAsk, api_key_source none, quota_mark null}`. No endpoint-mode
live run (keys).

## What differs from the plugin, and why

- **The not-spooled run warning.** C3 says `warning    : telemetry event not spooled (<why>) -
  dropped` for every event that did not reach the spool: it does not retry after the commit. The
  plugin says `- dropped` only for a forgetting-marker refusal and otherwise retries for 5 s and
  says `- at the commit (<why>) and for 5 s after it`. The not-spooled count is kept either way.
- **The forgetting reason** names `c3 telemetry --forget --local` (wave 3b), so the plugin's
  harness regex on `codex-telemetry.ps1 -Forget -Local is deleting` cannot match a C3 run.
- **The endpoint preflight runs `claude --version` (4f, F25-1; 4g, F32-1).** The plugin reports
  an endpoint entry available once its launcher is found and its token variable set; C3 also
  requires the launcher to run - in the endpoint turn's environment minus its token, the probe its
  harness string comes from too (the plugin probes the harness string in auth subscription's
  environment for every entry) - so a wrong `--engine-exe`, a configured launcher that does not run
  or one that fails under the route's variables is unavailable before any turn instead of failing
  at the spawn.
- **The transcript guard falls back to the derived projects directory and resolves links (4f,
  F25-2).** The plugin checks the projectsDirectory only when `claude auth status` ran (never on
  the endpoint route, nor after the 60-minute short-circuit or `-SkipPreflight`) and compares
  lexically; C3 derives `<CLAUDE_CONFIG_DIR or ~/.claude>/projects` when no status reported one and
  catches a junction or symbolic link into the repository. Both only add refusals the strict tree
  check would otherwise turn into a failed run. (4g, F32-2) The derivation reads the child's
  environment with Claude Code's own rule (`USERPROFILE` on Windows, `HOME` elsewhere) and is
  fail-closed: a location it cannot establish (no `CLAUDE_CONFIG_DIR` and no home variable, or a
  blank one) refuses the run - the plugin, checking no derived directory, starts it.
- **Native Messages API** - not here (decision P4): Claude Code is spawned exactly as the plugin
  spawns it; a native route is an improvement candidate for a later wave.
- Otherwise no deliberate difference: the argv, the child allow list, the init and model proofs,
  the killed-turn rules, the quota mark, the roster texts and the providers row are the plugin's,
  as the five harnesses above check through the shim.
