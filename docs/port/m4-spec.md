# M4 port spec — panel, sizing, scheduler, detached/non-blocking runs

Source: `C:\Users\Dmytro\claude-codex-consult` at commit `4ffb422673cb30cadc4eecaecd72a89d6b674afd`
(2026-09-27). File refs below are `plugins/codex-consult/scripts/<file>:<line>` unless stated
otherwise, relative to that repo.

## 1. Panel plan algorithm

Entry point: `codex-consult.ps1:1932`'s `-Panel`/`-PanelAll` branch, driven by
`Select-PanelMembers` (common.ps1:5642), `Select-PanelRouting` (common.ps1:5967) and
`Get-PanelPlan` (common.ps1:6196). Inputs: the reviewer roster (`reviewers[]` with `provider`,
`model`, `engine`, `panel` "always"|"weighty", `lab`, `roles`), the purpose, `-Engine`/`-Model`,
`-PanelSize`/`-PanelAll`, `-PanelOrder`, `-PanelSeed`, `-Require`, `-Role`/`-Roles`,
`-PanelConcurrency`, and every ledger/findings.json of the collab root.

**Availability filter (common.ps1:5642-5679).** For every roster entry, in roster order: skip if
`-Engine`/`-Model` do not match; resolve its identity; if the engine's launch invariant blocks it
mark `skipped`/`refused`; otherwise run `Get-PreflightVerdict -RosterWalk` (credentials, a
recorded auth failure, a usage limit with/without a reset time) unless `-SkipPreflight` — not
`available` -> `skipped`/`unavailable`.

**Weighty gate.** A `run`-state entry whose roster `panel` is `"weighty"` is skipped (reason
`"weighty reviewer; purpose <p> is light (use -PanelAll)"`) unless `-PanelAll` or the purpose is
framing/decision/core-contract/acceptance/stuck. `-PanelAll` takes every eligible member,
weighty included, forcing size 0 (not combinable with `-PanelSize`).

**Ordering / seats (`Select-PanelRouting`, common.ps1:5967+, "Companions").** *Eligible* =
members `Select-PanelMembers` marked `run` — one set for size, ranking, draw and exploration.
Required reviewers (`-Require`, else roster `require: {"<purpose>": [...]}`) take the first
seats in roster order regardless of the draw; one that is out refuses the whole panel before
anything starts (exit 5).

*Size* (`Get-PanelDefaultSize`, common.ps1:5069/5063): `(none)`/chore/checkpoint = 1,
diff-review = 2, framing/decision = 3, core-contract/acceptance = 4, stuck = 0 (every eligible
member). Overridden by `-PanelSize <n>` (not with `-PanelAll`); at least the required count, at
most the eligible count; **no backfill**. An eligible entry without a seat is `not-picked`
(reason `"panel size k"`), never a skip. A framing/decision panel seated below 2 warns unless
`-PanelSize` was explicit.

