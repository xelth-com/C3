# CLI surface mapping — plugin PowerShell scripts to `c3`

> **Status (updated 2026-09-29):** the mapping below was written as a design proposal before
> the binary existed. Today's `c3` (built from `4e8d1a8`) implements the whole surface —
> `c3 providers`, `c3 consult`, `c3 findings`, `c3 scoreboard`, `c3 hook`, and beyond the
> parity port `c3 panel`/`consult --panel`, the detach/status/wait/prune/kick controls, the
> `agy`/`muse`/`http` engines, `c3 pack`/`explain`/`snapshot`, `c3 index`, `c3 router`, `c3 mcp`,
> and telemetry (`c3 telemetry`/`complain`/`forget-me`). **`c3 <subcommand> --help` is the
> authoritative current flag list.** The rows once marked RESERVED are now implemented; the
> open questions at the foot of this file are resolved inline. The tables below stay as the
> PowerShell→`c3` naming record.

Source repo read for this: `C:\Users\Dmytro\claude-codex-consult` at HEAD (branch tip after
the R12 design/decisions handoffs), files `plugins/codex-consult/scripts/codex-consult.ps1`,
`codex-findings.ps1`, `codex-providers.ps1` (see `docs/port/harness-shim.md`/`m1-acceptance.md`
for that one — repeated here only for completeness), `codex-scoreboard.ps1`,
`codex-consult-hook.ps1`, `codex-consult-common.ps1`. Nothing there was modified.

