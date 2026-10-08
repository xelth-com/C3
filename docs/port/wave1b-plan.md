# Wave 1b of the 0.6.1 parity: the roster `plan` (2026-10-08)

Finding F02-7 of the parity framing (`.collab/parity-0.6.1-2026-10-08`, decision P2): the plugin's
roster key `plan` is a cross-engine compatibility dependency, not a claude-engine option - and C3's
roster validator refused it, so every 0.6 plugin roster that names a plan refused every C3 run.
This wave ports the key and the machine-wide plan infrastructure as SHARED code (any engine: codex,
agy, muse, http), from the plugin at tag `v0.6.1`: `CHANGELOG.md` `## [0.6.0]` (wave 29b E1, E5,
E7, E15, E16), `codex-consult-common.ps1` (`Read-ReviewerRoster`, `Get-EndpointHealth
-Fingerprints`, `Get-PlanQuota`, `Get-PlanQuotaVerdict`, `ConvertTo-AvailabilityRecord`,
`Get-EndpointGroups -Scheduling`, `Get-PanelPlan`, `Register-MachineRunning`,
`Get-MachineRunningCount`, `ConvertTo-MachineHealthEntries`, `Update-MachineHealth`),
`codex-consult.ps1` (the direct-run plan check, the panel scheduler's wait) and
`codex-providers.ps1` (the rows' plan verdicts). The claude engine itself (auth modes, the
`endpoint` block, the rate-limit event that writes a quota mark) is wave 4.

## A. The roster key (E1, E5, E7)

Module `crates/c3-core/src/roster.rs` (`validate_roster`), `roster_ext.rs` (`ext.c3.reviewers`).

- **`plan`** on an entry of ANY engine: a slug `^[a-z][a-z0-9-]{1,31}$` (`PLAN_SLUG_RE`, matched
  case-sensitively; `is_plan_slug`), kept as `RosterEntry.plan` (`""` without one). Refusal, the
  plugin's text: `entry <n>: plan must be a slug of 2 to 32 characters - lowercase letters, digits
  and "-", starting with a letter (e.g. "zai"; got <the value as compact JSON>)`.
- **The key list** is the plugin's, in its order: `entry <n> has an unknown key '<k>' (allowed:
  provider, model, codex_config, auth, endpoint, plan, panel, engine, lab, roles, timeout_sec,
  stall_sec, context_tokens, ext)` - unknown keys stay fail-closed.
- **`endpoint`** is accepted by name and refused on every entry C3 runs, with the plugin's text:
  `entry <n>: endpoint applies only to engine claude with auth "endpoint" (this entry: engine
  <engine>)` (the claude engine with auth `endpoint` is wave 4; until then C3's engine check refuses
  `engine: "claude"` before it).
- **The per-entry order** of the checks is now the plugin's: the delimiter rule, `timeout_sec`,
  `context_tokens`, `stall_sec`, `ext`, `plan`, `lab`, `roles`, `provider`, ... (before this wave C3
  checked the three numeric keys after the engine; only a roster with several bad keys sees a
  different first refusal).
- **`parallel`** may name a plan: `parallel names the provider label '<k>', which no entry of the
  roster uses (as its provider label or its plan)`.
- **http routes** (C3's `ext.c3.reviewers`, invisible to the plugin) take the same `plan` with the
  same rule and wording (`ext.c3.reviewers entry <n>: plan must be a slug ...`), so an OpenRouter or
  other native route that spends a plan's quota joins the plan.

## B. Quota propagation (E5; E15's read side)

Modules `crates/c3-core/src/plan.rs` (new, pure), `health.rs`, `verdict.rs`, `availability.rs`;
wiring in `crates/c3/src/providers.rs` (`Ctx::plan_quota`, `Ctx::plan_verdict`) and
`consult/orchestrate.rs` (`resolve_preflight`).

- **One record set.** `endpoint_health_set(consults, fingerprints, now)` reads the records of
  several fingerprints as one set; every `Record` carries its `fingerprint`. `plan_quota` takes the
  plan's routes - the resolved identities of the roster's entries naming the plan, one per
  fingerprint, the first entry's label (`plan_routes`) - over the task ledgers PLUS the
  machine-wide file's records (so another repository's limit counts), at the consult clock.
- **The rule.** When the set's newest {usable reply, quota failure} is a quota failure that still
  blocks (a reset ahead, or without one 60 minutes, 10 for a burst), every OTHER route of the plan
  is out until that time. Auth, transport and capability failures stay route-local; a usable reply
  on any route of the plan after the limit clears the plan (not the failed route's own limit);
  without a plan nothing propagates.
- **The verdict** (`plan_quota_verdict`, applied after the entry's own preflight verdict, only when
  that is `available` or `unknown`/`unknown` - sign-in not checked; never on the route that
  recorded the failure, whose own verdict says it):
  - reason `plan <p> (usage limit on <label> until <iso>)`, kind `quota`;
  - without a reset: `plan <p> (usage limit on <label> hit <iso>, reset unknown; retry after
    <iso>)` (a burst: `burst limit (429) on ...`), kind `quota-unknown-reset`;
  - refusal `provider <P> is not usable: its plan <p> hit a usage limit on <label> at <when>
    (<message>) that lasts until <iso>; nothing was started` / `... hit a usage|burst limit on
    <label> at <hit> (<message>) and named no reset time - out until <iso>; nothing was started`,
    plus ` (pass -SkipPreflight to launch anyway)` on a direct run (not in a roster walk);
  - `<label>` is the provider label of the route the record was recorded on, `another route` when
    no roster entry of the plan has that fingerprint; `PreflightVerdict.plan_quota = {plan,
    label}`, `burst` set.
- **Where it applies** (as in the plugin): the single-run roster walk (`walk_full`, skip reasons in
  the ledger `roster.skipped`), the panel selection (`panel_members`, skip kind `unavailable`), the
  availability view (`roster_availability`: `c3 providers -Short`, the hook line; the D16 group
  marking then spreads it over the entry's endpoint group), the `c3 providers` rows (a codex row by
  the plan of the label's first codex entry that names one; an engine row by its label's first
  entry), the `roster: ... would select` line, and a direct run (`-Provider` or `-Thread` with a
  roster entry, a panel member's own preflight): refused before anything starts, exit 1; the dry
  run reports it on `preflight :`.
- **The one-line view** (`convert_to_availability_record`): `plan <p> (usage limit on <label> until
  <local>, in <rel>)`, without a reset `plan <p> (limit|burst limit hit on <label> <local>, reset
  unknown; retry after <local>, in <rel>)`; e.g. `codex-consult: out - ZAI :: glm-5.3 (until ...),
  ZAIB :: glm-5.3 (plan zai (usage limit on ZAI until ...)); 1 of 3 reviewers available`.
- **Quota marks (E15, read side and file keeping).** A usable reply whose ledger entry carries
  `engine_run.quota_mark` (class `quota`) also counts as that quota failure 1 ms after its
  completion (`bridge_outcome "failed: quota (a rate limit rejected a request during a usable
  turn)"`), so the route - and its plan - stays out until the mark's reset. The machine-wide file's
  `ok` record keeps its `quota_mark` when C3 rewrites the file, stays while the mark's `until` is
  ahead (the prune rule), and is read back as `engine_run.quota_mark`. C3 writes no mark yet (the
  claude engine's rate-limit event, wave 4).
- **Side fixes found on the way** (they are part of the plan texts): a reset-less burst's verdict
  reason now reads `burst limit (429) hit <iso>, reset unknown; retry after <iso>` and its one-line
  phrase `burst limit hit <local>, ...` (C3 said `usage limit hit` / `limit hit` for both); a
  machine-wide record's stored `until` (and a quota mark's) is its `until` (wave 26c D2's
  tie-break, `ConvertTo-MachineHealthEntries` carries it).