*Mode.* `-PanelOrder roster`: required first, then roster order, no draw. `-PanelOrder routed`
(default): a seeded weighted draw over `Get-RoutingScore` — rate `(yes + 0.5·partly + 2p)/(n+4p)`,
`p=0.5`, scaled to `[0.25, 2]`, neutral `1.125`; basis purpose+topics >= 3 pooled marks, else
purpose >= 3, else all-purpose >= 3, else neutral — over the last 90 days of ratings. While
**no** eligible member has >= 3 ratings, routing falls back to roster order (`fallback: "no
ratings"`). Seed = SHA-256 of `<task>|<purpose>|<brief sha256>|<eligible lineages sorted>
|<nonce>` (nonce: `-PanelSeed`, else `CODEX_CONSULT_TEST_PANEL_SEED`, else today's UTC date).
Per seat: SHA-256(seed || seat as 4 BE bytes); first 8 bytes give a uniform pick (top 53 bits /
2^53), next 8 decide exploration (< 0.2 = uniform pick over the pool). *Lab diversity*: while
fewer than `min(K, labs scoring >= neutral)` labs are seated, the pool is un-seated labs scoring
>= neutral (`lab-draw`/`-explore`); else every remaining entry (`rank-draw`/`-explore`). Lab =
roster `lab`, else the model id's vendor prefix, else its own lab (warns).

**Endpoint groups / `parallel` cap (`Get-EndpointGroups`:6136, `Get-PanelPlan`:6196).** One group
per provider label, merged with any label sharing its provider fingerprint (e.g. two agy labels
on one Google sign-in). A group's `Limit` is 1 unless the roster's `parallel: {"<label>": n}`
raises it (smallest across merged labels). `Effective` = sum of `min(limit, positions)` per
group, capped by `-PanelConcurrency` if smaller. `Text`: `"at once"` (effective >= member
count), `"one after another"` (<= 1), else `"at most <n> at a time"`. `-PanelConcurrency`: `0` no
cap (default), `1` strictly sequential, `k` at most k overall on top of per-group limits.

**Numbering.** After every recovery record is judged (active refuses the panel; inactive
consumed, skipping past), `n`/`NN` go to every seated member up front, member `k`: `n=n0+k-1`,
`NN=NN0+k-1`, in seat order. Files and ledger keep this order regardless of finish order.

## 2. The member run

Each member is a full child process of the same script, `-PanelSpec <base64 JSON>`
(`Start-PanelMember`:2007; spec built at 2020-2074). Command line: `-NoProfile -ExecutionPolicy
Bypass -File <panelSelf> -Task <Task> -PanelSpec <b64>` only — no `-DetachId` (that belongs to
the panel run itself, not its members). Stdout/stderr redirect to
`<temp>/codex-consult-panel-<panelId>/<NN>.out|.err`; stdin an empty file; no extra env vars. The
JSON carries: `id`, `position` (=k), `of`, `members` (panel-wide list), `roster_position`/
`provider`/`model`/`engine`, `skipped`, `listed_ids` (open findings at panel start), `n`, `nn`,
`consult_id`, `parent_pid`, `parent_start_time`, `sibling_nns` (every OTHER member's NN, union —
an agy tree check, D7), `concurrency`, `limits`, `asked`, `routing`, `role`, `panel_warnings`,
and an `args` object mirroring every CLI option of the run.

**Pending record.** `.consult.pending-<NN>.json` (`New-PendingRecord`), written by the panel run
for every seated member, state `reserved`, BEFORE any member starts (2366-2377), naming
`panel {id, position, of, parent_pid, parent_start_time}`. A member accepts its spec only when
the record names its own panel/n/NN/parent; it rewrites the record with its own `pid`/
`start_time` and only THEN checks the parent is alive — gone already: it withdraws the record
and stops, nothing started. It re-checks the parent right before launching its reviewer.

**Files.** Reply `handoffs/<NN>-<enginePrefix>-<ReplyName>-<providerSlug>.md`; ledger carries the
`panel` record (section 4).

**Failure / survivor propagation (F15-3).** A failing member never stops the others at the
default concurrency. Only with `-PanelConcurrency 1`: after each member finishes, the panel run
re-reads its pending record (2434-2443); if still `Active` (survivors), every remaining
`waiting` slot is marked `blocked`, reason `"not started: the previous member (<lineage>) left
surviving processes (<NN>.json state <state>); recover the task first"`.

**Required-member propagation (D7).** A required member without a usable reply, or a failed
start, stops all not-yet-started members; the panel exits 5.

**"Same open-findings snapshot" rule.** `listed_ids` is computed once, before any member starts,
from findings open at panel-start time, and baked into every spec — every member sees the same
set, not a sibling's mid-run answer; the NEXT panel on the task does see this one's findings
(blind within a wave, not across waves).

## 3. Panel summary block — exact formats

Emitted by `Write-Summary` (:2493-2586). Verified against a real 10-member run
(`C:\Users\Dmytro\c3\.collab\c3-design\`, panel id `043d5bfe`, `...\scratchpad\panel-01.log`).

- **Header** (:2331): `Panel 043d5bfe: 10 of 10 roster entries run, at most 8 at a time (roster
  <path>; panel id 043d5bfe-...)` (dry run adds `"(dry run - nothing is executed or written)"`).
- **Per-member plan line** (:2338), one per entry before launch: `  #1 ZAI :: glm-5.3 - member,
  n=1, handoff 02` (skipped: `"skipped: <reason>"`; not seated: `"not picked: <reason>"`).
- **Concurrency line** (:2350): `Concurrency: at most 8 at a time - endpoint groups: ZAI x1, mimo
  x1, gemini x2 one after another, byteplus x3 2 at a time, kimi x1, alibaba x1, meta x1;
  -PanelConcurrency 0 (no cap)`.
- **Timeout line** (:2352): `Timeout: 1800 s per member (the default of purpose framing);
  continuation after a timeout kill: up to 900 s on the same thread`.
- **Progress line, per member, in FINISH order** (:2429): `  panel member 10 of 10 finished:
  meta :: muse-spark-1.3-contributor [muse] - usable reply (117 s)` (failure: `... - failed:
  codex exit 1 - exceeded retry limit, last status: 429 Too Many Requests, request id:
  0217904376429760f... (43.2 s)`).
- **Summary head** (:2578): `Panel 043d5bfe: 10 of 10 entries ran (wall clock 773.7 s; at most 8
  at a time)`; with companions counts: `"... (asked <k>, started <j>, usable <i>; wall clock <w>
  s; <planText>)"`; dry run: `"<n> of <m> entries would run (dry run) (wall clock <w> s;
  <planText>)"`.
- **Per-member row** (:2580-2592), roster order, lineage/status-or-verdict/counts/prior/tail:
  `  ZAI :: glm-5.3   ADVISE   0 blocker, 4 major, 2 minor, 1 note  prior: none  243.4 s
  handoffs/02-codex-merge-framing-zai.md`; a skip adds `(n=4 and handoff 05 stay unused)`; an
  unstructured reply reads `prose (no verdict)`.
- **Not-picked line** (:2593, companions only): `  not picked (panel size 2): <lineage>,
  <lineage>`.
- **Required-missing line** (:2632, exit 5): `  required member <lineage> without a usable
  reply: <lineage> - exit 5`.

## 4. Ledger `panel{}` record

Written per member into `sessions.json`'s `codex.consults[].panel` (:2947-2963), same shape in
every entry: `id` (guid), `position`/`of` (this seat k, and seats started), `members[]`
(`{provider, model, state, reason}`, roster order), `concurrency`/`limits`
(`Get-PanelPlan.Effective`; label -> group limit), `asked` (=`of`), `started`/`usable` (int|null,
filled AFTER the panel ends, best-effort commit :2601-2615), `routing` (object|null:
`panel.routing` — mode, order, fallback, seed, nonce, size, eligible[], picked[], explored[],
required[] — `$null` un-routed / no roster).

Ledger `consults[]` ordering is always by `n` (`Add-LedgerEntry`), so seats keep roster/seat
order even though members commit in whatever order they finish.

## 5. The detached run (`-Detach`, `-Status`, `-Wait`, `-Prune`)

Writers: `codex-consult-common.ps1:7462+`. Readers: `codex-consult-detached.ps1`
(`Read-DetachedStatus:286`, `Get-DetachedJudgement:345`, `Test-PidAlive:156`,
`Get-DetachedPhrase:401`, `Format-DetachedListLine:390`) — dot-sourced by common.ps1, and alone
by the SessionStart hook.

**CLI.** `-Detach` (a single run, or `-Panel`/`-PanelAll`; refused with `-DryRun`, `-Status`,
`-Wait`, `-PanelSpec`) makes every check a real run would before the task lock — `-DryRun`'s
checks plus a missing launcher, an active recovery record, the preflight — in the CALLER's
process (a refusal: exit 1, nothing written); on success writes `starting` and returns at once,
exit 0, printing 3 lines (`:915`). `-Status [-Id <id|prefix>] [-Prune]`: read-only report of
every detached run, newest first, or the one `-Id` matches. `-Wait [-Id <id>]
[-WaitTimeoutSec <s>]`: polls every 2 s until done, prints as `-Status`. `-DetachId <guid>` is
INTERNAL to the background; refused by hand.

**The background process.** `<host> -File codex-consult.ps1 -Task <t> -CollabDir <absolute>
-DetachId <guid>` (:872,876), in the caller's cwd, `-Brief`/`-Artifact`/`-CollabDir` made
absolute. Windows: `cmd.exe /c` + ShellExecute, hidden, redirected, inheriting no caller handle
(refused if a path has `%`); elsewhere `/bin/sh -c 'exec nohup ... </dev/null >log 2>&1'`. Every
other CLI argument travels in the status file's `starting.args` (base64 CLIXML), not the command
line. An inline `-Prompt` instead goes to `<task>/.consult.detached-<id8>.prompt.txt`
(`args.PromptFile`); the background reads and deletes it.

**Status file** `<task>/.consult.detached-<id8>.status.json` (atomic replace; foreground writes
`starting` ONCE, then only the background writes). Fields, in canonical order
(`ConvertTo-DetachedRecord`:237):

| Field | Written when |
|---|---|
| `status_version` (int `1`), `id`/`id8`, `task`/`kind` (`run`\|`panel`) | foreground, at `starting` |
| `state` (`starting`\|`running`\|`done`) | foreground `starting`; background `running` then `done` |
| `exit` (int\|null) | background, at `done` |
| `started`/`updated`/`finished`/`wall_seconds` | foreground / every write / at done / at done |
| `pid`, `start_time`, `host` | background's self-report (first thing it does) |
| `budget_sec`, `purpose`, `reply_name`, `brief` | foreground |
| `members[]` `{position, lineage, state, outcome, wall_seconds, n, handoff, reply}` | foreground creates `pending`/`skipped`; background updates as members progress |
| `summary`, `log` | background at `done` (or refusal line); foreground (path) |
| `args` (base64 CLIXML\|null) | foreground only (`starting`); dropped by every self-report |

Member states: `pending`, `running`, `usable`, `failed`, `skipped`, `killed`, `blocked`,
`commit_blocked`, `orphan` — all but `pending`/`running`/`skipped` count as "finished", plus a
`skipped` whose outcome starts `not started:`.

**Liveness** (`Get-DetachedJudgement`:345): `done` -> exit 0 if `exit==0` else 1; `starting` (no
pid, < 60 s since `started`) -> 2; `never-started` (no pid, >= 60 s) -> 1; `elsewhere` (`host` !=
this machine, never judged further) -> 2; `running` (`Test-PidAlive` true here) -> 2; `died`
(pid recorded, not alive) -> 1; `unreadable` (empty/unparseable/no id/bad state) -> 1.
`Test-PidAlive` (:156): alive only if a process with that id exists AND its start time still
matches (guards pid reuse).

**`-Wait` polling**: every 2 s until done or gone; default timeout = `budget_sec`
(`Get-DetachedBudget`:7481 — per group `ceil(members/limit) x` the longest kill guard, largest
group; with `-PanelConcurrency` also `ceil(N/cap) x guard`; the larger + 120 s). Still running
after `-WaitTimeoutSec`: exit 3, run untouched.

**`-Prune`** (the one writing form of `-Status`): deletes status+log of `done`/`died` runs last
written > 7 days ago; an unreadable file too, 7 days after its own last write; a never-started
run only if its log is also untouched that long.

**SessionStart / `-List`** (`Get-DetachedPhrase`:401, `Format-DetachedListLine`:390): `-List`
prints `"detached <id8>: <judgement text> (codex-consult.ps1 -Task <t> -Status -Id <id8>)"` per
non-done run; the hook prints one aggregate phrase, e.g. `"; 1 detached consultation running
(task my-task)"` (finished = within 24h).

## 6. Exit codes

**Panel / single run** (:206-210, 2632-2638): `0` usable (panel: every member usable); `1` a
refusal or failure (panel: not every member usable); `5` a required reviewer unavailable before
start, or no usable reply during the panel; `6` detached background whose final status write
failed (result only in its log).

**`-Status` / `-Wait`**: `0` every queried run done and exit 0; `1` a run done with failure,
died, never started, or unreadable; `2` still running/starting/elsewhere (dominates: 2>1>0
across multiple runs); `3` (`-Wait` only) still running after `-WaitTimeoutSec`; `4` query
refused (`-Id` matches 0 or >1 runs, or an incompatible option).

## 7. Harness acceptance rows for M4

`tests/harness-panel.ps1` groups: `UNIT` (plan/grouping, D7 ignore prefixes, numbering past
leftovers, ledger order, endpoint health, pending liveness x3, roster `parallel` validation) ·
`DRY` x4 (plan text, pre-assigned numbers, no writes) · `RUN` x6 (true parallelism, `n`-sorted
ledger regardless of finish order, console order, `panel` fields, cleanup) · `NOLOSS` x3
(concurrent write-lock commits lose nothing) · `INFLIGHT` x3 (task lock held for the whole
panel) · `TIMEOUT` x4 (kill-with/-without survivors, F15-3) · `SEQ` x2 (`-PanelConcurrency 1`
strictly sequential) · `AGY` x2 (D7 residual) · `PARENT` x2 (panel run killed, members
self-commit then self-clear) · `SPEC` x4 (a member refuses on a gone/mismatched/dying parent) ·
`BLOCKED`/`ORPHAN`/`MEMBERKILL`/`GUARD` (write-lock contention, mid-commit-kill orphans, kill
guard firing/recovering).

`tests/harness-detach.ps1` groups: `UNIT` (judgement, budget, paths/arg round-trip, record
completion, phrase, list line, snapshot exclusion) · `IGNORE` (`.gitignore` coverage) · `REFUSE`
x6 (every foreground refusal writes nothing) · `SINGLE` x7 / `PANEL` x5 (full lifecycle: return,
status progression, `-Status`/`-List`/hook, `-Wait`, parity with a blocking run) · `WAITTIME` x2
(`-WaitTimeoutSec` exit 3; `-Wait` exit 1 on a failed member) · `AGY`/`REFUSEDBG`/`KILL`
(task-lock refusal as the background's own final status; a killed background recovered next
run) · `FABRIC` x5 (multi-run ordering, `-Id` ambiguity, timing, `-Prune`, invalid combos) ·
`OUTER`/`COLLIDE`/`CWD`/`ENC` (crash still finalizes status, id8 collision retried, cwd-relative
paths, UTF-8 preserved).

`tests/harness-3b.ps1:700` (`T4`) runs the whole harness against a copied scripts directory.
`tests/harness-pending.ps1` has no panel-specific rows.

## 8. What wave 26 ("Companions") changes — already landed at HEAD

Contrary to a naive reading of the roadmap: **D1-D11 and D14-D17 are already implemented and
shipped at this HEAD**, not "in progress". `Select-PanelRouting`, `Get-PanelDefaultSize`,
`Resolve-RequiredReviewers`, `Select-RoleAssignment`, lab diversity, and the
`panel.routing`/`not-picked`/`asked,started,usable` fields all exist in the current scripts; the
README documents them under "Companions (0.5.0, wave 26)" with a live example, and commits
`1a24112`/`6f5b301`/`aabc988` predate this HEAD. `ROADMAP.md` alone still lists R14-R16 as
"planned" — a docs lag, not a gap. `D12` (roster `ext` extension point, `d592526`) is also
landed: `ext` (top-level and per-entry) validates as an object and is otherwise ignored, reserved
for this port.

**Implication:** implement the FULL companions set (size defaults, `-Require`, roles,
routed/roster order, lab diversity) as part of M4, not a stub. Round-trip `ext` untouched.

## 9. Open questions

1. `ROADMAP.md` R14-R16 aren't marked done though code/README show them shipped — confirm doc lag.
2. Confirm which primitives the Rust bridge has vs. needs: independent stdout/stderr redirection
   per child, a process-tree kill, PID+start-time liveness.
3. CLIXML is PowerShell-specific; the port needs its own JSON wire format for the background's
   argument set with the same round-trip guarantee — not a line-for-line target.
4. The agy/muse sibling-tree-check residual (D7: same-task sibling writes are invisible to a
   member's check, another task's write is not) is a documented source gap — decide whether the
   port keeps, tightens, or drops it.
5. Confirm the port models `-PanelConcurrency` x per-provider `parallel` as one two-level cap
   (`min(cap, group effective)` summed across groups), not one flat limit.
