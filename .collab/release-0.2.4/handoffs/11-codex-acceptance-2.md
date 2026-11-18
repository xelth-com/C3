# Handoff 11 - Codex: acceptance-2

Date: 2026-11-16 02:28 local. Author: Codex (model gpt-6-astra, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 17.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 4bc54f8a-f363-44e7-bc6e-433eaf3ed56b.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c features.multi_agent=false -c agents.enabled=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-8023e3b884ee4527b38fd193b0ba3504.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread). Result thread: `01a1e1ec-c2d2-7de3-9bda-760504cedd03` (source: events).
Brief: `.collab/release-0.2.4/handoffs/10-claude-acceptance-2.md` (sha256 a11089db3d94). Reviewed: 08121e5, base 08121e5773c95e97b4123d0e839b295f2c237fca, tree sha256 4be3336d21a5, 0 changed files.
Bridge outcome: usable reply. Wall time: 783.3 s. Tokens: in 2181341 (cached 2051840), out 12707, reasoning 3249.
Quota (rollout): 5h 25% until 07:16; wk 48% until Nov 22; this run 2181341 input tokens (2051840 cached) in 36 calls, max 130961 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 50 % of the 5-hour window.
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Recovery still discards unresolved update records, and Kimi reply handling, secondary brief staging, and the sub-agent guard retain correctness gaps. Findings: 1 blocker, 3 major, 0 minor, 0 note (F11-1..F11-4, tracked in `findings.json`). Structured reply: `handoffs/11-codex-acceptance-2.reply.json`.
Raw event stream: `handoffs/11-codex-acceptance-2.events.jsonl`.
Verbatim reply follows.

---

**Q0.** Kimi creates a fresh detached HEAD worktree for each turn. Normal returns, launch failures and timeouts invoke cleanup through `Drop`; cleanup errors are ignored, and abrupt termination remains unproven (`crates/c3/src/engines/kimi.rs:200`). Events remain outside the disposable checkout. Killed turns preserve salvage with candidate conversation trust; successful parsing promotes the session to verified.

Tool-result messages and narration attached to tool calls are excluded, but separate assistant text preceding a tool call survives. Consequently, a pre-read verdict can become a successful reply without any post-read answer: finding #2.

R7a fixes the ordinary primary turn, demonstrated by its new test. It does not fix read-insist: secondary engines pass `brief: None` while creating another HEAD worktree. An uncommitted brief is therefore unavailable precisely when a zero-tool primary must finally read it: finding #3.

The unknown-engine roster concern is refuted: entries retain their original positions and become inactive; active selection filters them (`crates/c3-core/src/roster.rs:1005`). Budget freshness now prevents old observations from boosting weights, while an old high reading may deliberately hold an unexpired window. F06-4 remains partly open because an older rollout can still mask newer health data.

**Q1. HOLD at verified HEAD `08121e5773c95e97b4123d0e839b295f2c237fca`.** Finding #1 is a release blocker; findings #2–#4 also need correction before acceptance. The supplied 1,217 passing tests and successful CI are useful evidence, but do not exercise these combinations. I made no edits, delegated nothing and did not rerun tests.

**Q2.**

- **R1: not closed.** The initial update now retains its journal, but recovery returns `resolved = true` after failed or unavailable rollback, causing the next pass to clear it (`crates/c3/src/maintain/mod.rs:1087`, `:1092`). The new tests stop before that recovery pass.
- **R2: closed by inspection and targeted test.** Journal write errors propagate and prevent installation (`maintain/mod.rs:141`, `:889`).
- **R3: closed for the reported race.** The pre-smoke snapshot supplies both protection checks and the swap basis; changed contents abort the swap (`maintain/mod.rs:1183`; `maintain/models.rs:253`). The regression exercises that mismatch.
- **R4: closed.** Reserve precedes on-request and weight gates; the combined required-entry test checks refusal and override (`providers.rs:847`).
- **R5: closed.** Each quota window retains its observation time; the merged-old-weekly test checks that it cannot boost (`budget/mod.rs:489`).
- **R6: closed for the reported cross-observation merge.** Future windows are removed before selection; the regression preserves the genuine 97% reading (`crates/c3-core/src/quota.rs:1208`, `:2805`).