## C. The scheduling group (E7)

Module `crates/c3/src/panel/plan.rs` (`endpoint_groups`, `panel_plan`, `Runner.plan`).

- A panel member whose roster entry names a plan adds the key `plan:<p>` to its label's group keys,
  so the members of one plan share ONE concurrency group across engines and routes (the
  availability view keeps its groups by endpoint: only a quota failure propagates over a plan).
- The group's limit: a label's own `parallel` value, else the smallest of its plans' values; then
  never above any of its plans' values (default 1). So `"parallel": {"zai": 2}` lets two routes of
  plan zai run at once; raising both labels but not the plan keeps them one after another; without
  the plan two endpoints run at once. The dry run's `Concurrency:` line and the detached budget
  follow.

## D. The machine-wide running record and the plan wait (E16)

Modules `crates/c3-core/src/health.rs` (`MachineRunning.plan`, `machine_running_count`,
`MachineRunningCount`), `plan.rs` (`format_machine_wait`), `crates/c3/src/consult/orchestrate.rs`
(`Context.plan`, the registration), `crates/c3/src/panel/run.rs` (the scheduler).

- **The file**: ONE per machine, `<codex home>/codex-consult-health.json`
  (`CODEX_CONSULT_HEALTH=<path>` names another, `none` disables it), written under
  `<file>.lock`, shared with the plugin byte-for-byte (the PowerShell 5.1 JSON format):
  `{ "health_version": 1, "endpoints": [ { "endpoint", "class", "kind", "until", "retry_after",
  "repo", "when", "message"[, "quota_mark": {class, kind, until, retry_after, message, when}] } ],
  "running": [ { "endpoint", "label", "pid", "start_time", "repo", "task", "nn", "panel",
  "since"[, "plan"] } ] }`.
