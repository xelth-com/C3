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
  not set`), auth endpoint `ok: env <NAME> set` with no probe; a usable reply within 60 minutes;
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
- **Native Messages API** - not here (decision P4): Claude Code is spawned exactly as the plugin
  spawns it; a native route is an improvement candidate for a later wave.
- Otherwise no deliberate difference: the argv, the child allow list, the init and model proofs,
  the killed-turn rules, the quota mark, the roster texts and the providers row are the plugin's,
  as the five harnesses above check through the shim.
