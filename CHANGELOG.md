# Changelog

All notable changes to C3 are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the versions follow
[Semantic Versioning](https://semver.org/). The specification of every compatibility wave is the
PowerShell plugin [`claude-codex-consult`](https://github.com/xelth-com/claude-codex-consult) at its
tag `v0.6.1`; the finding ids (Fnn-k, En, Dn) are those of the parity task's reviews and of the
plugin's own waves. Each wave's details are in `docs/port/wave*.md`.

## [Unreleased]

### Added

- **`c3 doctor` now flags a pinned agent model.** Each `.claude/agents/*.md` whose frontmatter
  `model:` names a concrete id (e.g. `claude-opus-4-8`) instead of a floating alias is reported,
  one line per file, with the alias to write instead (derived from the id's family). The aliases
  `opus` / `sonnet` / `haiku` / `fable`, `inherit`, and a missing `model:` line are fine. A pin
  makes the workspace dirty (exit 1); `--json` carries the same data.
- **C3 now repairs a repository's model settings, not only reports them.** `c3 doctor --fix` and the
  SessionStart hook (default ON; off with `C3_REPO_MODELS=off` or `"repo_models": false` in the C3
  config) rewrite a stale `.claude/agents/*.md` Claude pin to its alias, and let the repository-root
  `AGENTS.md` follow the roster's successor (same family/major, higher minor, keeping the original
  casing — e.g. `GLM-5.3` -> `GLM-5.4`). Each edit changes one line and is left uncommitted for
  `c3 finish`; the hook never runs git, writes outside the repository (a linked directory or file is
  skipped), or fails a session.
- **Every child C3 starts is marked with `C3_CHILD=1`** (reviewer CLIs, worker CLIs, launcher
  probes). The hook's repair above does nothing when it sees the mark: a reviewer on the `claude`
  engine runs the same SessionStart hook in the repository under review and must stay read-only,
  and a worker's patch must hold only its own change.
- **`c3 branches`: every branch's state is visible and recorded in the repository.** It lists every
  local branch but the default one and every worktree other than the default branch's, with the
  recorded state, `merged` by ancestry or — when not — whether the content is already on the default
  branch (`git cherry`: the cherry-pick case), commits ahead, last commit date and worktree path, plus
  the start folder (branch, commits behind the default branch's upstream as the local refs know it,
  the number of uncommitted changes to tracked files - untracked files are not counted, a C3-managed
  repository always has some); `--json` gives the same data. The throwaway worktree of a running
  `kimi` review is C3's own and is left out. `--set <branch> --state
  work|waits|parked --note "<text>" [--owner <name>]` stores one line `c3: <state> | <owner> | <date>
  | <text>` as the branch description (shared by all worktrees, gone with `git branch -d`; a foreign
  description is kept below it); `--clear` removes only C3's line. A branch neither merged nor
  recorded is **unknown**.
- **Branch checks in `c3 doctor`, the SessionStart hook and `c3 finish`.** Doctor reports unknown
  branches and worktrees without a recorded state as leftovers (exit 1) and warns, without changing
  the exit code, when the start folder is off the default branch or dirty; `--json` carries the same.
  The hook prints one line only when something is unknown or the start folder is off the default
  branch (ancestry only, no `git cherry`; read-only; nothing inside a C3 child or outside a git
  repository; default ON, off with `C3_BRANCH_CHECK=off` or `"branch_check": false`). `c3 finish`
  names them after its commit (also in `--json`) and never refuses because of them.
- **Branch and worktree rules** in the `02-delegation.md` template, this repository's own rule file
  and the `coordinate` skill: merge instead of cherry-pick, a recorded state for every unmerged
  branch, no turn ends with an unrecorded branch or worktree, the opener closes, the start folder
  stays on the default branch and clean, guests leave no uncommitted edits, worktree removal never
  `--force`s over a link, a parked privacy/security WIP is listed in the tech-debt file.

### Changed

- **A new Claude model id is accepted without a C3 release.** The closed `CLAUDE_MODELS` table is
  kept (it documents the known ids and the plugin parity), but a roster / `--model` of a `claude`
  entry (auth subscription or api-key) and the telemetry class `anthropic` now also accept any id of
  the published shape `claude-(fable|opus|sonnet|haiku)-<digits>[-<digits>]` (case-sensitive, an
  optional `[1m]`). A private endpoint's model name, which does not match the shape, is still
  withheld from telemetry.

### Fixed

- **The `kimi` engine's throwaway worktree removal could follow a link.** The reviewer can run
  commands in that worktree; a junction or symlink it left there (e.g. a `node_modules` junction into
  the main checkout) could be followed by `git worktree remove --force`. Every symlink and junction
  inside the worktree is now unlinked first, without descending into it, and only then is the
  worktree removed.
- **`c3 init` templates pinned old models.** `opus-worker.md` / `sonnet-worker.md` pinned
  `claude-opus-4-8` / `claude-sonnet-5`; they now write the floating aliases `opus` / `sonnet`
  (and the worker prose no longer names a model version), so a freshly-initialised worker follows
  the newest model.
- **`eck_finish_task` in the worker templates** is now `c3_finish_task` (the tool a C3 workspace
  actually exposes).
- **`claude-fable-5-2` (the current Fable) was missing from the closed Claude model table**, so a
  roster entry naming it was refused. It is added to the table in `c3-core` and the telemetry class
  list, and to the model lists in `README.md` and the setup-providers skill.

## [0.2.4] - 2026-11-18

### Added

- **`c3 finish` / `c3 fail` / `c3 manifest` and the MCP tools `c3_finish_task`, `c3_fail_task`,
  `c3_manifest_edit`** (eck port Part A, `docs/eck-port.md`): C3 now commits, but only through
  `c3 finish` - `git add -A` minus a never-commit set (raw event streams, `.collab` locks and
  pending files, work-run `.log`/`.stderr`), a secret gate on the staged content (path and kind
  named, never the value; `--allow-secret`), the staged list printed, then the commit; the report
  goes to `.eck/lastsnapshot/AnswerToSA.md` and newest-first into `.eck/JOURNAL.md`; a delta
  snapshot (full when no anchor) lands in `.eck/snapshots/` and `.eck/lastsnapshot/` with the
  report embedded, pruned beyond `--keep` (10), the never-commit set left out of its body.
  `c3 fail` does the same without committing (`--fail` delta to the working tree). `c3 manifest`
  edits `.eck/*.md` only (no path traversal; `append_to_section` appends at the section's end).
  The MCP tools take eck's argument names.
- **Snapshot additions (eck port Part B):** `c3 snapshot --base <commit | snapshot file>` (the
  header now records `- Anchor: <sha>`; an ad-hoc base does not move the anchor);
  `.eckignore` (gitignore syntax) honoured by every snapshot and pack; `--profile a,b,-c,<glob>`
  from `.eck/profiles.json`; the `finish`/`fail` hand-off snapshot drops every hidden path
  except the `.eck/*.md` manifests (JOURNAL.md excluded) - the first full hand-off of this
  repository was 19.5 MB with every `.collab` brief inside.
- **Project setup (eck port Part C):** `c3 init` creates the missing `.eck` manifests as
  `[STUB]`s, adds `.eck/` to `.gitignore`, writes `CLAUDE.md`, `.claude/rules/01-c3-protocol.md`,
  `02-delegation.md` and the sonnet/opus/haiku worker agents only when missing, and registers
  `c3 mcp` in `.mcp.json` beside the other servers; `c3 doctor` reports leftover stubs, the MCP
  registration, eck leftovers and the snapshot folder size; `c3 scout <dir> [--depth]` and
  `c3 fetch <glob>...` write into `.eck/scouts/`; plugin skills `c3-scout` and `c3-fetch`. The
  `finish` secret gate now scans only the ADDED lines (an old test fixture no longer re-blocks
  every commit touching its file). Fixed: `c3 snapshot --depth 0` printed an empty tree.
- **Dead detached runs leave a death record:** written by the background, or once by the next
  `--status`/`--wait` reader (which side vanished, child exit, elapsed, last engine event, a
  sanitized log tail); `--status`/`--wait` print it and the raw-stream path when it may hold a
  usable reply.
- **Per-model health mark and panel re-seat:** a "model not supported / does not exist /
  unknown model id" failure marks that roster entry (not the provider) unavailable for 24 h, a
  usable reply clears it; a panel seat lost at preflight is re-seated from the same draw.
- **Cross-model tests in `c3 work`** (consilium 2026-10-29, `.collab/consilium-2026-10-29-test-workers`):
  every work record holds `patch.diff` (the run's own change from each file's start content;
  pre-dirty baselines kept as loose git objects); `--role tests` inlines another author's change
  (the working tree against HEAD, or `--of <id>`'s `patch.diff`) into the prompt with a
  cases-first test-writer preamble; `--scope <glob>` plus the test-path defaults is checked after
  the run (`scope_violations` in `run.json`, `OUT OF SCOPE`, exit 1, also on a resumed run); a
  tests run is refused on the muse worker; `--status` prints the role and the violations. The
  tests were written by a GLM worker run through `--role tests` itself.

- **`c3 update` — self-update check and apply** (wave 11, `docs/port/wave11-update.md`): C3 now
  checks whether a newer release of itself exists and offers it; it applies only on an explicit
  flag. The source of truth is the git tags of `xelth-com/C3`, read once a day through the
  unauthenticated GitHub tags API (highest `v<major>.<minor>.<patch>`, compared with
  `CARGO_PKG_VERSION`); the answer is cached at `<codex home>/c3/update-check.json`
  (`{checked_at, latest, latest_sha, installed, source}`, atomic write, a failed fetch kept silent
  and the previous cache kept). The SessionStart hook prints one extra line when a newer version is
  on record — `c3: update available v<latest> (installed <installed>) - review:
  https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply` —
  refreshed within a 3 s budget and never failing a session; startup/resume/compact output is
  otherwise unchanged, and `plugin/hooks/c3-hook.{ps1,sh}` forward the line. `c3 update`
  (`--check`) forces a check and reports installed vs latest, whether an update exists, the
  changelog URL, the detected install kind and the exact commands `--apply` would run, changing
  nothing (exit 0 up to date, 3 an update exists, 1 on a fetch failure). `c3 update --apply` runs
  them, stopping at the first failure: `cargo install --git … --rev <latest_sha> --locked c3-cli`
  (+ `--features index-surreal` when built with the indexed bundle); then, if a **clean** source
  checkout on a branch is known (config `source_checkout`, env `C3_SOURCE_CHECKOUT`, or the git work
  tree around `CLAUDE_PLUGIN_ROOT`), the release tag is fetched from the canonical URL, verified
  against the API commit and fast-forwarded to it (a dirty tree / detached HEAD / mismatch /
  non-fast-forward is printed and skipped, never failing the install); then
  `claude plugin update c3@c3` when `claude` is on PATH, else the command is printed. The automatic
  check is ON by default and silenced with `C3_UPDATE_CHECK=off` or `"update_check": false` in the
  C3 config; in `CODEX_CONSULT_TEST_MODE=1` no network is ever touched. There is no release
  signature yet: the install trusts GitHub's HTTPS transport and the API's release commit.
- **Grok reading duty on every non-raw purpose (C3):** a live test showed Grok answering a brief in
  one model call without opening a file and INVENTING code with line citations (a `RetryPolicy`
  field and a loop that do not exist), while the same brief by reference drew 5 tool calls and a
  grounded answer. For grok, the brief is now referenced by path on every non-raw purpose (never
  inlined), and a primary turn with zero tool calls fails `capability` - "it cannot have read the
  brief" on the non-review purposes (framing, decision, stuck, chore), the existing review wording
  on the four review purposes. The failure drives the existing read-insist re-ask (up to
  `CODEX_CONSULT_READ_INSIST`, default 2; `0` = fail at once; the non-review re-ask tells it to open
  the brief), and a non-review prompt carries a `Reading duty` paragraph. `raw` grok consultations
  keep the old inline and are exempt on non-review purposes; kimi and every other engine are
  untouched.
- **`c3 maintain` - C3 keeps the engine CLIs and the seated model ids fresh by itself:** once a
  week the SessionStart hook starts a detached pass (never blocking the hook) and the next session
  prints its one-line report (`c3: maintenance 2026-11-15 - codex 0.155.1 -> 0.169.0 ok; grok up
  to date; ...`). Per engine: installed and latest version, and when an update exists - a smoke of
  the INSTALLED version first, the update, the version re-read, a second smoke, and a rollback
  when that fails. The smoke is two real turns through `c3 consult` in a throwaway repository
  (one plain, one that must read a file holding a random token). codex updates through npm
  (rollback by version), grok through `grok update` (`--version` pins), kimi through the vendor
  installer (`KIMI_VERSION` pins; https only, a redirect only to the vendor's one listed mirror); agy has
  neither a check nor a pin and is report-only; muse and claude update themselves and are only
  recorded. An engine with a live run or process is skipped and retried the next day. A version
  released less than `C3_MAINTAIN_MIN_AGE_DAYS` (3) days ago is held when its date is known. The
  smoke never spends the ChatGPT allowance when a custom codex provider can carry it, and never a
  plan whose window is at 90 % or more. Models: the lists of `grok models`, `agy models` and the
  plan endpoints are stored and new ids announced; a seated id is replaced only by a successor
  with the same prefix and suffix, the same major and a higher minor, only after it passed the
  smoke, with the roster backed up (`.bak-maintain-<date>`); stamped ids, major jumps and new
  families are announced only. `c3 maintain` reports, `--run [--engine] [--dry-run] [--force]`
  runs a pass now, `--smoke [--engine] [--dry-run]` runs only the smoke. Hardened after the
  wave's diff-review (GLM-5.3, Muse Wonder, Grok; 30 findings): the pass lock is taken by atomic
  creation; an engine is re-checked for use right before its update, by command line as well as
  image name, and an unreadable process table counts as in use; a pending-update record written
  before every install lets the next pass verify or roll back an update interrupted by a crash;
  an unparsable installed version is never updated; every update is pinned to the checked
  version and every update and rollback is confirmed by re-reading `--version`; model ids are
  accepted only in a strict shape, a dotted date stamp also makes an id unswappable, the roster
  is written through a temp file and not at all when it changed during the pass; update and
  installer children run without the provider keys in their environment; a download that
  reaches the size cap is never run; the detached pass is started without a shell. ON by default;
  `C3_MAINTAIN=off` or `"maintain": false` in the C3 config stops the hook's passes,
  `C3_MAINTAIN_DAYS`, `C3_MAINTAIN_SKIP`, `C3_MAINTAIN_MODELS=swap|announce|off` and
  `C3_MAINTAIN_SMOKE_<ENGINE>=provider::model` tune it.
- **Budget-aware panel routing (`c3 budget`):** every roster lineage's panel draw weight is now
  scaled by a plan multiplier - the plan's remaining quota share divided by the remaining share of
  its period, the most constraining window of the plan governing, clamped to 0.25..2.0 (an idle
  plan at most doubles a seat, and the boost never lifts a lineage already scored below neutral -
  a penalty, `mult < 1`, always applies). Multipliers are read live for the `openai` account (codex
  rollouts), for kimi (`GET api.kimi.ai/coding/v1/usages`, cached 5 minutes) and for z.ai and Muse
  (see "Live quota" below); every other plan reads the operator's own snapshot from `c3 budget set <plan> --window <w> --used <pct> --resets
  <iso>` (`c3 budget show` lists them all; a snapshot is `stale` past 7 days or once its own window
  has reset, whichever comes first). `CODEX_CONSULT_BUDGET_ROUTING=off` pins every multiplier to
  1.0. `c3 router explain` gains `weight`, `mult` and `budget` columns (the latter naming the
  governing window), the per-member multiplier is recorded so `c3 router replay` reproduces a seat
  offline, and `c3 providers` fills the QUOTA column for every plan that has windows, not only the
  built-in ones.
- **The native `kimi` engine:** drives Moonshot's Kimi CLI headless (`kimi -p <pointer>
  --output-format stream-json -m <model>`), resumed with `-S <session id>` read from the CLI's own
  `session.resume_hint` line; the assembled prompt goes to a temp file and only a short pointer
  travels on argv (refused past 30000 chars). Model ids are whatever the CLI's own `config.toml`
  declares (`kimi-code/k3` default, `kimi-code/k3-256k`, `kimi-code/k3.1-pro`,
  `kimi-code/kimi-for-coding`, ...; `kimi-code/k3.1-pro` confirmed live by the 2026-11-08 smoke in
  `.collab/smoke-kimi`). The CLI has no read-only flag and can write, so every kimi turn runs with
  its cwd set to a throwaway detached git worktree of HEAD (removed after the run, recorded on the
  handoff's `Isolation:` line), with the strict main-tree check as a second guard - any
  working-tree or `.collab` change there FAILS the run. Roster: `engine: kimi`; replies are
  `handoffs/<NN>-kimi-<slug>.*`.
- **Read-insist re-asks a zero-tool review:** a grok or kimi reviewing primary turn that made no
  tool calls is re-asked up to `CODEX_CONSULT_READ_INSIST` times (default 2; `0` fails at once)
  before the existing zero-tool-calls `capability` guard fails the run; a re-ask that reads (tool
  calls > 0) becomes the primary reply - kimi's re-ask resumes the same thread (grok's re-ask is a
  new conversation, see below). Both engines' review prompts now say outright that a reply with no
  tool calls is rejected. The handoff header gets a `Read insist: <n> turn(s), tool calls 0 -> <k>`
  line and the ledger a `read_insist: {attempts, final_tool_calls}` entry.
- **Live quota for z.ai and Muse:** the budget module reads the z.ai plan's 5-hour token window
  from `GET https://api.z.ai/api/monitor/usage/quota/limit` (the plan key; the monthly tools
  allowance is ignored) and the Muse 5-hour and weekly windows from `muse usage --json` (no model
  turn; "no usage yet" yields nothing), cached like the kimi source, ahead of the operator's
  snapshot. Both show in the QUOTA column of `c3 providers` and weigh the panel draw.
- **`c3 --version` names the build commit:** `c3 0.2.4 (1a2b3c4)`, with `-dirty` when tracked
  files differed from HEAD at build time, so an installed binary can be told from the tree.

### Changed

- **The bare default reviewer depends on the purpose, and two roster `ext` settings protect scarce
  reviewers.** A single consultation with no `--provider` is no longer always the roster's first
  entry: on the standing purposes **acceptance** and **core-contract** it stays the first available
  entry (the standing partner); on every other purpose it is ONE reviewer picked by the same seeded
  weighted draw a panel uses (usefulness score x effective budget multiplier), among the entries a
  panel of that purpose would seat, so repeated bare consultations spread instead of draining the
  first entry. The bare run prints the pick, its weight, the candidate count and why the first entry
  was not it; `--panel-seed` reproduces the draw (now accepted on a single consult too). Two new
  per-entry `ext` settings: `reserve_percent: N` (1..50) makes an entry **protected** - HELD on a
  non-standing purpose once `used% >= min(elapsed% + N, 100 - N)` on any long budget window (24 h+,
  non-stale, reset ahead), i.e. it may not run more than N points ahead of the calendar and keeps
  its last N % for acceptance / core-contract; it is then skipped by the bare default and the panel
  (skip kind `reserve`), refused when named explicitly, and an outage when `--require`d - unless
  `--spend-reserve` (new flag, single and panel). `on_request: true` makes an entry **on-request** -
  never seated automatically (not by the bare default, a panel draw or `--panel-all`); it runs only
  when named exactly by `--provider`/`--model` or a `--require` matcher (skip kind `request`, "on
  request only"). `c3 providers` notes the hold on the entry's row (VERDICT stays `available`) and
  `c3 router explain` marks entries `[held: reserve]` / `[on request]`. `c3 maintain`'s smoke never
  targets a protected or an on-request seat. After the wave's diff-review (Grok, Muse contributor;
  7 findings): the hold and the on-request rule are evaluated before the light/weighty gates, so
  neither a stand-in promotion nor a `--require` can seat such an entry; the hold also applies with
  `--skip-preflight` and to a reviewer pinned by `--thread` or a resume/fork; a bare `--mode
  resume|fork` continues the reviewer of the task's latest verified consultation when it is still
  eligible and not held, and otherwise draws and starts a new thread, saying so; an old quota
  reading is a floor - it may hold a protected reviewer but never raises a weight; the hold, the
  draw and `router explain` read one set of windows per run.
- **The ChatGPT account's budget window is no longer forgotten after two idle days.** When today's
  and yesterday's codex rollouts give nothing, `budget::openai_windows` falls back to the newest
  observation in the rollouts of the last 8 days, then to the recorded quota of the machine-wide
  health file / ledgers, keeping only windows whose reset is still ahead and showing the real
  observation age ("seen Nd ago"). The health verdict's own two-day rule is unchanged.
- **Grok takes the reply schema in the prompt, not as `--json-schema`:** under the native flag
  every text Grok writes must be schema-shaped, so its opening remark came out as a schema-valid
  placeholder answer ("I am reading the brief before answering") and, carrying no tool call,
  ended the turn - the root of the "Grok answered without reading" failures. Live on one fixture
  brief (grok-4.7, CLI 1.0.77): with the flag 3 of 5 fresh conversations ended on a placeholder
  with 0 tool calls; without it every conversation read the files and closed with a valid JSON
  answer. `--schema-transport native` restores the flag for one run.
- **A grok read-insist re-ask is a new conversation:** it carries the whole original prompt with
  one insisting line in front instead of resuming the thread that did not read; the recorded
  thread is the accepted turn's, and a run whose every attempt read nothing records none. The
  ledger's `read_insist` gains `new_conversation`. kimi keeps the same-thread re-ask.
- **The roster loader skips an entry with an unknown engine instead of refusing the whole file:**
  an entry whose `engine` is a well-formed name this build does not recognize is kept at its
  roster position, marked `inactive` and `unknown_engine` (so `#n` lookups still line up, but it
  is never selected, scheduled or probed), and reported once as a roster warning - `entry <n>:
  engine "<name>" is unknown to this c3 build - entry skipped (upgrade c3 to use it)` - surfaced by
  `c3 providers`. A malformed engine value (not even a well-formed name) still refuses the file as
  before. Motivated by a field report: the native kimi entry in a live roster made an
  older, not-yet-upgraded c3 refuse every consultation on that machine.

### Fixed

- **A run that died in a re-ask is described by that re-ask** (field report, xelixir, detached
  run a7dde5eb): a grok diff-review whose re-ask made 57 tool calls over ten minutes and then
  stopped without a final answer was recorded as "answered in one model call ... 0 tool calls
  (wall 0 s)" with the first attempt's placeholder as the reply. Now the outcome is the last
  attempt's own failure, classified like a primary turn's and named `read-insist attempt N after
  a zero-tool primary; K tool calls`; the summary's events file is that attempt's stream; its
  salvaged text is the reply; the member wall is the total of the attempts. The grok answer is
  the text after the last tool call, so an interim note is never taken for it, and the grok
  prompt says the structured answer is written once, at the end. From the wave's diff-review: a
  grok turn that wrote a note, read files and ended without any text after its last tool call is
  a failed turn (the note is kept as salvage, never as the answer); a re-ask that could not even
  start keeps the zero-tool outcome and names the launch failure; the reply of a run that died in
  a re-ask is that attempt's text after its last tool call, or empty - never the first attempt's
  placeholder.
- **Quota reads (from the same review):** a quota GET that carries a plan key (z.ai, kimi) never
  follows a redirect; the live-source caches are keyed by source and plan; a live window whose
  reset has passed is dropped instead of feeding the draw its pre-reset usage.
- **`c3 budget set --resets` accepts minute-precision instants:** `2026-11-16T15:18Z` and
  `2026-11-16T15:18+01:00` - the shape `--help` advertises - were refused as "must be a date or ISO
  instant"; only the form with seconds parsed.
- **A grok code review that read nothing is no longer usable:** on `acceptance`, `core-contract`,
  `diff-review` and `checkpoint`, a grok reply with zero tool calls (one model call, e.g. "HOLD -
  I am still reading", 13.8 s) fails with class `capability` (no outage mark); framing and decision
  answers without tool calls stay usable.
- **Review-2026-10-30 fixes:** the `finish` secret gate fails closed (unreadable list or diff),
  reads `+` lines inside hunks only, scans typechanges, binary blobs and the commit message,
  refuses key-container files; agy writing-tool calls always fail the drift check, tool names come
  from every turn; a claude init listing a writing tool is not write-disabled; panel re-seat exit
  and summary over the final seats, a new seat number, the endpoint concurrency group,
  `routing.picked` rewritten; no death record while the bridge lives (task lock around the final
  write); a model-unsupported mark survives later failures; detached children try
  `CREATE_BREAKAWAY_FROM_JOB`.

- **grok reviewer answered an invented brief** — a grok `--json-schema` turn can answer in one
  model call without calling `read_file`; it then made up the brief from the question numbers
  (panel 2026-10-29, handoff 05). The brief's text is now inlined into the prompt after the
  "Read the brief at" line for engines whose spec sets the new `EngineSpec::inline_brief` (grok
  only). Tests in `consult::prompt` and `lineage` written by a GLM worker run (`c3 work`).

- **Wave 11b (`c3 update` hardening, Astra handoff 97)** — the self-update now binds to the GitHub
  tags API's release **commit**, not the tag name: `--apply` installs with
  `cargo install --git … --rev <latest_sha>` and synchronises a source checkout by fetching the
  release tag from the canonical `https://github.com/xelth-com/C3.git` into a dedicated ref (never
  the checkout's `origin`), verifying its peeled commit equals `latest_sha` before any merge and
  fast-forwarding to that sha — a fork or stale local tag with the release's name can no longer move
  the checkout (F97-2). The checkout's identity (work-tree root, branch, HEAD, cleanliness) is
  captured in the plan and **revalidated immediately before the merge**, after the fetch, so a
  branch switch, a detached HEAD or a tree that became dirty during the lengthy `cargo install`
  skips the sync instead of fast-forwarding the wrong thing (F97-1). The cache now records
  `attempted_at` (every attempt) separately from `checked_at` (last success): the hook backs off for
  1 h after a failed attempt and 24 h after a success, offers only from a success younger than 7
  days (a stale answer that may name a deleted release is dropped silently), and `c3 update` prints
  the cached data with a `refresh failed` note when a forced refresh fails (F97-3).
- **Wave 11c (Astra acceptance minor, handoff 99)** — the self-update backoff test now observes the
  fetch directly: the test-mode fetcher counts each actual attempt into
  `C3_UPDATE_TEST_FETCH_COUNT_FILE` (test-mode only, no network) and the test asserts exactly one
  fetch across two hook runs, so a throttle miss can no longer hide behind whole-second timestamps
  (F99-2).
- **Codex sub-agents stay off on codex-cli 0.169.0 (and a guard catches it if they don't):** the
  CLI update moved the live subsystem to multi-agent v2, whose `spawn_agent` tool surface
  `-c features.multi_agent=false` no longer removes (a model's `multi_agent_version` metadata
  outranks the feature flag), so an acceptance run still spawned three sub-agents and burned ~14 %
  of the ChatGPT weekly allowance in minutes. Every codex turn now also passes
  `-c agents.enabled=false` (proven live on a GLM reviewer with a forced-v2 model: the tool surface
  is present without it and gone with it); the legacy switch is kept for older CLIs and both are
  harmless on a CLI that does not know the key (codex ignores an unknown `-c` key without
  `--strict-config`, which the bridge never passes). The user's opt-in still wins: a
  `--codex-config` that turns sub-agents ON (`agents.enabled=true`, or `features.multi_agent=true`
  with `agents.enabled` unnamed) passes none of the off switches; one that turns them OFF by a
  single key still gets the switch it did not name, so "off" means off on every CLI.
  As defence in depth, the watch that already polls a run's rollout for the quota budget (every
  15 s) now also notices a sub-agent - a `spawn_agent` call in the rollout, or a child rollout
  naming this thread as parent - and stops the main, continuation or repair turn at once, keeping
  the partial exactly as the budget stop does; the outcome names the codex version, and the failure
  is class `operator` (a tooling failure, not counted against the reviewer's quota health or its
  capability).
- **Acceptance findings of the standing partner (task release-0.2.4, handoff 09, recovered from
  its rollouts after the run above was stopped; R1-R7):**
  - `c3 maintain` keeps its recovery journal after a failed install or rollback (R1): a nonzero
    installer, an update that did not take, or a rollback that did not restore now retains the
    pending-update record, so the next pass re-probes and smokes or rolls back the result instead
    of reading a half-installed CLI as "up to date".
  - `c3 maintain` fails closed when the pending record cannot be written (R2): `write_pending` /
    `set_pending` propagate directory, write and rename errors, and the pass holds without running
    the installer when the write-before-install record was not persisted.
  - The `c3 maintain` model swap is immune to a roster edit during the smoke (R3): the protection
    and saturation gate and the swap's comparison basis are read from one fresh pre-smoke
    snapshot, and `apply_swap` refuses to write when the roster on disk changed since it, so
    protecting the entry or re-pointing its old model during the smoke aborts the swap.
  - A protected on-request reviewer is no longer spent by an exact `--require` (R4): the reserve
    hold is evaluated before the on-request gate, so an entry that is both `on_request` and over
    its `reserve_percent` carries the hard `reserve` skip and a non-standing panel that requires
    it refuses (exit 5) unless `--spend-reserve`.
  - Budget windows keep their own observation age (R5): a merged quota reading uses each window's
    own `primary_observed_at` / `secondary_observed_at`, so a recent 5-hour reading no longer makes
    a retained old weekly reading look fresh and raise a routing weight above 1.
  - A future-dated quota observation can no longer evict a real one (R6, the open part of F06-4):
    future-dated per-window candidates are dropped before the account merge, so a clock-skewed
    reading cannot win on recency and discard a genuine held reading (a 97 % weekly whose reserve
    hold would vanish).
  - An uncommitted brief reaches the `kimi` reviewer (R7): the brief is staged into kimi's
    throwaway HEAD worktree at its repo-relative path, so a brief that exists only in the caller's
    working tree is the one the reviewer reads. The other R7 question was refuted: a skipped
    unknown-engine roster entry keeps its position, so `#n`, the first entry and `--provider`
    resolve as the file shows them.
- **Second acceptance pass of the standing partner (task release-0.2.4, handoff 11, F11-1..F11-4):**
  - `c3 maintain` no longer clears a pending update whose recovery failed (F11-1): after a failed
    recovery smoke, only a rollback that both runs AND restores the previous version clears the
    record. A failed rollback, a rollback that did not restore, or no rollback path now reports
    `pending update unresolved: ...` (never `recovered`) and keeps the record, so a later due pass
    smokes the still-broken CLI again instead of reading it as up to date. The recovery smoke stays
    behind the normal due/forced gate (the SessionStart hook spawns a pass only when one is due), so
    a retained record never spends a consultation on every session start.
  - The `kimi` reviewer's reply is only the answer after its last tool call (F11-2): text a kimi
    turn emits BEFORE a tool call (an early verdict or placeholder) is no longer taken as the usable,
    verified reply when the turn ran tools but produced no answer after the last one - that turn now
    fails like a stream that ended without an answer, and salvage keeps only the post-tool tail, as
    the grok adapter already does. In both adapters that failure is now recorded with its reason as
    the message: the reviewer's interim text is no longer the failure evidence, so a placeholder that
    mentions "quota" or "timeout" cannot be classed as a quota or transport failure of the provider.
  - Every `kimi` turn stages the operator brief into its worktree (F11-3): read-insist, continuation
    and repair turns create a fresh HEAD worktree too, so an uncommitted or locally modified brief is
    present at its repo-relative path on the very turn a zero-tool primary is re-asked to read it; a
    staging failure now refuses the turn before the reviewer launches.
  - The sub-agent guard no longer kills a compliant resumed run (F11-4): the scan for a `spawn_agent`
    call in the run's own rollout is scoped to the current turn (a call counts only when its line
    timestamp is at/after the turn's launch), so a resumed thread whose history holds an old spawn
    call is not stopped when the current turn spawns nothing; a new spawn still stops it, and the
    child-rollout detection is unchanged. A forked thread is covered only as long as Codex keeps the
    original timestamps on the history it copies - not observed on codex-cli 0.169.0.
- **Third acceptance pass (handoff 13): ACCEPT at 7c1e91f.** One minor finding is left open, F13-1: the
  time bound of the sub-agent guard is the start of the budget watch - taken after the reviewer process
  is launched and set two seconds back - not the launch itself, so a `spawn_agent` call from the last
  two seconds before a resumed turn would still stop it. F06-4 stays open in one part (the rollout
  reading is returned before the recorded one; the quota scan is scoped by provider, not by account).
  No real CLI update, rollback, roster swap or reserve hold has run live yet.

## [0.2.3] - 2026-10-22

### Added

- **R30 wave-boundary compaction** (`plugin/skills/coordinate/SKILL.md`): a Claude Code coordinator
  now compacts right after a wave launches, not mid-review, and recovers its identity afterwards.
  A detached launch (`c3 consult --detach`, `c3 consult --panel --detach`, the MCP `c3_panel`
  tool) prints one extra stderr line after its `status`/`come back` lines — `compact now (wave
  boundary): /compact keep the task ..., the detach id ... and the coordinator record; then
  \`c3 consult --task ... --status --id ...\` on wake` — not in `--dry-run`, not from the
  background child, not from panel members. The SessionStart hook now also fires on the `compact`
  source (`plugin/hooks/hooks.json` matcher, `c3 hook --source`): after a compaction it prints one
  re-hydration line restoring the coordinator identity (`CODEX_CONSULT_COORDINATOR`, resolved
  against the roster) when a coordinator record exists, so the session picks its detached runs (the
  availability line already names them) back up; `startup`/`resume` output is unchanged.
- **`c3 work` — worker runs** (`docs/work-mode.md`): a roster model (engine claude, role
  `"worker"`) as an ordinary coding worker for low-stakes or test work — it edits the tree and
  reports; not a consultation (no evidence check, no findings, no rating, no ledger entry). The
  prompt is a fixed preamble (never git, never commit, a final report) + the brief + `--prompt`;
  the child runs with the write tools under an allow list (`Bash(cargo test|check|clippy|fmt:*)`
  by default, `--allow` to replace, git refused); the record is
  `<collab>/<task>/work/<NN>-<provider>-<model>/` with `brief.md`, `events.jsonl`, `report.md`,
  `diff.txt` (git status/diff computed by c3, never by the worker) and `run.json` (status, turns,
  tool calls, files changed, usage — no secret values anywhere). One live run per task;
  `--detach`/`--status`/`--wait` as the consult flags (status exit 0 done / 1 died / 2 running).
- **`c3 work` — quota, resume and the alarm** (`docs/work-mode.md`): a worker that runs out of its
  provider's usage limit no longer just dies. The run is classified `quota` (a rejecting
  `rate_limit_event`, or the limit wording — the engine's shared `quota_evidence`), `run.json`
  records `quota {text, reset_at, resume_at, resumes, exhausted}`, and the SAME child session is
  resumed with `claude --resume` when the window reopens: automatically in a detached run (the
  background parks `waiting_quota` and sleeps in 30 s slices until `resume_at`, waking on a kick
  file), or on demand with `c3 work --resume <id>` / `--kick`. A foreground quota run prints the
  resume time and the exact resume command and exits 3; resumes are capped by `--max-resumes`
  (default 3), after which the run is `quota` with `exhausted: true` and needs a human. The
  resume window when the provider gives no reset time is `--quota-window-sec` (default 18000 = 5 h)
  or a roster entry's `ext.quota_window_sec`.
- **`c3 work` — the codex and muse engines** (`docs/work-mode.md` "v1.1 engines"): a worker run
  may now drive the `codex` engine (`openai :: gpt-6.1-sol`, `codex exec --sandbox workspace-write`
  — for complex programs) and the `muse` engine (`meta :: muse-spark-1.3-contributor`, near-free on
  the Muse Code subscription) beside the existing `claude` engine. `select_worker` accepts a worker
  entry on any of the three engines; the refusal names all three. Per engine: codex takes its prompt
  on stdin and names its thread in the stream (a resume uses `codex exec resume <thread>`); muse
  takes a `--prompt-file`, writes with `--approval-mode never --trust-workspace --disable-sandbox`
  (shell needs the sandbox off on Windows), and mints its `--session-id` up front so a resume
  continues it. `--allow` applies to claude only; codex/muse write through the sandbox/workspace.
  `run.json`'s `engine` and the `--status` line's `[engine]` suffix name the engine. A second
  byteplus worker entry (`deepseek-v4-1-flash-261017`) was added to the roster.


### Changed

- Tech-debt audit (2026-10-22, `.eck/TECH_DEBT.md`): `c3_core::engine::SubprocessEngine::{run,
  continue_turn}` no longer panic with a stale "milestone 2b" `unimplemented!` - they return the new
  `EngineError::PlanOnly` (that type only plans launches; execution lives in the `c3` adapters);
  stale `allow(dead_code)` and the `_wall` import shim removed; `.gitattributes` gains `* text=auto`
  so line endings normalise the same on every machine (the byte-exact fixture rules are unchanged);
  `docs/port/harness-results.md` points at the RC5-RC7 gate documents for every run after Run 17.
- Phase 2 switch done on the operator's machine (`docs/port/phase2-switch.md`): the `c3` plugin is
  the only enabled reviewer bridge; the operator's `astra`/`workers` skills name `c3:*`.


### Fixed

- **muse-wonder-1.0 refused before reaching Meta**: the caps-v1 muse catalog (`c3-core` effort
  table, telemetry classes) declared only the two spark ids, so every `--model muse-wonder-1.0`
  run (and the weighty roster seat) failed with "no effort vocabulary declared for model" while
  the muse CLI itself answered. The id is declared now. The consult reference documents the Muse
  priority: spark-contributor by default, spark-1.3 only when the contributor is down, wonder only
  for hard reviews and consilia.


- **`c3 work` wave 10c — Astra acceptance pass 3 (handoff 93, F93-1)** (`docs/port/wave10a-work-acceptance.md`):
  - **F93-1 (blocker)** the status/`--wait` reader no longer performs an unlocked recovery write:
    `read_record` is now a pure fallible reader, and the dead→`died` transition is done under the
    task admission lock via `recover_dead_run`, which re-reads and writes only when the record is
    still the same generation (pid + pid_start + attempt) and its owner is still dead — so a delayed
    status read can no longer overwrite a newly claimed live resume. `live_run` now checks liveness
    directly; the display/resume listings go through the locked `read_for_display`. Regression:
    `work.rs::a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim`.
- **`c3 work` wave 10b — Astra acceptance pass 2 (handoff 91, F91-1..F91-4, F89-4/F89-5 remainders)** (`docs/port/wave10a-work-acceptance.md`):
  - **F91-1 (blocker)** the resume claim now validates permitted status, exhaustion, owner and the
    attempt generation under the admission lock and computes the attempt from the locked re-read;
    a stale claim (now done/exhausted/advanced) refuses before any spawn.
  - **F91-2 (blocker)** admission fails closed on inspection errors: `run_dirs`/`read_record`/
    `live_run`/`other_live_run` return errors, only a confirmed `NotFound` is absence, and an
    unreadable or replaced record refuses a second launch (naming the directory, not contents).
  - **F91-3 (major)** event-stream READ failures are artifact errors: attempt temp files are kept
    until read and durably appended; `refresh_artifacts` never replaces `report.md` when
    `events.jsonl` cannot be read; the run becomes `artifact_error`.
  - **F91-4 (major)** success needs affirmative terminal evidence: claude a `result` with
    `is_error` present and false (subtype success/absent), codex a `turn.completed` (an
    `agent_message` alone is not success), muse its completion event — extraction kept separate.
  - **F89-4 remainder** the restricted grammar refuses a bare shell/exec tool (a bare tool is
    allowed only for a read/write whitelist) and denies shells, interpreters and git by exe
    basename (trailing `.exe`/`.cmd`/`.bat`/`.ps1` stripped); path separators are refused.
  - **F89-5 remainder** `report.md`, `diff.txt` and the persisted argv pass the project's single
    pack sanitizer (secret-shaped values redacted); `events.jsonl` stays raw; the doc says so.
- **`c3 work` wave 10a — Astra acceptance pass (handoff 89, F89-1..F89-8)** (`docs/port/wave10a-work-acceptance.md`):
  - **F89-1 (blocker)** worker liveness: every `running`/`waiting_quota` record now carries the
    OWNING process's pid + start time (a foreground run writes its own pid BEFORE spawning the
    child); liveness is that pid-with-start-time being alive, and the 120 s start grace only covers
    the window before any pid is written. A foreground run past 120 s stays `running` and still
    makes a second same-task launch refuse.
  - **F89-2 (blocker)** admission: one task-level admission lock covers starts, resumes and the
    detached alarm — re-read under the lock, refuse if another run is live, durably claim the
    record (`running`, new pid/start, attempt number) before spawning; per-attempt event/stderr
    file names (`.events-resume-<n>.jsonl`). Two concurrent `--resume` of one record: exactly one
    spawns.
  - **F89-3 (blocker)** artifact durability: `append_resume`/`refresh_artifacts` return `Result`;
    the resume's attempt files are deleted only after the append succeeded and was flushed; a
    `report.md`/`diff.txt`/`events.jsonl` write failure makes the run `artifact_error` (exit
    non-zero) with the error in `run.json`, never `done`.
  - **F89-4 (major)** a real allow-list grammar (`<Tool>`, `<Tool>(<exe>)`, `<Tool>(<exe>:*)`, no
    shells/git) replaces the substring check; the new `--unrestricted` opt-in (flag or roster
    `ext.unrestricted`) lifts it and is required for the muse worker's `--disable-sandbox`; codex
    keeps `--sandbox workspace-write`; `run.json` records `unrestricted` and `permission`; the doc
    states the boundary precisely.
  - **F89-5 (major)** `run.json` `usage` is rebuilt from an allow list of numeric counters (never a
    cloned object); the record contract in the doc is made precise (events.jsonl is the raw stream
    retained verbatim; run.json/report.md/diff.txt carry no secret values).
  - **F89-6 (major)** the effective `max_resumes` and `quota_window_sec` are persisted at the first
    run and reused by a later plain `--resume` unless the operator passes the flag.
  - **F89-7 (minor)** `files_changed` compares content identity (`git hash-object`) before/after,
    so a pre-dirty file the worker edited further is listed.
  - **F89-8 (major)** success requires the engine's successful terminal event in addition to exit
    0; an error terminal or rejecting rate-limit takes precedence over exit 0; an empty stream with
    exit 0 is `died` with reason `no terminal event`.

## [0.2.2] - 2026-10-15

- Build: the `index-surreal` feature (embedded SurrealDB code index) is now opt-in - the default `cargo install` is the light bundle; `--features index-surreal` for the http/browser packs (README 'Light bundle or indexed bundle').

### Fixed

- **Wave 9 - the four consilium defects (`docs/port/wave9-consilium-defects.md`)**: (D1) a panel starts new threads for every member unless `--mode fork` is given, and the header prints the mode; (D2) C3's own lines name `c3 consult --task <t> --status --id <id>` / `--wait --id <id>` / `c3 providers` / `c3 findings`, not the plugin's `.ps1` scripts (the shims keep the plugin spelling for the harness oracle via `C3_PLUGIN_COMPAT=1`); (D3) a detached single run holds the task lock only for its bookkeeping, so detached runs of one task run side by side; (D4) a roster entry may declare `ext.c3.plan_caps` / `reset_hint`, and two instant 429s inside an hour mark it out until the cap's reset instead of the 10-minute burst.
- **Wave 9b - the review of wave 9**: a detached run is admitted only beside records of a live owning bridge (never survivors, unverified processes or a dead bridge); the reservation is durable and verified before the task lock is released; the cap pair counts distinct consultations (the machine record carries `consult_id`); a non-ASCII `reset_hint` is a validation error, not a panic; the shims set `C3_PLUGIN_COMPAT` for their child only; the cap policy is documented as inferred exhaustion (`docs/port/wave9-consilium-defects.md`).
- **Wave 9c - the round-2 review**: the cap pair is counted by consultation identity only (same non-empty consult id = one hit; different ids are distinct at any instant; a legacy row matches a ledger row only when endpoint, failure text and the second all agree), and the admission test no longer needs PowerShell (`docs/port/wave9-consilium-defects.md`).
- **Wave 9d - the acceptance review**: the cap pair is built from id-carrying rows only (a row without a consult id keeps the plain 10-minute burst) and only from hits younger than 24 h at READ time, so neither a legacy mirror nor an unpruned old row can produce or prolong the cap verdict (`docs/port/wave9-consilium-defects.md`).
- **Wave 9e - the second acceptance pass (F76-1)**: the read-time bound on a cap-pair hit is now the entry's current cap period, not a flat 24 h, so a hinted entry stays out until its declared reset (a hit counts while `next_cap_reset(hint, hit) > now`) while an unhinted entry keeps the 24 h bound; `endpoint_health` retains id-carrying hits for 35 days and `plan_cap_verdict` applies the per-entry rule, keeping the verdict identical across every reader (`docs/port/wave9-consilium-defects.md`).
- **Wave 9f - the third acceptance pass (F78-1, F78-2)**: the cap detector takes the newest qualifying pair of in-period hits (a later isolated 429 no longer hides an established pair), and machine-health retention keeps id-carrying instant-429 cap evidence for 35 days regardless of its expired burst `until` with the 500-record cap evicting such evidence last, so a reader in another repository still sees the pair until the declared reset (`docs/port/wave9-consilium-defects.md`).
- **Wave 9g - the fourth acceptance pass (F80-1)**: `update_machine_health` now durably drops cap evidence superseded by a newer clearing success (a usable reply) before retention and the record cap run, so the asymmetric lifetimes of evidence (35 days, evicted last) and the success (24 h, evicted first) can no longer resurrect an obsolete refusal for a machine-only reader (`docs/port/wave9-consilium-defects.md`).
- **Wave 9h - the fifth acceptance pass (F82-1)**: the newest clearing success of an endpoint is now a durable anchor - kept 35 days and evicted only in the record cap's last tier, after cap evidence - so a success can no longer be discarded while its cap pair waits in a busy (exclusively held) journal; evidence arriving late from a skipped journal or a delayed retry is still superseded (`docs/port/wave9-consilium-defects.md`).
- **Wave 9i - the sixth acceptance pass (F84-1)**: exactly one clearing anchor per endpoint (the newest success, ties broken by record key) is protected, so redundant equal-time successes are ordinary rows and can no longer fill the cap's protected tier and evict another endpoint's only clearing proof (`docs/port/wave9-consilium-defects.md`).


## [0.2.1] - 2026-10-14

- Plugin: the coordinate skill carries the waiting rule (wakeups instead of blocking waits); the SessionStart hook prints it; README: the local-checkout install path.

### Fixed

- **Wave 8j - one total order per window (8i caveat)**: every quota instant comparison is made at whole-second granularity and a tie goes to the rank, with the legacy windows resolved from a separate slot, so the pick is the same for every candidate order (`docs/port/wave8-quota.md`).
- **Wave 8i - lossless quota instants (F62-1)**: the per-window instants keep their fractional seconds, a comparison involving a whole-second instant is made at second granularity (a tie goes to the rank), and a legacy record's unknown window instants never win on recency against a known one (`docs/port/wave8-quota.md`).
- **Wave 8h - per-window instants persisted (F60-1)**: the quota record (ledger entry, machine-wide health record) carries `primary_observed_at` / `secondary_observed_at` for each present window, read by the recorded selection; the aggregate `observed_at` is only the fallback for older records (`docs/port/wave8-quota.md`).
- **Wave 8g - per-window observation instants (F58-1)**: each retained quota window keeps its own observation instant through the rollout, account and recorded/live aggregation, so a partial newer observation can no longer promote an older window (`docs/port/wave8-quota.md`).
- **Wave 8f - account cache and fail-closed windows (F55-1, F55-2)**: the account cache keeps the raw rollout observations and the selection runs at each lookup's instant; a newer observation replaces a window only where its own is valid (`docs/port/wave8-quota.md`).
- **Wave 8e - quota clock and finite percentages (F52-1, F52-2)**: one comparison instant per aggregation (sampled at the entry point, passed down) and a non-finite or out-of-range percent is an absent window, so neither a reset crossing mid-fold nor a "NaN" share changes the pick (`docs/port/wave8-quota.md`).
- **Wave 8d - quota tie order (F50-1)**: tied per-window observations are ranked by one total order - a future reset beats a past one beats a missing one, then the higher share, then the later reset - so the result no longer depends on the order (`docs/port/wave8-quota.md`).
- **Wave 8c - quota merge per window (F48-1)**: at an equal `observed_at` the endpoint window (recorded, live, sub-agent and rollout aggregation) merges per window - the higher primary and, independently, the higher secondary - so a blocking weekly window is never hidden (`docs/port/wave8-quota.md`).
- **Wave 8b - quota tie (F46-1)**: the endpoint window selection (recorded and live) picks the newer `observed_at`, and at an equal instant the fuller primary (then secondary) window, so a 30 % observation can no longer hide a 99 % one (`docs/port/wave8-quota.md`).

### Added

- **Wave 8 - quota windows** (`docs/port/wave8-quota.md`; the plugin's wave 30 / TECH_DEBT T15 of
  its `[Unreleased]` 0.6.2, `bade7cb..a162b8d`): after every codex run the ledger records `quota`
  `{primary, secondary, observed_at}`, `tokens` `{input, cached, output, thread_input, calls,
  max_input, context_window, subagents {count, input, calls}}`, `subagents_allowed` and `budget`
  after `commit_wait_ms`, read from the thread's rollout and its sub-agent rollouts
  (`c3_core::quota`); the endpoint health takes the newest observed window (ledgers, machine-wide
  records - which now carry `quota`, a run without an outcome record writing class `""` -, the
  codex home's newest rollout for the built-in openai endpoint): >= 98 % with its reset ahead is
  `unavailable (5-hour window 99 % used, resets <iso>)` like a usage limit, >= 80 % says `low
  quota: ...`, a past reset shows `5h reset (last 96% until 18:32)`; `c3 providers` has a QUOTA
  column and `--json` rows a `quota` object; `--budget <percent>` (env
  `CODEX_CONSULT_QUOTA_BUDGET`, default 90, 0 off) stops the main turn at the budget (partial kept,
  no continuation, class operator, telemetry class quota; detached runs and panel members the
  same); the replay guard `CODEX_CONSULT_FORK_MAX_KB` (default 512; warned from 128 KB) and the
  partial's `--mode new` hint; `-c features.multi_agent=false` on every codex turn unless the
  user's config names `features.multi_agent`; the reading discipline in the purpose paragraphs of
  acceptance, diff-review, checkpoint and core-contract; the dry run's `quota`, `budget`,
  `sub-agents` and `replay` lines; the consult shim forwards `-Budget`.

## [0.2.0] - 2026-10-14

The compatibility release: parity with claude-codex-consult 0.6.1, measured by the plugin's pinned
v0.6.1 harnesses through the shim (`docs/port/harness-shim.md`).

### Added

- Wave 1 (`wave1-parity.md`): the 0.6.1 rules of wave 28 - the time-only reset parse, the Codex
  app-server exclusion, the panel light, the re-read anchor.
- Wave 1b (`wave1b-plan.md`): the roster `plan` - quota propagation, the scheduling group and the
  machine-wide plan wait as shared code (F02-7).
- Wave 1c: F04-1..F04-3 - a Unicode-safe `.exe` strip, DST edges by a UTC round trip, the panel
  records from the final routing.
- Wave 2 (`wave2-telemetry.md`): 2a `consult_ref`, the rating judge and `rating_rev`, the backfill,
  the ledger order; 2b a durable telemetry outbox and the forget-me retry identity (F02-1, F02-3);
  2c the closed telemetry classes and the telemetry shim (F02-2, F02-4); C3 keeps its own app id,
  salt and outbox (decision P7).
- Wave 2b (`wave2b-compat.md`): the pre-0.6 gaps of the RC2 triage - the test-mode warning and
  the child scrub, the launcher quoting, the health journal, kick during a timeout continuation,
  the pending outcome, the store rename retry, the member step cap, the launch-mark hooks, STREAM
  D6, the hook's pointer line and `--explain`, `--require` on a single run, the role and topic
  validation, the shim fixes.
- Wave 3a (`wave3a-recovery.md`): the recovery hardening of the plugin's wave 28e (E1, E18, E19,
  E23, E25, E28) and F06-1.
- Wave 3b (`wave3b-notspooled.md`): one not-spooled file per producer, the fold that saves its
  record before it deletes, the legacy staging, the forgetting marker on pid and start ticks (E2,
  E3, E20, E24, E26; decision P8, the test-mode plugin-home switch).
- Wave 4 (`wave4-claude.md`): the `claude` engine - roster keys, the model table, the preflight,
  the turn, the panel seat, the endpoint mode; the not-spooled warning says "- dropped".
- Wave 5 (`wave5-leftovers.md`): `--brief-prefix` (`CODEX_CONSULT_BRIEF_PREFIX`, the plugin's
  wave 27c D6) with the reply-prefix and slug refusals and the dry run's `brief prefix:` line; the
  telemetry sender's intake handling - every batch of at most 100, the 429 `Retry-After` rule, the
  400 `events[i]` drop, the 413 halving, the 403 stop, the 60 s flush deadline and the 8 s request
  bound, the record's `rejected[]` and the plugin's result line; the sender lock's owner record
  (`flush.owner.json`) behind "sender busy" / "sender stuck" in the refusal, the notes and
  `--status`.

### Fixed

- Wave 2d: F09-1..F09-6 - the deletion transaction, the reread rule, closed legacy events, one
  coordinator resolver.
- Wave 2e: F11-1..F11-5 (the wave 2b review).
- Wave 2f: F14-1 - the local-only forget records its cleaning transaction before the first
  removal (F14-2 stays as the plugin: the provider match is case-sensitive).
- Wave 2g: F19-1..F19-3 - the complaint joins the deletion protocol, an unreadable salt never ends
  a cleanup, every outbound event is re-serialised.
- Wave 2h: F22-1 - a kept health journal lists its applied record keys, so a replay after an
  eviction never applies a record twice.
- Wave 3c: F23-1..F23-5 - the recovery invariant fail-closed (a failed lookup is never "gone", an
  unreadable start time never skips a scan row, the kept evidence is written at the kill).
- Wave 3d: F24-1..F24-6 (the wave 3b review).
- Wave 4f: F25-1..F25-4 - the endpoint launcher probe, the transcript guard in every auth mode, the
  plugin's model case rule, a redacted child-environment debug print.
- Wave 5: `CODEX_CONSULT_COORDINATOR` is refused with the plugin's character rule
  (`Get-IdentityStringProblem`: "the provider 'open::ai' must not contain '::'") instead of a
  provider-label pattern the plugin does not have; interior blanks are accepted as the plugin does.
  The coordinator's dry-run line and the "not in the roster" console line follow the plugin's
  `Format-CoordinatorText` / `Format-CoordinatorId` (no ` [codex]` suffix, "(model not named)", an
  unresolved `#n` shown as such).
- Wave 6 (`wave6-live-diffs.md`, the RC3 live matrix): the ledger's `coordinator` record carries
  `host_by`, `in_roster` and `unresolved` in the plugin's order, and a rewrite of `sessions.json`
  keeps every entry it read verbatim (key order, explicit nulls) apart from what C3 changed in it.
- Wave 6: the `c3 providers` header counts this repository's consultations only, not the
  machine-wide health records.
- Wave 6: the agy and muse engines' reviewer line (dry run, reply header) is `engine <name>
  (<launcher>)` as the plugin's, not the codex endpoint text.
- Wave 6: telemetry is sent by a detached `c3 telemetry --flush` started right after a
  consultation's, a panel's, a rating's or a backfill's commit (not waited for, allow-listed
  environment), not at the start of the next run.
- Wave 7 (`wave7-acceptance-fixes.md`), acceptance round 1 fixes: F36-1, F36-2, F30-2 failure
  path, F29-1/F32-2 documented. F36-1 - a repository-relative path strips the root char by char
  (a Unicode `CLAUDE_CONFIG_DIR` outside the repository no longer panics the Claude transcript
  guard; Windows folds every letter as `OrdinalIgnoreCase`). F36-2 - the guard compares the NFC
  form Claude Code writes under as well as the path as written, the root likewise; any form inside
  refuses. F30-2 - the write before a kill reports its result to the killer; a failed one is
  retried once into `.consult.pending-kill.json`, which the recovery reads as a record; both
  failing, the kill still runs, not confirmed, and the next run is refused from the launch record
  and its fail-closed scans. F29-1 (the shared journal with plugin 0.6.1) and F25-1/F32-1/F32-2
  (what the endpoint preflight and the transcript guard cannot see) are stated as known limits.
- Wave 7b (`wave7-acceptance-fixes.md`, "7b"), acceptance round 2 (F38-1): no kill without a
  record - the record before a kill goes to the recovery record, else its fallback, else a third
  location outside the collab directory, `<codex home>/c3/pending-kill/<task>-<run id>.json`, which
  the recovery reads and consumes as the fallback (and the end of a run that writes the record
  clean removes); all three failing, the tree is left alive, the run ends failed naming its root
  and the three errors, and the next run is refused from the launch record while it lives.
- Wave 7c (`wave7-acceptance-fixes.md`, "7c"), acceptance round 3 (F40-1, F40-2): the codex home's
  kill records that cannot be listed (the directory cannot be opened, an entry cannot be read) are
  an inspection failure, never "no records" - the consult and panel recovery, the dry run, the
  findings refusal and `-List` and the kick refuse naming the directory and the error (only an
  absent directory is empty); a kick whose kill did not happen (its record written nowhere) is
  acknowledged `not killed` - the kick names the live root and why, says the tree is left alive and
  the task refused, and exits 1.
- Wave 7d (`wave7-acceptance-fixes.md`, "7d"), acceptance round 4 (F42-1) and a sweep of the
  family: a recovery record - the record, its fallback, a codex home copy, the running record -
  whose metadata lookup, read or parse fails is an inspection failure, never an absent record (only
  `NotFound` is): every reader refuses naming the file and the error (`the recovery record <path>
  cannot be read (<error>): the task is refused until it can be read`; `-List` prints it as
  `pending:`, the kick exits 1); the task directory's own listing that fails refuses the same way;
  the record before a kill never replaces a record that cannot be read.

### Changed

- The workspace version is 0.2.0 (`c3 --version` prints `c3 0.2.0`); the Claude Code plugin and
  its marketplace entry carry 0.2.0.

## [0.1.0]

The parity port of the plugin's 0.5.x surface (milestones 1-6) and the subsystems after it: packs
and the `http` engine (milestone 7), the index (8), the router (9), the stdio MCP server (10) and
the index federation (11).
