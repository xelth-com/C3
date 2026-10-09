# RC3 live matrix, 2026-10-09

Repository `C:\Users\Dmytro\C3`, branch main at 60c1e98 (code at f62ca02). Built from the main checkout with
`CARGO_TARGET_DIR=C:\Users\Dmytro\C3\target cargo build -j 2` after touching the three crate roots (1 m 14 s, the
known linker warning only). `c3 --version` = `c3 0.1.0`; the binary contains `forget-pending` and `kill_unconfirmed`;
the copy `%TEMP%\c3-rc3\c3.exe` ran every C3 step. Plugin side: `plugins/codex-consult/` of tag v0.6.1 (e1f4576)
staged with `git archive` under `%TEMP%\c3-rc3\plugin\`, run with Windows PowerShell. Launchers: codex-cli 0.155.1,
Claude Code 2.1.294, agy (version unknown to both sides), muse-cli 1.4.4-R5419.1.

Roster: a copy of `~/.codex/codex-consult-roster-0.6.json` at `%TEMP%\c3-rc3\roster.json` through
`CODEX_CONSULT_ROSTER`; two derived scratch rosters: `roster-sub.json`
(`[{"provider":"anthropic","engine":"claude","model":"haiku","auth":"subscription"}]`, the 0.6 roster has no
subscription entry) and `roster-panel.json` (row 5). The operator's files were not edited. Six scratch repositories
under `%TEMP%\c3-rc3\repos\` (`git init`, `README.md`, `inventory.py`, `config.json`, one commit). The brief (one
line, `Name the three files in this repository and one risk in inventory.py.`) was copied to
`.collab/<task>/brief.md` before each run. Nothing was written to an agy or muse repository while its run was active.

Availability first: `c3 providers` and the plugin's `codex-providers.ps1 -Short` both said `out - openai ::
gpt-6-astra (until Wed 11:37, in 4d 22h); 11 of 12 reviewers available`. openai and kimi were not called (kimi showed
`available` because its 5-hour mark had expired at 13:31; the brief rules it out). Budget: 14 live consultations used
of 14 (8 single runs, 4 panel members, 2 interchange runs), every one `chore` / 60 words, default timeouts. Dry runs
are not counted.

Secrets: no key value was printed, read back or written. The dry-run outputs, run logs, ledgers and the endpoint
runs' event streams were scanned for long non-hash tokens: none in the outputs and ledgers; the events' long tokens
are `msg_`/`call_` ids, git object paths, a thinking signature and JSON key names (classified by their prefix only).

## 1. codex engine via coding-plan providers (repo `codex`)

| route | wall (run / process) | outcome | model line (reply header) | ledger `reviewer` (classes) | warnings |
|---|---|---|---|---|---|
| `--provider ZAI --model glm-5.3` | 51.3 s / 52.7 s | usable reply | `Author: Codex (model glm-5.3, effort low), Codex CLI 0.155.1` | provider/model from -Provider/-Model, engine codex, harness codex-cli 0.155.1, provider_fingerprint, provider_config {base_url, name, wire_api responses} | none |
| `--provider mimo --model mimo-v2.6-pro` | 310 s / 311.5 s | usable reply | `Author: Codex (model mimo-v2.6-pro, effort low)` | same classes; roster `codex_config applied` (model_catalog_json) | none |
| `--provider byteplus --model deepseek-v4.1-flash` | 41.6 s / 43.4 s | usable reply | `Author: Codex (model deepseek-v4.1-flash, effort low)` | same classes; roster `codex_config applied` (model_supports_reasoning_summaries) | none |

`engine_run` is null for codex (as in the plugin). Effort `low sent` through the caps-v1 mappings (zai-v1, mimo-v1,
ark). mimo is slow on its own (310 s, 245k input tokens, no cache), not a bridge delay.

## 2. agy engine (repo `agy`)

| route | wall | outcome | model line | ledger `reviewer` | warnings |
|---|---|---|---|---|---|
| `--provider gemini --model gemini-3.8-flash-high` (roster: engine agy) | 207.7 s / 211.1 s | usable reply | `Author: Gemini (agy) (model gemini-3.8-flash-high, effort tier in the model id), agy-cli (version unknown)` | engine agy, harness agy-cli (version unknown), provider_config {engine, launcher} | none |

Tree check: `tree_check {outcome clean, files []}`, `tree_sha256` = `tree_sha256_after`, brief unchanged; the
tracked files' hashes before and after are equal, `git status` shows only `?? .collab/`. `engine_run {turns 1}`.
Reply quality (not a bridge matter): agy listed `.collab/rc3-agy/brief.md` as a repository file and missed
`config.json`.

## 3. muse engine (repo `muse`)

| route | wall | outcome | model line | ledger `reviewer` | warnings |
|---|---|---|---|---|---|
| `--provider meta --model muse-spark-1.3-contributor` (roster: engine muse) | 48.7 s / 51.1 s | usable reply | `Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort low), muse-cli 1.4.4-R5419.1` | engine muse, harness muse-cli 1.4.4-R5419.1, provider_config {engine, launcher, credential_mechanism oauth} | none |

The sign-in preflight passed (`ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth)`); tree check
clean; `engine_run {turns 1, msp_schema_version 1}`; tokens not reported by muse. Same reply-quality slip as agy
(brief listed, `config.json` missed).

## 4. claude engine (repo `claude`)

| route | wall | outcome | model line | ledger `engine_run` (auth, model_resolved, quota_mark) | warnings |
|---|---|---|---|---|---|
| `auth: subscription`, model `haiku` (scratch roster) | 8 s / 11.0 s | usable reply | `Engine turns: 1 (claude -p, auth subscription; model claude-haiku-5-5; init tools Glob, Grep, Read; permission denials 0)` | subscription, claude-haiku-5-5, null (api_key_source none, mcp_servers 0, dontAsk, cost_usd 0.0012) | `claude model haiku is an alias: the alias floats; ...` and `claude rate limit status allowed_warning (seven_day); resets at 2026-10-13T08:00:00Z` (seven-day utilisation 0.92) |
| `ZAI-claude` DRY RUN | - | plan printed | `preflight   : available (ok: env ZAI_API_KEY set)`; `endpoint    : https://api.z.ai/api/anthropic (ANTHROPIC_BASE_URL); token from env ZAI_API_KEY (ANTHROPIC_AUTH_TOKEN - the value is never shown); API_TIMEOUT_MS 3000000; plan zai; no claude auth status - the model the init event names is the proof`; argv `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model glm-5.3 --effort low [--add-dir <brief dir>] --session-id <uuid>` | - | token absent from the output |
| `ZAI-claude` (endpoint, ZAI_API_KEY) | 9.2 s / 11.0 s | usable reply | `Engine turns: 1 (claude -p, auth endpoint; model glm-5.3; init tools Glob, Grep, Read; permission denials 0)` | endpoint, glm-5.3, null (rate_limit null, cost_usd 0.020) | none |
| `mimo-claude` (endpoint, MIMO_API_KEY) | 16.1 s / 17.7 s | usable reply | `Engine turns: 1 (claude -p, auth endpoint; model mimo-v2.6-pro; ...)` | endpoint, mimo-v2.6-pro, null (cost_usd 0.026) | none |

