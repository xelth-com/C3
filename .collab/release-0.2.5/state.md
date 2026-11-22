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
- 2026-11-22 14:00: the weekly `c3 maintain` pass (due 15:19 today, with codex 0.169 -> 0.171 waiting)
  postponed to 2026-11-24 15:19 (`~/.codex/c3/maintain/state.json` next_due; backup
  `state.json.bak-2026-11-22`) so the Codex update is smoked by A2's sub-agent check, not before it.
  Watch that pass with the new binary - it closes the "first real runs" entry.