- **The running row** of every run while its engine turn runs (codex, agy, muse and the http seat):
  `endpoint` = the route's fingerprint, `label`, `pid` + `start_time` = the BRIDGE process (`c3`;
  the start time in the plugin's UTC round-trip form), `repo`, `task`, `nn`, `panel` (the panel id
  or `""`), `since`, and - when the run's roster entry names one - `plan` (absent otherwise, as the
  plugin writes it). Removed by the run after its turn (`unregister_machine_running`, also on a
  failed start); a row whose pid + start time is gone (the process died, the pid was reused) is
  pruned by every write and never counted (`pid_alive`).
- **The wait**: before a panel member starts, the live rows OUTSIDE its panel whose endpoint is one
  of its group's fingerprints OR whose plan is one of its group's plans (any engine, route or
  repository) count against the group's limit with the members running; at the limit it waits
  (polling) and prints once:
  `  panel member <k> of <n> waits: <c> run(s) elsewhere on this machine use its plan zai
  (parallel limit 1): ZAI in <repo> task t handoff 01 (plan zai, pid 4242)` - `its endpoint` when
  every counted row matches by endpoint, `its plan <p>[, <q>]` when every one counts only through
  its plan, `its endpoint or its plan <p>` when mixed; `(plan <p>, ` marks the rows that count only
  through their plan. A single run (no panel) never waits, as in the plugin; a dry run never
  waits. `-Status` and the dry run print no plan field of their own for codex/agy/muse runs (the
  plugin's `plan ...` dry-run line belongs to the claude endpoint block, wave 4).

## Tests

Rust (`cargo test --workspace -j 2`: 551 passed, 0 failed - 535 before + 16 new):
`roster::tests::{plan_is_a_slug_on_any_engine, the_unknown_key_list_and_the_endpoint_key_are_the_plugins, parallel_may_name_a_plan}`,
`roster_ext::tests::an_http_reviewer_may_name_a_plan`,
`plan::tests` (7: routes, propagation one way and both ways, quota only, the reset-less and burst
texts, the eligible verdicts, a quota mark, the three wait-line forms),
`health::machine_health_tests::{running_rows_carry_the_plan_and_count_by_plan, a_quota_mark_survives_a_rewrite_and_is_read}`,
`providers::roster_walk_tests::{a_usage_limit_on_one_route_marks_every_route_of_its_plan_out, the_plan_propagates_both_ways_and_not_without_a_plan}`,
`panel::plan::tests::a_plan_is_one_scheduling_group_across_routes`, and the burst / plan one-line
phrases in `tests/formats.rs::availability_record_out_quota_short`. `cargo clippy --workspace
--all-targets -- -D warnings` clean, `cargo fmt --all -- --check` clean.

## Measurements

Scripts directory `%TEMP%\c3-wave1b\scripts`: the five shims of `tests/shim/` + the plugin's
`codex-consult-common.ps1` and `codex-consult-detached.ps1` and `schemas/consult-reply.schema.json`
from tag `v0.6.1` (`git show`, byte-identical to the RC2 scripts directory), `C3_EXE` = a copy of
this branch's debug `c3.exe`; each harness through `tests/run-all.ps1 -ScriptsDir <dir> -Only
<harness>` under pwsh 7.6.6, ONE AT A TIME, holding `%TEMP%\codex-consult-tests\HARNESS.lock`.
"main" is `bd96c44` (wave 1 merged) as the parallel RC2 run measured it on this machine the same
hour (`%TEMP%\c3-rc2`, same scripts, same host), so every remaining failure is classed by evidence.

| Harness (0.6.1) | main | wave 1b | Remaining failures - all identical on main |
|---|---|---|---|
| harness-roster | 118 / 7 | **118 / 7** | FILE (`claudeauth`, `claudemodel`: the claude engine's roster keys - wave 4); WALK (the 0.6.x `context_window` ledger field order); TIMEONLY x3 (a pwsh-7 host artifact: the harness reads the ledger with `ConvertFrom-Json`, which turns the ISO `retry_after` into a `DateTime` whose `[string]` never equals the ISO text - wave 1's 121/4 was measured on another host; the Rust tests carry the 69 samples); RATE x2 (0.6.1 F06-1/F06-2 rating fields - wave 2). No harness-roster check names `plan`. |
| harness-panel | 60 / 2 | **60 / 2** | SPEC x2 (a member's panel run dying during its preflight; F07-1/F11-6) - outside this wave. |
| harness-lock2 | 11 / 0 | **11 / 0** | - |
| harness-claude | 17 / 5, stops | **17 / 5, stops** | DRYRUN x4, ENGINEEXE: `unknown engine 'claude'` - wave 4; the harness stops there (no summary line), so none of its plan checks is reached. |
| harness-visibility (extra: the burst wording) | - | 34 / 0, stops | stops at UNIT24C (`Invoke-EngineTurn`, a function of the plugin's real `codex-consult.ps1`, which a shim directory does not hold) before its BURST rows; the burst texts are covered by the Rust tests. (The RC2 run lost its scripts directory's `codex-consult.ps1` before reaching it.) |
| **plan scenario** (`tests/scenarios/plan-wave1b.ps1`, this wave) | - | **11 / 0** | - |

The plan scenario (`pwsh -File tests/scenarios/plan-wave1b.ps1 -Scripts <the scripts dir> -Fake
<plugin>/tests/fake-codex3.cmd -Work <scratch dir>`, `C3_EXE` set, under the harness lock) replays every plan check of `harness-claude` (which cannot run before wave 4)
with two CODEX routes of plan `zai` on two endpoints (`ZAI`, `ZAIB`) and `openai`, through the
same shims and fake codex: E5 - the failing ZAI run records class quota with its reset and the
machine-wide record; the listing `ZAIB ... unavailable (plan zai (usage limit on ZAI until
<iso>))` beside ZAI's own `usage limit until`; `-Short` `ZAIB :: glm-5.3 (plan zai (usage limit on
ZAI until ...`; the table's `would select openai :: gpt-5.1 (skipped: ZAI :: glm-5.3 (usage limit
until ...), ZAIB :: glm-5.3 (plan zai (usage limit on ZAI until ...`; the direct run `-Provider
ZAIB` refused (`provider ZAIB is not usable: its plan zai hit a usage limit on ZAI at ... that
lasts until ...; nothing was started (pass -SkipPreflight to launch anyway)`, exit 1, no ledger
entry); its dry run's `preflight :` line; a roster-walk run takes openai with the plan reason in
`roster.skipped`; ANOTHER repository without a ledger sees both out through the machine-wide
file; without the plan ZAIB stays available; an auth failure stays on its route; both ways, and a
usable reply on ZAI after ZAIB's limit clears the plan for ZAI while ZAIB keeps its own. E7 - the
dry-run panel `Concurrency: at most 2 at a time - endpoint groups: ZAI+ZAIB x2 one after another,
...`, `"parallel": {"zai": 2}` and no plan -> `at once`. E16 - two repositories, one machine-wide
file: E's held ZAI run (15 s) holds a running row with `"plan": "zai"`; F's panel prints `  panel
member 1 of 2 waits: 1 run(s) elsewhere on this machine use its plan zai (parallel limit 1): ZAI in
<E> task t handoff 01 (plan zai, pid N)`, starts ZAIB only after E finished, openai does not wait,
both usable, `running[]` empty afterwards.

## What differs from the plugin, and why

- **No claude engine yet.** Every plan check of `harness-claude` (ENDPOINT E5 x3, E7 through the
  bridge, ACCEPT E15 x3 and E16) names a claude endpoint entry (`ZAI-claude`), which C3's engine
  check refuses; the shim scenario above replays each of them with two codex routes of one plan.
  The ENDPOINT E1 (roster read) and E7 (`Get-PanelPlan`) checks run the plugin's own dot-sourced
  functions (green for any binary); their C3 equivalents are the Rust tests above.
- **The `http` engine** can carry a plan (C3's `ext.c3.reviewers` only) - beyond the plugin, which
  cannot see those routes; a plugin run in the same machine still counts them through the shared
  running rows (`plan`) and health records (fingerprints), but its own `Get-PlanQuota` only knows
  the routes of its `reviewers[]`.
- **Pre-existing, unchanged here:** C3's record sort breaks a completion tie by `n` before `until`
  (the plugin: `until` before `n`) - only an exact tie between a ledger and a machine-wide record
  differs; C3 has no machine-health journal (wave 28b D13) and does not apply its dedup key (A6
  marks a quota-marked record as distinct) - C3 never writes a mark, so the dedup case cannot arise
  from C3's own writes.

## Left to wave 4 (the claude engine)

The claude roster keys (`engine: "claude"`, `auth` `subscription`/`api-key`/`endpoint`, the
`endpoint` block `{base_url, env_key, timeout_ms}` with its validation, the claude model table -
harness-roster's FILE `claudeauth`/`claudemodel`), the endpoint identity (`cc-engine-v1|claude|
endpoint|<canonical base_url>|<env_key>`) and its local preflight, the child environment, the
`plan` in the claude endpoint's `provider_config` and its dry-run line (`... API_TIMEOUT_MS
<n>; plan <p>; ...`), the engine's `ParallelScope engine` scheduling key (D5), and the WRITE side
of E15 (a rejecting `rate_limit_event` beside a usable reply: `engine_run.quota_mark`, the
machine-wide `quota_mark`, the journal dedup key A6).
