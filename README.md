# C3 — claude-codex-consult in Rust

**Status: parity with claude-codex-consult 0.6.1** (the compatibility release 0.2.0; measured by
the pinned v0.6.1 harnesses through the shim - see `docs/port/harness-shim.md` and
`docs/port/rc5-verification-2026-10-09.md` when it exists; the changes in `CHANGELOG.md`).
The full surface runs. The parity port (milestones 1–6) is in the binary —
`c3 providers`, `c3 consult` (with the `codex`, `agy`, `muse`, `claude`, `grok`, `kimi` and `http`
engines),
`c3 findings` (statuses and ratings), `c3 scoreboard`, `c3 hook`, `c3 telemetry`/`complain`/
`forget-me`, the reviewer roster and the routed **panel** with detach/status/wait/prune/kick —
and so are the heavy subsystems after it: **packs and the `http` engine** (milestone 7,
run live against OpenRouter — see `docs/port/m7-status.md`), the **index** (milestone 8,
SurrealDB behind the opt-in `index-surreal` feature), the **router** (milestone 9) and the **stdio MCP server**
(milestone 10, `c3 mcp`). What is measured by which harness: milestone-1 parity by
`docs/port/m1-acceptance.md` (byte-for-byte against `codex-providers.ps1`) and the per-milestone
`m*-acceptance.md` / `m7-status.md` notes, plus `cargo test` (the c3 lib, `c3-core`, and the
`http_engine` integration suite incl. a seeded-secret grep of every written file) and
`cargo clippy -D warnings`. **Deliberately not ported: multi-host coordination.** The PowerShell
plugin coordinates from Claude Code, Codex CLI, Z Code, Kimi Code or a plain shell; C3's
coordinator host is **Claude Code only** (host-invariance across shells stays with the plugin).
The PowerShell plugin [`codex-consult`](https://github.com/xelth-com/claude-codex-consult)
remains installable (`/plugin marketplace add xelth-com/claude-codex-consult`, then
`/plugin install codex-consult@claude-codex-consult`) and its `.collab` files are byte-compatible
with C3's — either bridge reads the same task history. Its README is the specification this port
follows section by section. Page: <https://xelth.com/C3/>.

## What it is

A coordinator — your Claude Code, Codex CLI or any shell session — asks a **roster of
reviewers from different labs** to look at a one-page brief. Reviewers are read-only:
nothing is edited by them. Every consultation is recorded as files next to the code (the
brief, the reviewer's reply verbatim, a JSON ledger, **findings tracked by id** that you
verify and move yourself), and a **panel** runs several reviewers in parallel and
reconciles them. Reviewers are subscriptions the user already has: the ChatGPT plan through
the Codex CLI, z.ai GLM, Xiaomi MiMo or any Responses-API provider through a
`[model_providers.<name>]` table, Gemini through Google's Antigravity CLI `agy`, Meta Muse
through the Muse Code CLI, Claude through Claude Code headless (`claude -p`; the subscription,
an API key, or a coding plan's Anthropic-compatible endpoint), Grok through the Grok Build
CLI `grok` (a read-only reviewer on the SuperGrok subscription), and Kimi through the Kimi Code
CLI `kimi` (its turns run in a throwaway detached git worktree, since the CLI has no read-only
flag). The bridge never creates, prints,
stores or commits a key. C3 adds
one API path for people who will not juggle subscriptions - OpenRouter (or any
OpenAI-compatible endpoint) through a native `http` engine that reads its key from the
environment and sends the reviewer a prepared pack instead of tools - and, on top of the
same files, packs for chat models, a local SurrealDB index and telemetry-driven panel
routing. The design is in [docs/DESIGN.md](docs/DESIGN.md).

## Why a Rust rewrite

One static binary instead of PowerShell 5.1/7 on three OSes; a typed ledger and findings
store; the same behaviour under Claude Code, Codex CLI and a plain shell without a host
adapter (plugin ROADMAP R13, host invariance); and a place to make the bridge fast enough
to run a panel non-blocking (R12).

## Plan — port order (each step lands with tests and the plugin's evals as the oracle)

1. **Providers and preflight** — the Codex config walk (`[model_providers.*]`, `env_key`),
   `codex login status`, the roster file, the availability verdicts of `codex-providers.ps1`.
2. **One consultation** — brief → `codex exec` (and the `agy` / `muse` engines), the reply
   file with its header lines, the events stream, the `-DryRun` output byte for byte.
3. **Ledger and findings** — `sessions.json`, `findings.json`, ids, statuses, ratings,
   the atomic write order and the lock/recovery rules.
4. **Roster, panel, purposes, effort vocabularies, peak windows.**
5. **Telemetry and complaints** (plugin ROADMAP R17) — the terms below, the spool, `--complain`,
   `--forget-me`.
6. Claude Code packaging (skill, hook, evals) as a thin layer over the binary.

Steps 1-6 are the parity port. The heavy subsystems are explicit milestones after it
(see [docs/DESIGN.md](docs/DESIGN.md), section 10):

7. **Packs and the `http` engine** — snapshots and reviewer/explainer packs from one
   sanitizing builder; OpenRouter reviewers get a pack, never tools.
8. **Index** — embedded SurrealDB (per project, or one server per user across projects),
   rebuildable from files + repo, opt-in (`--features index-surreal`), never required for a consultation.
9. **Router** — panel thickness (`shadow` / `council` / `consilium`) and a versioned,
   auditable draw from the scoreboard with priors distributed by the maintainer's server.
10. **MCP server** — read-and-record tools for Claude Code without the plugin.

## The `claude` engine (plugin 0.6.0; C3 wave 4)

Claude Code headless as a reviewer, SPAWNED exactly as the plugin spawns it (no native Messages
API here - decision P4 of the parity task): `claude -p --output-format stream-json --verbose
--restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode
dontAsk --model <m> [--effort <e>] [--json-schema <the schema TEXT>] [--max-turns <n>]
[--add-dir <dir>...] (--session-id <minted uuid> | --resume <thread> [--fork-session])`, run from
the repository root, the prompt on stdin. Replies are `handoffs/NN-claudecode-<slug>.*` (the
prefix `claude` stays the coordinator's default brief prefix: `--brief-prefix <slug>`, else
`CODEX_CONSULT_BRIEF_PREFIX`, else `claude` - the coordinator's briefs are
`handoffs/<NN>-<prefix>-<slug>.md`; a reply prefix (`codex`, `agy`, `muse`, `claudecode`, `grok`,
`kimi`, `http`)
or a value that is not a lowercase slug is refused before anything starts, and the dry run prints
`brief prefix:`). Launcher: `--engine-exe`,
`CODEX_CONSULT_CLAUDE_EXE`, `claude.exe` / `claude.cmd` / `claude` on PATH, then
`%USERPROFILE%\.local\bin\claude.exe`.

- **Roster entry:** `{"provider": "anthropic", "engine": "claude", "model": "claude-opus-5-5",
  "auth": "subscription"}` (or `--engine claude --model sonnet`; the label defaults to
  `anthropic`). `model` is REQUIRED and one of the closed table: `opus`, `sonnet`, `haiku`,
  `fable`, `claude-fable-5-2`, `claude-fable-5-1`, `claude-fable-5`, `claude-opus-5-5`, `claude-opus-5`,
  `claude-opus-4-8`, `claude-opus-4-7`, `claude-opus-4-6`, `claude-sonnet-5-5`, `claude-sonnet-5`,
  `claude-sonnet-4-6`, `claude-haiku-5-5`, `claude-haiku-4-5`, each optionally ending in `[1m]`
  - spelled exactly so (the lookup is case-sensitive, as the plugin's: `OPUS` is refused).
  The table documents the known ids, but a new Claude model needs no C3 release: any id of the
  published shape `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]` (case-sensitive, an
  optional `[1m]`) is also accepted. The shape only admits the id; telemetry sends it by name only
  when it is in the table or published (see "The classes" under Telemetry), else as `other`.
  An alias floats: every thread is pinned to the id its first init event resolved
  (`engine_run.model_resolved`), and every later turn of it sends that id.
- **Three auth modes.** `subscription` (the default): the claude.ai login, checked by
  `claude auth status` (local, free; `ok: signed in (claude.ai subscription)`), and every turn's
  init event must name `apiKeySource none`. `api-key`: `ANTHROPIC_API_KEY` must be set NOW (a
  local check no ledger evidence replaces) and the init must name it. `endpoint`: a third-party
  Anthropic-compatible endpoint spelled out in the entry - `"endpoint": {"base_url":
  "https://api.z.ai/api/anthropic", "env_key": "ZAI_API_KEY", "timeout_ms": 3000000}` plus an
  optional `"plan": "zai"`; the model is the provider's own id (`glm-5.3`, never an Anthropic
  one - only a model the subscription cannot serve proves the billing), the preflight is local
  (`ok: env ZAI_API_KEY set`, no `claude auth status`, no live request; the launcher must answer
  `claude --version` - one that does not run is unavailable before any turn), and the child gets
  `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN` (the value of the variable `env_key` names, read at
  the launch, never logged) and `API_TIMEOUT_MS` - and nothing else of `ANTHROPIC_*`.
- **The child environment is an ALLOW list** (system, locale, proxy and trust variables,
  `CLAUDE_CONFIG_DIR`; `DISABLE_AUTOUPDATER=1` set): every host marker, test-mode variable,
  inherited gateway, model override and operator variable is absent - for every turn, the sign-in
  check and the version probe alike. The ledger records the names only
  (`engine_run.child_env_allowed`).
- **Transcripts outside the tree:** a run is refused before any turn when `CLAUDE_CONFIG_DIR`
  or the projects directory Claude Code writes its transcripts to (the one `claude auth status`
  reported, else `<CLAUDE_CONFIG_DIR or ~/.claude>/projects` - every auth mode, links resolved)
  lies inside the repository under review.
- **Read-only by evidence:** every init event (a killed turn's too) must list no tool outside
  Read, Grep, Glob, StructuredOutput, no MCP server and the permission mode `dontAsk`; the model
  every assistant message names must be the pinned one; and the strict tree check fails a run
  that changed the working tree or the collab directory (managed settings and their hooks still
  apply). A turn killed on its timeout whose init failed that proof is failed with that reason
  and never continued.
- **Rate limits:** a rejecting `rate_limit_event` beside a successful result keeps the reply
  usable, warns, and records `engine_run.quota_mark` - the route (and its plan) is out until the
  reset, in this repository's ledger and in the machine-wide health file.
- **Panels:** the claude members of a panel run one at a time (one scheduling group) unless
  `"parallel": {"<label>": n}` raises their labels; a `plan` makes the plan's codex and claude
  routes one group, and a panel member waits for a run of its plan elsewhere on the machine.

The endpoint route stays where the plugin's comparison kept it: "**Decided 2026-10-08 (wave 29c,
12 pairs per plan, handoff 40 of `.collab/claude-engine-2026-09-30/`): the route STAYS for z.ai
(credits 0.27x, wall 0.46x, structured 12/12 on both routes, blind marks 4.33 vs 4.08) and for
MiMo (credits 0.17x, wall 1.16x - no faster than codex on that plan, structured 12/12 both, blind
marks 5.0 vs 3.08).**" (plugin README, "Engines (wave 29)", "Endpoint mode"). Ported in
`docs/port/wave4-claude.md`; setup text in `plugin/skills/setup-providers/SKILL.md` section 4c.

## The `grok` engine (C3 reviewer, xAI Grok Build CLI)

Grok Build CLI (`grok`) as a READ-ONLY reviewer on the user's SuperGrok subscription (reviewer
use only; `c3 work` is out of scope). Spawned from the repository root, the prompt in a
`--prompt-file` (stdin empty, like muse):
`grok --prompt-file <f> --output-format streaming-json [--json-schema <the schema TEXT>] -m <model>
[--reasoning-effort <e>] --disable-web-search --tools read_file,list_dir,grep --disallowed-tools
use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents
[--max-turns <n>] [--resume <thread>]`. The reply is the concatenated `text` events of the
streaming-json stream; token usage comes from the `end` event, and the conversation id is READ
from that event (grok mints its own and ignores a caller-supplied one), resumed with `--resume`.
Replies are `handoffs/NN-grok-<slug>.*`. Models: `grok-4.7` (default), `grok-4.7-build-fast`,
`grok-4.6`, `grok-4.5`.

- **Read-only.** The tool allowlist is `read_file,list_dir,grep`; `--disallowed-tools` also removes
  grok's MCP gateways (`use_tool`, `search_tool`) and the write/shell tools, and `--permission-mode
  dontAsk` denies anything else. The bridge's tree check WARNS (it never fails the reply) when the
  working tree or collab directory changed during a run - grok's own tools cannot have written.
- **Billing guard.** Grok bills the SuperGrok subscription through the grok.com oauth sign-in
  (`~/.grok/auth.json`, read as a local file - never its contents). `XAI_API_KEY` /
  `GROK_CODE_XAI_API_KEY` in the environment would bill the xAI API per token instead, so c3 STRIPS
  them from the grok child unless the operator opts in with `C3_GROK_ALLOW_API_KEY=1`; `c3 providers`
  and the dry run report the route (`subscription (grok.com oauth)` vs `API key (XAI_API_KEY)`).
- **Concurrency and conduct.** One subscription is one scheduling group (parallel 1). Grok Build
  advertises headless/CI use (x.ai/build), but its AUP bans "unauthorized" automated access, so run
  it only on the user's own signed-in subscription, at parallel 1, and never spoof identity.

## Worker runs (`c3 work`, design `docs/work-mode.md`)

A worker run is a roster model as an ordinary CODING WORKER for low-stakes or test work: it
reads and edits the repository, runs the test commands it is allowed, and reports. It is NOT
a consultation — no evidence check, no findings, no rating prompt, no review-ledger entry
(`c3 consult --status` never lists it). A roster entry opts in with `"roles": ["worker"]` (the
existing `roles` key) on one of three engines:

- **`claude`** — GLM-5.3, GLM-5.3-flash (z.ai), DeepSeek-4.1-flash (byteplus): the engine that
  already drives them for reviews; writes with the Claude Code tools under the `--allow` list.
- **`codex`** — `openai :: gpt-6.2-sol`, for complex programs: `codex exec --sandbox
  workspace-write`; the write mode is the sandbox, so `--allow` does not apply.
- **`muse`** — `meta :: muse-spark-1.3-contributor`, near-free on the Muse Code subscription:
  writes and runs the shell headlessly (`--approval-mode never --trust-workspace
  --disable-sandbox`); the write mode is the workspace, so `--allow` does not apply.

`--model` overrides the entry's model id on the same engine. `run.json`'s `engine` and the
`--status` line's `[engine]` suffix name which one ran.

```
c3 work --task <slug> --brief <file> --provider <roster label> [--model <id>]
        [--prompt <text>] [--effort low|medium|high|xhigh] [--max-turns <n>=80]
        [--timeout-sec <s>=3600] [--allow "Bash(cargo test:*)"]...
        [--detach] [--status [--id <id>]] [--wait [--id <id>]]
        [--resume [<id>]] [--kick [--id <id>]] [--max-resumes <n>=3] [--quota-window-sec <s>=18000]
```

The prompt is a fixed worker preamble (never run git, never leave the repository, never
commit; finish with a final report) plus the brief file plus `--prompt`; the child runs with
the write tools (`Read,Edit,Write,Grep,Glob,Bash`, `--permission-mode acceptEdits`) under the
allow list (defaults: `Bash(cargo test|check|clippy|fmt:*)`). The record lands in
`<collab>/<task>/work/<NN>-<provider>-<model>/`: `brief.md` (the prompt as sent),
`events.jsonl` (the child's stream verbatim), `report.md` (the final assistant text),
`diff.txt` (git status before/after plus `git diff --stat`, computed by c3 — the worker
never runs git) and `run.json` (status `done|died|timeout|refused`, turns, tool calls,
files changed, usage; the argv and scrubbed env NAMES, never a secret value).

Refused before anything starts: a missing/unreadable brief, a prompt above the engine's
1 MiB cap, a label that is not a worker (the message lists the workers) or on an engine that
is not claude/codex/muse, the entry's credentials unusable (claude: `env_key` unset; muse: the
Muse sign-in not oauth), a missing launcher, `--allow` naming git
(a worker never commits), and a second run on a task that already has a live one
(`--status` exit codes: 0 all done, 1 one died/timed out, 2 one running).

The live-member rule does NOT apply to a worker run: a worker is expected to change the
tree, so you may keep working (and writing under the collab directory) while one runs.

**Cross-model tests (`--role tests`, `--of`, `--scope`, `patch.diff`).** Every run's record
holds `patch.diff`: the unified diff of what THIS run changed, from the content each file had
when it started (a file already dirty shows only the worker's hunks; the baseline content is
stored as loose git objects, never a commit). `--role tests` makes the run a test writer for
another author's change: without `--of` the change is the working tree against HEAD (the
coordinator's own code, `.collab` left out); with `--of <id>` it is that done run's
`patch.diff`. The change is inlined into the prompt between `<author-change>` tags with a
preamble: list the cases, say which existing test covers each, write tests only for the
uncovered ones, change only test code, report (never fix) a change it thinks is wrong.
`--scope <glob>` (repeatable) names the paths a run may change; a tests run adds the test-path
defaults (`tests/**`, `**/tests/**`, `**/tests.rs`, `**/*_test.rs`, `**/*_tests.rs`), so name the
source files whose `#[cfg(test)]` modules it may extend. The check runs AFTER the run (no
engine can refuse an edit by path): files outside the scope go to `run.json`
`scope_violations`, print as `OUT OF SCOPE`, and the run exits 1 (`--status` prints them but
its exit code still follows the status). A tests run is refused on the muse worker (its
unrestricted shell would read the author's change), with an empty change, and with `--of` a run
that is not done or has no `patch.diff`; a tester on the author's own provider gets a warning.

```
c3 work --task t --role tests --brief tests.md --provider ZAI-claude \
        --scope crates/c3/src/work/record.rs          # tests for the coordinator's change
c3 work --task t --role tests --of 01 --brief tests.md --provider byteplus-claude
```

**Quota, resume and the alarm.** When the worker runs out of its provider's usage limit the
run is classified `quota` (not `died`) and `run.json` records when the window resets. A
detached run resumes itself: the background parks `waiting_quota`, sleeps until the window
reopens, and relaunches the SAME child session with `claude --resume`. A foreground run
prints the resume time and the exact `c3 work --task <t> --resume <id>` line and exits 3;
`--kick` forces a resume now (waking a sleeping detached run, or resuming a foreground one).
Resumes are capped by `--max-resumes` (default 3); past the cap the run stays `quota` with
`exhausted: true` and needs a human. `--status`: `waiting_quota` reads as running (exit 2),
`quota` as needing a hand (exit 1).

## Quota windows and the reading budget (plugin wave 30; C3 wave 8)

A subscription's quota is spent in windows - on a ChatGPT plan a 5-hour window and a weekly one -
and ONE review can spend a whole 5-hour window: a fork replays its parent's context on every tool
call, and a reviewer's own sub-agents replay theirs. C3 reads what Codex itself writes into each
thread's rollout (`<codex home>/sessions/<yyyy>/<mm>/<dd>/rollout-<ts>-<thread>.jsonl`, the last
`token_count` that names a window) and acts on it, as the plugin does (`docs/port/wave8-quota.md`):