Ledger `reviewer.provider_config` for the endpoint routes: `{engine claude, launcher, credential_mechanism endpoint,
base_url, env_key (the NAME), plan}`; subscription: `{engine, launcher, credential_mechanism subscription,
auth_method claude.ai, api_provider firstParty}`. The child environment is the allow list (endpoint:
`ANTHROPIC_AUTH_TOKEN`, `ANTHROPIC_BASE_URL`, `API_TIMEOUT_MS` added). Transcript-location guard: silent on every
claude run (no warning, no refusal); the transcripts landed outside the repositories, in
`~/.claude/projects/C--Users-Dmytro-AppData-Local-Temp-c3-rc3-repos-claude` (3) and `...-repos-panel` (2). Tree check
clean on all three runs.

## 5. Mixed panel (repo `panel`)

Roster `roster-panel.json`: #1 ZAI glm-5.3 (codex, plan zai), #2 gemini gemini-3.8-flash-high (agy), #3 ZAI-claude
(claude endpoint, plan zai), #4 mimo-claude (claude endpoint, plan mimo, `panel: light`).

`--panel --purpose chore` alone seats ONE member (`size 1 (the default of purpose chore)`, the other three `not
picked: panel size 1`), so the run used `--panel --panel-size 4 --purpose chore --max-words 60`.

