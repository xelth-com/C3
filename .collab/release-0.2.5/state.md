# release-0.2.5 - state

Operator, 2026-11-22: "давай запускай приёмку на релиз" - release the five commits after v0.2.4
(0a6ad4d repo model repair, a65aa10 c3 branches, c7820be + b086cc1 maintain public catalogs,
ba43ff6 GPT-6.2 telemetry ids). None of them was read by any model before this task.

Plan (as release-0.2.4): diff-review panel -> fixes + full suite + CI -> version 0.2.5 + CHANGELOG
heading -> acceptance by the standing partner (openai :: gpt-6.2-astra, a NEW thread, no
--spend-reserve) -> annotated tag on the acceptance-records commit after green CI -> install by
swap -> xelth.com/C3 page after the tag.

- 2026-11-22 ~13:55: diff-review panel launched (detached). Brief `handoffs/01-claude-diff-review.md`,
  patch `handoffs/01-claude-diff-review.patch` (`git diff v0.2.4..HEAD`, head ba43ff6). Members, by
  the operator's rules (Sol for the simpler work, the cheap pair Grok + Muse contributor, no Astra,
  no Wonder): openai :: gpt-6.2-sol (handoff 02), meta :: muse-spark-1.3-contributor (03),
  grok :: grok-4.7 (04). OpenAI 5h window 55 % until 18:09 at launch, week 9 %.
  Detach id `7a3574a8`. Come back: `c3 consult --task release-0.2.5 --status --id 7a3574a8`.
  Muse contributor usable after 208 s; Sol and Grok still running at 13:55.
- 2026-11-22 13:50: CI for ba43ff6 green (run 45022742138); the installed binary swapped to
  `c3 0.2.4 (ba43ff6)` (old one kept as `~/.cargo/bin/c3.exe.old-b086cc1`).
- 2026-11-22, operator: "может тогда сначала закрыть весь техдолг а потом проверять у астры?" then
  "давай как считаешь лучше". So: close the open `.eck/TECH_DEBT.md` items BEFORE the acceptance, one
  acceptance for everything. Waves (workers in isolated worktrees, no commits; the coordinator applies
  each diff to main, runs the full suite, `c3 finish`, CI, a cheap diff-review panel):
  - A1 reviewer reliability (agy false FAILED, kimi brief / empty seats, grok death record) - running
  - A2 codex guard (smoke proves the sub-agent switch, F13-1, fork, reviewer isolation) - running
  - B1 reviewed revision for claude-engine reviewers (`--rev`/worktree, `--diff <range>`)
  - B2 maintain / repair leftovers (pending update, C3_CHILD child test, telemetry published path,
    crashed kimi worktree, rule text for existing projects)
  - C1 one vendor per panel
  - C2 CI `--no-fail-fast` + flake, `c3 finish` names a red CI, the hook's wait rule text
  Closed by record, not code: F06-4 (decided boundary, one ChatGPT account), 0.2.4 first runs
  (observation). Stays open by the operator's word: reviewers vs the knowledge base.
  Briefs: the session scratchpad `briefs/` (common.md + one per wave).
- 2026-11-22 14:05, diff-review panel 7a3574a8 result (wall 954 s, 3 of 3 usable, all rated useful):
  Sol REJECT 5 blocker / 12 major / 4 minor (588 s; 5h window 55 -> 81 %, 1.75 M input tokens, sub-agents 0);
  Muse contributor HOLD 1 blocker / 2 major / 2 minor / 1 note (176 s); Grok HOLD 4 major / 5 minor (921 s).
  36 findings (F02-1..21, F03-1..6, F04-1..9), none rejected so far. They go BEFORE the remaining tech
  debt, grouped with the overlapping debt items:
  - P1 repair + C3_CHILD: F02-1,2,3,4,5,6, F03-1,4, F04-4,6 (+ debt: child-mark test)
  - P2 branches: F02-7,8,9,10,11, F03-3,6, F04-1,7,9 (+ debt: stale kimi worktree)
  - P3 maintain: F02-12,13,14,15,16,17,18, F04-2,3,8(fetch) (+ debt: pending update)
  - P4 telemetry + kimi removal: F02-19,20,21, F03-2,5, F04-5,8(claude shape) (+ debt: published-path tests)
  Rounds (two heavy workers at a time - 16 GB shared): A1+A2 (running) -> P1+P2 -> P3+P4 -> B1+C1 -> C2.
  Brief B2 is dissolved into P1-P4 and C2.
- 2026-11-22 ~14:45, wave A2 reported (worktree `.claude/worktrees/agent-a2960789f7936b0c4`, not yet on
  main): the codex smoke reads its threads' rollouts and FAILS on `multi_agent_version` != disabled or a
  multi-agent tool (missing rollout = fail), feeding the rollback; F13-1 launch boundary before the spawn
  (both tests of the entry, each fails on the old bound); forks observed on ZAI - an exec fork references
  its parent (`history_base`), nothing copied, every line stamped after the fork; a sub-agent fork copies
  and re-stamps history, so the scan now starts at its last `task_started` (gap ~0.8 s left, minor);
  reviewer isolation: `features.plugins=false` + `skills.include_instructions=false` on every codex
  REVIEW turn (30 skills -> none, 74.6k -> 66.8k input tokens on a probe), not on worker turns. Live
  `c3 maintain --smoke --engine codex` on ZAI::glm-5.3-flash passed (debug build). Follow-up asked of the
  same worker: one reviewer-is-not-a-coordinator sentence in every review prompt (the global
  `~/.codex/AGENTS.md` still reaches codex reviewers). P1 started (A1 still running).
- 2026-11-22 ~14:50, wave A1 reported and APPLIED to main's working tree (patch from worktree
  `agent-af4dcf348f1736d1f`, 18 files, clean 3-way apply): agy tool steps judged from agy's own log
  (`finish` hand-over and parameterless steps are not writes); a complete schema-valid reply turns a tree /
  collab change into a warning ("reply kept"), ledger written; `c3 providers` usable / failed-with-reply /
  failed-without-reply per reviewer (`reply_outcomes`); kimi: `worktree_isolated` engine flag, an
  unreachable brief refuses the seat before launch (preflight wording, a panel re-seats), the prompt names
  up to 20 uncommitted paths; panel members' priors come from the pre-panel snapshot (`listed_ids`);
  grok death record names the stream of the turn that ended the run; guard-failure wall includes the
  zero-tool primary. 2(d) skipped (no robust "cannot read the brief" signal; (a)+(b) remove the cause).
  Worker's runs: c3 lib 692/0, kimi_engine 11/0, grok_engine 18/0, compat_wave2b agy 4/0.
  Accepted decisions: a complete agy reply with no tool call and a tree change warns; the kimi brief
  refusal also under --skip-preflight. Left: priors of a SINGLE run still read the store at render
  time (minor, debt); kimi still fails on main-tree changes -> added to brief B1 (item 4).
- 2026-11-22 ~15:05: A2 + its follow-up (`REVIEWER_NOT_COORDINATOR` appended to the Constraints paragraph
  of every review prompt, raw too; not in `WORK_PREAMBLE`) applied on top of A1 (12 files; only
  CHANGELOG conflicted, both sides kept). Full suite on main running (log: scratchpad `suite-A.log`).
  Worktrees `agent-af4dcf348f1736d1f` (A1) and `agent-a2960789f7936b0c4` (A2) kept until the commit.
- 2026-11-22 15:08: full suite green on main (fmt ok, clippy clean, 40 binaries, 1298 passed, 0 failed);
  committed a115422 (c3 finish) and pushed; CI run 45031254736. A1/A2 worktrees removed (no reparse
  points inside, reset + clean, `git worktree remove` without --force), their branches deleted. P1's
  branch recorded `work`. P2 started from a115422.
- 2026-11-22 ~15:55, wave P1 reported and APPLIED to main's working tree (worktree
  `agent-aa05e0920dd1bedaa`, 15 files; CHANGELOG conflict resolved, both sides kept). All ten findings
  fixed: F02-1/2 temp file + snapshot re-check + rename, hard-linked / linked files reported never
  edited; F02-3/F03-4 only a concrete Claude id rewritten, `[1m]` kept, value span only (quotes, comment
  survive), one shared frontmatter parser; F02-4 AGENTS.md follows only active unprotected entries, one
  successor or nothing; F04-4 doctor --fix / refresh skip under C3_CHILD; F02-6 repo root by walking up
  (no subprocess), hook branch line on a thread with a 2 s deadline; F03-1/F02-5/F04-6 C3_CHILD on the
  claude child env, auth/version probes, detached maintain (explicit env block). Worker's runs green
  (c3-core claude 20, c3 lib filters 41 + 87, init_doctor 14, claude_engine 16, work 2, maintain 28,
  branches 4). Accepted decisions: stricter key / id parsing (`Opus` no longer rewritten), doctor --fix
  exit 1 on a failed repair, link skips reported each session, `child_env_allowed` now lists C3_CHILD
  (the pinned plugin harness may compare it - not run). Coordinator's own addition: `C3_CHILD=0` /
  false / off / no no longer marks a child (`c3_child_value_marks`, test). Open (minor, debt):
  `find_repo_root` ignores GIT_DIR / GIT_WORK_TREE; the 2 s hook deadline could flake the branches hook
  test on a loaded CI runner. P3 started from a115422.
- 2026-11-22 ~16:30: CI for a115422 green (run 45031254736). Wave P2 reported and APPLIED on top of P1
  (worktree `agent-ad9d34f33576d0037`, 10 files). Conflicts resolved by hand: `hook/mod.rs` keeps P1's
  2 s `branch_line_within` wrapper around P2's new `branches::hook_line`; `cli/doctor.rs` keeps both
  P1's repair problems and P2's stale-worktree removal, and the removal is skipped inside a C3 child
  (coordinator's addition, F04-4's rule). P2: descriptions read NUL-framed and kept byte-exact, a
  `c3:` line is C3's only when it parses, an unreadable or concurrently changed value is never written
  over, odd branch names validated (`git check-ref-format`); every branch git call with
  `GIT_OPTIONAL_LOCKS=0` (index byte-identical, proven against a plain `git status` control); patch
  checks capped at 50; default branch `c3.defaultBranch` (`c3 branches --default`) -> origin/HEAD ->
  init.defaultBranch -> main/master -> unknown (a finding; `merged` null, never merged on a failed
  query); `patches_on_default` relabelled as patch equivalence with qualified refs; kimi throwaway =
  exact `c3-kimi-wt-<32 hex>` directly under temp; owner record `.git/worktrees/<name>/c3-owner`
  (pid + start) written by `KimiWorktree::create`; stale ones reported and removed by `doctor --fix`.
  Worker's runs: branches lib 14, kimi lib 13, branches 9, kimi_engine 11. Full suite running
  (scratchpad `suite-P12.log`).
- 2026-11-22 ~16:45, wave P3 reported (worktree `agent-a9e92400cb221f99e`, not applied yet - the P1+P2
  suite is running on the tree). Fixed: public GETs own redirect policy (same host + port, no userinfo,
  every hop re-checked, <= 3), one 15 s deadline over hops and body, streaming 4 MiB cap, the kimi host
  pair only for the installer / marker; date gate: future (> +1 day) rejected, any source dating the
  candidate not newer rejects it, such pairs stored (`models.older`, <= 500), own-list ids gated too;
  endpoint lists keyed `endpoint:<scheme>://<host>:<port><path>|<key var>|<plan>`; F02-16 both smoke
  turns through a one-entry roster (`CODEX_CONSULT_ROSTER`) holding the exact entry with the model
  changed (test fails on the old label binding), engine-update smokes bound the same way; the writer
  edits `reviewers[position].model` found by parsing and verifies the only delta; MODELS=off
  authoritative for `--models`; an unresolved pending update tries a newer vendor release keeping the
  original `previous`, no-rollback path tested with agy. Worker's runs: maintain lib 71, telemetry 30,
  maintain 34, claude_engine 15. Accepted decisions: codex custom-provider smoke with no roster; an
  explicit `C3_MAINTAIN_SMOKE_<ENGINE>` still by label; `ext.c3.reviewers` http entries announced only.
  Debt (minor): smoke consult children lose `CODEX_CONSULT_TEST_*` (scrub_host_markers).
- 2026-11-22 17:08: P1+P2 suite green (40 binaries, 1331 passed, 0 failed); clippy then flagged my test
  module mid-file (moved to the end; clippy, fmt clean). Committed 23b1455, pushed, CI 45036679436.
  P1 worktree removed + branch deleted; P2 worktree reset clean but LOCKED by this session's agent
  harness (pid 2440 = this Claude Code session) - recorded `parked`, remove once unlocked, no force.
  P3 applied to main (11 files, clean); full suite on it running (scratchpad `suite-P3.log`). C1 (one
  vendor per panel) started from 23b1455. P4 waits for the P3 commit (it changes
  `maintain::models::published_from`, which P3 reworked).