- **Ledger** (codex runs, after `commit_wait_ms`): `quota {primary, secondary, observed_at}`,
  `tokens {input, cached, output, thread_input, calls, max_input, context_window, subagents {count,
  input, calls}}` (the run's share of the thread's running total - a fork inherits its parent's),
  `subagents_allowed`, `budget`; the summary's `quota      :` line and the handoff's
  `Quota (rollout):` line.
- **Verdict**: the newest observed window of an endpoint (its ledgers, the machine-wide health
  records - which now carry `quota` - and, for the built-in `openai` endpoint while the
  machine-wide health is on, the codex home's newest rollout): at 98 % or more with its reset ahead
  the endpoint is `unavailable (5-hour window 99 % used, resets <iso>)` like a usage limit with a
  known reset (roster walk, `--require`, panel, `--short`, a real run refused); at 80 % or more the
  verdict says `available (low quota: 5-hour window 84 % until 18:32)`. `c3 providers` has a QUOTA
  column (`5h 96% until 18:32; wk 15% until Oct 20`) and `--json` rows a `quota` object.
- **Plan caps (`ext.c3.plan_caps`, `ext.c3.reset_hint`)**: a roster entry whose plan has a monthly /
  weekly / daily cap may say so under its own `ext` (`"ext": {"c3": {"plan_caps": ["monthly"],
  "reset_hint": "monthly:27T00:00+08:00"}}`; the plugin ignores `ext`). Two instant 429s with no
  window text within an hour (distinct consultations, newer than the last usable reply) then mark the
  entry `unavailable (plan quota: 429 twice in <n> min, no window text; retry after <iso>)` until the
  hint's next reset instead of the 10-minute burst. That is INFERRED exhaustion, not evidence: the
  hint is the operator's statement about the plan; without one the entry is out until the pair is 24 h old and then
  probed (eligible again - not proof that capacity returned; another instant 429 marks it out
  again); a transient pair on a capped plan costs the time to the reset - clear it with a
  successful explicit run (`--skip-preflight`: the verdict is advisory for a direct run) or by
  editing the hint.
- **Inactive plans/labels (`ext.c3.inactive`)**: a top-level `"ext": {"c3": {"inactive":
  ["byteplus", ...]}}` lists plan slugs or provider labels (each matches an entry by its `plan`
  OR its `provider`) to keep fully preconfigured but INVISIBLE. A matched entry keeps its roster
  position (so `#n` of the others stays valid) and is never selected, scheduled, probed or listed:
  it is dropped from panels (every purpose / `--panel-all`), `c3 work` worker roles, the router
  scoreboard, coordinator-identity resolution, availability/health probes and the SessionStart
  `N of M reviewers available` count, and `c3 providers` hides its rows and the codex-config
  provider row an item names (with a footer `inactive (hidden): byteplus - see ext.c3.inactive in
  <roster path>`). A direct request for one (`#7`, `byteplus-claude`, `byteplus-claude :: ...` in
  `--require`, a matcher, `--provider`) fails with `reviewer '<x>' is inactive (ext.c3.inactive in
  the roster)`. A `parallel` key that names an inactive plan/label stays valid. The plugin ignores
  `ext`, so the roster still loads there unchanged.
- **Protected and on-request reviewers (`ext.reserve_percent`, `ext.on_request`)**: two settings
  under a roster entry's own `ext`. `"reserve_percent": N` (1..50) protects a scarce reviewer: on a
  long budget window (24 h or more, reset still ahead) the entry is HELD when
  `used% >= min(elapsed% + N, 100 - N)` - it may not run more than N points ahead of the calendar
  and its last N % is kept back. A held entry is skipped by the default and by a panel and refused
  when named; the hold is waived on `acceptance` and `core-contract` and by `--spend-reserve`.
  `"on_request": true` keeps an entry out of every automatic seat: it runs only when named exactly.
  A consultation without `--provider` goes to the roster's first available entry only on
  `acceptance` and `core-contract`; on every other purpose it is one routed pick
  (`--panel-seed` reproduces it). `c3 providers` and `c3 router explain` show both states.
- **One seat per lab, and the upper tier on weighty questions**: a panel seats at most ONE roster
  entry per lab - the model's maker, from the entry's `"lab"` or else its model id's prefix (`glm`
  zhipu, `qwen` alibaba, `deepseek` deepseek, `kimi`/`k3` moonshot, `mimo` xiaomi, `gemini` google,
  `muse` meta, `grok` xai, `gpt`/`o<n>` openai, `claude` and its aliases anthropic, `dola`/`seed`
  bytedance; a claude endpoint entry without a `lab` takes its host's), never the provider label:
  `kimi` and `kimi-claude` are one lab, Qwen and DeepSeek behind `alibaba-claude` are two. Each lab
  is represented by its best-weighted eligible entry (the first in roster order without ratings)
  and the draw runs among labs; a sibling shows `not picked: lab zhipu already seated by #3`, and a
  size above the lab count is cut to one seat per lab with a warning. Every `--require`d entry is
  seated (two of one lab with a warning), `--panel-all` seats every eligible entry (one warning
  names the shared labs), and the bare default's single routed pick is unchanged. On a weighty
  purpose (`framing`, `decision`, `core-contract`, `acceptance`, `stuck`) a panel of two or more
  seats gives one seat to a `"panel": "weighty"` entry when one is eligible (available, not held,
  not on-request) and no required entry already is one (rule `tier-draw`; `tier` in roster order).
  The ledger's `panel.routing.ext.c3.seating` names the rules whenever they could change the seats,
  so `c3 router replay` replays them; `c3 router explain` marks an entry its lab's pick keeps out.
  The plugin-compat run (`C3_PLUGIN_COMPAT=1`, the harness oracle) keeps the plugin's draw.
- **`--budget <percent>`** (env `CODEX_CONSULT_QUOTA_BUDGET`, default 90, 0 = off): the stall watch
  reads the run's rollout every 15 s and stops the main turn at the budget - the partial kept, no
  continuation, `failed: stopped at the quota budget: 5-hour window 91 % used, resets 18:32; the
  partial is kept` (provider_failure class `operator`, telemetry class `quota`).
- **Replay guard**: a `--mode fork|resume` whose parent rollout is over `CODEX_CONSULT_FORK_MAX_KB`
  (default 512) is refused before anything starts; from 128 KB on it is warned about.
- **Sub-agents off**: every codex turn passes `-c features.multi_agent=false` AND
  `-c agents.enabled=false` right after `-m` (the latter is what disables codex-cli 0.169.0's
  multi-agent v2, whose tool surface the feature flag alone no longer removes), unless
  `--codex-config` (or the roster's `codex_config`) turns sub-agents on (`agents.enabled=true`, or
  `features.multi_agent=true` with `agents.enabled` unnamed); an item that turns them off by one
  key only still gets the other switch.
  As defence in depth a guard watches each run's rollout and stops any turn that spawns a sub-agent
  anyway (a `spawn_agent` call made in the current turn, scoped by its launch time so a resumed
  thread's older history does not false-stop it, or a child rollout of this thread), keeping the
  partial and recording the stop as a tooling failure (class `operator`). `c3 maintain`'s codex
  smoke reads its own rollouts and fails - rolling a CLI update back - when the switch did not hold.
- **Reviewer isolation**: every codex review turn also passes `-c features.plugins=false` and
  `-c skills.include_instructions=false`, so the reviewer sees none of the operator's Codex plugins
  (their skills, MCP servers and apps) and no skill list; a key `--codex-config` names is left to
  it. `c3 work` worker turns do not get these.
- **Reading discipline**: the purpose paragraphs of acceptance, diff-review, checkpoint and
  core-contract end with the plugin's block (`git diff <range> --stat` first, one path at a time
  with a line cap, the documents first, the verdict even on an incomplete read).

## Telemetry (on by default, off with one line)

Installing C3 means accepting these terms. C3 reports **one anonymous event per
consultation** (and one per usefulness rating) to the maintainer's intake,
<https://xelth.com/T/>, so the bridge can be improved when someone the maintainer never hears
from uses it. Everything collected is shown in the open, as counts, on <https://xelth.com/C3/>.

* **Sent - classes only:** `app_id` (`c3`), the C3 version, an instance id
  (`sha256(salt file + machine name)` — stable per installation, meaningless elsewhere), a
  severity, and `details`: the engine, the reviewer's VENDOR CLASS and model (below), the purpose
  (one of the eight purposes, `none` or `other`), the outcome class (`usable`,
  `usable-after-continuation`, `failed:<class>` with a closed class), wall seconds, token counts,
  findings counts, structured or not, a format retry or not, panel size, the federation peer
  COUNT, OS, runtime, and `consult_ref` (below). `tags` repeat `[provider, model]`.
* **The rating event** (`c3 findings --rate`, `c3 telemetry --backfill-ratings`): `details` exactly
  `engine, provider, model, purpose, mark, age_days, bridge_version, os, ps_version, judge,
  rating_rev, consult_ref` (the plugin's 0.6.1 shape; `ps_version` is `unknown` - C3 runs no
  PowerShell), `title` the mark, `client_time` the MARK's own time on every path. `judge` is
  `{provider, model, source}` - who gave the mark, resolved at rating time and saved in the mark:
  `rating_actor` (`CODEX_CONSULT_COORDINATOR` of the rating process), `consult_coordinator` (the
  coordinator the consultation recorded) or `unknown` - classes only, never a label or a host.
  `rating_rev` is the mark's revision (1 + the consultation's highest); the intake keeps per
  `(instance_id, consult_ref)` the rating with the highest `rating_rev`.
* **`consult_ref`:** a random id minted for each consultation (a lower-case guid, derived from
  nothing - not the `consult_id` the reviewer sees), kept in the ledger right after `consult_id`
  and sent in the consultation event and every rating event of it, so the intake can link them.
  A pseudonymous correlation key: whoever holds both the telemetry and a shared `sessions.json`
  can join them on it.
* **The classes** (the plugin's closed vendor table; `c3/src/telemetry/classes.rs`): the vendor
  class is derived from the HOST of the endpoint the reviewer talked to (`openai` - also Codex's
  built-in provider -, `zai`, `xiaomi`, `byteplus`, `moonshot`, `alibaba`, `minimax`), else from
  the engine (`agy` google, `muse` meta, `claude` anthropic, `grok` xai), else `other`; the model is sent only
  when it EQUALS an entry of that class's closed list (after lower-casing, `[1m]` stripped),
  else `other` (`unknown` without a model). A roster label, a model or a purpose as typed - say
  `customer-acme` - never leaves the machine. **C3 extension (the `http` engine):** the host
  `openrouter.ai` is the class `openrouter`; its model is sent in OpenRouter's `<vendor>/<model>`
  form only when the vendor slug is one of `openai`, `anthropic`, `google`, `z-ai`, `moonshotai`,
  `minimax`, `xiaomi`, `qwen` (alibaba's list), `meta-llama` (meta's list) AND the model part
  equals an entry of that class's list (`openai/gpt-5.1`), else `other`; an http endpoint of a
  listed vendor's host takes that vendor's class, any other host `other`. **Published names:** a
  model outside its class's list is also sent when `c3 maintain` holds it from a TRUSTED public
  source FOR THAT CLASS - the built-in catalog (OpenRouter; `qwen/qwen4-max` counts as `qwen4-max`
  of `alibaba` only), or a configured catalog / page declared `"public": true` (a page's plain ids
  count for the class of its own host, e.g. `docs.z.ai` -> `zai`) - judged per catalog URL, never
  per host (another URL on the same host is trusted only by its own entry). An endpoint's own model list, an
  engine's list, an undeclared configured source, another class's name and a reviewer without a
  class never count; without a maintain state the closed lists alone decide.
* **Never:** task names, prompts, briefs, paths, thread ids, `consult_id`, finding texts, notes,
  topics, labels as typed, the coordinator's host, user names, keys. The server drops
  secret-named keys on arrival and reports how many; C3 treats a non-zero count as its own bug.
* **Off:** `CODEX_CONSULT_TELEMETRY=off` (any value other than unset/`on`/`1`/`true`/`yes`
  counts as off), or `--telemetry off` per run - a run's own `--telemetry on|off` wins over the
  variable, and a panel passes it to its members. The first run after an install prints this
  notice once; the dry run says whether it is on.
* **Never blocks a run; never lost silently:** events queue in C3's own outbox
  (`<codex home>/c3/telemetry/spool.ndjson`, one `{v, kind, queued_unix, body}` line each - not
  the plugin's `telemetry-spool/`: C3 is its own app with its own salt and instance id), are sent
  by a detached sender a run starts right after its commit (a consultation, a panel - once -, a
  rating, a backfill; the run never waits for it, and it gets an allow-listed environment only: no
  key, no host marker) in batches of at most 100 (one flush ends after 60 s, one request after 8 s),
  retried by the next sender and dropped 7 days after they were queued. The sender follows the
  intake's answers as the plugin does: a 429 whose `Retry-After` is at most 60 s is waited for
  once (else the batch stays queued), a 400 naming `events[i]` drops that event and resends the
  rest (at most three per flush), a 413 halves the batch and drops an event too large alone -
  each refused event is one line of the last flush's `rejected` - and a 403, another refusal or no
  answer stops the flush with the outbox kept. Appends and the sender are synchronised by a lock,
  and the sender removes only the lines it delivered or dropped - an event queued while it sends
  stays queued. The sender lock's owner record (`flush.owner.json`) lets `--status` and a refused
  sender say "sender busy since <t>", and after 30 minutes "sender stuck since <t> (pid <n>)". An event that cannot be
  spooled is said and counted (`c3 telemetry --status`); a rating's event is retried and else
  left for `c3 telemetry --backfill-ratings`.
* **Complaints:** `c3 --complain "<text>"` prints the exact payload (your text plus the last
  ledger entry's summary - classes only), asks before sending, and prints a public reference
  like `T-7KQ4-M2XZ` to quote on the forum section <https://xelth.com/F/p/c3> or in a mail.
* **Delete my data:** `c3 forget-me` (or `c3 telemetry --forget --public-ref <ref> [--local]`,
  or `DELETE https://xelth.com/T/v2/instances/<instance_id>?public_ref=<ref>`) asks the intake to
  remove every event and complaint this installation ever sent - proved by a complaint's public
  reference - and removes the local salt, outbox and references only AFTER the intake confirmed
  it. A DELETE the intake does not confirm deletes nothing locally: the instance id and the
  reference stay, the pending deletion is recorded (`forget-pending.json`) and nothing is
  spooled or sent until `c3 forget-me` retries it with the same identity and the intake
  confirms. Without any public reference the intake cannot verify ownership:
  `c3 forget-me --local` (or `c3 telemetry --forget --local`) removes the local data only, and
  says that the intake keeps what was sent.
* **`c3 telemetry`:** `--status` (the switch and its source, the intake, the outbox, the events
  not spooled since the last flush, the sender lock's owner, a pending deletion, the last flush,
  the instance id),
  `--flush`, `--forget`, `--backfill-ratings [--dry-run]`. The intake is
  `CODEX_CONSULT_TELEMETRY_URL` when set (https only; plain http only to a loopback host in test
  mode), else `https://xelth.com/T`.

Client rules and a 30-line Rust reference client:
[xelth.com/docs/t-hub](https://github.com/xelth-com/xelth.com/tree/main/docs/t-hub).

## Build

```sh
cargo build --release          # target/release/c3
./target/release/c3 --version
./target/release/c3 providers --short   # the reviewer availability line
cargo test && cargo clippy --all-targets -- -D warnings
```

### Light bundle or indexed bundle

- **Light bundle (the default).** `cargo install --path crates/c3-cli` builds without
  SurrealDB, in minutes; `c3 index` reports `none`. Every consultation works: the reviewers
  that run through Codex, Claude Code, agy and muse read the repository themselves.
- **Indexed bundle.** `cargo install --path crates/c3-cli --features index-surreal` adds the
  embedded SurrealDB code index (~26 min more to build, ~180 MiB store per project). Build it
  for the `http` engine's reviewer packs and the planned browser engine, where the pack's
  periphery comes from the index.
- **Invariant:** the index is never required for a consultation. (Flipped 2026-10-14: the
  index is opt-in.) CI builds the default on Linux and Windows and the indexed bundle on Linux.

### Platforms

Windows 11 is where every harness ran and where the PowerShell plugin's evals are the oracle;
since the Linux port (`docs/port/linux-port.md`) the same tree builds, lints and tests on
Ubuntu x86_64 too, and `.github/workflows/ci.yml` runs fmt, clippy and the test suites on
`ubuntu-latest` and `windows-latest` (the `index-surreal` build only on Ubuntu). What differs on
Linux: process liveness and the tree kill come from `/proc` and `kill` instead of the Win32
APIs and `taskkill /T`, the file locks are `flock` advisory locks instead of share modes, and
a detached run is a child in its own process group instead of a `cmd.exe` launch; the files
written are the same bytes. The tests that need Windows (cmd.exe launchers, Windows launcher
paths) are gated `#[cfg(windows)]` with a reason each. macOS is untested. For a sandbox whose
egress proxy carries the credential and re-terminates TLS, the `http` engine honours
`HTTPS_PROXY`, `C3_HTTP_AUTH_PROXY=<host,...>` (no `Authorization` header, no key read) and
`C3_HTTP_CA_BUNDLE=<pem>` (extra trust anchors) — see `docs/port/linux-port.md`.

A cargo workspace of three crates: `crates/c3-core` (contracts and file formats),
`crates/c3` (runtime modules), `crates/c3-cli` (the binary). Each ported piece brings its
own dependencies when it needs them; SurrealDB arrives with milestone 8 behind the opt-in `index-surreal` feature.
The plugin's harnesses can drive the binary through `tests/shim/` (see
`docs/port/harness-shim.md`); `docs/port/m1-acceptance.md` lists the assertions milestone 1
answers to.

### Known limits (0.2.0)

- **Shared health journal with plugin 0.6.1:** with plugin 0.6.1 on the same machine the
  once-only replay of a kept journal record holds only for C3's own updates; the plugin's updater
  can re-apply a record the cap evicted until plugin 0.6.2 (its T14) - `docs/port/wave2b-compat.md`,
  "Limits".
- **Claude endpoint preflight:** the `claude --version` probe proves the launcher runs, not that it
  takes a `-p` turn; a launcher that answers `--version` but rejects the turn fails its first turn,
  and one whose `--version` needs the token is reported unavailable - `docs/port/wave4-claude.md`.
- **Claude transcript guard:** the derived location reads the child's `CLAUDE_CONFIG_DIR`, else
  `USERPROFILE` (Windows) or `HOME` (elsewhere) - Claude Code's `os.homedir()` sources - and
  refuses when neither is set; a wrapper launcher that sets `CLAUDE_CONFIG_DIR` inside itself is
  not seen before the spawn (the strict tree check fails such a run after its turn).
- **No kill without a record:** a reviewer tree is killed only when the record naming it is on
  disk - the recovery record, its fallback or `<codex home>/c3/pending-kill/<task>-<run id>.json`;
  all three failing, the tree is left alive and the task refused while it runs. A helper left by an
  EXTERNAL death of its recorded ancestors (not by C3), below an unrecorded intermediate and with a
  command line unrelated to codex, is not detectable by either bridge -
  `docs/port/wave7-acceptance-fixes.md`, "7b".

## Install into Claude Code

`plugin/` is a Claude Code plugin directory: a `SessionStart` hook and three skills
(`consult`, `coordinate`, `setup-providers`) with brief and role templates and an `evals/`
suite — a thin layer over the `c3` binary built above, carrying no reviewer logic of its own.
Install the binary first (`cargo install --path crates/c3-cli`, or a release download, on
`PATH` or pointed at with `C3_EXE=<path>`), then add the plugin directory in Claude Code
(`/plugin marketplace add`, then `/plugin install`, against this repository or a local
checkout). The plugin does **not** ship the `c3 mcp` server enabled — a skills-driven Claude
Code session already has the surface, and an always-on server would be a second, unguided way
in; add it opt-in with a repo-root `.mcp.json` (see `docs/port/mcp.md`). See
[plugin/README.md](plugin/README.md) for the exact commands, prerequisites and how it relates
to the PowerShell [`codex-consult`](https://github.com/xelth-com/claude-codex-consult) plugin
(both can be installed side by side; their `.collab` files are byte-compatible). The
switch-over procedure from the PowerShell plugin to C3 is
[docs/port/phase2-switch.md](docs/port/phase2-switch.md).

Another Claude Code session on the same machine, from a local checkout: `claude plugin marketplace
add <path to the C3 checkout>`, then `claude plugin install c3@c3` (the marketplace is named `c3`;
user scope; other sessions pick it up at their next restart), and the binary
`cargo install --path crates/c3-cli --force` (the light bundle; add
`--features index-surreal` for the embedded SurrealDB index - see "Light bundle or indexed bundle"). Once a newer plugin version is published (the version is
bumped at each release), `claude plugin update c3@c3`.

## Updating C3

C3 checks whether a newer release of itself exists and tells you — it never updates on its own.
The source of truth is the git **tags** of this repository, read once a day through the
unauthenticated GitHub tags API (`https://api.github.com/repos/xelth-com/C3/tags`); the newest
release is the highest `v<major>.<minor>.<patch>` tag, compared with the running binary's version.

- **The offer.** The SessionStart hook prints one extra line when a newer release is on record:
  `c3: update available v<latest> (installed <installed>) - review:
  https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply`. It
  refreshes the cached answer at most once every 24 h, within a 3 s budget, and is silent on any
  failure — a broken or offline check never delays or breaks a session.
- **The check.** `c3 update` (or `c3 update --check`) forces a check now and prints the installed
  and latest versions, whether an update exists, the changelog URL, the install kind it detected,
  and the exact commands `--apply` *would* run — then changes nothing. It exits `0` when up to
  date, `3` when an update exists (so a script can branch) and `1` on a fetch failure.
- **Applying.** `c3 update --apply` installs, stopping at the first failure, and every step is bound
  to the COMMIT the GitHub API named for the release (`latest_sha`), never to a tag name: (a)
  `cargo install --git https://github.com/xelth-com/C3.git --rev <latest_sha> --locked c3-cli`
  (plus `--features index-surreal` when the running binary was built with the indexed bundle); (b)
  if a **clean** source checkout on a branch is known (config `source_checkout`, env
  `C3_SOURCE_CHECKOUT`, or the git work tree around `CLAUDE_PLUGIN_ROOT`), the release tag is
  fetched from the canonical `https://github.com/xelth-com/C3.git` (never the checkout's `origin`)
  into a temporary ref, its peeled commit is compared to `latest_sha` (a mismatch skips the sync
  with "release tag does not match the GitHub API commit"), the checkout's identity (path, branch,
  HEAD, cleanliness) is re-checked right before the merge, and only then `git merge --ff-only
  <latest_sha>` runs - a dirty tree, a detached HEAD, a changed branch or HEAD, or a
  non-fast-forward is printed and skipped, never failing (a); (c) `claude plugin update c3@c3` when
  `claude` is on PATH, else it prints that command to run by hand. It refuses when you are already
  up to date or the check failed, and never touches a dirty checkout.
- **The off switch (default ON).** Set `C3_UPDATE_CHECK=off`, or `"update_check": false` in
  `<codex home>/c3/config.json`, to silence the automatic hook check. `c3 update` run by hand still
  checks. Nothing here is ever written outside `<codex home>/c3/`, no redirect is ever followed, and
  nothing but the plain GET is ever sent.
- **Throttle and stale-offer policy.** The cache records the last successful check (`checked_at`)
  separately from the last attempt (`attempted_at`). The automatic hook does not attempt a refresh
  within 24 h of a success or within 1 h of a failed attempt, and it offers only from a success
  younger than 7 days — an older answer is dropped silently, since the release it names may no
  longer exist. A forced `c3 update` whose refresh fails still prints the cached data, annotated
  `(checked <time>, refresh failed: <reason>)`.
- **Unsigned-release limitation.** `cargo install --git --rev` fetches the source at the API's commit
  over HTTPS from GitHub and builds it locally; the checkout sync verifies that the tag's commit
  equals that sha, but there is **no release signature** to verify yet. The trust you are extending
  is to GitHub's transport and to the `xelth-com/C3` repository, exactly as for a manual
  `cargo install --git`. Signed releases are future work.

## Keeping the reviewer CLIs current

Unlike its own release, C3 does update the reviewer CLIs by itself: `c3 maintain`. Once a week the
SessionStart hook starts a detached pass (the hook never waits for it) and the next session prints
its one-line report.

- **Per engine.** The installed and the latest version are read; when an update exists, the
  INSTALLED version is smoke-tested first, then the update is installed pinned to the checked
  version, `--version` is re-read, the smoke runs again, and a failed smoke rolls the update back.
  The smoke is two real turns through `c3 consult` in a throwaway repository; the second must
  return a random token from a file. codex updates through npm, grok through `grok update`, kimi
  through the vendor installer; agy is report-only; muse and claude update themselves and are only
  recorded. An engine with a live run or process is skipped and retried the next day. A smoke of a
  roster seat runs through exactly that entry (a one-entry roster), never whatever its provider
  label would resolve to. An interrupted update whose new version keeps failing its smoke, with no
  working rollback, is not left frozen: a newer vendor release is tried, the original previous
  version kept as the rollback target.
- **Models.** The ids come from the engines' own lists (`grok models`, `agy models`), from each
  Anthropic-compatible endpoint's `/v1/models` (sent with that endpoint's key; a list is shared
  only by entries with the same endpoint, key variable and plan), and from public catalogs. A
  seated id is replaced only by a successor with the same prefix and suffix and a higher version —
  a higher major included (`qwen3.8-max` -> `qwen4-max`, `glm-5.3` -> `glm-6`; an id without a
  minor counts as `.0`) — and only after the smoke passed on that entry's own plan; the roster is
  backed up first (`.bak-maintain-<date>`) and only that entry's `model` is edited (located by
  parsing, the result checked to differ in nothing else). The roster is re-read and compared with
  the snapshot the smoke was judged on immediately before the rename, and the swap is abandoned
  when it differs (passes are serialised by the pass lock; an edit landing between that re-read and
  the rename itself is not seen). A protected or on-request roster entry is
  never smoked or edited: its successor is only announced.
- **Public catalogs.** Once a week (inside the pass) or on request (`--models`), C3 reads
  `https://openrouter.ai/api/v1/models` with a plain GET that carries **no key** (https only; never
  an IP address or `localhost`, never a name that resolves to - or an answer that comes from - a
  loopback, private, link-local, unspecified or multicast address; at most 3 redirects, each to the
  same host and port and without credentials; one 15 s deadline over all of it - name lookup,
  connect and the body read included; the read stopped at 4 MB; at most 5000 ids). Ids are
  `<vendor>/<model>`; a `:variant` (`:free`, `:batch`) is skipped and every id must have the strict
  model-id shape. A catalog id is only a candidate: for any active, unprotected entry — also one
  whose endpoint gives no list, like Alibaba, Xiaomi MiMo or BytePlus — the model part
  (`qwen4-max`) is smoked on the entry's own plan; a failing one is reported ("seen in the catalog,
  not served by this plan") and tried again on a later pass, one smoke per entry and pass. Dates
  decide first: a catalog id needs a date after the seated id's (or, when the catalog does not list
  the seated id, within 180 days); a date more than ten minutes in the future is no evidence (more
  than a day ahead, the id is refused); one dated not after the seated id by ANY public source is
  refused - and once a source dated it older, it stays refused in later passes while its seated
  model is in the roster (at most 500 such records are kept, but the cap never evicts one whose
  seated id is still an active roster model: it applies to the other records, oldest first, and
  when the protected records alone exceed it they are all kept).
  An id of the entry's own list needs no date, but is refused the same way when a public source
  dates both ids. The pass
  also announces "new in the family": a catalog id not seen before with the prefix of a seated id,
  a version not lower and another suffix (seated `gemini-3.8-flash-high`, catalog
  `gemini-4-hydrogen`) — at most 5 per entry, and on a catalog's first collection only ids created
  within the last 30 days. The SessionStart line stays short; `c3 maintain` prints the full list.
  `"model_catalogs": ["<https url>" or {"url": "<https url>", "public": true}, ...]` in the C3
  config replaces the default catalog list; `"model_pages": [{"name": "<label>", "url": "<https
  url>", "pattern": "<regex with one capture group>"[, "public": true]}]` adds vendor pages whose
  every capture is a candidate id. Every source feeds successor candidates, but only the built-in
  catalog and a source declared `"public": true` make its ids published names telemetry may send
  (bound to their vendor - see "The classes" under Telemetry); an endpoint's own list never does.
  A catalog is identified by its full URL (scheme, host, port, path, query): every configured URL
  is fetched and recorded on its own, and trust is per URL, never per host.
- **By hand.** `c3 maintain` prints the report; `c3 maintain --run [--engine <name>] [--dry-run]
  [--force]` runs a pass now; `c3 maintain --models [--dry-run]` runs only the model step (no CLI is
  checked or updated, the weekly schedule does not move; with `--dry-run` nothing is smoked or
  edited); `c3 maintain --smoke [--engine <name>] [--dry-run]` runs only the smoke.
- **Switches (default ON).** `C3_MAINTAIN=off`, or `"maintain": false` in
  `<codex home>/c3/config.json`, turns the automatic pass off. `C3_MAINTAIN_DAYS` (7) sets the
  interval; `C3_MAINTAIN_MIN_AGE_DAYS` (3) holds a version released more recently than that when
  its date is known; `C3_MAINTAIN_SKIP=kimi,grok` names engines never touched;
  `C3_MAINTAIN_MODELS` is `swap` (default), `announce` or `off` (`off` fetches nothing, an
  explicit `--models` included - it says so and stops); `C3_MODEL_WEB=off`, or `"model_web": false`, turns
  the public catalogs and pages off;
  `C3_MAINTAIN_SMOKE_<ENGINE>=provider::model` names the smoke reviewer of an engine. State lives
  in `<codex home>/c3/maintain/`.

## Keeping a repository's model settings current

A project's own agent files can pin an old model and so quietly run a worker on a stale one. C3
both reports this (`c3 doctor`) and repairs it in place.

- **What is refreshed.** A `.claude/agents/*.md` whose frontmatter `model:` pins a concrete Claude
  id (`claude-opus-4-8`) is rewritten to the floating alias (`opus`) so the worker follows the
  newest model; a pin that names no known family is reported but left for a human. The
  repository-root `AGENTS.md` (the OpenCode agent file eckSnapshot writes) follows the roster: when
  one of the active roster entries is the successor of its `model:` (same family and suffix, a
  higher version), its line is rewritten to that id (keeping the original casing, `GLM-5.3` ->
  `GLM-5.4`, `Qwen3.8-Max` -> `Qwen4-Max`), never to an id the maintain state's catalog dates as
  not newer (`grok-4.20` is older than `grok-4.7`) — and because `c3 maintain` only seats a successor after its smoke passed, `AGENTS.md`
  never runs ahead of a tested id. Each edit changes one line and nothing else, and is **left
  uncommitted** for `c3 finish`.
- **When.** `c3 doctor --fix` applies the repairs and then reports what is left; the SessionStart
  hook applies them automatically and prints one line only when it changed something. No git
  command is run and nothing is written outside the repository (an agents directory or a file that
  is a link or another reparse point is skipped; a `.git` that is no repository, and `GIT_DIR` set
  in the environment, mean no repair). A file is replaced by rename after it is read again: a file
  that changed since the repair read it is left as it is (an edit landing between that re-read and
  the rename itself is not seen). The hook does not do this inside a session C3 itself started:
  every reviewer, worker and launcher probe, and every child of `c3 update`, gets `C3_CHILD=1`, so a
  read-only reviewer on the `claude` engine cannot change the tree it reviews and a worker's patch
  holds only its own change.
- **Switch (default ON).** `C3_REPO_MODELS=off`, or `"repo_models": false` in
  `<codex home>/c3/config.json`, turns the automatic hook refresh off (`c3 doctor --fix` is always
  explicit).

## Branches and worktrees

Branches integrated by cherry-pick look unmerged forever, and a branch's state usually lives only in
one session's memory. C3 keeps the state in the repository and checks it.

- **The record.** One line stored as the branch description (`git config branch.<name>.description`),
  shared by every worktree and gone with `git branch -d`:
  `c3: <state> | <owner> | <YYYY-MM-DD> | <text>`. Three states: `work` (in work; the owner names the
  session or person), `waits` (finished, waits for the operator; the text is the question put to
  him), `parked` (a WIP put aside; the text is the condition for merging it). A description whose
  first line is not such a full record (a note starting with `c3: hello` included) is somebody's own
  note: it is kept byte for byte, and the branch counts as unrecorded; so is a line whose date field
  is not a real `YYYY-MM-DD`. A description C3 cannot read exactly is never written over; the
  description is read again immediately before the write, and the write is skipped when it changed
  since C3 read it (a writer landing between that re-read and git's own write is not seen).
- **The default branch** is found independently of the checkout: `c3.defaultBranch` in the local git
  config (`c3 branches --default <name>`; it wins), then `origin/HEAD`, `init.defaultBranch`, `main`,
  `master` - each only when that local branch exists. Otherwise it is reported unknown (a `c3
  doctor` finding) and no branch counts as merged.
- **`c3 branches`** lists every local branch but the default one, and every worktree other than the
  default branch's: the recorded state, `merged` by ancestry or — when not — whether its patches are
  already on the default branch (`git cherry`: patch equivalence, the cherry-pick case; a later
  revert is not seen), commits ahead, the last commit date and the worktree path. The patch check
  costs two git processes per unmerged branch and runs for the first 50 (those without a recorded
  state first); the rest are listed "patches not checked". A branch is **unknown** when it is
  neither merged nor recorded. It also
  shows the start folder (the primary worktree): its branch, how far it is behind the default
  branch's upstream as the local refs know it (no fetch) and its uncommitted-change count. `--json`
  gives the same data. `c3 branches --set <branch> --state work|waits|parked --note "<text>"
  [--owner <name>]` records a state (the owner defaults to `CODEX_CONSULT_COORDINATOR`; `waits` and
  `parked` need a note); `c3 branches --clear <branch>` removes it. Both refuse inside a session C3
  itself started (`C3_CHILD`). Nothing is ever deleted or merged.
- **The checks.** `c3 doctor` reports unknown branches and unrecorded worktrees as leftovers (exit 1)
  and warns, without changing the exit code, when the start folder is off the default branch or
  dirty. The SessionStart hook prints one line only when there is something to say (cheap: ancestry
  only, no `git cherry`; read-only; silent inside a C3 child and outside a git repository).
  `c3 finish` names unknown branches and worktrees after its commit and never refuses because of them.
- **Switch (default ON).** `C3_BRANCH_CHECK=off`, or `"branch_check": false` in
  `<codex home>/c3/config.json`, turns the hook line off (`c3 branches` and `c3 doctor` are explicit).
- **A red CI run is named.** `c3 finish` (after its commit) and the SessionStart hook ask `gh run list
  --branch <default>` for the latest completed run and print `CI: the latest run on main failed -
  <title> (run <id>)` when it failed; never a refusal, under a 10 s (hook: 3 s) deadline, silent
  without `gh`, a login or a GitHub remote, and the hook asks once a day per repository. Switch
  (default ON): `C3_CI_CHECK=off`, or `"ci_check": false` in `<codex home>/c3/config.json`.
- **Old rule text is completed.** `c3 init` never rewrites `.claude/rules/02-delegation.md`; `c3 doctor`
  reports a C3-written copy that lacks the "BRANCHES AND WORKTREES" section and `c3 doctor --fix`
  appends exactly that section (a rewritten file is only reported).
- **C3's own throwaway worktree.** Before the `kimi` engine removes the worktree its reviewer ran in,
  every symlink and junction inside it is unlinked without being followed, so a forced removal can
  never empty a checkout a link points into. Such a worktree (directly in the system temp directory,
  named exactly `c3-kimi-wt-<32 hex digits>`) is never a finding while its run lives: the run
  records its process in the worktree's git admin directory, and a worktree whose record cannot be
  written is not used. Once that process is gone, or the worktree's folder is gone (git calls it
  prunable), it is a "stale c3 worktree" in `c3 branches` and `c3 doctor`, and `c3 doctor --fix`
  removes it the same link-safe way. A throwaway path that is itself a link, one without an owner
  record (whatever its age) and one whose owner cannot be checked (its start time cannot be read)
  are reported and left alone. Every read path runs git with `GIT_OPTIONAL_LOCKS=0`, so no check
  ever rewrites or locks the index.

## License

MIT — see [LICENSE](LICENSE).