I do not require changing the documented always-zero maintenance exit policy. It requires truthful reports and durable unresolved state; finding #1 violates the latter.

**Q3.** The recovered partial review contains no additional concrete conclusion beyond R1–R7. This pass adds the historical-spawn false stop in finding #4. Grok’s inspected paths now reject missing post-tool answers, distinguish launch failure, replace dead-attempt salvage and clear abandoned fresh threads (`engines/grok.rs:711`; `consult/orchestrate/secondary.rs:1463`, `:1524`). The remaining budget-source precedence/account-scoping concern is recorded under F06-4, without duplicating it.

Reading was targeted. I did not read the complete release diff, all historical handoffs/findings, the full CHANGELOG, installer implementations or CI logs. Live updates, rollback, Kimi installation, roster swaps and real-account holds remain unproven.

**Q4.** Before trusting zero, observe an engine-specific `old -> actual-version ok` report, independently matching `--version`, and a passing explicit smoke. Failed recovery must retain its pending record across repeated passes. For a real hold, observe reviewer identity, used percentage, elapsed percentage, reserve and observation age; explicit selection must say nothing started, and automatic routing must select another eligible reviewer. Detailed checks follow in `first_run_checklist`.

## Requested checks

- **RC1** — Cwd `C:\Users\Dmytro\C3`; workspace-write; 15 minutes: extend the failed-rollback fixture through a forced recovery pass with rollback still failing, then run that regression. Finding #1 is settled by whether `pending.json` retains the engine.
- **RC2** — Same cwd; workspace-write; 15 minutes: feed Kimi a valid structured assistant verdict, a tool call/result, then a valid resume hint and exit zero, with no later answer. Finding #2 requires rejection and empty final-answer salvage.
- **RC3** — Same cwd; workspace-write; 15 minutes: combine the uncommitted-brief fixture with zero-tool primary and read-insist; make the fake read the original relative brief during turn two. Finding #3 requires matching brief contents in both worktrees.
- **RC4** — Same cwd; workspace-write; 15 minutes: resume a fixture containing an old spawn call, emit no new spawn, and run beyond one guard poll; then append a new spawn. Finding #4 requires only the new spawn to stop the run.

---

### Findings