- 2026-11-22 17:40: CI for 23b1455 green (45036679436). P3 suite green (40 binaries, 1349 passed, 0 failed,
  fmt + clippy clean); committed 8e6fd3e, pushed, CI 45039380914. P3 worktree removed (no links), branch
  deleted. P2 worktree still locked by the harness (left `parked`). C1 branch recorded `work`. P4 started
  from 8e6fd3e (told to build on P3's source keys, P2's `remove_throwaway_worktree`, A1's staging).
  Remaining after P4 + C1: B1 (reviewed revision, `--rev`, `--diff`, kimi main-tree false fail), C2
  (CI / finish red-CI line / hook wait rule / rule text refresh), then debt file update, version 0.2.5,
  a cheap diff-review of a115422..HEAD, and the acceptance.
- 2026-11-22 ~18:10, wave C1 reported and APPLIED to main (worktree `agent-a5340700e80b3b489`, 10 files;
  CHANGELOG conflict resolved, cargo check clean). One seat per lab (best-weighted per lab, then a draw
  among labs; size capped at the lab count with a warning; reserve refills only unseated labs);
  `--require` two of one lab -> both seated + warning; `--panel-all` one warning; the routed bare default
  unchanged; on a weighty purpose with >= 2 seats a weighty entry gets the first seat (`tier-*` rules);
  lab table: grok -> xai, o<digit> -> openai, last path segment tried - all 17 roster families
  resolve; plan / summary / `router explain` name the "lab already seated" skip. The rule also applies
  to roster order (the 2026-11-20 GLM + GLM-flash panel most likely came from the no-ratings roster-order
  fallback). Plugin-compat mode keeps the old draw. Record marker `panel.routing.ext.c3.seating` only when
  the rule changed something. Worker's runs: panel 55, router 8, router replay 2, panel_light_required 2,
  reviewer_protection 8. C2 started from 8e6fd3e. Suite for C1 + P4 together when P4 reports.
- 2026-11-22 ~18:40: CI for 8e6fd3e green (45039380914). Wave C2 reported (worktree
  `agent-a3c9b81bf779f7b85`, not applied): CI `--no-fail-fast` on all test steps + git stderr in the
  kill_record_durable helper; `ci_check.rs` - `c3 finish` prints `CI: the latest run on <default>
  failed - <title> (run <id>)` (JSON `ci`), the hook at most once a day per repo (cache
  `<codex home>/c3/ci-check.json`), gh deadline 3 s / line 5 s, `C3_CI_CHECK=off` / `ci_check`; the
  1800 s waiting rule in both hook scripts, the coordinate skill's first lines and the --detach notices
  (stderr); `setup/rules.rs` - doctor reports a C3-written 02-delegation.md without BRANCHES AND
  WORKTREES, `--fix` appends that section (temp + rename, not in a child, a rewritten file only warned).
  FOUND by the worker and confirmed here: the plugin hook scripts forward only a whitelist of `c3 hook`
  lines (status, coordinator pointer, compaction, update, now CI), so the maintenance, branch and
  model-repair lines built since 0.2.4 never reached a plugin session (the actions ran, silently).
  Follow-up sent to the same worker: forward every `c3: ` / `codex-consult: ` / `CI: ` line (ordered,
  de-duplicated, <= 10, cut at 400 chars), both scripts byte-identical, with a test.
- 2026-11-22 ~19:20: the C2 follow-up done (`crates/c3-cli/tests/hook_scripts.rs`, 3 tests, ps1 under
  PowerShell 5.1 and sh both run, byte-identical; minor: sh cuts non-ASCII by bytes). C2 APPLIED to
  main on top of C1 (24 files, no conflicts; new files staged); cargo check clean. Waiting for P4, then
  one full suite for C1 + C2 + P4 and a commit; then B1.
- 2026-11-22 ~19:50, wave P4 reported and APPLIED (worktree `agent-a196c549549e6146c`, 11 files; CHANGELOG
  conflict resolved; cargo check clean). Telemetry: only the built-in catalog host (openrouter.ai) and a
  configured catalog / page declared `"public": true` confer published status, read at send time; a
  published id counts only for the vendor class its source attributes it to (OpenRouter slug, else the
  source host's class), never for an unknown vendor; a Claude-shaped id is sent only from CLAUDE_MODELS
  or that set; `[1m]` stripped case-insensitively; end-to-end tests through `c3 telemetry
  --backfill-ratings` (11 reviewer cases). kimi removal: every reparse point a link, never descended;
  each directory re-checked against the root's canonical path; any error / remaining link / linked root
  -> no `worktree remove`, no prune, no `remove_dir_all`; `remove_throwaway_worktree` returns bool and
  doctor reports a refusal as not removed. Worker's runs: lib filters 83 + 144, telemetry_parity 9,
  kimi_engine 12, branches 9, maintain 34. Accepted: trust from the current config, plain ids bound by
  the source host, unknown OpenRouter slugs never sent, no prune on a refused removal. Debt (minor): a
  new Claude model stays `other` in telemetry until a release (OpenRouter spells `claude-opus-4.5`
  with dots); the walk's re-check is by path (only link removal sits in that window). Full suite for
  C1 + C2 + P4 running (scratchpad `suite-C12P4.log`). B1 brief updated with A1 / P2 / P4 state.
- 2026-11-22 14:00: the weekly `c3 maintain` pass (due 15:19 today, with codex 0.169 -> 0.171 waiting)
  postponed to 2026-11-24 15:19 (`~/.codex/c3/maintain/state.json` next_due; backup
  `state.json.bak-2026-11-22`) so the Codex update is smoked by A2's sub-agent check, not before it.
  Watch that pass with the new binary - it closes the "first real runs" entry.
- 2026-11-22 ~21:30, full suite for C1 + C2 + P4 GREEN. Two background runs were stopped by Claude
  Code's memory-pressure reaper (another session built `movfast`); `maintain`'s one FAILED line in the
  partial log was the kill, not the code: `maintain` alone 34/34. The rest ran in the foreground, one
  binary at a time (`-j 2`, `--test-threads=1`): mcp_integration 1, notspooled_parity 21,
  panel_light_required 2, reviewer_protection 8, snapshot_partb 4, subagent_guard 4, telemetry_parity 9,
  update 12, wave6_live_diffs 5, wave9_consilium 4, work 29 - all ok. fmt and clippy clean. Committed
  as one commit (C1 + C2 + P4). Next: B1 alone, then TECH_DEBT, the version bump, a cheap diff-review
  panel, Astra's acceptance.
- 2026-11-22 ~22:05: C1 + C2 + P4 committed as 08ae5d5 (secret gate: `panel/plan.rs` allowed after
  review - the test fixture line that assigns the auth kind `endpoint`, a false positive) and pushed; CI run 45069617073
  watched. Worktrees of C1 and P4 removed (no links, content equal to the saved patches), branches
  deleted; C2's and P2's worktrees are locked by this session's agent harness (pid 2440) - both
  recorded `parked`, remove once unlocked, no force. Wave B1 launched (opus-worker, own worktree,
  base 08ae5d5). Held until B1 is applied: TECH_DEBT.md edits (the worker reads it by line numbers),
  the version bump and the CHANGELOG heading.
- 2026-11-22 22:36: CI for 08ae5d5 GREEN on all three jobs (run 45069617073, attempt 2). Attempt 1:
  the Windows job was cancelled at 16 min 16 s inside `work` (test
  `a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim`), no failed test before it, no
  timeout in `ci.yml`, nothing in the code cancels a run; the rerun of that job passed in 15 min.
  Cause not found - if it repeats at the same test, look at the test, not at the runner.
- 2026-11-22 ~23:05, wave B1 reported and APPLIED (worktree `agent-a25720cc1ff94d197`, 25 files, clean
  apply; main's tree equals the worker's, file by file). `c3 consult --rev <rev>`: every local-cwd
  engine runs in a throwaway worktree of the resolved sha with the brief, `handoffs/` and `state.md`
  staged in; prompt, handoff `Isolation:` line and ledger `worktree {reason, rev, sha}` name it. A
  claude-engine diff-review / acceptance without `--rev` reads a worktree of HEAD. `--diff <range>`
  writes the patch into `handoffs/`, names it in the prompt, ledger `diff {range, patch, sha256, files,
  bytes}`; a named range without `--diff` on an engine without a shell warns. A reviewer in a
  throwaway worktree is checked against that worktree; a live-tree change only warns. `KimiWorktree`
  -> `engines/worktree.rs` (name, owner record, link-safe removal unchanged); new
  `consult/reviewed.rs`. Accepted: one worktree per run; `*.events.jsonl` not staged; http + `--rev`
  warns; a worktree / patch that cannot be made = capability failure; claude, grok, muse only warn on
  a write in their worktree; a relative brief outside the repository refused for a claude
  diff-review / acceptance; findings stay bound to the live HEAD. Worker's open question on resume
  across directories: checked live, Claude Code 2.1.330 resumes a session by id from a second and a
  third directory with the first one deleted - no regression. Not verified for the kimi CLI.
- 2026-11-22 ~23:40, verification of B1 on main: fmt clean; clippy clean in the worker's identical
  tree; c3-core and c3 all green; c3-cli unit tests and 10 of 24 integration binaries green (branches,
  ci_check, claude_engine 18, compat_wave2b, compat_wave5, delegation_rule, finish, grok_engine,
  hook_scripts, init_doctor). The background run was then stopped by the memory-pressure reaper
  inside kill_record_durable (another session's `link.exe` for movfast held 6.8 GB, 0.4 GB free).
  NOT run locally with B1: kill_record_durable, kimi_engine (14 green in the worker's run), maintain,
  mcp_integration, notspooled_parity, panel_light_required, reviewer_protection, snapshot_partb,
  subagent_guard, telemetry_parity, update, wave6_live_diffs, wave9_consilium, work. Not restarted
  locally (the harness rule after a reap); B1 committed and the rest left to CI (Linux + Windows).
- 2026-11-22 ~23:30: `.eck/TECH_DEBT.md` brought up to date by a worker (12 items closed, 4 partly,
  F06-4 moved to decided boundaries, 14 minor debts added; backup in the scratchpad `TECH_DEBT.bak`).
  Five review briefs written: `handoffs/05..09-claude-diff-review-*.md`, one per commit range
  (ba43ff6..a115422, ..23b1455, ..8e6fd3e, ..08ae5d5, ..B1). Seats: 1 Sol + Muse contributor;
  2 Grok + GLM-5.3; 3 kimi-claude k3.1-pro + Muse contributor; 4 Grok + DeepSeek; 5 Grok + Muse
  contributor + GLM-5.3. Scratch rosters in the scratchpad `rosters/roster-p1..p5.json`. To launch
  with the new binary: `--rev <head of the range> --diff <range> --panel --panel-all`.
- 2026-11-22 23:38: B1 committed as e4f42a8 and pushed; CI run 45078884744 watched (it carries the 14
  c3-cli binaries not run locally). The 36 findings of the first panel set to `implemented` with
  their wave and commit (P1/P2 23b1455, P3 8e6fd3e, P4 08ae5d5).
- 2026-11-22 23:50: the five-part diff-review started with the NEW debug binary
  (`target/debug/c3.exe`, built from the tree that became e4f42a8; it reports `08ae5d5-dirty`) -
  the first live run of `--rev` / `--diff`. Launch line per part: `CODEX_CONSULT_ROSTER=<scratch
  roster> target/debug/c3.exe consult --task release-0.2.5 --purpose diff-review --mode new --brief
  <brief> --rev <head> --diff <range> --panel --panel-all --detach`.
  - part 1 (brief 05, ba43ff6..a115422, Sol + Muse contributor): detach id `58fdcafb`
  - part 2 (brief 06, a115422..23b1455, Grok + GLM-5.3): detach id `072ee133`
  - parts 3, 4 after these two; part 5 last (Grok and Muse are in several parts).
  Come back: `c3 consult --task release-0.2.5 --status --id <id>`. No writes under the tree or
  `.collab` until both are done (Muse member).
- 2026-11-23 00:03: CI for e4f42a8 failed on Windows in ONE test (claude_engine, the live-cwd
  assertion compared `RUNNER~1` with the long path - a test defect); fixed in 4f43074, CI green on
  all three jobs (run 45081072873). So B1 is verified on Linux and Windows, the 14 c3-cli binaries
  not run locally included. B1's worktree removed, branch deleted.
- 2026-11-23, diff-review (one panel per task at a time - the task lock refuses a second one, so the
  five parts run in sequence; binary copy `scratchpad/bin/c3-review.exe`):
  - part 1 `58fdcafb` done, 2 of 2 usable (n=4 Muse HOLD 6 major / 1 minor, n=5 Sol HOLD 2 blocker /
    8 major / 2 minor; Sol: 5h window 9 -> 21 %, week 28 -> 30 %; both rated yes). Findings F10-1..7,
    F11-1..12. Blockers: F11-3 brief staging can write outside the worktree through a link the
    revision carries; F11-7 the maintain smoke passes without proof of the sub-agent switch.
    F10-6 rejected (Codex ignores unknown -c keys). The rest accepted -> wave R1 (brief
    `briefs/R1-review-fixes-reliability.md`, opus-worker, worktree `agent-ac4c333d5cd5125e3`).
  - part 2 `e5147fa6` done, 2 of 2 usable (n=6 GLM-5.3 ACCEPT 1 minor / 3 note, rated partly; n=7
    Grok HOLD 1 blocker / 3 major / 5 minor, rated yes). Findings F12-1..4, F13-1..9. Blocker:
    F13-1 `doctor --fix` can remove through a link named like a throwaway worktree. F13-9 wontfix
    (decided boundary: the rename race needs a local writer). The rest accepted -> wave R2 (brief
    `briefs/R2-review-fixes-repair-branches.md`, opus-worker, worktree `agent-aa5609ac7c9c247c3`).
  - part 3 `45a285b0` running (brief 07, 23b1455..8e6fd3e, kimi-claude k3.1-pro + Muse contributor).
  - parts 4 (brief 08, roster-p4, --rev 08ae5d5 --diff 8e6fd3e..08ae5d5) and 5 (brief 09, roster-p5,
    --rev e4f42a8 --diff 08ae5d5..e4f42a8) to launch.
  Note: a reviewer of an OLD revision marks the prior findings `still-open` because the fixes are
  later commits - an artefact, the store's statuses did not change. The first live run of `--rev` /
  `--diff` worked: one worktree per member, patch in handoffs, worktrees removed.
- 2026-11-23 ~01:10, diff-review continued:
  - part 3 `45a285b0` done, 2 of 2 usable (n=8 Muse HOLD 3 major / 2 minor, n=9 kimi-claude k3.1-pro
    HOLD 5 major / 1 minor; both rated yes). Findings F14-1..5, F15-1..6, no blocker: the catalog
    fetch can reach a local / private address and its 15 s deadline does not bound connect or a
    trickling body; the 500-pair cap forgets `older` refusals; the entry-bound smoke does not check
    the entry is still the chosen one and writes its roster at a predictable temp path; pending
    recovery compares against a stale installed version. F15-4 wontfix (decided boundary). The rest
    accepted -> wave R3 (brief `briefs/R3-review-fixes-maintain.md`), to launch AFTER R1 is applied
    (both touch `maintain/smoke.rs` and `maintain/mod.rs`).
  - part 4 `d243d6b5` done, 2 of 2 usable (n=10 Grok HOLD 4 major / 5 minor / 1 note, n=11 DeepSeek
    ACCEPT 8 minor / 2 note; both rated yes). Findings F16-1..10, F17-1..10: telemetry trust is per
    host, not per URL (F16-1, F17-5 -> moved into R3 as item 10); a run title from GitHub reaches
    the session unsanitised; the CI check's deadline does not cover its git calls or kill children;
    wrong default branch / run order; explain / replay differ from the real draw; lab table gaps; a
    missed weighty seat is silent; the daily gate file races. F16-5 wontfix (decided boundary). The
    rest accepted -> wave R4 (brief `briefs/R4-review-fixes-panel-ci.md`), to launch AFTER R2 is
    applied (it reuses R2's kill-the-children helper in the hook).
  - part 5 `bbe17837` running (brief 09, 08ae5d5..e4f42a8, Grok + Muse contributor + GLM-5.3).
    Its fixes (wave R5) after R1 is applied (both touch `engines/worktree.rs`, `consult/reviewed.rs`).
  Decided boundaries so far (wontfix, to name in the acceptance brief): F13-9, F15-4, F16-5 - all
  three need a local process with the user's own rights racing a check-then-write window; F10-6
  rejected. Worker queue (two heavy workers at a time): R1, R2 running -> R3 after R1, R4 after R2.
- 2026-11-23 ~01:50, diff-review finished (5 of 5 parts, 11 consultations, all usable, all rated):
  - part 5 `bbe17837` done, 3 of 3 usable (n=12 Grok HOLD 6 major / 1 minor, n=13 Muse REJECT
    1 blocker / 5 major, n=14 GLM-5.3 HOLD 1 major / 4 minor / 1 note; all rated yes). Findings
    F18-1..7, F19-1..6, F20-1..6. Blocker F19-1: `--rev` on the http engine only warns and sends a
    pack of the LIVE tree. Others: a one-revision `--diff` puts untracked files' contents (a secret)
    into the patch; the patch name is shared by runs with different ranges; under `--rev` the
    records and new findings name the live HEAD, not the revision read (and prior findings are
    offered for re-check against the wrong revision - the `still-open` artefact seen in parts 1-4);
    staging reads through a source-side link; a link in the reviewed revision can lead out of the
    worktree; `git status` on the live repo without --no-optional-locks; the bridge's own patch
    counts as a tree change. F19-4 is covered by R2, F20-5 by R1; the first half of F19-5 rejected.
    The rest accepted -> wave R5 (brief `briefs/R5-review-fixes-reviewed-revision.md`), AFTER R1.
  Totals of the second review: 82 findings (F10..F20), 5 blockers (F11-3, F11-7, F13-1, F19-1 + the
  http one counted once), 3 decided boundaries (F13-9, F15-4, F16-5), 1 rejected (F10-6).
- 2026-11-23 ~01:40, operator: C3 must be usable without eckSnapshot. Decisions: the project folder
  becomes lowercase `.c3` at the repository root (he first wrote `.C3`; lowercase after I named the
  existing `.collab/.c3/` and Linux case rules); it holds 'not only the collab files but all of
  eckSnapshot's innards'; created with stubs when missing; snapshots 'are apparently not much
  needed - only to hand something to a web reviewer that has no bridge'. Plan (wave S1, after the
  R waves, inside 0.2.5, then a sixth review part): `.c3/` git-ignored at the root with the
  manifests, the journal, the finish report, scouts and the cache now in `.collab/.c3/`; a one-time
  move of an existing `.eck/` by `c3 init` / `c3 doctor --fix` / `c3 finish` (never the hook, never
  a merge of two folders); `c3 finish` writes no snapshot by default; `.eckignore` -> `.c3ignore`
  and `--eck` -> `--state` with the old names still read; the `[STUB]` / `[STUB:` mismatch fixed;
  the root `AGENTS.md` rewritten. `.collab/` (committed records) stays where it is - told to the
  operator, not objected to so far. Recon report: 20 literals, no shared constant, ~80 test hits.
- 2026-11-23 ~03:10, waves R1 and R2 reported and APPLIED to main together (R2 first, clean; R1 on
  top by 3-way, the only conflict CHANGELOG.md, both sides kept).
  - R1 (12 items, both blockers were real at HEAD): staging checks the destination in the
    revision's tree and on disk, writes a new file and renames; a staging failure is a pre-start
    refusal; the prompt names only tracked changed paths; agy steps merged before classification,
    an unexplained change is `unattributed`, a changed bound input withholds the reply (agy, runs
    outside a worktree); the member spec carries the findings as shown (`listed`); the smoke has
    `Unproven`, treated as a failure by every automatic path; rollout identity, terminal record,
    absolute CODEX_HOME; one guard inspection after exit; secondary prompts carry the role
    sentence; the death record uses the run's own stream; `reply_kept` on failed ledger entries.
    Accepted: `reply_kept` only on failed entries (ledger order pinned by telemetry_parity). Open:
    the bound-input rule for muse / grok / claude outside a worktree (still a warning - debt); the
    agy denial-retry prompt lacks the role sentence (-> R5); no panel integration test for the
    re-seat after a staging refusal; agy's Cascade reader names are unconfirmed by a fixture.
  - R2 (8 items): a throwaway path that is a link is reported and never removed; the owner record
    is mandatory (creation fails without it); no owner record -> removable only when git calls it
    prunable (the 6-hour rule is gone); liveness has `cannot tell`; a stray `.git` is not a
    repository and `GIT_DIR` set disables the hook's repair; a record needs a real date; every
    child of `c3 update` is marked; `--set` / `--clear` refuse in a child (coordinator added
    `--default`); the hook deadline kills the scan's git children (`run_registered`);
    `AGENTS.md` never follows to a model dated older; a failed `git cherry` is unchecked; wording.
    Accepted: C3's own re-executions (detached consult / work, panel members, MCP calls) stay
    unmarked; a stray `.git` stops the upward search.
  Verification on the combined tree: fmt, clippy clean; c3-core + c3 all green (793 lib tests);
  c3-cli kimi_engine 18, claude_engine 18; on R2 alone branches 14, init_doctor 15, update 12. The
  remaining c3-cli binaries are left to CI (local background runs get reaped under memory
  pressure).
- 2026-11-23 03:15: R1 + R2 committed as 99e2da7 (secret gate: the saved part-4 patch allowed after
  review - its one hit is the same fixture line of `panel/plan.rs`), pushed, CI run 45093260152
  watched. Their 30 findings set to `implemented`. R1's worktree removed; R2's is locked by the
  harness - recorded `parked`. Launched from 99e2da7: wave R3 (worktree `agent-a0df9cb5a58b0d27e`)
  and wave R5 (worktree `agent-acc7c7565a4b2f142`; the agy denial-retry prompt added as item 12).
  Queue: R4 when one of them reports (brief names R2's `run_registered` / `ScanChildren`), then S1
  (`.c3` folder) after R4 and R5 are applied, then a review of R1-R5 + S1, the version, Astra.
- 2026-11-23 ~03:10 CET, CI for 99e2da7 (run 45093260152): Linux green on both jobs; the Windows
  job sat in its Test step for 36 minutes (normal: 15) until I cancelled it; GitHub kept NO log of
  the cancelled job (BlobNotFound), so the hanging test is unknown. Attempt 2 started 02:52 CET and
  is watched with a cap. Locally on the combined tree, each with a hard time limit: compat_wave2b
  20, subagent_guard 4, grok_engine 18 green, no hang; maintain 30 passed / 6 failed - all six say
  `grok skipped: in use`: another session's reviewer runs a real `grok.exe` on this machine and the
  maintain tests' in-use probe reads the real process table (environmental; debt: make the probe
  injectable in test mode). `timeout-minutes` added to both CI jobs (45 / 30), uncommitted - goes
  with the next commit.
- 2026-11-23 03:12 CET: CI for 99e2da7 GREEN on all three jobs at attempt 2 (Windows 19 min 20 s).
  So the 36-minute hang of attempt 1 is intermittent, test unknown (no log kept). Second
  intermittent Windows incident after the cancellation inside `work` (08ae5d5, attempt 1). Debt:
  split the Windows test step per crate (the API shows the running step even when the log is
  lost), so a hang can be located; the job timeout (45 min) now bounds it.
- 2026-11-23 ~03:45 CET, wave R3 reported (10 items, none was fixed at HEAD) and APPLIED to main's
  working tree (clean apply, `patches/r3.patch`): no IP-literal / localhost catalog host, a private
  peer's answer discarded before the body, also refused at name lookup; one deadline (per-hop
  connect = time left, body in 8 KiB chunks); `older` pairs of seated models never evicted;
  `Target.seated` + `bound_roster` refuses a changed entry; the smoke's own scratch directory with
  exclusive create; protection fields through `validate_roster`; a stamp later than now + 600 s
  never qualifies; pending recovery re-reads the installed version; MODELS=off returns before the
  lock; catalog source = full URL (`web:https://host:port/path?query`), trust per URL, legacy host
  records read but never trusted. Follow-up sent to the same worker: a printed source name must not
  carry the query string (it can hold a token) - one display function. When it reports: reverse
  `patches/r3.patch`, apply the new full patch, verify (maintain lib tests, c3-cli maintain and
  telemetry_parity - with no real grok.exe running), commit together with the ci.yml timeouts.
  Wave R4 launched from 99e2da7 (worktree `agent-a7d4436e8f37ebda0`); R5 still running.
- 2026-11-23 ~04:40 CET: the R3 follow-up done (`catalog_source_display`: no query string in the
  one printed line, the stored key keeps the full URL; maintain 38 green in the worker's run). Its
  final patch re-applied to main (note: `git apply --3way` stages its result - before re-applying a
  corrected patch, reverse the old one AND `git reset -q`). On main with R3: clippy clean, c3 lib
  809 passed.
- 2026-11-23 ~04:45 CET, wave R5 reported (12 items; F19-4 and F20-5 confirmed fixed at HEAD) and
  APPLIED on top of R3 (`patches/r5.patch`, no conflict): under `--rev` the http engine gets the
  same throwaway worktree and its pack is built from it, the patch is part of the pack; every
  `--diff` patch goes through `pack::redact` (ledger `redactions`); patch file
  `<stem>.<sha8>.patch`, never overwritten; under `--rev` the Reviewed line, the ledger and new
  findings name the revision read, `live_head` beside it, and when it is not the live HEAD the open
  findings are withheld (one sentence, `prior_findings_withheld`); `GIT_OPTIONAL_LOCKS=0` on every
  live-repo call; the collab snapshot is taken after the patch is written; staging never reads
  through a link; links leading out of the worktree are unlinked right after the checkout; the
  worktree check uses `--ignored`; `check_rev_value` / `parse_diff_spec` shared by CLI and MCP, git
  gets values after `--end-of-options` (git >= 2.30); the agy denial-retry prompt has the role
  sentence. Accepted: record switch for `--rev` runs only; extra ledger keys `live_tree_sha256`,
  `prior_findings_withheld`; a pack drops a root `.git` file. Not covered by a test: the live-tree
  agy half of the own-patch item; an end-to-end http run (`--base-url` must be https).
  Verifying R3 + R5 together on main (fmt, clippy clean; lib tests running), then one commit with
  the ci.yml timeouts.
- 2026-11-23 05:25 CET: R3 + R5 + the ci.yml job timeouts committed as 5d92ca7, pushed, CI run
  45105895566 watched (40-minute cap). Verified locally on the combined tree: fmt, clippy; c3-core
  all green; c3 lib 816 of 817 (the one failure is the 5-second timing test
  `consult::ingest::tests::normalise_is_linear_on_a_large_reply`, 9.7 s on the loaded machine,
  1.5 s alone - debt: a wall-clock assertion); c3-cli mcp_integration 2, telemetry_parity 11,
  kimi_engine 20, claude_engine 18. NOT verified locally: c3-cli `maintain` (a real grok.exe of
  another session runs here; the run was also cut by its time limit: 24 passed, 1 failed, 13 not
  reached) - left to CI. Their 31 findings set to `implemented`. R5's worktree removed; R3's is
  locked by the harness - `parked`. Still running: wave R4. Then S1.
  Open findings left `proposed`: only R4's (F16-2..4, F16-6..10, F17-1..4, F17-6..10).
- 2026-11-23 05:45 CET: CI for 5d92ca7 (R3 + R5) GREEN on all three jobs (run 45105895566, Windows
  18 min 25 s) - `maintain` included, which could not be verified locally.
- 2026-11-23 ~05:50 CET, wave R4 reported (10 items) and APPLIED to main (`patches/r4.patch`, the
  only conflict CHANGELOG.md, both sides kept): the CI line is `CI: the latest completed run on
  <branch> failed (run <id>) - gh run view <id>` with no title, pull-request runs ignored,
  `--json` `ci` is an object; R2's `ScanChildren` / `run_registered` moved to a shared module
  `bounded.rs`, one deadline covers every git and gh process of the check and `stop` kills the
  whole process tree (for the branch line too); `remote_default_branch` (origin/HEAD without a
  local branch), runs ordered by parsed time, 20 runs; a GitHub remote by its host; one gate file
  per repository; `router explain` uses the draw's own eligibility (it now runs the member
  election, so it does the login / preflight checks), the routed record carries
  `ext.c3.draw` at full precision when rounding lost something; labs for minimax / meta-llama /
  llama, prefixes at a boundary; a missed upper-tier seat is a warning; the .ps1 never cuts a
  surrogate pair and its catch path prints the waiting rule; wording for the delegation rule.
  Verification on main (R1-R5 all applied): fmt, clippy clean; c3-core 251, c3 lib 828, every
  other c3 test binary green; c3-cli ci_check 10, hook_scripts 5, delegation_rule 6, finish 8,
  panel_light_required 2, branches, init_doctor, update, wave9_consilium, notspooled_parity,
  wave6_live_diffs green; the remaining binaries were still running locally at commit time - CI
  runs them all.
- 2026-11-23 05:56 CET: R4 committed as 748ba3a, pushed, CI run 45108728499 watched. Its 17
  findings set to `implemented`. The store now: 118 findings - 114 implemented, 3 wontfix (decided
  boundaries F13-9, F15-4, F16-5), 1 rejected (F10-6), 0 proposed. R4's worktree removed. Local
  runs on main with R1-R5: wave6_live_diffs, snapshot_partb, compat_wave5 green as well.
- 2026-11-23 ~06:00 CET: wave S1 launched from 748ba3a (opus-worker, worktree
  `agent-a8d34992991e2e59f`, brief `briefs/S1-standalone-c3-folder.md`). After it lands the
  coordinator migrates this repository's own `.eck/` with the new binary.
  Verification round prepared meanwhile: three delta briefs being written (handoffs 21-23: fix
  commits 99e2da7, 5d92ca7, 748ba3a), to run with `scratchpad/bin/c3-review2.exe` (built from the
  tree that became 748ba3a; reports `5d92ca7-dirty`) as `--rev 748ba3a --diff <range>`: with the
  revision equal to the live HEAD the prior findings ARE offered, so the reviewers' `fixed` marks
  land in the store. Planned seats: V1 Sol + Muse contributor + Grok; V2 kimi-claude + Grok +
  Muse contributor; V3 Grok + GLM-5.3. A fourth part for S1 after it is committed.
  New debts to write into TECH_DEBT after S1: the maintain tests' in-use probe reads the real
  process table (a real grok.exe fails them); `normalise_is_linear_on_a_large_reply` is a
  wall-clock assertion; the intermittent Windows CI hang / cancellation (split the test step per
  crate); the bound-input rule covers agy only; no panel test for a re-seat after a staging
  refusal; agy's Cascade reader names unconfirmed; the hook's 3 s CI budget under load; no
  end-to-end http test under `--rev`; `git >= 2.30` for `--end-of-options`.
- 2026-11-23 ~06:35 CET: the verification briefs are written (handoffs 21-23). The dry run of V1
  found a REGRESSION from wave R1: no panel member could start (`could not start the member
  process - os error 206`, the command line limit) because the member spec now carries the text
  of every listed finding and went on the command line as `--panel-spec <base64>`; with this
  task's 114 open findings it is far past 32 767 characters. No test had more than a few findings.
  Fixed by the coordinator: the parent writes the spec to `member-<nn>-<i>.spec` in the panel's
  temp directory and passes `--panel-spec-file`; `cli/consult.rs` reads it into the same option
  (a missing or empty file is an unreadable spec, never an absent one). Tests: unit
  `member_spec_arg_tests::the_member_spec_is_read_from_its_file`, integration
  `panel_light_required::a_panel_with_many_open_findings_starts_its_members` (160 findings); the
  live dry run with 114 findings now plans all three members. clippy clean, panel unit tests 62.
- 2026-11-23 06:13 CET: the spec-file fix committed as b9f665a and pushed. Verification panel V1
  launched: detach id `33fa9671` (brief 21, `--rev b9f665a --diff 4f43074..99e2da7`, Sol + Muse
  contributor + Grok, binary `scratchpad/bin/c3-review3.exe`). Then V2 (brief 22, roster-v2,
  `--diff 99e2da7..5d92ca7`) and V3 (brief 23, roster-v3, `--diff 5d92ca7..748ba3a`; fill the CI
  line of brief 23 first), always `--rev <HEAD at launch>`. Wave S1 still running.
- 2026-11-23 ~06:30 CET: CI for 748ba3a (R4) FAILED on Windows in one new unit test,
  `ci_check::tests::the_checks_deadline_kills_gh_and_its_children`: `outlived the check's
  deadline: [6980] of [2752, 7660, 6980]` - gh.cmd's grandchild survived `ScanChildren::stop`.
  `kill_tree_by_pid` on Windows was one `taskkill /F /T` whose result nobody read. Fixed by the
  coordinator: the descendants are enumerated BEFORE the kill (pid + start time) and every one
  still alive with the same identity is terminated after it (the pattern `kill_tree_checked`
  already uses). Local: fmt, clippy clean, ci_check / bounded / hook unit tests 20 passed. Linux
  was green in that run.
- 2026-11-23 06:22 CET: the tree-kill fix committed as fdf193a, pushed; CI runs 45110403225
  (b9f665a) and 45111334886 (fdf193a) watched.
- 2026-11-23 ~06:40 CET, verification V1 `33fa9671` done: 2 of 3 usable. n=15 Grok HOLD 3 minor /
  1 note, prior 32 fixed, 2 still-open (F13-6, F18-5); n=16 Muse ACCEPT 1 minor / 2 note, prior 30
  fixed; both rated yes. n=17 Sol FAILED: transport, `os error 11001` (host unknown) at the start,
  0 s - a network failure, not rated; to be repeated alone after V3 (the part's blockers were
  Sol's). New findings F24-1..4, F25-1..3, none major. Decisions: F24-1, F25-1 wontfix (the same
  decided boundary); F24-3 wontfix (R2's decision: a stray `.git` stops the repair); F25-2 wontfix
  (documented: without catalog dates the version rule decides - F13-6's undated half); F25-3
  wontfix (R1's decision: the 5 s slack stays where the rollout gives no turn boundary).
  ACCEPTED, to fix in a last small wave R6: F24-2 (a death record prefers a pending record's
  stream over the newer noted one) and F24-4 (agy `merge_parameters` does not merge into a nested
  object that already has keys). F18-5 `still-open` per Grok (`stage_file` reads with
  `std::fs::read`) - R5 added the link refusal before the read; V2 covers it.
  V2 launched: detach id `ccc5cfa8` (brief 22, `--rev fdf193a --diff 99e2da7..5d92ca7`,
  kimi-claude + Grok + Muse contributor).
- 2026-11-23 ~06:45 CET, operator (`/goal`): 'bring it to the end, finish off all tech debts, merge
  to main, push and deploy, bring the content of xelth.com/C3 up to date' - a session goal with a
  stop hook. Standing authorisation for: closing every open tech-debt item, commits to main,
  pushes, the release (version, acceptance, tag), the install and the xelth.com/C3 page.
- 2026-11-23 06:45 CET: CI GREEN for b9f665a (run 45110403225) and fdf193a (run 45111334886), all
  three jobs each. The R4 unit test passed on b9f665a WITHOUT the tree-kill fix, so that failure
  was intermittent; the fix stays (the old code did not check the kill).
- 2026-11-23 ~07:00 CET, the plan to close ALL tech debt (20 open entries of TECH_DEBT.md + about
  ten recorded tonight). Three waves, briefs in the scratchpad `briefs/`:
  - D1 bridge (`D1-debt-bridge.md`): single-run findings snapshot; bound-input rule for every
    engine outside a worktree; F24-2 death-record stream; F24-4 agy nested merge; collab paths out
    of the uncommitted-paths paragraph; a panel test for `--rev`; a panel test for the re-seat
    after a staging refusal; an e2e http run under `--rev` if honest; decide the guard's fork gap
    and the smoke children's test variables; the timing test becomes a linearity test.
  - D2 tooling (`D2-debt-tooling.md`), AFTER S1: secret gate (a plain word is no secret) + MCP
    `allow_secret`; pre-finish checks (`finish_checks`); `c3 branches --record-unknown` and
    `--close`; `c3 hook` caps its own lines; the hook test without the 2 s deadline; doctor checks
    git >= 2.30; the knowledge-base decision ('quote it in the brief') in the skills; the memory
    reaper paragraph in the coordinate skill.
  - D3 maintain / telemetry / CI (`D3-debt-maintain-ci.md`), LAUNCHED from fdf193a (worktree
    `agent-ac53dd0b4c5c5ac8d`): maintain tests' in-use probe by a test hook; a swap across a major
    held when the list price more than doubles; a roster id matched through the lab table; a
    published Claude id (dots -> dashes) known to telemetry; the CI test step per crate; decisions
    (b), (e), (g) of the catalog entry.
  Closed by observation after the install (the coordinator, live): a real `c3 maintain` pass (the
  first real runs of 0.2.4 / the catalog check), a live kimi-engine consultation (housekeeping
  files, resume across directories), the pinned plugin harnesses for the effort relaxation.
  Cannot be closed now, stays an observation: agy's reader names (gemini is out until 2026-11-28).
  Decisions without code: the branch record is local to one clone (by design); the hook's 3 s CI
  budget (a check that does not fit stays silent); the catalog's third-party date.
- 2026-11-23 ~07:25 CET, verification V2 `ccc5cfa8` done, 3 of 3 usable, all rated yes: n=18 Grok
  HOLD 2 major / 1 minor / 1 note (prior 29 fixed, 2 still-open), n=19 kimi-claude REJECT 2 minor /
  1 note (prior 25 fixed, 6 still-open), n=20 Muse ACCEPT 1 major / 2 minor (prior 31 fixed).
  Findings F27-1..4, F28-1..3, F29-1..3. Majors: F27-1 (+ F15-1 still-open) the address predicate
  lets CGNAT, 0.0.0.0/8, benchmark and fec0::/10 through; F29-1 a proxy from the environment would
  bypass the peer check; F27-2 the post-checkout link sweep descends without the P4 re-check.
  Still-open parts: F14-1 (connect shares the pre-lookup budget), F15-3 (an endpoint / plan edit
  that keeps provider and model is still smoked). Routed: F27-1, F29-1, F14-1, F15-3 -> sent to the
  running D3 worker as items 7-10; F27-2, F27-3, F28-1, F28-2, F28-3, F29-3 and the untracked-files
  visibility (F18-4 / F19-2 / F20-6: print and record the untracked files a one-revision patch
  carries, `--diff-tracked-only`) -> D1 brief items 12-18. wontfix: F29-2 (the decided boundary),
  F27-4 (the lookup thread cannot be interrupted). F19-5's first half stays rejected.
  V3 launched: detach id `e9bf7f2d` (brief 23, `--rev fdf193a --diff 5d92ca7..748ba3a`, Grok +
  GLM-5.3). Then Sol alone on V1's range.
- 2026-11-23 ~07:20 CET: addition sent to the S1 worker (point 11): in other projects the collab
  directory is INSIDE the state folder (`--collab-dir .eck/collab` in the operator's global
  skills) - after the move both spellings must resolve, a consult may trigger the move, a failed
  rename (a live run holds files) falls back to the legacy path for that run. After S1 lands the
  coordinator updates the global `astra` / `workers` skills to `.c3/collab`.
- 2026-11-23 ~07:50 CET, verification V3 `e9bf7f2d` done, 2 of 2 usable, both rated yes: n=21
  GLM-5.3 ACCEPT 2 minor / 4 note (prior 18 fixed), n=22 Grok HOLD 2 major / 4 minor (prior 13
  fixed, 4 still-open). Findings F30-1..6, F31-1..6. Majors: F31-1 a deadline stop does not reap a
  grandchild whose parent shim already exited (and `within` does not join its worker); F31-2 with
  `origin/HEAD` unset the CI check still asks about a local main / master. Routed: F31-1, F31-2,
  F31-3 / F30-3 (same-repo double gh: exclusive gate), F31-4 / F30-5 (github host names), F31-5 /
  F30-6 (temp leftovers) -> D2 brief items 9-12; F31-6 / F30-1 (`router explain` starts probes),
  F30-2 (tier warning when pins fill the panel), F30-4 (replay mixing precise and rounded) -> D1
  brief items 19-21. F17-6 stays as documented.
  Sol alone on V1's range launched: detach id `f204c8dd` (`--rev fdf193a --diff 4f43074..99e2da7`).
- 2026-11-23 ~08:35 CET, Sol alone on V1's range, `f204c8dd`, usable, rated yes (n=23): HOLD,
  1 blocker / 3 major / 1 note, prior 16 fixed, 19 still-open. 5h window 25 %, WEEK 38 % - no more
  Sol runs before the acceptance. Findings F32-1..5:
  - F32-1 BLOCKER: staging creates and publishes by path name after the validation - a directory
    component that changes identity sends the bytes outside the worktree. This is the class I had
    recorded as a decided boundary (F13-9, F15-4, F16-5, F24-1, F29-2). The boundary is WITHDRAWN
    for everything inside a worktree or the repository: wave D4 (`briefs/D4-containment-by-
    handles.md`) moves staging (both sides), the link sweep, the removal walk, the repository
    repair and the patch write to directory handles (`cap-std`), with race tests through a test
    hook. F15-4 (the roster rename in the codex home) stays a boundary.
  - F32-2 major (supersedes F11-8): the smoke proves the switch per rollout, not per turn -> D3
    item 11 (sent).
  - F32-3 major: an agy reader whose file name holds `&&` is classified as a command -> D1 item 22.
  - F32-4 major (supersedes F13-5): the runner unregisters a child at pipe EOF, before it exits ->
    D2 item 13 (with F31-1, item 9).
  - F32-5 note: the member-spec file fix must stay - implemented (b9f665a).
  Still-open per Sol, routed: F10-3 / F11-4 (a committed brief deleted live is refused at the
  entry) -> D1 item 23; F10-7 / F11-11 / F24-2 -> D1 item 3; F24-4 -> D1 item 4; F11-12 -> D1 item
  24; F12-1 (Linux procfs) -> D2 item 14; F13-2 (HEAD alone) -> D2 item 15; F27-2 -> D4. Kept as
  decided / documented: F10-1 / F11-1's 'an unlogged write stays usable' (attribution by the tool
  log is the evidence there is; a live-tree agy cannot be tested - gemini is out until 2026-11-28),
  F11-9 (the 5 s slack without a turn boundary), F12-3 (a cloud placeholder is not repaired),
  F12-4 (git's own config write), F13-6 (no dates - the version rule).
  D1 brief: items 12 and 14 moved into D4; 22 items. D2 brief: 15 items. Order after S1 lands:
  D4 and D2 in parallel, then D1 (it shares `consult/reviewed.rs` and `engines/worktree.rs` with
  D4). D3 is running.
- 2026-11-23 ~09:40 CET, wave S1 reported (11 points, all done) and APPLIED to main (clean apply,
  `patches/s1.patch`): new `crates/c3/src/state_dir.rs` owns the folder names, the ignore-file
  names, `ensure`, the one-time move and `collab_dir`; `.c3/` holds the manifests, `last-report.md`,
  scouts, snapshots and the cache that was in `<collab>/.c3/`; EVERY writing command moves an
  `.eck/` once (not only the five named - else `c3 scout` would create a fresh `.c3/` beside it);
  no snapshot by default (`--snapshot`); `--state` / `.c3ignore` with the old names as fallbacks;
  the stub detector takes `[STUB]` and `[STUB:`; doctor reports and `--fix` replaces the old
  template sentence; `AGENTS.md` rewritten. Point 11: a collab path inside the folder follows the
  move in both spellings, the move waits while a run is live under `.eck/`, a failed rename falls
  back to the legacy path for that run. Accepted: 'both folders exist' is a warning; inside a C3
  child a writing command refuses when `.c3/` is missing; plain `c3 snapshot` output goes to
  `.c3/snapshots/`.
- 2026-11-23 ~09:45 CET, wave D3 reported (11 items, all done) and APPLIED on top (CHANGELOG
  conflict, both sides kept): the maintain tests' in-use probe is a test hook; a swap across a
  major is announced only when the catalog's list price more than doubles; a roster id with a
  lab / plan token is matched through the vendor slug and written back in the plan's spelling; a
  dotted `anthropic/` id is also published in dash form; CI test step per crate; global-unicast
  table for public sources (lookup and peer, every hop); no proxy from the environment; the
  connect gets the time left after the lookup; the entry fingerprint (canonical JSON, sha256) in
  `Target`; the smoke proves the switch per turn (F32-2). Decisions (b), (e), (g) written for the
  tech-debt file. Worker's note for the operator: `openai :: gpt-6.2-sol` is neither protected nor
  on request, so in swap mode a catalog successor of Sol would be smoked on the ChatGPT account
  and seated (Astra is protected) - left as it is, to tell the operator.
  Verification on main with S1 + D3: fmt, clippy clean; c3-core 252, c3 lib 852, every other c3
  binary green; c3-cli finish 18, init_doctor 19, snapshot_partb 7, delegation_rule 7,
  mcp_integration 2, maintain 44 (no real grok needed any more), telemetry_parity 12. The other
  c3-cli binaries: the S1 worker ran branches, ci_check, update, claude_engine, kimi_engine, work
  on its tree; CI runs all.
- 2026-11-23 10:05 CET: S1 + D3 committed as 70ed824 (no snapshot), pushed, CI run 45122961752
  watched. Their worktrees removed. Findings F27-1, F29-1, F32-2 set to `implemented`. The
  installed binary stays `c3 0.2.4 (ba43ff6)` until the release: it writes `.eck/`, so this
  repository's own folder is NOT moved yet - installing the new binary would also start moving the
  folders of every other project, which belongs to the release, not to the middle of it.
  Launched from 70ed824: wave D4 (worktree `agent-a04c52be497ae4fe2`, `cap-std`, the handle
  containment) and wave D2 (worktree `agent-a85da25d0039ad2b5`, 15 tooling items). D1 waits for
  D4 (shared files). Two review briefs for 70ed824 are being written (handoffs 33 S1, 34 D3), to
  run with `scratchpad/bin/c3-review3.exe` (a build WITHOUT S1 - it cannot move a folder):
  33 with Grok + GLM-5.3, 34 with kimi-claude + Muse contributor, `--rev 70ed824 --diff
  d37a397..70ed824`.
- 2026-11-23 ~10:50 CET: CI for 70ed824 (run 45122961752): default build GREEN on Linux and Windows;
  the index-surreal job failed in clippy (`field collab_root is never read`, `cli/index.rs` - S1
  removed its only reader, which the default build never compiles). Fixed by the coordinator in
  aa71c6e (the field is gone), pushed, CI run 45126024204 watched.
- 2026-11-23 ~10:55 CET, review of S1 `fcb610cd`: 1 of 2 usable. n=25 Grok HOLD 6 major / 2 minor,
  rated yes; n=24 GLM-5.3 failed (z.ai 529 overloaded - the provider, not rated). Findings
  F36-1..8, ALL ACCEPTED -> wave S2 (brief `briefs/S2-c3-folder-move-safety.md`): one mover at a
  time and the loser re-reads the state (F36-1: today a lost race re-creates `.eck/` beside the
  moved `.c3/`); the leftovers are relocated INSIDE `.eck/` first and the rename is the last step
  (F36-2); no write / read / move through a link, `.c3` itself must be a real directory (F36-3);
  a read-only command never moves the folder (F36-4); a collab path is normalised before it is
  classified (F36-5: `sub/../.eck/collab`); the live-run guard fails closed and holds under the
  move lock, a starting run takes the lock too (F36-6); the switch's wording (F36-7); nested
  `.c3/` / `.eck/` out of a patch (F36-8). To launch when a worker slot frees.
  Review of D3 launched: detach id `d5d3e391` (brief 34, kimi-claude + Muse contributor).
- 2026-11-23 11:08 CET: CI for aa71c6e GREEN on all three jobs (run 45126024204), index-surreal
  included - main with S1 + D3 is verified.
- 2026-11-23 ~11:20 CET, review of D3 `d5d3e391`: 1 of 2 usable. n=27 Muse ACCEPT 2 minor / 3 note,
  rated yes; n=26 kimi-claude failed (the Kimi plan's 5-hour limit). Findings F38-1..5. Accepted:
  F38-2 (Teredo 2001::/32, 6to4 2002::/16 and IPv4-compatible ::/96 are missing from the
  non-global IPv6 table) -> the last small wave. wontfix with reasons: F38-1, F38-3, F38-4, F38-5.
  A second reader for D3: Grok alone, detach id `da22521b` (brief 34).
- 2026-11-23 ~11:25 CET, wave S2 launched from aa71c6e (worktree `agent-a9482c1199dade578`).
- 2026-11-23 ~11:30 CET, a peer session (`xelixir-wt-mobile-21`) asked that C3's cargo runs join
  the compiler turn the other sessions use: the file `HOLD` in `C:/Users/Dmytro/twenty-crm-wt` is
  taken before ONE cargo command (created only if absent, one line naming session and command,
  removed afterwards, never somebody else's) and builds start only with >= 6 GB free RAM. Agreed.
  Wrapper: `scratchpad/turn.sh <label> cargo ...`; the three workers (D4, D2, S2) were told to
  run every cargo command through it, the coordinator does the same. Expect the waves to take
  longer.
- 2026-11-23 ~11:50 CET, Grok alone on D3 `da22521b`, usable, rated yes (n=28): HOLD 2 major /
  3 minor / 1 note, F39-1..6. Accepted -> wave R6 (sonnet-worker, worktree
  `agent-af517700ab95d71d9`, from aa71c6e): F39-1 (= F38-2: IPv4-compatible ::/96, 6to4 2002::/16,
  Teredo 2001::/32 and benchmarking 2001:2::/48 into the non-global table), F39-2 (the price ratio
  only from two rows of one vendor in one source - no cross-vendor fallback a decoy row could
  use), F39-4 (a roster token is an alias only when it names the model's maker or the plan, not
  the endpoint host's lab alone). wontfix with reasons: F39-3, F39-5, F39-6.
  Store: 171 findings - 118 implemented, 13 wontfix, 1 rejected, 39 proposed (all routed to the
  running waves D1, D2, D4, S2, R6).
  Running: D4, D2, S2, R6 - all through the compiler turn. Waiting: D1 (after D4).
- 2026-11-23 ~12:30 CET, wave R6 reported (3 items) and committed as 84dbca4 without a rebuild on
  main (its three files are identical to the worker's verified tree: clippy clean, `maintain::`
  100, c3-cli maintain 44), pushed. F38-2, F39-1, F39-2, F39-4 set to `implemented`.
- 2026-11-23 ~12:35 CET, the CRM session (`9eck-com-1c`) reported C: at 9.5 GB free and falling.
  Deleted the `target` directories of the five finished worker worktrees (they are locked by the
  agent harness, so the worktrees themselves stay `parked`): C: 9.2 -> 20.9 GB free. From now on
  a wave's `target` is deleted as soon as the wave is merged. `turn.sh` now stamps its HOLD line
  when the turn is taken (it stamped the start of the wait). A release build is announced to the
  peers before its turn.
- CORRECTION, written 2026-11-23 09:18 (the machine's clock): the clock times in the entries above
  from '~06:35 CET' on were ESTIMATED by the coordinator and run ahead of the real time by up to
  3.5 hours (the entry dated '~12:30' was written at 08:58). The commit times are the anchors:
  b9f665a 05:12, fdf193a 05:21, d37a397 06:36, 70ed824 07:16, aa71c6e 07:43, 84dbca4 08:58. From
  here on entries carry the clock's time.
- 2026-11-23 09:18: running - D4, D2, S2 (all three through the compiler turn, shared with the CRM
  and the xelixir sessions), CI run 45134957899 for 84dbca4. Waiting: D1 (after D4).
- 2026-11-23 09:40: CI for 84dbca4 (R6) GREEN on all three jobs. `cargo clean` on main (8.1 GB)
  after the CRM session reported the disk again: C: 23 GB free. The three running workers hold
  about 9 GB of `target` together.
- 2026-11-23 10:00: a STALE compiler-turn file of this session blocked every session for about 15
  minutes: the S2 worker's wrapper was killed (a background task ended) before its trap removed
  the file. The S2 worker reported it instead of deleting it; the coordinator checked (pid gone,
  no cargo / rustc running) and removed it. `turn.sh` now removes a stale turn of THIS session
  family by itself (line names `c3-release-0.2.5`, pid gone, file older than a minute, no cargo or
  rustc anywhere) and traps HUP too. Both peer sessions were told.
- 2026-11-23 10:40: the xelixir session had waited for the compiler since 10:26 (its wrapper checks
  RAM before it creates HOLD, C3's wrappers took the file first) and asked for one turn. `turn.sh`
  got a pause (`scratchpad/turn.pause` holds an epoch time until which no C3 wrapper takes the
  turn), installed by a RENAME - editing the script in place at 10:00 had broken wrappers that were
  already waiting (bash reads a script as it runs; the S2 worker found it). Pause set until 10:55;
  the two waiting compile jobs (S2 init_doctor, D2 ci_check) were stopped and their workers told
  to rerun them.
- 2026-11-23 11:01, wave D4 reported (every use site converted) and committed as 272920d, pushed;
  CI run 45152085896 watched. New module `crates/c3/src/contain.rs` on `cap-std` 4.0.3 /
  `cap-fs-ext` 4.0.3: staging (both sides), the link sweep, the removal (through handles, then
  `git worktree prune`; no `worktree remove --force`, no `remove_dir_all`), the repository repair,
  the rules rewrite and the patch write. Proof on Windows: a junction as a path component is
  refused; a held handle keeps the directory and its ancestors from being renamed; the directory-
  symlink half is skipped here (no privilege). Race tests through `CODEX_CONSULT_TEST_CONTAIN_RACE`.
  Not converted by instruction: the maintain roster writer, the owner record in git's admin dir.
  Not rebuilt on main (its files equal the worker's tree; the commits since its base touch other
  files) - CI is the check. Findings F32-1, F24-1, F29-2, F18-5, F27-2, F16-5, F13-9, F28-1 set to
  `implemented` (F13-9, F16-5, F24-1, F29-2 had been wontfix: the boundary is withdrawn; F15-4
  stays). D4's `target` deleted, worktree removed.
  Accepted: a link inside the repository on the way to the brief / handoffs is refused or skipped
  (it was followed); the worktree's root handle stays open during a run, so on Windows the
  worktree directory cannot be renamed while a reviewer runs.
  Wave D1 launched from 272920d (worktree `agent-a2ae6fdd082032180`, 22 items). Running: D2, S2,
  D1 - all through the compiler turn.
- 2026-11-23 11:29: CI for 272920d (D4) GREEN on all three jobs (run 45152085896) - the handle
  containment is verified on Linux and Windows.
- 2026-11-23 11:29, wave D2 reported (15 items, all done) and committed as 0eea5f1, pushed, CI run
  45156540894 watched. Applied on top of D4 (conflict only in CHANGELOG.md, both sides kept);
  on main: fmt clean, `cargo clippy --workspace --all-targets -- -D warnings` clean through the
  compiler turn (a 10-minute wait for a 5-minute run); the tests of the combination are left to
  CI. The old installed gate refused the commit once more: CHANGELOG.md and README.md now hold
  the example line that assigns the plain word `endpoint` to `auth` - allowed after checking that it is the only hit in each.
  NOT run anywhere yet: the Linux-only paths (process groups in the deadline stop, `/proc`
  liveness) - CI's ubuntu job is their first run.
  Accepted: `--close` refuses a branch whose patches are on main but that is not merged by
  ancestry (`git branch -d` would refuse it); every registered child leads its own process group
  on Unix; the daily claim is per UTC day; a `config.json` that does not parse refuses the finish.
  Findings F31-1..5, F30-3, F30-5, F30-6, F32-4 set to `implemented`. D2's `target` deleted.
  To do after the install: write `.c3/config.json` with `{"finish_checks": ["cargo fmt --all
  -- --check"]}` for this repository.
  Running: S2, D1.
- 2026-11-23 11:51, wave S2 reported (F36-1..8, all done) and committed as ac26f5a, pushed; CI run
  45159614961. On top of D4 + D2: one conflict in `finish/mod.rs` merged by hand (D2's journal line
  for skipped checks + S2's `write_journal` through the resolved folder); fmt and workspace clippy
  clean through the compiler turn; tests of the combination left to CI. The first commit attempt
  was refused by the old gate over a line of this file; the same shell line had already gone on
  and written the previous HEAD into brief 42 and into the F36 notes - both corrected.
  S2 summary: an exclusive move lock in git's common directory and a re-read of the state after
  any wait or rename error; leftovers relocated inside `.eck/` first, the rename last; a linked
  `.c3` refused, links inside `.eck/` never followed; two collab resolvers (the default never
  moves); collab paths normalised first; the live-run guard fails closed, repeated under the
  lock, `begin_run` for a starting run; nested folders out of a patch. Known: its link checks use
  `symlink_metadata` + the reparse test, not `contain.rs` (not in its base) - a follow-up.
  Reviews: brief 40 (D4) launched, detach id `169783af` (`--rev 272920d --diff 84dbca4..272920d`,
  roster-w40: Grok, DeepSeek, Muse contributor - 2 of 3 seated). Then 41 (D2, roster-w41, `--rev
  0eea5f1 --diff 272920d..0eea5f1`) and 42 (S2, roster-w42, `--rev HEAD --diff 0eea5f1..ac26f5a`).
  Running: D1 (worker), CI for 0eea5f1 and ac26f5a.
- 2026-11-23 12:12: CI for ac26f5a (D4 + D2 + S2) GREEN (run 45159614961) - the Linux-only paths of
  D2 ran there for the first time.
- 2026-11-23 12:12, review of D4 `169783af`: 2 of 3 usable (DeepSeek skipped: the Alibaba plan's usage
  limit). n=29 Muse HOLD 3 major / 3 minor / 2 note, n=30 Grok HOLD 1 major / 2 minor; both rated
  yes. Findings F43-1..8, F44-1..3. The containment itself held; the majors are all in the `--diff`
  patch placement: F43-1 `create_dir_all` by path before the anchor is opened; F43-2 = F44-1 an
  abandoned empty claim blocks the patch name for ever; F43-3 a failed rename's cleanup can delete
  a sibling's finished patch -> sent to the running D1 worker as items 25-27 (it is in that code).
  For the last small wave (R7): F43-5 (the repair's discovery still reads by path), F43-6 (a
  failed removal stops at the first error), F43-7 + F44-2 (CHANGELOG: the exceptions and the
  changed behaviour for a link inside the repository on the way to the brief), F44-3 (a FIFO
  swapped in after the kind check blocks the read - open non-blocking / check the handle first),
  and S2's link checks moved onto `contain.rs`. wontfix: F43-4 (classification only), F43-8
  (intended).
  Review of D2 launched: detach id `f67bd985` (brief 41, Grok + GLM-5.3). Then brief 42 (S2).
- 2026-11-24 09:04: RESUMED after a stop of about 21 hours. On 2026-11-23 ~12:25 the account's WEEKLY usage
  limit was reached (this session's many opus workers used the rest of that week); the D1 worker
  was cut off mid-wave with its worktree saved; the limit reset at 09:00 on 2026-11-24. Budget is
  tight from here: fewer turns, no brief-writing workers, opus only where judgment is needed.
  State found: main at ac26f5a, CI green for it; CI run 45156540894 (0eea5f1) was CANCELLED by
  the 45-minute job timeout - its Windows job sat 50 minutes in the step `Test c3-cli` (the
  per-crate split shows it; c3-core and c3 had passed), no log again. Third intermittent Windows
  incident, all in the c3-cli integration tests; the next commit's run was green.
  The weekly `c3 maintain` pass (due today 15:19, the installed binary is still 0.2.4 ba43ff6)
  postponed to 2026-11-26 15:19 (`state.json.bak-2026-11-24`): it must run with the new binary.
- 2026-11-24 09:04, review of D2 `f67bd985` (done 2026-11-23): 2 of 2 usable, both rated yes. n=31 GLM-5.3
  ACCEPT 4 minor / 5 note (F45-1..9), n=32 Grok HOLD 4 major / 2 minor / 2 note (F46-1..8).
  Majors: F46-1 the deadline stop still misses a grandchild forked after its snapshot, a child
  with no recorded start time, a setsid daemon; F46-2 (= F45-1) the secret rule became too loose -
  any one-case word under 24 letters passes under EVERY keyword (`password = "hunterhunter..."`);
  F46-3 (= F45-8) `--close` deletes ignored files; F46-4 (= F45-3) `gh` may answer for another
  repository (GH_REPO, several remotes). All accepted -> wave R7a (brief
  `briefs/R7a-review-fixes-tooling.md`, 7 items; the secret rule: strict again under every
  keyword, a plain single word passes only under the bare keyword `auth`).
  Wave R7b (brief `briefs/R7b-review-fixes-handles-folder.md`): S2's folder operations onto
  `contain.rs`, F43-5, F43-6, F43-7 + F44-2, F44-3, F46-5.
  Launched from ac26f5a: R7a (worktree `agent-a64372534a0f70ef6`), R7b (worktree
  `agent-abb8d1b4e3cdf1d5c`). D1 worker RESUMED by message (items 25-27 repeated). Verification of
  S2 launched: detach id `349c6254` (brief 42, `--rev ac26f5a`, Grok + Muse contributor + Qwen).
  OpenAI week: 54 % used until Nov 29 - enough for the acceptance, no Sol runs.
- 2026-11-24 09:25, verification of S2 `349c6254`: 2 of 3 usable (Qwen failed: the Alibaba plan's monthly
  quota is exhausted until 2026-11-26), both rated yes. n=33 Muse HOLD 1 major / 2 minor / 1 note
  (F47-1..4), n=34 Grok HOLD 3 major / 2 minor (F48-1..5); F36-1, -4, -6, -8 confirmed fixed by
  both. Still open in their reading, all sent to the running R7b worker as items 7-11: F48-1 a
  LINKED `.c3` / `.eck` is still followed by consult / panel / work and by readers (only `ensure`
  refused it); F48-2 a loser of the move lock, or a lock taken over by age from a live owner, can
  re-create `.eck/`; F48-3 = F47-1 a lone legacy `anchor` in a half-done `.c3/` is read as C3's
  own (-> a layout marker written last); F48-4 = F47-3 spellings the file system maps to the
  folder (trailing dot / space, 8.3 names, an alias link); F47-4 the lock's takeover rule in the
  CHANGELOG. F48-5's first half (a directory link inside `.eck/` blocks the move) is by design.
- 2026-11-24 09:25, wave D1 reported (25 items, 24 done; item 8 - an end-to-end http run under `--rev` - not
  done: it needs a loopback TLS server in the test) and committed as 2ffa02a, pushed, CI run
  45355843912. Two hand-merged conflicts (`consult/args.rs`, CHANGELOG); fmt and workspace clippy
  clean on main; tests of the combination left to CI. The old gate refused once more: the review
  replies about the secret rule quote example secrets (AWS's documented example key, sample
  passwords) and saved patches hold C3's own redaction markers - all listed and allowed; the
  marker false positive was added to R7a's secret item. 15 findings set to `implemented`.
  Decided in D1, for TECH_DEBT: the guard's 0.8 s gap after a fork that re-stamps history (needs
  a heuristic; today's Codex forks by reference) and the smoke children's test variables
  (obsolete). Store: 208 findings - 161 implemented, 9 wontfix, 1 rejected, 37 proposed (all in
  R7a / R7b).
  Review of D1 launched (brief 50 written by the coordinator, roster-w50: Grok + Muse contributor
  + GLM-5.3, `--rev 2ffa02a --diff ac26f5a..2ffa02a`). Running: R7a, R7b.
- 2026-11-24 09:45: CI for 2ffa02a (D1) GREEN on all three jobs (run 45355843912).
- 2026-11-24 09:45, review of D1 `177fb994`: 3 of 3 usable, all rated yes. n=36 Grok HOLD 4 major / 2 minor
  (F51-1..6), n=37 Muse HOLD 2 major / 1 minor / 1 note (F52-1..4), n=38 GLM-5.3 ACCEPT 4 minor /
  3 note (F53-1..7). Decisions -> wave R8 (brief `briefs/R8-review-fixes-bridge.md`, worktree
  `agent-a137dbf7ed9e5adfe`, from 2ffa02a), two of them SIMPLIFICATIONS instead of one more patch:
  - agy (F51-1, F52-1, F53-2: the step classifier is wrong in new shapes every round): agy becomes
    worktree-isolated like kimi - the comparison of its own worktree is the evidence - and the
    classifier shrinks to the tool's name (reader allow-list). NOT exercised against the real agy
    CLI: its plan is out of quota until 2026-11-28 - watch the first live run.
  - the patch (F51-3, F52-2, F53-6): the name is its content (`<stem>.<sha8>.patch`), so the
    claim protocol goes; hard link or one atomic rename, bytes compared when the name exists;
    stale temp files of C3's pattern older than a day are removed.
  - a one-revision `--diff` carries tracked changes only by default, `--diff-untracked` adds the
    untracked files (F18-4 / F19-2 / F20-6 were still open for every reviewer); MCP gets the
    parameter.
  - the brief hash from the staged bytes (F51-2); an input that cannot be re-read is a warning
    (F51-4); an unreadable findings store at the start of a single run (F53-3); the death
    record's tie rule and panel members' streams (F51-6, F52-4, F10-7); the isolation paragraph
    is always printed (F53-1); names with format characters are counted (F53-5).
  wontfix: F52-3 (an outside collab root is the operator's anchor).
  This is the LAST cheap-model review round: after R7a, R7b and R8 land - the version, the
  acceptance by the standing partner. Running: R7a, R7b, R8.

## 2026-11-24 12:03 - waves R7a, R7b, R8 merged on main (the last review round's fixes)

- All three workers reported with fmt and clippy clean but with tests only partly run: the
  compiler-turn wrapper waited 35 minutes for 6 GB of free RAM that never came (4.6 GB free, nobody
  compiling), and after I lowered the bar (4 GB while no cargo/rustc runs) the harness stopped their
  background runs for low memory. I merged the three patches myself (R8 on 2ffa02a clean; R7a and
  R7b from ac26f5a by 3-way; conflicts: CHANGELOG three hunks, tests/branches.rs one - both sides
  kept) and ran everything in the FOREGROUND on the merged tree: clippy -D warnings clean;
  c3 lib 936 passed, c3-core 253; c3-cli branches 18, finish 27, ci_check 14, hook_scripts 5,
  init_doctor 22, compat_wave2b 21, snapshot_partb 7, panel_light_required 5, mcp_integration 3,
  claude_engine 19, kimi_engine 21 - all passed. The rest of c3-cli is left to CI.
- Coordinator's corrections on top of the waves:
  1. R8 item 5: an input that is GONE after the run (or was missing and appeared) is a CHANGED
     input (sha256_after "missing"); only an input that is there and cannot be read is a warning.
  2. R7b item 2: a link on the way to a repair candidate is not a failed repair - the SessionStart
     line is silent, c3 doctor prints "[i] <file>: not checked" (not_checked in --json) and stays
     clean. Otherwise a repository whose AGENTS.md is a link to CLAUDE.md got a line every session.
  3. F38-1: every CI test step has its own timeout-minutes (15/15/25), job limits 60/50 - a hung
     step now fails with its log. This is also the diagnosis tool for the Windows hang.
- Findings: 225 - 210 implemented, 14 wontfix, 1 rejected, 0 proposed. New wontfix: F38-3, F38-4,
  F38-5, F43-4 (notes in the store).
- Known limits left by the waves (for the acceptance brief): R7b item 9 - a READ through a collab
  resolver into a linked .c3/.eck is not refused, and the task directory is still created by
  create_dir_all after begin_run checked the root; a live but hung owner of the move lock blocks
  the move (commands go on in .eck and say so); R7a - a setsid daemon survives the deadline stop,
  on Windows an already reaped child is not swept when the process table cannot be read; R8 - agy's
  worktree isolation never ran against the real CLI; the Cf table is duplicated in reviewed.rs and
  ci_check.rs; --diff-untracked fails on Windows for a name with U+202E (Git for Windows).
- Next: commit, push, CI; version 0.2.5; TECH_DEBT; the acceptance.

## 2026-11-24 12:09 - e599f27 and adac2ed pushed; the acceptance runs

- e599f27 (waves R7a, R7b, R8 + three corrections) and adac2ed (version 0.2.5, CHANGELOG heading
  `## [0.2.5] - 2026-11-24`, brief 54) are on origin/main. CI: run 45380754438 for e599f27 was in
  progress (Linux default green), a second run started for adac2ed.
- The tech-debt file (.eck/TECH_DEBT.md, ignored) is updated: 11 items closed or decided, a section
  "State at the 0.2.5 release" with the open observations and what is left to build. The pinned
  plugin harness for the effort relaxation is DROPPED: its scripts directory is gone and parity
  with the retired PowerShell bridge is no release gate since the Phase 2 switch.
- **Parked: the standing partner's acceptance.** Detach id f761a18d
  (f761a18d-3e56-4fe6-aebf-2cd6fd3713a8), brief
  .collab/release-0.2.5/handoffs/54-claude-acceptance.md, new thread, purpose acceptance,
  `--rev HEAD` (adac2ed), run with the build of the merged tree
  (scratchpad/bin/c3-review4.exe). Come back:
  `c3 consult --task release-0.2.5 --status --id f761a18d`.
- After ACCEPT: commit the records, wait for green CI, annotated tag v0.2.5, push the tag; release
  build and install; `c3 doctor --fix` here (the folder move); .c3/config.json with
  finish_checks; the skills astra / workers to .c3/collab; the page.

## 2026-11-24 12:36 - CI green for e599f27 and adac2ed; the acceptance was stopped by the quota budget

- CI: runs 45380754438 (e599f27) and 45381568432 (adac2ed) are green on all three jobs.
- Acceptance run f761a18d (handoff 55) was stopped after 240 s at the bridge's quota budget: the
  ChatGPT 5-hour window reached 90 % (resets 14:09), week 68 % until Nov 29. 989 406 input tokens
  in 14 calls, the thread replays 907 KB per tool call. The partial
  (handoffs/55-codex-acceptance.partial.md) holds two agent messages; the second: "I found two
  places where the final round still relies on a check before a separate write. I'm tracing their
  effects on the move lock and on the patch bytes named in the ledger." Rated: partly.
- I traced both myself:
  1. **The move lock's takeover is compare-then-remove (real).** Two processes that judged the same
     stale record: the first removes it and creates its own lock between the second's compare and
     its removal, the second removes THAT lock - two movers. Fixed: the compare and the removal
     (takeover and the owner's release) happen under an OS lock on `<lock>.gate`
     (`takeover_gate`, state_dir.rs); the gate file is never removed. state_dir tests 26 passed.
  2. **The patch placement without hard links is judge-then-rename (a limit, said now).** A rename
     replaces: a FOREIGN file with other bytes appearing at the content-addressed name in the
     window is replaced. With hard links the name is judged again. Said in the CHANGELOG entry and
     at the code; not changed (the claim protocol was removed on purpose in R8).
- Wave D5 (worktree-agent-a888c6b15fe9a86ec, opus) runs: one Cf table, write_in temp sweep, the
  move lock's owner in the message, an untracked file git cannot hash, the telemetry family label.
- Plan: land D5 + the gate fix in one commit, CI; at 14:10 the acceptance again - a NEW thread, a
  short brief (pass 2) that names the partial and the changes since adac2ed. One full window must
  do: no Sol run, nothing else on the ChatGPT account until then.

## 2026-11-24 13:00 - d476520 pushed (the lock gate, wave D5, the fixture); D6 runs; the acceptance waits for 14:09

- d476520 on origin/main: the move lock's takeover gate, wave D5 (one Cf table, write_in temp
  sweep, the lock message names its owner, an untracked file git cannot read is left out, a dotted
  Claude id in telemetry), and the test fixture renamed. Verified in the foreground: clippy clean,
  c3 lib 940, c3-core 253, c3-cli init_doctor 22, ci_check 14, telemetry_parity 12,
  snapshot_partb 7, finish 27. CI run 45389095446 in progress.
- **The U+202E mystery was Windows Defender.** The fixture `<U+202E>txt.exe` of
  a_one_revision_patch_leaves_untracked_files_out_unless_asked has the name shape of a spoofed
  executable: Defender quarantined it in the middle of the test (Trojan:Win32/Artoelo.A, nine
  detections today, reported by the peer session dmytro-cd and by the operator with a screenshot).
  Two workers had read the failing `git diff` as a limit of Git for Windows. The fixture is
  `a<U+202E>b.txt` now; the operator was told to finish the dialog with Remove, not Allow.
- Wave D6 (worktree-agent-a0be8bab7b841e153, opus): one end-to-end test of the http engine under
  --rev, no product code - the last "to build" item besides the Windows CI hang.
- Tech-debt file: 7 open items left - four observations that need a live run (maintain pass
  2026-11-26, agy after 2026-11-28, kimi, the folder move on real projects), the Windows CI hang
  (needs an occurrence with its log), the http end-to-end test (D6), the operator's open question
  about a knowledge base for reviewers.
- Next: land D6, green CI, then at 14:10 the acceptance pass 2 with a self-contained brief 56.

## 2026-11-24 13:26 - 0f52246 pushed (wave D6); brief 56 written; the acceptance run 2 waits for 14:09

- 0f52246: the http engine under --rev proved end to end
  (wave6_live_diffs.rs::an_http_review_under_rev_sends_the_revision_not_the_live_tree). The worker
  was blocked first - base_url must be https - and I decided the exception: plain http only to a
  loopback host and only in test mode (parse_base_url, the telemetry intake's rule). Verified in
  the foreground: clippy clean, c3-core 254, wave6_live_diffs 6. The D6 worktree is removed (no
  force), its branch was already gone.
- CI: d476520's Windows job FAILED once - claude_engine.rs
  a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run: "claude --version did
  not finish within 15 s" (the fake launcher's start on the runner). Not the change; rerun started
  (run 45389095446). This is the first LOG of the Windows trouble - recorded in the tech-debt file
  with the likely common cause (a fake CLI's process start that is very slow on a loaded runner).
  Run 45393203031 (0f52246) in progress.
- Tech-debt file: 6 open items - four observations that need a live run, the Windows CI flake,
  the operator's question about a knowledge base for reviewers.
- Brief 56 (handoffs/56-claude-acceptance-2.md) replaces 54: self-contained, with "Since your
  first run", the budget advice (read narrowly, about sixty calls), the new decided items (the
  loopback exception, the patch placement limit, the Unix rename of an empty .c3).
- The ChatGPT 5-hour window stood at 96 % at 13:25 (somebody else on the account spends it too);
  it resets at 14:09. Launch the acceptance right after that:
  `c3 consult --task release-0.2.5 --purpose acceptance --mode new --brief
  .collab/release-0.2.5/handoffs/56-claude-acceptance-2.md --reply-name acceptance-2 --rev HEAD
  --detach` with the build of the head.

## 2026-11-24 13:33 - a live kimi observation with the 0.2.5 build: resume is broken (pre-existing); wave D7

- While the ChatGPT window is closed I ran the kimi observation of the tech-debt list, task
  smoke-kimi, build of 681af51 (scratchpad/bin/c3-review5.exe), kimi-code/kimi-for-coding:
  1. `--mode new --rev HEAD`: usable reply in 15.7 s (handoffs/03-kimi-live-1.md). The real CLI
     in its throwaway worktree wrote nothing there - the "housekeeping file fails the run" rule
     did not trip. That observation is closed.
  2. `--mode resume`: FAILED at once - kimi: "Session ... was created under a different
     directory." (handoffs/05-kimi-live-2.md). The worktree path is random per run, and kimi binds
     a session to its directory. v0.2.4 had the same random path (engines/kimi.rs:173 there), so a
     kimi thread could never be resumed - not a regression of 0.2.5, but a defect.
- Wave D7 (worktree-agent-ab347e154aaa57462, opus): a run that resumes a thread of a
  worktree-isolated engine (kimi, agy) gets its worktree at the parent run's path (C3's own shape
  only, the path must be free); the fake kimi CLI learns the real one's directory rule.
- CI commit 681af51 pushed: the Windows job excludes its build and scratch directories from the
  virus scanner (an experiment against the launcher-start flake).

## 2026-11-24 14:24 - 5031982 pushed (wave D7 + the rename retry); the acceptance run 2 is live; D7 seen working on the real kimi

- 5031982 on origin/main: wave D7 (a resumed thread of a worktree-isolated engine runs at its
  parent's worktree path), the patch rename retried on "access denied" (the lib test
  an_empty_leftover_claim_is_replaced_and_other_bytes_are_refused failed once with os error 5 on
  the rename over a file written a moment before - a scanner holding it), brief 56 updated.
  Verified in the foreground: clippy clean, c3 lib 946, kimi_engine 24.
- CI: green for d476520 (rerun), 55ea230, 681af51; 0f52246 and 5031982 running.
- **Parked: the acceptance run 2.** Detach id 11e86185 (11e86185-e664-4d87-8db8-9d9bea9eabd5),
  started 14:19:40, brief handoffs/56-claude-acceptance-2.md, new thread, `--rev HEAD` (5031982),
  bridge build c3 0.2.5 (5031982) = scratchpad/bin/c3-review6.exe. The 5-hour window was fresh.
  Come back: `c3 consult --task release-0.2.5 --status --id 11e86185`.
- **D7 on the real kimi CLI (task smoke-kimi, the same build):** handoff 06 a new thread, usable,
  worktree c3-kimi-wt-bf2a4f10...; handoff 07 `--mode resume`: usable, the SAME worktree path,
  the same session 2d183b84, and the model named what it had answered in the turn before. The kimi
  observation of the tech-debt list is closed. Open items there: 5.

## 2026-11-24 14:39 - the acceptance run 2: REJECT (6 blockers, 3 majors, 1 minor); four fix waves launched

- Run 11e86185, handoff 57 (handoffs/57-codex-acceptance-2.md), 660 s, usable, rated yes. Quota
  after it: 5-hour window 56 % until 19:10, WEEK 78 % until Nov 29 - one more pass fits under the
  reviewer's reserve (15 %), not two. The delta brief must be small and the fixes right.
- Verdict REJECT at 5031982. Findings and their routing:
  - F57-1 [blocker] a page label reused for another URL publishes the earlier page's private ids
    in telemetry -> A1.
  - F57-2 [blocker] an unreadable nested tool inventory counts as switch evidence -> A2.
  - F57-3 [blocker] a pending recovery records the target version verified after smoking a third
    installed version -> A2.
  - F57-4 [blocker] the lock's creation is outside the gate: a creator suspended after the
    exclusive create returns as a second owner -> A3.
  - F57-5 [blocker] 8 hex digits in the patch name collide, and the no-hard-link rename replaces
    -> A4 (full digest in the name; without hard links an exclusive per-run name).
  - F57-6 [blocker] a linked TASK directory below a real .c3/collab redirects the records -> A3
    (inside C3's folder: task and handoffs made through handles, a link refuses; a collab root
    outside the folder stays the operator's anchor; the swap-after-check limit is said).
  - F57-7 [major] the D7 resume claim does not bind checkout and cleanup to the claimed directory
    -> A4. F57-8 [minor] format characters in the isolation prompt's tracked names -> A4.
  - F57-9 [major] the test-mode plain-http exception accepts the NAME localhost -> A2 (loopback
    literals only).
  - F57-10 [major] the roster write can lose a hand edit, and F02-2 (the repair's rename, judged
    still open) -> A1: publish by an atomic exchange (ReplaceFileW / renameat2 EXCHANGE /
    RENAME_SWAP) and keep the displaced file when it is not the basis.
  - Her answer 2, unnumbered: a writer with the migration switch off creates .c3 beside a running
    move -> two folders -> A3.
- Closed in her reading: F02-1, F02-16, F02-21, F03-1, F11-3, F32-1, F13-1, F19-1; the gate closes
  the takeover she traced; D6's test inspects the real POST body.
- Workers (opus, isolated worktrees, briefs scratchpad/briefs/A-common.md + A1..A4, shared build
  directory C:/Users/Dmytro/C3/target, cargo in the foreground): A1
  worktree-agent-afe78aa41678e4cff, A2 ...a76de6a03b3444e02, A3 ...a25efa81418ed1b5f, A4
  ...a7176f103262d6171.
- The release pre-build (cargo build --release -p c3-cli) finished: the dependencies are warm.
- D7 seen working live on the real kimi (smoke-kimi handoffs 06, 07).
- Next: land A1-A4 (3-way, conflicts in CHANGELOG and consult/orchestrate/mod.rs expected), the
  whole foreground verification, CI, then ONE delta pass (a new thread, a short brief that lists
  each finding with its fix and closing test).

## 2026-11-24 16:17 - ea74a4d: the fixes for the acceptance review are on main, CI green; the delta pass

- Waves A1-A4 all reported; I merged the four patches (3-way; one CHANGELOG conflict, both sides
  kept) and committed them as ea74a4d. On top: the telemetry intake's test-mode rule accepts a
  loopback literal only (F57-9, the same rule as base_url), and the unit test
  telemetry::classes a_published_id_of_the_maintain_state_is_sent_as_itself got a page record
  with its URL (F57-1's new format; the worker had not run that module).
- **My mistake in the briefs:** I gave all four workers one shared build directory
  (CARGO_TARGET_DIR=C:/Users/Dmytro/C3/target). Cargo names the workspace crates' artifacts the
  same in every worktree, so a worker could link another worktree's build (A3 saw it: its new
  strings were in no binary; my own first lib run on main failed to compile against a foreign
  c3-core). Never share a target directory between worktrees of one workspace. All numbers below
  are from main after touching every .rs file (a forced rebuild).
- Verified in the foreground on ea74a4d: fmt clean; clippy -D warnings clean; c3 lib 961,
  c3-core 256; c3-cli finish 28, kimi_engine 25, panel_light_required 6, work 30, maintain 46,
  telemetry_parity 13, init_doctor 22, delegation_rule 7, wave6_live_diffs 6, claude_engine 19,
  compat_wave2b 21, branches 18.
- CI run 45418818364 (ea74a4d): GREEN on all three jobs, Linux included - the rustix exchange
  code compiled and its tests passed. (5031982's run had failed on Linux in the patch test: the
  "empty patch" defect A4 fixed - a temporary index copy with a new modification time.)
- Found on the way: a one-revision --diff patch could be EMPTY for a file edited to the same size
  in the same instant as the index (copy_index now keeps the index's modification time).
- Brief 58 (handoffs/58-claude-acceptance-delta.md): the delta for the standing partner - a table
  finding / fix / closing test / remaining limit, four questions, ACCEPT or REJECT.

## 2026-11-24 16:34 - the delta pass waits for the 19:10 window; live checks of the fixes on the real kimi

- The ChatGPT 5-hour window stood at 86 % at 16:18 (56 % after my run at 14:31), the week at 83 %:
  the peers' own acceptance runs (9eck-com-62: 14:13, 15:52; eckwms-movfast-09: 14:10, 15:58;
  9eck-com-1c: Sol panels in the morning). I asked all three to keep off the `openai` entries
  until 19:45; all three agreed. eckwms-movfast-09 still needs one Astra delta (about 3 points)
  after my pass and one full acceptance (4-5 points) if the week has room - I report the week's
  figure to it when my pass ends. No third pass for this release this week.
- be46b1d on origin/main: brief 58 and the records. CI for ea74a4d green (three jobs).
- More foreground runs on the head: c3-cli ci_check 14, hook_scripts 5, snapshot_partb 7,
  mcp_integration 3, compat_wave5 5, grok_engine 18, kill_record_durable 12, notspooled_parity 21.
- **Live with the head's build and the real kimi CLI (task smoke-kimi):** handoff 08 - a new
  thread with --rev HEAD --diff HEAD~2..HEAD, usable; the patch
  02-claude-live-kimi-0.2.5.f2468f37...b9107.patch has exactly that sha256 and the ledger names
  it (F57-5 seen). Handoff 09 - --mode resume, usable, the same worktree path
  c3-kimi-wt-2dc92cb9..., the same session (F57-7 seen). A resume with a directory placed at that
  path: refused before launch, the directory and its file untouched, no handoff, no worktree.
- Brief 58 carries these observations and the extra test numbers.
- Launch at 19:11: `scratchpad/bin/c3-review7.exe consult --task release-0.2.5 --purpose
  acceptance --mode new --brief .collab/release-0.2.5/handoffs/58-claude-acceptance-delta.md
  --reply-name acceptance-delta --rev HEAD --detach` (rebuild the binary if the head moved).

## 2026-11-24 16:44 - waiting for the 19:10 window: the move rehearsed on a copy of this repository's folder; the release build is warm

- 4b21f1e on origin/main (brief 58 with the live kimi observations).
- **The move rehearsed on real data (the reviewer's first-run checklist, point 2).** A scratch
  repository with a COPY of this repository's `.eck/` (23 files, 60 MB), the head's build,
  `c3 doctor` then `c3 doctor --fix`: "moved .eck/ to .c3/"; the old tool's anchor, update_seq
  and telemetry_queue.json went to .c3/legacy/; lastsnapshot/AnswerToSA.md became
  .c3/last-report.md; `.c3/` was added to .gitignore; the layout marker was written. Inventory
  by sha256 before and after: 19 files with the same name and hash, 4 moved as said, 1 new
  (layout), no content lost; `.eck` gone, one folder. The real move happens at the install.
- Release build of 4b21f1e: 8 min 28 s, target/release/c3.exe says "c3 0.2.5 (4b21f1e)". After
  ACCEPT only the tag commit's rebuild of c3-cli is left.
- Memory files updated: cargo in the foreground and never one build directory for several
  worktrees; the ChatGPT account is shared by all sessions (what a pass costs, agree first).

## 2026-11-24 17:03 - the operator: "GLM-5.3 -> GLM-5.4, it has just come out" - done by c3 maintain, live

- The operator's word (in Russian, 2026-11-24 about 16:55): GLM-5.3 is to be replaced by GLM-5.4,
  released today.
- Done with the tool built for it, the 0.2.5 release build (c3 0.2.5 (4b21f1e)):
  `c3 maintain --models --dry-run` - the public catalog (OpenRouter, 350 ids) names glm-5.4 as
  the candidate for ZAI-claude :: glm-5.3 ("would smoke and seat"); grok-4.20 refused by the
  catalog's date; nothing for glm-5.3-flash (only a "flashx-legal" announced); the ChatGPT entries
  only announced (gpt-6.2-astra-pro, gpt-6.2-sol-pro) - no smoke on that account.
  `c3 maintain --models` - "model ZAI-claude :: glm-5.3 -> glm-5.4, smoke passed (roster edited,
  backup taken)". Roster entry 3 is glm-5.4 now; entry 4 stays glm-5.3-flash. No displaced file
  (the roster's first write through the atomic exchange). Backups of the roster and the maintain
  state before it are in the scratchpad.
- **This is the first live model swap of c3 maintain** - the observation "never swapped a model in
  a live pass" of the tech-debt list is closed for the model step. The CLI-update step (codex
  0.169 -> 0.171 with the sub-agent smoke) stays for after the acceptance.
- Also: the global workers skill (~/.claude/skills/workers/SKILL.md) maps opus/sonnet to
  glm-5.4[1m] now (haiku stays glm-5.3-flash[1m]). Still naming 5.3 and left to their own
  mechanisms: AGENTS.md of each project (the 0.2.5 hook repair follows the roster once 0.2.5 is
  installed), the roster table of the xelth.com page (with the page update), README examples.
- In flight: the uniform task-directory rule (run_dirs: a linked task directory refuses under ANY
  collab root; only a collab root outside the repository is followed itself) - clippy clean,
  state_dir tests 30, c3 lib 961, kimi_engine 25; work and panel_light_required running.

## 2026-11-24 17:34 - the uniform task-directory rule, verified and committed (8087bad)

- The coordinator's own reading of F57-6 (not a reviewer's finding): wave A3 refused a linked task
  directory only inside C3's project folder; the default `.collab/` is tracked content and can
  arrive with a clone, so a link there was still followed. `state_dir::run_dirs` now has one rule:
  inside the repository every component from the repository root down to the collab root, the task
  directory and each sub-directory is opened without following it; a collab root OUTSIDE the
  repository is the operator's own anchor (followed, F52-3), what lies below it is not followed.
- Verified in the foreground on the working tree: fmt clean, clippy -D warnings clean, c3 lib 961,
  c3-core 256, c3-cli kimi_engine 25, work 30, panel_light_required 6, claude_engine 19,
  grok_engine 18, compat_wave2b 21, wave6_live_diffs 6.
- With it: README names GLM-5.4 for the claude worker engine; AGENTS.md of this repository was
  moved to GLM-5.4 by the SessionStart repair of the running build (it follows the roster).
- Next: push, CI, the dev build of the new head, brief 58 with the new commit named, the delta
  pass at 19:11 (the ChatGPT 5-hour window resets 19:10).

## 2026-11-24 18:15 - two cheap diff reviews of 8087bad before Astra's pass found a real hole; fixed

- While waiting for the ChatGPT window (19:10) the commit 8087bad was given to two reviewers in
  new threads (`--rev HEAD --diff 4b21f1e..HEAD`, brief 59): grok :: grok-4.7 (handoff 60, n=41)
  and ZAI-claude :: glm-5.4 (handoff 61, n=42 - the first review by the model seated today). Both
  HOLD, both with the same major, found independently.
- **F60-1 / F61-1 (major, real):** `c3 consult`, a panel and a detached run take the collab root
  from `resolve_collab_root`, which CANONICALISED it. A `.collab` (or `.c3/collab`) that is a link
  out of the repository arrived at `run_dirs` as its target, read as a directory outside the
  repository - the operator's own anchor - and was followed. The refusal of 8087bad held only for
  `c3 work` and for the unit test, which called `begin_run` with the unresolved path. Fix: the
  resolver keeps a path with a link BELOW the repository root as it is named
  (`state_dir::linked_below_root`); `repo_relative_normal` does the same for a spelling that is not
  a textual prefix of the root (F60-2: a link above the root, a verbatim path, an 8.3 name).
- F60-3: the CHANGELOG's tests clause corrected. F61-3: `c3 work` resolves `.`/`..` in its collab
  path by text before use (the check and the write name one directory). F61-2 rejected (the
  allocation loop skips an existing name, it never returns it). F61-4 decided: the accepted
  limitation F43-8 (every reparse point is a link), now also on the `.collab` chain; an ordinary
  process here reads the OneDrive root as a plain directory.
- Tests: the unit test asserts the resolver's result and the refusal for `.collab2`, a linked
  `.c3/collab3`, the repository reached through a link above its root, a verbatim path; new c3-cli
  test `a_linked_collab_root_refuses_the_run_and_writes_nothing_there` (`.collab` and `.c3/collab`
  as junctions; a consultation and `--detach`; exit 1, no kimi turn, nothing at the target).
- Verified in the foreground: fmt clean, clippy -D warnings clean, c3 lib 961, c3-core 256,
  c3-cli kimi_engine 26, work 30, panel_light_required 6.
- Findings store: 242 - 225 implemented, 15 decided (wontfix), 2 rejected, none proposed.
- CI for 8087bad (run 45438780916): see the next entry.

## 2026-11-24 18:37 - fe877b2 green in CI; brief 58 names the three code commits; the Windows flake has a second log

- fe877b2 pushed 18:16; CI run 45446676389 GREEN on all three jobs at the first attempt (18:36).
  Seen live with its build: a scratch repository whose `.collab` is a junction out of it -
  `c3 consult` refuses (".collab is a link ... nothing was started"), the target untouched; the
  8087bad build said "collab dir outside the repository" in a dry run of the same command.
- CI for 8087bad (run 45438780916): both Linux jobs green, Windows FAILED in
  `claude_engine.rs::a_decomposed_config_dir_inside_an_nfc_repository_refuses_the_endpoint_run`
  at its third consult ("`claude --version` did not finish within 15 s") - the same test and line
  as on d476520. The virus-scanner exclusion did not cure it. From the log: the binary started
  16:44:05, the test failed 17.4 s later, so its first two consults (each with a `--version`
  probe, which runs before the refusal they assert) took about 2 s and only the third probe hit
  the limit; the job runs one test thread and this is the first test of its binary. Cause not
  known. Rerun of the failed job started (attempt 2). Recorded in the tech-debt file with what to
  try; a hand-started workflow `.github/workflows/flake-probe.yml` loops the one test (and the
  whole binary, one thread) on a Windows runner to get a rate.
- Brief 58 updated: three code commits (ea74a4d, 8087bad, fe877b2), the F57-6 row with the
  resolver's rule, the evidence of both commits, the two CI results, the live observations, the
  outcome of the two diff reviews.
- Launch at 19:11 (after a dry-run look at the quota):
  `scratchpad/bin/c3-review9.exe consult --task release-0.2.5 --purpose acceptance --mode new
  --brief .collab/release-0.2.5/handoffs/58-claude-acceptance-delta.md --reply-name
  acceptance-delta --rev HEAD --detach` - never `--spend-reserve`.

## 2026-11-24 18:59 - the head is green; the flake measured: not reproducible in isolation

- CI for 431b610 (the head with brief 58): GREEN on all three jobs, first attempt. The rerun of
  8087bad's failed Windows job: green (attempt 2).
- `flake-probe` run 45450590467 (Windows runner): the one test alone 0 failed of 40, its whole
  binary with one test thread 0 of 8. In `ci` it failed 2 of 13 times since d476520, each time
  17-19 s into the binary (a green run: 4 s). So what precedes the binary in the job matters;
  the workflow got a third loop (the binaries before it in ci's order, then it, in one cargo
  call). Tech-debt entry updated.

## 2026-11-24 19:12 - Astra's delta acceptance pass launched

- Quota at 19:11 (dry run): the 5-hour window reset; week 83 % until Nov 29; preflight available.
- Launched 19:12:54 with the build of fe877b2 (`scratchpad/bin/c3-review9.exe`), head ea30cce
  (records and the measuring workflow only above fe877b2), `--purpose acceptance --mode new
  --brief 58 --reply-name acceptance-delta --rev HEAD --detach`; no `--spend-reserve`.
  Detach id 2ab4b336-fefe-4629-bec0-9682c952aef7; come back:
  `c3 consult --task release-0.2.5 --status --id 2ab4b336`.
- Running beside it: CI for ea30cce; `flake-probe` run 45454555632 (the sequence loop, 12 runs).
- After the reply: rate it (`c3 findings --task release-0.2.5 --rate <n>`), tell eckwms-movfast-09
  the week's figure. ACCEPT -> records commit, green CI, tag v0.2.5, release build, install,
  `c3 doctor --fix`, skills, the real `c3 maintain --run`, the xelth.com page. REJECT -> report;
  no third pass this week (the week resets Nov 29).

## 2026-11-24 19:27 - Astra's delta pass: REJECT (handoff 62, n=43) - five blockers, one minor

- Reply after 690 s; week 83 % -> 86 % (3 points). Rated useful: yes. eckwms-movfast-09 was told
  the figure; it started its own Astra delta at 19:33 (about 3 points more -> about 89 %).
- Closed in her reading: F57-1 (pages), F57-2 (unreadable inventories), F57-4, the switch-off
  writer, F57-5 (competing C3 placements), F57-8, F57-9. Not closed / new:
  - **F62-1** (blocker, supersedes F57-3): the recovery compares versions by numeric parts with
    the suffix dropped - an installed `1.0.80-rc.1` is the pending `1.0.80` for it. -> wave B1.
  - **F62-2** (blocker): `link/../.collab` - the link check collapses `..` by text, the resolver
    then canonicalises the original spelling (physical on Unix) -> an external anchor. The
    coordinator fixed this for `c3 work` only (F61-3). -> coordinator.
  - **F62-3** (blocker, supersedes F02-19): a catalog's cached ids have no collection-time public
    status (pages have since A1). -> wave B2.
  - **F62-4** (blocker): the displaced copy's name is chosen, then published by a replacing
    rename - two in one millisecond share a path; suffix 1000 is returned without a check.
    -> coordinator.
  - **F62-5** (blocker): a namespace nested inside `collaboration` loses the ancestry in the tool
    inventory - an offered tool counts as clean switch evidence. -> coordinator.
  - **F62-6** (minor): the full digest makes the patch's temp name longer than 255 bytes for a
    160-byte brief stem. -> coordinator.
  - Her answer 3: the external collab root is a reasonable operator choice; the hosted-tool list,
    delayed page publication, per-run patch copies and no distributed locking do not stop the
    tag. She names as blocking IF they are part of the promise: later path-based writes of a run
    (F57-6's limit), the rename fallback where no exchange exists (F57-10's limit); and F57-7's
    Unix window (git gets a pathname before the identity recheck). These need a decision: do
    them, or say in the CHANGELOG exactly what is promised.
- Quota: Astra's reserve is 15 % of the week; 14 % was left at 19:29. A third pass needs the
  operator's word (`--spend-reserve`) or waits for the reset on Nov 29.
- Waves started 19:34: B1 (F62-1) and B2 (F62-3), two opus-workers in isolated worktrees,
  type-check only (the disk has 16 GB free; the coordinator runs their tests on main).

## 2026-11-24 20:20-21:00 - round 3 of fixes: all six findings and the two conditional limits, merged on main (uncommitted, tests running)

- Coordinator, on main:
  - **F62-5**: `quota::walk_tool_list` keeps the `collaboration` ancestry at any depth (test
    `a_namespace_nested_in_the_multi_agent_namespace_keeps_its_ancestry`).
  - **F62-4**: the displaced copy's name is taken by a hard link (fails on an existing name, then
    `-2`, `-3` ...), the temp name removed after; no hard link -> a name with 32 random hex digits
    (`contain::place_displaced`; test `two_displaced_versions_never_share_a_name_and_nothing_is_replaced`).
  - **F62-6**: the patch stem is cut to 120 bytes at a character boundary (`PATCH_STEM_MAX`; test
    `a_long_brief_stem_is_bounded_so_every_patch_name_fits`).
  - **F62-2**: the resolver resolves `.`/`..` by text BEFORE its link check and canonical form
    (`resolve_collab_root_with`); unit test `l/../.collab4` through the resolver and straight to
    `begin_run`; `work.rs::a_collab_path_with_dot_dot_after_a_link_is_taken_by_text`.
  - **Her answer 3, the rename fallback**: decided - where no atomic exchange exists the write is
    REFUSED (file as it was, the command says why); `Publication::Rename` is gone; test
    `without_an_exchange_the_write_is_refused_and_the_file_is_as_it_was`. `c3 maintain` then seats
    no model on such a file system and the repair changes nothing there.
  - **Her answer 3, later path-based writes; F57-7's Unix window**: decided as limits, and the
    CHANGELOG now says what is promised (a link that exists at the run start; not containment
    against someone who has the operator's own rights during the run).
- Wave B1 (opus-worker, type-check only), **F62-1**: a version carries its exact release identity
  (only whitespace, a leading `v`, trailing punctuation dropped) and a separate semver ordering
  (a prerelease sorts before its final release); recovery, "did the update take", "is the previous
  back", "up to date" and the cool-down lookup compare the identity. Decisions taken by the
  worker, accepted: build metadata is part of the identity; a version token followed by other
  characters is unreadable; "did the update take" needs the exact pinned target. Three c3-cli
  tests, two unit tests.
- Wave B2 (opus-worker, type-check only), **F62-3**: a catalog record stores `url` and `public`
  like a page record; one rule (`web::sources_of`, `web::recorded`, `web::provenance`); a legacy
  custom catalog record publishes nothing until refreshed; the built-in catalog's legacy record
  is kept only under its exact canonical URL. One c3-cli test, two unit tests, fixtures updated.
- Both patches applied cleanly (`scratchpad/patches/b1.patch`, `b2.patch`). On the merged tree,
  foreground: fmt clean, clippy -D warnings clean, c3 lib 967, c3-core 257. c3-cli targets
  running.
- Disk: C: fell to 8 GB free (a peer asked for room); the incremental build cache was removed
  (15 GB free after). Astra's week: 88 % after the peer's delta (it wants one more, as C3 does).
- 21:00: c3-cli on the merged tree: `maintain` 49, `telemetry_parity` 14, `work` 31 - all passed
  (the queue waited 35 minutes for memory: another session compiled SurrealDB). No further local
  cargo runs for now: C: was at 8 GB free again and a peer asked for room - `target/debug` removed
  (13 GB free). The remaining c3-cli targets run in CI on three platforms.

## 2026-11-24 21:36 - round 3 in CI and at the cheap reviewers; the operator's word for Astra

- f78901b pushed 21:02; CI run 45476534138 GREEN on all three jobs, first attempt (the Unix
  assertions of this round ran there: the `..`-after-a-link tests, the exchange tests).
- Diff reviews of `ea30cce..f78901b` (brief 63): grok :: grok-4.7 (handoff 64, n=44) - ACCEPT,
  one note: `maintain/version.rs` `read_token` classed a dotted number with a component beyond
  u64 as "not version-shaped", so the scan went on to a later token although the rule says it
  stops. Fixed (`Token::Unclear` for two or more dotted components that do not all fit; unit
  assertions). Its reply's JSON did not parse (the bridge kept the text; no finding id).
  ZAI-claude :: glm-5.4 (handoff 65, n=45): still running at 21:36.
- The operator, 21:25 (in Russian): "давай мучай астру" - go on, use Astra: his word for the
  third acceptance pass, the reserve included. Also 21:29: "xelixir сейчас не собирается можешь
  им кеш удалить" - xelixir is not being built, its cache may go.
- Disk: C: fell to 2.7 GB free at 21:19 (a peer's alarm). Removed: xelixir's cargo output
  (`xelixir-wt-mobile/mobile/target` debug/release/aarch64 and six small crate targets, about
  1.6 GB; its apk, demo data and screenshots kept) and 7300 leftover C3 test directories in
  %TEMP% (about 1 GB). 19 GB free at 21:33. No local cargo runs until the peer says so; this
  commit's parser change is verified by CI only.
- Flake workflow, second run (45454555632): the sequence of binaries 0 failed of 12. In the
  loops 0 of 66 in total; in `ci` 2 of 17 Windows runs. Tech-debt entry updated.

## 2026-11-24 22:10 - the diff reviews' findings worked in; ready for Astra's third pass

- ZAI-claude :: glm-5.4 (handoff 65, n=45): ACCEPT with three minors and three notes, all true:
  - F65-1: eight places still described the rename fallback -> reworded (README, doc comments).
  - F65-2 (a regression of the F62-1 fix): after "did the update take" needed the exact target, a
    readable release the updater left instead was reported "did not take" and served unsmoked
    until the next pass. Wave B1's follow-up: it is smoked in the SAME pass, credited to itself
    (`handle_pending_smoke` with the pass's own smoke target), rolled back on a failed smoke.
  - F65-3: the commands' resolver had no test through `c3 consult` ->
    `work.rs::consult_takes_a_collab_path_with_dot_dot_after_a_link_by_text` (a dry run, Windows
    and Unix; it fails on both without the fix).
  - F65-4: five tests for the identity comparisons outside the recovery (the fake updater can
    leave another version than asked). F65-5: a doc comment moved back. F65-6: one built-in record
    without status goes through the binary again.
- Verified in the foreground on the merged tree (a cold build, no incremental cache): fmt clean,
  clippy -D warnings clean, c3 lib 967, c3-core 257, c3-cli maintain 54, telemetry_parity 14,
  work 32. CI for 2d99f72: see the brief.
- The operator, 22:05: a roadmap wish - field reports from every session that used C3, through
  telemetry, at a small token cost. Written to `.c3/ROADMAP.md` ("Next") with his words. The
  peer's report it refers to is `.c3/RATIONALIZATION.md` (moved from a `.eck/` the peer created).
- ChatGPT week: 90 % (a peer's two passes; its last one ended 20:24 with ACCEPT), the 5-hour
  window 44 % until 00:16. An acceptance run is what the reserve is kept for (the peer's run at
  86 % was not held).