Seating (identical text in the C3 and plugin dry runs):

```
Panel 0221bc94: 4 of 4 roster entries run, at most 2 at a time (...)
  #1 ZAI :: glm-5.3 - member, n=1, handoff 01
  #2 gemini :: gemini-3.8-flash-high [agy] - member, n=2, handoff 02
  #3 ZAI-claude :: glm-5.3 [claude] - member, n=3, handoff 03
  #4 mimo-claude :: mimo-v2.6-pro [claude] - member, n=4, handoff 04
Concurrency: at most 2 at a time - endpoint groups: ZAI+ZAI-claude+mimo-claude x3 one after another, gemini x1; -PanelConcurrency 0 (no cap)
```

Light stand-in rule: the light gate applies only to weighty purposes, so on chore #4 is a plain member. A dry run
with `--purpose decision` (C3 and plugin, identical) shows the rule:
`#4 mimo-claude :: mimo-v2.6-pro [claude] - member, n=4, handoff 04, stands in (no other entry of label mimo-claude)`.

Grouping: `plan zai` joins ZAI and ZAI-claude; the claude engine's one scheduling group (ParallelScope engine, wave
29 D5) pulls mimo-claude into the same group, hence `x3 one after another` (the plugin plans the same group).

| member | started - finished | wall | outcome | engine_run (auth, model_resolved, quota_mark) | tree check |
|---|---|---|---|---|---|
| #1 ZAI (codex) | 14:13:59 - 14:14:51 | 51 s | usable reply, prose | - | (codex: none) |
| #2 gemini (agy) | 14:13:59 - 14:18:34 | 274.4 s | usable reply, prose | turns 1 | clean |
| #3 ZAI-claude (claude) | 14:14:53 - 14:15:25 | 31.6 s | usable reply, prose | endpoint, glm-5.3, null | clean |
| #4 mimo-claude (claude) | 14:15:28 - 14:25:13 | 584.9 s | usable reply, prose | endpoint, mimo-v2.6-pro, null | clean |

Panel: `4 of 4 entries ran (asked 4, started 4, usable 4; wall clock 676.8 s; at most 2 at a time)`, exit 0, no
warnings in any member's ledger entry. Serialisation proven by the ledger times: #3 started 2 s after #1 finished,
#4 3 s after #3; gemini ran beside them. There is no separate "wait" line: C3 (like the plugin) prints a
`waits: N run(s) elsewhere` line only for runs of the group in OTHER panels/repositories (machine-wide health); inside
one panel the plan line `x3 one after another` is the statement. #4 was slow at the provider: the mimo endpoint
streamed ~60 `thinking_tokens` events for ~9 minutes after the third tool result and finished at 584.9 s
(`duration_api_ms` 582854), 15 s inside the 600 s chore timeout; no kill, no continuation.

Reconciled ledger: `sessions.json` has the four entries n=1..4, each `bridge_outcome usable reply`, each with
`panel {id 0221bc94-..., position k, of 4, members, concurrency 2, limits, asked 4, started 4, usable 4, routing}`,
`commit_wait_ms 0`. `c3 findings --task rc3-panel --list`: no findings (chore is raw); `--stats` lists n=1..4.
No `.consult.pending.json` anywhere under the scratch repositories; no child process survived (section 7).

## 6. Interchange plugin <-> C3 (repo `interchange`, task `rc3-x`, `CODEX_CONSULT_TELEMETRY=off` on both sides)