- **F11-1** [blocker] `crates/c3/src/maintain/mod.rs:1087`, `crates/c3/src/maintain/mod.rs:1092`, `crates/c3/src/maintain/mod.rs:722`, `crates/c3-cli/tests/maintain.rs:550` - R1 remains incomplete: pending-update recovery reports itself resolved after a failed rollback or when no rollback path exists, so the caller deletes the recovery record while the installed engine still fails its smoke. Trigger: A retained pending update reaches a subsequent maintenance pass; the installed version differs from the previous version, its smoke fails, and rollback fails or is unavailable. Evidence: read-code: The failed-smoke branch distinguishes rollback success in its message but unconditionally returns true at line 1087; the no-rollback branch also returns true.; read-code: The caller invokes clear_pending whenever handle_pending returns resolved=true.; read-code: The new failed-rollback test checks retention immediately after the initial update, without running recovery. Verify: Run RC1: retain a failed update, force a second pass whose smoke and rollback fail, and assert the engine remains in pending.json. Remedy: Return resolved=false on unsuccessful recovery, including absent rollback support; clear only after a passing smoke or verified restoration. Add repeated-recovery regression coverage.
- **F11-2** [major] `crates/c3/src/engines/kimi.rs:580`, `crates/c3/src/engines/kimi.rs:776`, `crates/c3/src/engines/kimi.rs:805`, `crates/c3/src/engines/kimi.rs:456` - Kimi can publish a pre-tool placeholder or verdict as a usable, verified reply when the turn performs tools but never produces a post-tool answer; its salvage also retains that pre-tool text. Trigger: A Kimi stream emits nonempty assistant content without tool_calls, subsequently emits a tool call/result, then ends with a valid session.resume_hint and exit code zero without further assistant content. Evidence: read-code: Assistant content is appended to one text buffer; encountering tool calls increments the count without clearing earlier text.; read-code: Nonempty accumulated text becomes usable; the primary review guard only checks that the total tool count is nonzero.; read-code: A usable turn becomes Completed with Verified conversation trust, while salvage concatenates all assistant content lacking tool_calls. Verify: Run RC2 with a schema-valid pre-tool verdict and no post-tool answer; inspect the outcome, reply and conversation trust. Remedy: Track assistant answer text after the last actual tool call separately; reject an empty answer tail and use that tail for salvage, as the Grok adapter already does.
- **F11-3** [major] `crates/c3/src/consult/orchestrate/secondary.rs:281`, `crates/c3/src/consult/orchestrate/secondary.rs:1127`, `crates/c3/src/engines/kimi.rs:174`, `crates/c3-cli/tests/kimi_engine.rs:335` - R7a only stages the operator brief for the primary Kimi turn. Read-insist and other secondary turns create fresh HEAD worktrees without staging it, leaving an uncommitted brief absent or a modified committed brief at its old contents. Trigger: An acceptance consultation supplies an uncommitted or locally modified relative brief, and the primary makes zero tool calls, causing a read-insist turn that must read that brief. Evidence: read-code: Both secondary Kimi constructors explicitly set brief=None, assuming the previous turn already read it.; read-code: Every run creates a new worktree of HEAD; operator brief staging only occurs when brief is Some.; read-code: The new brief test asserts one framing turn; the separate read-insist test does not supply an uncommitted operator brief. Verify: Run RC3 and inspect the actual contents read from the original repo-relative brief path during the secondary turn. Remedy: Stage the same operator brief into every Kimi worktree that can reference it, including read-insist, continuation and repair; propagate staging failures before launching the reviewer.
- **F11-4** [major] `crates/c3-core/src/quota.rs:688`, `crates/c3-core/src/quota.rs:737`, `crates/c3/src/engines/subprocess.rs:772` - The new sub-agent guard can kill a compliant resumed turn because it treats a historical spawn_agent call anywhere in the thread's rollout as a spawn in the current run. Trigger: Resume a thread that previously spawned an agent, for example during an explicitly opted-in run, with sub-agents disabled now; the resumed process remains running until a guard poll but makes no new spawn call. Evidence: read-code: rollout_has_spawn_agent reads the rollout with no timestamp or offset boundary. subagent_in_run does not pass its since argument to that scan; since is used only for child-rollout discovery.; read-code: The watch passes the current turn start, but any returned historical SpawnCall triggers process-tree termination and TurnStop::Subagent.; read-code: The inspected regression checks a spawn call without testing a call preceding the current turn. Verify: Run RC4 and confirm that a historical call alone does not produce an operator-class sub-agent stop. Remedy: Scope parent-rollout detection to the current turn using a launch-time byte offset or validated event timestamps/turn identity; retain detection of newly created child rollouts.

### Prior findings

- F02-1 - fixed - zai_get uses redirects(0) and only consumes responses below 300 at budget/mod.rs:687-698; verified by source inspection, without a live redirect test.
- F02-4 - fixed - The candidate is checked through seat_saturated before its smoke at maintain/mod.rs:1208; smoke.rs:148 delegates to the governing-window headroom calculation. Swap testing remains specific to the candidate.
- F03-9 - fixed - secondary.rs:1463 handles LaunchFailed separately; dead attempts replace raw_text even with empty salvage at :1524, and fresh abandoned threads are cleared. No launch-failure fixture was executed.
- F03-12 - fixed - Smoke scratch repositories use OS temp at maintain/mod.rs:1113; env.rs:253,258 restricts cross-host redirects to the exact code.kimi.ai -> cdn.kimi.ai pair over HTTPS.
- F04-7 - fixed - budget/mod.rs:690 disables redirects and :697 excludes 3xx response bodies from parsing/caching.
- F04-8 - fixed - maintain/mod.rs:1527-1532 supplies the executable directly, and :1572 calls CreateProcessW with file handles for logging rather than cmd.exe redirection.
- F06-1 - fixed - providers.rs:847-876 applies reserve and on-request gates before panel weight gates; stand_in_light_members at :971 only promotes rows whose skip_kind is light. R4 additionally fixes the combined reserve/on-request ordering.
- F06-2 - fixed - The reserve check at consult/orchestrate/mod.rs:2086 is outside the skip-preflight credential branch and applies to a selected protected roster entry.
- F06-3 - fixed - consult/orchestrate/mod.rs:1548-1588 supplies the previous eligible reviewer position to routing for non-standing resume/fork, and switches to new mode if another reviewer must be drawn.
- F06-4 - still-open - Age-based boosting, expired-window fallback and future-candidate selection were corrected. However, budget/mod.rs:391 and :404 still return rollout windows before comparing newer health observations at :410, so the supplied old-low-rollout/new-high-health scenario remains. quota.rs:1308-1318 also filters provider rather than account identity.
- F07-1 - fixed - Under the clarified invariant, old high usage may hold an unexpired window. Cache fetch timestamps are preserved at budget/mod.rs:526,656,769; per-window freshness caps old weights at 1, expired windows are dropped, and hold messages disclose age. Account/source precedence remains tracked separately under F06-4.
- F07-2 - fixed - The common reserve check at consult/orchestrate/mod.rs:2086-2105 covers explicit selection and thread-pinned selection independently of skip-preflight.
- F07-3 - fixed - Walk and panel holds use !ctx.no_network, matching panel multipliers; real single-run refusal and router explain use network=true. Source inspection confirms the original network-mode mismatch is removed; live cache-refresh behavior was not exercised.