Naming rule applied throughout: a PowerShell `-PascalCase` parameter becomes a `--kebab-case`
flag; a `[switch]` becomes a boolean flag with no value; a parameter with a default value keeps
that default on the `c3` side unless noted; a `[string[]]` comma-joined single-string parameter
(PowerShell's `-File` binding quirk) becomes a REPEATABLE `c3` flag (`--foo a --foo b`), which is
strictly more expressive and is what the shims parse the comma-separated plugin form into.

## 1. `c3 consult` (fronts `codex-consult.ps1`)

| PowerShell parameter | Type / default | `c3 consult` flag | Notes |
|---|---|---|---|
| `-Task` (mandatory) | string | `--task <slug>` | |
| `-CollabDir` | string, `.collab` | `--collab-dir <path>` | |
| `-Mode` | string, `''` | `--mode new\|fork\|resume` | empty default preserved as "unset" (auto: fork when a thread of this run's lineage is known, else new) |
| `-Thread` | string, `''` | `--thread <uuid>` | |
| `-Brief` | string, `''` | `--brief <path>` | c3 never writes briefs either |
| `-Prompt` | string, `''` | `--prompt <text>` | |
| `-Model` | string, `''` | `--model <id>` | |
| `-Purpose` | string, `''` | `--purpose <name>` | valid set: `framing \| decision \| checkpoint \| core-contract \| acceptance \| diff-review \| stuck \| chore` |
| `-Effort` | string, `''` | `--effort low\|medium\|high\|xhigh` | |
| `-Sandbox` | string, `read-only` | `--sandbox read-only\|workspace-write` | `danger-full-access` stays refused (validated by c3, not a valid-set member) |
| `-MaxWords` | int, `0` | `--max-words <n>` | 0 = purpose preset |
| `-TimeoutSec` | int, `0` | `--timeout-sec <n>` | 0 = purpose default (chore 600, checkpoint/none 900, framing/decision 1800, diff-review/core-contract/stuck 2400, acceptance 3600) |
| `-ContinueSec` | int, `-1` | `--continue-sec <n>` | -1 = min(timeout, 900); 0 = no continuation |
| `-Range` | string, `''` | `--range <base..head>` | diff-review/acceptance only; a single revision refused |
| `-ReplyName` | string, `reply` | `--reply-name <slug>` | |
| `-Artifact` | `string[]`, `@()` | `--artifact <path>` (repeatable) | plugin accepts one comma-joined string too; c3 flag is natively repeatable, shim splits on comma |
| `-Raw` | switch | `--raw` | |
| `-CodexExe` | string, `''` | `--codex-exe <path>` | env override `CODEX_CONSULT_EXE` |
| `-Provider` | string, `''` | `--provider <name>` | |
| `-NativeEffort` | string, `''` | `--native-effort <value>` | excludes `--effort` |
| `-OffPeakOnly` | switch | `--off-peak-only` | |
| `-SkipPreflight` | switch | `--skip-preflight` | |
| `-CodexConfig` | `string[]`, `@()` | `--codex-config k=v` (repeatable) | plugin: one comma-joined string `k=v,k2=v2`; c3: repeatable `--codex-config k=v [--codex-config k2=v2 ...]`; keys that would change the recorded identity/effort stay refused |
| `-SchemaTransport` | string, `''` | `--schema-transport output-schema\|prompt-only` | empty = caps-v1 capability table default |
| `-FormatRetry` | int (0/1), `1` | `--format-retry` / `--no-format-retry` | int flag ported to a boolean pair rather than `--format-retry <0\|1>`, matching `c3`'s other switches; not with `--raw` or `--purpose chore` |
| `-Panel` | switch | `--panel` | needs a roster; not with `--provider`, `--thread`, or `--mode resume` |
| `-PanelAll` | switch | `--panel-all` | |
| `-PanelSpec` | string, `''` (INTERNAL) | `--panel-spec <base64>` | never passed by a human; c3 keeps it as an internal/hidden flag for its own re-exec of panel members |
| `-PanelConcurrency` | int, `0` | `--panel-concurrency <n>` | 0 = no cap; 1 = strictly sequential; k = at most k at once |
| `-Engine` | string, `''` | `--engine codex\|agy\|muse` | |
| `-EngineExe` | string, `''` | `--engine-exe <path>` | env overrides `CODEX_CONSULT_AGY_EXE`, `CODEX_CONSULT_MUSE_EXE` |
| `-MaxModelSteps` | int, `0` | `--max-model-steps <n>` | muse only; refused with another engine |
| `-DenialRetry` | int (0/1), `1` | `--denial-retry` / `--no-denial-retry` | agy only |
| `-DryRun` | switch | `--dry-run` | |
| *(none — R12 design, not yet implemented in the plugin script's param block)* | | `--detach` | RESERVED. Plugin decision doc `.collab/nonblocking-2026-09-26/handoffs/05-claude-r12-decisions.md` (R12 D1-D12): background run; refused together with `--panel-spec`, `--dry-run`, `--status`, `--wait` (D8) |
| *(R12, internal)* | | `--detach-id <id>` | RESERVED, internal — set by `--detach`'s own background re-exec, never passed by a human (mirrors `-PanelSpec`) |
| *(R12)* | | `--status [<id-or-prefix>]` | RESERVED. Without an id: every status file of the task, newest first, exit = worst state (2 running > 1 failed > 0 ok) (D7); an ambiguous id prefix matching several files refuses with exit 4 naming them |
| *(R12)* | | `--list` | RESERVED. Same liveness judging as `--status`/hook (`Test-PidAlive`) (D6) — note: this is a DIFFERENT `--list` than `c3 findings --list`, scoped to `c3 consult` background runs |
| *(R12)* | | `--wait [--wait-timeout-sec <n>]` | RESERVED. Defaults to the computed budget (D4: per-endpoint-group ceil(members/limit) × guard, max over groups, vs a global `--panel-concurrency` cap, the larger + 120s) |
| *(R12)* | | `--prune` | RESERVED. The one *writing* form of `--status` (D8): removes `done` and `died` status files older than 7 days |

Exit-code table — `c3 consult` (from `codex-consult.ps1`'s own conventions, `docs/DESIGN.md` and
the classifier rules; see `m2-acceptance.md` for the harness rows this is derived from):

The plugin is the contract, and the plugin has exactly two outcomes: a usable reply (exit 0)
and `Stop-WithError` (exit 1). There is **no** exit 2/3/130/143 — every refusal and every
failed run is exit 1 (`Stop-WithError` writes the message and exits 1). C3 matches this. The
`provider_failure.class` (`auth`/`quota`/`capability`/`transport`/`unknown`) and the
timeout-continuation salvage are recorded in the ledger and the summary, not in the exit code.

| exit | meaning |
|---|---|
| 0 | usable reply ingested (single run), or every panel member produced a usable reply (`-Panel`/`-PanelAll`); a `--dry-run` always exits 0 |
| 1 | every `Stop-WithError` path: a usage error (bad/missing parameter, a validated refusal such as `-CodexConfig` naming an identity key, an unknown `-Provider`), a **preflight refusal** (credentials, endpoint health, `-OffPeakOnly` inside a peak window, usage limit), an unresolved/unusable identity or roster, a **failed run** (timeout kill, provider failure of any class, launch failure), *or* a panel run where at least one member did not produce a usable reply |
| 4 | RESERVED (R12 `--status <ambiguous-prefix>` only) |

## 2. `c3 findings` (fronts `codex-findings.ps1`)

| PowerShell parameter | Type / default | `c3 findings` flag | Notes |
|---|---|---|---|
| `-Task` (mandatory) | string | `--task <slug>` | |
| `-CollabDir` | string, `.collab` | `--collab-dir <path>` | |
| `-List` | switch | `--list` | |
| `-All` | switch | `--all` | only with `--list` |
| `-Stats` | switch | `--stats` | |
| `-Id` | string, `''` | `--id <F..-k>` | needs `--status` |
| `-Status` | string, `''` | `--status proposed\|implemented\|verified\|rejected\|wontfix\|superseded` | |
| `-Note` | string, `''` | `--note <text>` | required for `rejected` and for a reopen to `proposed`; only valid with `--id`/`--status` or `--rate` |
| `-Evidence` | string, `''` | `--evidence <text>` | required for `verified`; only valid with `--id`/`--status` |
| `-Rate` | int, `0` | `--rate <n>` | consult ledger entry number, > 0 |
| `-Useful` | string, `''` | `--useful yes\|partly\|no` | needs `--rate`; `--note` required when `no` |
| *(not present in the plugin script)* | | `--json` | Flagged by the supervisor's brief as a "?" on `codex-findings.ps1`'s own param block — **the reference script has no `-Json` switch at all** (checked the whole param block); this row is a proposed C3-only addition, not a 1:1 port. See Open questions. |

Modes are mutually exclusive (plugin: `Stop-WithError "choose exactly one of: -List [-All], -Stats,
-Id <F..> -Status <status>, or -Rate <n> -Useful yes|partly|no."`) — `c3 findings` should validate
the same way: `--list`[`--all`] xor `--stats` xor (`--id` + `--status`) xor (`--rate` + `--useful`).

Exit-code table — `c3 findings`:

| exit | meaning |
|---|---|
| 0 | list/stats printed, or a status/rating change committed |
| 1 | every error path (`Stop-WithError`): usage error, unknown task/finding id, missing required companion arg, task lock held by another run (`Enter-TaskLock` refusal), an active pending/recovery record, a write-lock (`Enter-StoreCommit`) timeout, no `sessions.json`/`findings.json` yet |

The plugin has no distinct "lock refused" exit code — it is exit 1 like every other
`Stop-WithError` path; `c3 findings`'s message text is the only differentiator, matching the
plugin (see `m3-acceptance.md` for the exact refusal wording harnesses assert on).

## 3. `c3 scoreboard` (fronts `codex-scoreboard.ps1`; **new to the `c3` CLI surface** — no
   subcommand for this exists yet, per the supervisor's brief)

| PowerShell parameter | Type / default | `c3 scoreboard` flag | Notes |
|---|---|---|---|
| `-CollabDir` | string, `.collab` | `--collab-dir <path>` | |
| `-Task` | string, `''` | `--task <slug>` | one task only; empty = every task under `<CollabDir>` |
| `-Json` | switch | `--json` | array of row objects instead of the printed table |

Exit-code table — `c3 scoreboard`:

| exit | meaning |
|---|---|
| 0 | table/JSON printed (including the empty case: no tasks yet) |
| 1 | usage error (`-Task` fails the slug pattern, or names a task directory that does not exist under `-CollabDir`) |

## 4. `c3 hook` (fronts `codex-consult-hook.ps1`, the SessionStart line)

| PowerShell parameter | Type / default | `c3 hook` flag | Notes |
|---|---|---|---|
| `-CollabDir` | string, `.collab` | `--collab-dir <path>` | |

No other parameters exist on the plugin side. Behavior: `c3 hook` must reproduce exactly the
SessionStart contract — one line to stdout, always exit 0, never touches the network or the
filesystem beyond reading task ledgers under `<CollabDir>`, computed as `c3 providers --short
--json --no-network --collab-dir <dir>`'s `line` field internally (the plugin hook literally
shells out to `codex-providers.ps1 -Short -Json -NoNetwork`; `c3 hook` may call its own
`providers` logic in-process instead, but the stdout line text and the "codex CLI not found on
PATH" / "reviewer check failed - ..." fallback wording must match). This is the
`SessionStart` hook line registered in the plugin's `hooks/hooks.json`; C3's equivalent
packaging (milestone 6) would register `c3 hook` the same way.

## 5. Exit-code table — `c3 providers` (fronts `codex-providers.ps1`, for completeness; full
   detail already in `m1-acceptance.md`)

| exit | meaning |
|---|---|
| 0 | available (with `-Provider`); always 0 without `-Provider` (table/`-Json`/`-Short`) |
| 1 | usage error (unknown provider name given to `-Provider`) |
| 2 | unavailable (with `-Provider`) |
| 3 | unknown (with `-Provider`); also the `-Provider gemini` "`agy models` did not finish within N s" timeout case (wave 24b) |
| 127 | shim-only, not a plugin code: the `tests/shim/*.ps1` convention for "the `c3` binary itself could not be located" (see `tests/shim/codex-providers.ps1`'s own `.NOTES`) — applied uniformly to all four new shims below for consistency |

## 6. Environment variables

Grepped `$env:` (plus `[Environment]::GetEnvironmentVariable`) across `codex-consult.ps1`,
`codex-findings.ps1`, `codex-providers.ps1`, `codex-scoreboard.ps1`, `codex-consult-hook.ps1` and
`codex-consult-common.ps1`.

| variable | read by | meaning |
|---|---|---|
| `CODEX_HOME` | common.ps1 (`Get-CodexHome`) | Codex config home; falls back to `~/.codex` under `$HOME`/`$USERPROFILE` when unset |
| `HOME` / `USERPROFILE` | common.ps1 | home directory resolution for `CODEX_HOME` and for the Muse `~/.config/muse/auth.json` path (Windows: `USERPROFILE`, else `HOME`) |
| `OPENAI_BASE_URL` | common.ps1 | honoured by Codex for its built-in `openai` provider; when set, the identity/endpoint display and `base_url_source` note it explicitly (`base_url_source: 'OPENAI_BASE_URL'`) |
| `CODEX_CONSULT_EXE` | common.ps1 (`Resolve-CodexLauncher`), consult/providers scripts | explicit override for the `codex` launcher path; same override precedence tier as `-CodexExe` (explicit param wins, then this env var, then PATH) |
| `CODEX_CONSULT_AGY_EXE` | common.ps1 (engine table `ExeEnv`) | explicit override for the `agy` (Gemini/Antigravity) launcher path, same precedence as `-EngineExe` for that engine |
| `CODEX_CONSULT_MUSE_EXE` | common.ps1 (engine table `ExeEnv`) | explicit override for the `muse` launcher path, same precedence as `-EngineExe` for that engine |
| `CODEX_CONSULT_ROSTER` | common.ps1 (`Get-RosterSource`) | path to the reviewer roster JSON (must exist or every run refuses); literal value `none` disables the roster entirely (default file at `<CODEX_HOME>/codex-consult-roster.json` is then ignored too); unset = use the default file if present |
| `CODEX_CONSULT_PEAK_<PROVIDER>` | common.ps1 (`Get-PeakEnvVarName`, dynamic name built from the provider's name uppercased with non-alnum -> `_`) | declares a peak window for that provider: `"<days> <HH:MM>-<HH:MM> <+HH:MM\|-HH:MM>"`, e.g. `CODEX_CONSULT_PEAK_ZAI="Mon-Fri 14:00-18:00 +08:00"`; a run inside the window warns, or refuses with `-OffPeakOnly` |
| `CODEX_CONSULT_PEAK_<PROVIDER>_EXCEPT` | common.ps1 | comma-separated `YYYY-MM-DD` (or range) dates excluded from that provider's peak window |
| `CODEX_CONSULT_NOW` | common.ps1 (`Get-ConsultClock`) | TEST HOOK: one or more comma-separated ISO-8601 timestamps with an offset that freeze/fast-forward the "now" used for peak/health evaluation; malformed values are a hard error naming the bad token |
| `TBH_CREDENTIAL_BACKEND` | common.ps1 (Muse sign-in check) | must be `file` for the Muse CLI's local keychain-backend file (`~/.config/muse/auth.json`) to be readable by the preflight check; any other value (or unset) is "sign-in not checkable" |
| `CODEX_CONSULT_TEST_LOGIN_TIMEOUT`, `CODEX_CONSULT_TEST_COMMIT_PAUSE_MS`, `CODEX_CONSULT_TEST_LAUNCH_PAUSE_MS`, `CODEX_CONSULT_TEST_MEMBER_PAUSE_MS`, `CODEX_CONSULT_TEST_PANEL_GUARD_SEC`, `CODEX_CONSULT_TEST_SURVIVORS`, `CODEX_CONSULT_TEST_WRITE_LOCK_SEC` | common.ps1 | TEST-ONLY HOOKS used exclusively by the harnesses to shorten timeouts, inject artificial pauses at commit/launch/member-start points, and fake surviving-process lists for lock/pending races; no production behavior depends on them. `c3`'s own test doubles should expose equivalent knobs (naming left to the Rust implementation; not part of the user-facing CLI surface) |

Not found anywhere in the four scripts or `codex-consult-common.ps1` (present only in the
supervisor's brief as forward references to later C3 milestones, not in the plugin today):
`CODEX_CONSULT_TELEMETRY`, `CODEX_CONSULT_COORDINATOR`, `CODEX_CONSULT_ROOT`. These are C3-side
design names for milestones 5 (`telemetry`, an opt-out spool) and the coordinator-identity rule
(`docs/DESIGN.md` §4 "Coordinator identity (`--coordinator provider::model`, plugin R13)") — the
plugin repo implements neither today (R13 has no corresponding decisions handoff at this HEAD,
unlike R12). Flagged as an open question below.

## Open questions (resolved)

1. **`c3 findings --json`**: RESOLVED — not added. `c3 findings` has no `--json` flag today
   (confirmed against `c3 findings --help`); the machine-readable surface is `c3 scoreboard --json`
   and `c3 index ... --json`. `--list`/`--stats` print for a human.
2. **R12 flags (`--detach`/`--status`/`--wait`/`--prune`, `--kick`/`--member`)**: RESOLVED —
   implemented. All are live on `c3 consult` (see `c3 consult --help`); `--detach-id`/`--panel-spec`
   are internal re-exec flags. The exit-code shape (0 done/usable, 1 done-with-failure, 2 running,
   4 ambiguous id) matches the design doc.
3. **`--coordinator` / `CODEX_CONSULT_COORDINATOR`**: RESOLVED — implemented as the env var
   `CODEX_CONSULT_COORDINATOR` (`<provider> :: <model> [engine]`, a roster `#<n>`, or a label). The
   ledger records the coordinator identity and the scrubbed child-env NAMES; see the `coordinate`
   skill.
4. **`-FormatRetry`/`-DenialRetry` arity**: RESOLVED — `c3` takes them as `--format-retry <0|1>`
   (default 1) and `--denial-retry <0|1>` (default 1), an integer mirroring the plugin, not a
   boolean pair.
5. **`CODEX_CONSULT_TELEMETRY`**: implemented (milestone 5) — `on|off` (wave 2: the plugin's
   `Get-TelemetrySwitch` - any other value counts as off), also `--telemetry on|off` per run, which
   wins over the variable (`c3 consult`, `c3 findings --rate`, `c3 telemetry --flush |
   --backfill-ratings`); `c3 telemetry` takes the plugin's `codex-telemetry.ps1` forms (`--status`,
   `--flush`, `--forget`, `--backfill-ratings`, see `wave2-telemetry.md`). `CODEX_CONSULT_ROOT` is not a `c3` env var; the collab dir is `--collab-dir`, resolved
   against the git repo root.