| step | side | result |
|---|---|---|
| (a) n=1 | plugin `codex-consult.ps1 -Task rc3-x -Provider ZAI -Model glm-5.3 -Purpose chore -MaxWords 60` | usable reply, mode new, 43.7 s (48.2 s process), `telemetry: off (CODEX_CONSULT_TELEMETRY)` |
| (b) n=2 | C3 `consult --mode fork ...` | dry run: `parent : newest thread of lineage ZAI :: glm-5.3 (consult n=1)`, thread = the plugin's; real: usable reply, mode fork, parent_thread = n=1's thread, 25.1 s (26.5 s), no warnings |
| (c) rate n=2 | plugin `codex-findings.ps1 -Rate 2 -Useful yes` | `codex-findings: consult n=2 (ZAI :: glm-5.3, chore) rated yes.` exit 0 |
| (c) rate n=1 | C3 `findings --rate 1 --useful yes` | `codex-findings: consult n=1 (ZAI :: glm-5.3, chore) rated yes.` exit 0 |
| (d) list | plugin `-List` vs C3 `--list` | byte-identical text (0 findings, 0 orphans) |
| (d) stats | plugin `-Stats` vs C3 `--stats` | byte-identical (n=1 and n=2 rows; `ZAI :: glm-5.3 ... yes 2`) |
| (e) providers | `codex-providers.ps1` vs `c3 providers` | every row identical (verdicts, credentials, effort, last failures from the machine-wide health); ONE line differs, below |

Neither side warned when reading, forking from, rating or listing the other's entries.

Ledger shape differences (`git diff` style):

```diff
 sessions.json, n=1 (written by the plugin) vs n=2 (written by C3): 87 keys each, identical key order
   "coordinator": {"provider": null, "model": null, "engine": null, "host": "claude-code",
-                  "host_by": "markers", "source": "inferred", "in_roster": null, "unresolved": null}   (plugin)
+                  "source": "inferred"}                                                                (C3)

 sessions.json, the plugin's n=1 entry after C3 appended n=2 (C3 rewrites the file):
-  "host_by": "markers", "source": "inferred", "in_roster": null, "unresolved": null
+  "source": "inferred", "host_by": "markers"
   (C3's Coordinator struct has no host_by - kept as an extra field but moved to the end - and skips the
    None-valued in_roster / unresolved, so the plugin's two explicit nulls are dropped)

 findings.json ratings: n=2 (plugin) and n=1 (C3) have the same keys in the same order
   {n, consult_id, lineage, provider, model, engine, purpose, topics, consult_when, useful, note, when,
    rating_rev 1, judge {provider anthropic, model other, source consult_coordinator}}
   the file after the C3 rating = the plugin's file + the appended entry (same PowerShell-style layout, LF, no BOM)
   ratings do not touch sessions.json on either side

 consult_ref: present on both (uuid), one per entry, distinct; consult_id likewise
 roster.path: "...\Temp\c3-rc3\roster.json" (plugin) vs "...\Temp/c3-rc3/roster.json" (C3) - the value of
   CODEX_CONSULT_ROSTER as each shell set it (PowerShell vs Git Bash), recorded verbatim by both; not a tool difference
```

`providers` difference:

```diff
- endpoint health: ...\interchange\.collab (1 task ledger, 2 consultations), read at ... - the ledgers of THIS repository   (plugin)
+ endpoint health: ...\interchange\.collab (1 task ledger, 46 consultations), read at ... - the ledgers of THIS repository  (C3)
```

C3 counts `read_all_task_consults_health` (the repository's consultations plus the machine-wide health file's
synthetic records, `providers.rs:203`), the plugin only the repository's. The verdicts are the same; only the count in
the header disagrees with its own wording (an empty repository showed `0 task ledgers, 31 consultations`).

## Telemetry