## Verdict: HOLD

Recovery still discards unresolved update records, and Kimi reply handling, secondary brief staging, and the sub-agent guard retain correctness gaps.

### Blockers

- **F11-1** `crates/c3/src/maintain/mod.rs:1087`, `crates/c3/src/maintain/mod.rs:1092`, `crates/c3/src/maintain/mod.rs:722`, `crates/c3-cli/tests/maintain.rs:550` - R1 remains incomplete: pending-update recovery reports itself resolved after a failed rollback or when no rollback path exists, so the caller deletes the recovery record while the installed engine still fails its smoke. Verify: Run RC1: retain a failed update, force a second pass whose smoke and rollback fail, and assert the engine remains in pending.json. Remedy: Return resolved=false on unsuccessful recovery, including absent rollback support; clear only after a passing smoke or verified restoration. Add repeated-recovery regression coverage.

### Unproven scenarios

- The supplied test counts and CI run 43770037495 were not independently rerun or inspected; test bodies were reviewed selectively.
- No real CLI installation, rollback, Kimi installer execution, roster swap or ChatGPT-account reserve hold was performed.
- Kimi worktree cleanup after abrupt coordinator termination, locked files or failed git removal remains unverified; cleanup errors are ignored.
- The new sub-agent switches were not independently validated against a live ChatGPT-provider tool surface, and the guard's real process-tree cleanup was not exercised.
- Cross-account quota isolation and selection between conflicting rollout and health observations remain unresolved under F06-4.
- Reading excluded the complete release diff, full historical handoffs/findings, full CHANGELOG, installer implementations and CI logs; this is a targeted acceptance review.

### First-run checklist (observable)

- [ ] Confirm c3 --version identifies the reviewed build. For an actual update, require the engine-specific 'old -> actual-version ok' report and an independent engine --version matching that actual version; a generic exit 0 or 'up to date' is insufficient evidence of an update.
- [ ] Run an explicit smoke for the updated engine and observe its passing result. In a retained or instrumented smoke trace, both consultation turns must succeed and the second reply must contain the exact token read from token.txt.
- [ ] Inspect pending.json: the engine record must disappear only after successful verification or verified restoration. After a failed recovery/rollback it must remain across another forced pass; 'recovered: rollback failed' must never count as success.
- [ ] For a model swap, require the report naming provider, old model and successor, the daily backup, and a roster diff changing only the intended model. A concurrent roster edit must produce an aborted-swap report.
- [ ] For the first real hold, require output naming the reviewer, governing long window, used percentage, elapsed percentage, reserve and age when old; compare these with the current account's quota/reset observation.
- [ ] On a held non-standing explicit selection, require a nonzero refusal saying nothing was started, including with --skip-preflight and a pinned --thread. A required held panel member must refuse; automatic routing must visibly skip it and name an eligible replacement.
- [ ] For Kimi, confirm the operator brief is readable with matching contents on every attempt, the accepted answer follows the final tool call, and the temporary path is absent from git worktree list after completion.
- [ ] For a resumed Codex run, confirm historical agent activity alone causes no stop. A new forbidden spawn must produce an operator-class failure, preserved partial output and confirmed process termination.