Matrix runs 1-5 ran with telemetry on; the interchange with it off (the plugin's spool `~/.codex/telemetry-spool`
stayed empty, its last flush 12:17 predates this run). `c3 telemetry --status` after the matrix: `spool: 2 event(s)
... oldest queued 14:18:34`, `not spooled: none since the last flush`, `last flush: 14:15:25 - delivered 1 - dropped
0, kept 0` (http 200). The two queued events were the panel's gemini (14:18:34) and mimo-claude (14:25:13) members:
C3 starts its sender at the START of every telemetry-on run (`flush_in_background` in `consult::run`, joined with a
3 s cap), so each run delivers what earlier runs queued and the last run's event waits for the next run; the plugin
starts a detached sender after the commit (a panel's members leave it to the panel run). One `c3 telemetry --flush`
then delivered them: `done - delivered 2, kept 0, dropped 0`; status `spool: 0 event(s)`, `not spooled: none`.
Nothing was not-spooled at any point. C3 and the plugin keep separate spools and salts (C3 instance `a09d233e...`,
plugin instance `b43bb9fa...`).

## 7. Afterwards

No process named codex, claude, agy, muse, c3 or node started after 13:59:51 (this worker's start) is alive; the
codex/claude processes alive are the operator's from 2026-10-07/08. Every scratch repository's `git status` is
`?? .collab/` only. `.collab` contents:

- `codex`: `rc3-zai`, `rc3-mimo`, `rc3-byteplus` - each `brief.md`, `sessions.json`, `.consult.lock`,
  `.consult.write.lock`, `handoffs/01-codex-reply.md` + `.events.jsonl`
- `agy`: `rc3-agy` - the same set with `01-agy-reply.*`
- `muse`: `rc3-muse` - the same set with `01-muse-reply.*`
- `claude`: `rc3-sub`, `rc3-zaic`, `rc3-mimoc` - the same set with `01-claudecode-reply.*`
- `panel`: `rc3-panel` - `01-codex-reply-zai.*`, `02-agy-reply-gemini.*`, `03-claudecode-reply-zai-claude.*`,
  `04-claudecode-reply-mimo-claude.*`, `sessions.json`, locks, brief
- `interchange`: `rc3-x` - `01-codex-reply.*` (plugin), `02-codex-reply.*` (C3), `findings.json`, `sessions.json`,
  locks, brief

The lock files stay on disk after a run with the last holder's pid (dead); the next run on either side took them
without a word. The dry runs (including the plugin's) wrote nothing.

## Other differences seen on the way

- The reviewer line of the agy and muse engines (dry run `reviewer :` and the reply header `Reviewer:`): C3 prints
  `endpoint (default), wire_api: (default)`, the plugin prints `engine agy (<launcher>)` / `engine muse
  (<launcher>)`. The claude engine is special-cased in C3 (`engine claude (<launcher>)`, matching the plugin); agy and
  muse fall through to the codex endpoint display (`consult/orchestrate.rs` around line 8505). Fingerprints, harness
  strings and the ledger's `provider_config` are the same on both sides.

## Conclusions

1. Proven live: C3 main drives all four engines on this machine's real subscriptions - codex over three coding plans
   (z.ai, MiMo, BytePlus), agy (Gemini), muse (Meta) and claude on the claude.ai login and on two Anthropic-compatible
   endpoints (z.ai, MiMo) - with usable replies, clean tree checks, the endpoint token never shown, the transcript
   guard silent and no surviving child.
2. Proven live: a four-member mixed panel (codex + agy + two claude endpoints) seats, groups (plan zai plus the claude
   engine scope) and serialises as the plugin plans, reconciles four ledger entries and leaves no pending record.
3. Proven live: the plugin and C3 share one task - C3 forks the plugin's thread, each rates the other's consultation,
   `-List`/`-Stats` are byte-identical, the ratings have the same shape, and neither side warns on the other's entries.
4. Not proven live: the light stand-in on a weighty purpose (dry run only, identical to the plugin), the
   machine-wide `waits: ... elsewhere` line, a timeout kill / continuation, a quota mark, and kimi/openai (out).
5. Differs from the plugin: the C3 coordinator record omits `host_by`/`in_roster`/`unresolved` and normalises a
   plugin entry's coordinator when it rewrites the ledger; the providers header counts machine-health records as
   "consultations"; the agy/muse reviewer line shows the codex endpoint text; the telemetry sender runs at the start of
   the next run, not after the commit, so a session's last event waits for the next run or `--flush`.
