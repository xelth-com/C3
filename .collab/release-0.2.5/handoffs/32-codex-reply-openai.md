# Handoff 32 - Codex: reply-openai

Date: 2026-11-23 06:33 local. Author: Codex (model gpt-6.2-sol, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6.2-sol (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 82b5a8bf-302e-4c24-9061-ca91517eb739.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6.2-sol -c features.multi_agent=false -c agents.enabled=false -c features.plugins=false -c skills.include_instructions=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-8ce70e5ac7534253905765b125aa0d56.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-7cbdb74b75634170b77a58180eec4b80` of revision fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f (--rev fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f; removed after the run).
Parent thread: (none - new thread). Result thread: `01a206d7-6987-7401-8f9e-9122d3c40f39` (source: events).
Brief: `.collab/release-0.2.5/handoffs/21-claude-verify-staging-smoke-doctor.md` (sha256 117034b5c3ae). Reviewed: fdf193a, base fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f, tree sha256 77688b041006, 0 changed files. Live HEAD at launch: fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f.
Bridge outcome: usable reply. Wall time: 901.9 s. Tokens: in 2736969 (cached 2584192), out 16745, reasoning 3885.
Quota (rollout): 5h 25% until 10:14; wk 38% until Nov 29; this run 2736969 input tokens (2584192 cached) in 28 calls, max 149631 per call of 475000; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 90 % of the 5-hour window.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Staging can still write outside the worktree, and the Codex smoke can pass without evidence tied to both turns. Findings: 1 blocker, 3 major, 0 minor, 1 note (F32-1..F32-5, tracked in `findings.json`). Structured reply: `handoffs/32-codex-reply-openai.reply.json`.
Raw event stream: `handoffs/32-codex-reply-openai.events.jsonl`.
Verbatim reply follows.

---

## Q1. Prior fixes

I reviewed `4f43074..99e2da7` against checkout `fdf193a2`. Paths below are relative to `crates/c3/src/` unless shown otherwise.

- **F10-1, F10-2, F11-1 partly.** Unknown tools fail closed. Nested updates can still lose write evidence, missing or changed step indexes split reads, and unattributed writes can leave replies usable. `engines/agy.rs:425,506`, `consult/orchestrate/tree.rs:433`.
- **F10-3, F11-4 partly.** Typed staging refusal and readable `..` paths work. An already-deleted brief fails `is_file()` before the HEAD fallback. `consult/orchestrate/mod.rs:2403,3985`, `engines/worktree.rs:725`.
- **F10-4, F11-6 fixed.** Frozen records drive blocker judgment and current-store updates use a write lock. `panel/run.rs:973`, `consult/orchestrate/mod.rs:5798,6259`.
- **F10-5, F11-7 fixed** for the MCP-name, opted-in-seat and blank-thread cases. **F11-8 partly.** Rollout identity and integrity are checked. Positive evidence still belongs to the thread as a whole. `maintain/smoke.rs:706,728,747`, `crates/c3-core/src/quota.rs:917,981`.
- **F10-6 left open by decision.** The handoff reports a check on Codex 0.169.0. Older CLIs remain unverified.
- **F10-7, F11-11 partly.** Panel handoff fallback and single-run persistence exist. An old pending path still outranks the recorded continuation. `consult/detach.rs:1247,1324,1504`, `consult/orchestrate/mod.rs:4250`.
- **F11-2 fixed** for detected live agy input drift. **F11-3 fixed** for the static committed-link case. **F11-5 fixed** for ordinary live-only names and newline/backtick cases. `consult/orchestrate/tree.rs:255,343`, `consult/reviewed.rs:433`, `engines/worktree.rs:1012,1026`.
- **F11-9 partly.** Post-exit inspection catches a logged spawn after it happened. A timestampless old child still uses mtime. `engines/subprocess.rs:870`, `crates/c3-core/src/quota.rs:631`.
- **F11-10 fixed** for the three reported prompts. A later commit covers denial retry. **F11-12 partly.** Markdown partials count. A deleted sidecar still counts historically. `consult/orchestrate/secondary.rs:33,45,82`, `consult/orchestrate/ledger.rs:376`, `providers.rs:2714`.
- **F12-1 partly.** Windows access denial is preserved as unknown. Linux collapses `/proc` read errors into absence. **F12-2 fixed** for root resolution. **F12-3 and F12-4 partly.** Wording improved. Reparse refusal and the concurrent-writer gap remain. `setup/branches.rs:955`, `liveness/proc.rs:1379,1546`, `providers.rs:2489`, `setup/repo_models.rs:400`, `README.md:685`.
- **F13-1, F13-8 fixed** for linked roots and the ownerless age rule. **F13-2 partly.** A fake `.git` containing only `HEAD` still qualifies. **F13-3, F13-4, F13-7 fixed.** `setup/branches.rs:151,621,993,1376`, `setup/repo_models.rs:549`, `update/mod.rs:710`.
- **F13-5 partly.** A child can leave the deadline registry before exit. **F13-6 partly.** Known older pairs are refused. An undated successor can still win. **F13-9 still open** by decision. `bounded.rs:129`, patch lines 6098–6106, `maintain/models.rs:429`, `README.md:685`.

## Q2–Q6

**Q2.** Normal directories and a linked temp ancestor can work. Windows reparse entries are refused conservatively. Path substitution can still redirect staging or race the removal sweep.

**Q3.** Stage and scratch cleanup ignore deletion errors. Failed owner recording can leave a registered worktree for manual cleanup. Ordinary maintenance returns release the pass lock.

**Q4.** A legitimate agy reader can fail on a filename containing `&&`. Frozen prior text also exceeded Windows command-line limits before `b9f665a` moved specs to files.

**Q5.** Every automatic consumer holds explicit `Unproven`. Manual smoke prints it and exits zero. The changelog overstates staging isolation, per-turn smoke proof and reader classification.

**Q6.** The cited tests miss link substitution, actual Linux permission denial, reserve re-seating after staging failure and a shared rollout with evidence for only one turn. I inspected targeted hunks and call sites. Unrelated fixes and remaining test hunks were outside this pass.

## Requested checks

- **RC1** Run a fault-injected staging test from the repository root on Windows and Unix. Workspace-write, 10 minutes. Simulate parent identity changing between validation and creation. Assert no out-of-root write or leftover. Finding #1.
- **RC2** Extend the shared-thread smoke fixture from the repository root. Workspace-write, 5 minutes. Give both turns terminal records and only the first switch evidence. Expect `Unproven`. Finding #2.
- **RC3** Run a fake-engine panel from the repository root. Workspace-write, 10 minutes. Inject a staging refusal after an optional seat reserves. Observe a reserve launch and zero launches of the refused reviewer. F11-4.

---

### Findings

- **F32-1** [blocker] `crates/c3/src/engines/worktree.rs:248`, `crates/c3/src/engines/worktree.rs:259`, `crates/c3/src/engines/worktree.rs:269`, `crates/c3/src/engines/worktree.rs:275`, `crates/c3/src/engines/worktree.rs:359`, `CHANGELOG.md:500` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there. Trigger: A mutable worktree directory changes identity between the parent checks and directory creation, temporary-file creation or publication. Evidence: read-code: The code checks the parent, opens a random temporary pathname with create_new, writes and syncs it, then checks again. Both the final rename and the cleanup resolve pathnames again.; inferred: Exclusive creation protects the leaf. The parent lookup can still reach another directory, and rejection after writing cannot guarantee cleanup if the pathname has changed again. Verify: Use an injected directory-identity change between the validation and create calls and assert that staging refuses before any write outside the opened worktree. Remedy: Bind directory traversal, file creation, cleanup and publication to verified directory handles and refuse if a platform cannot safely keep those operations within the worktree.
- **F32-2** [major] `crates/c3/src/maintain/smoke.rs:718`, `crates/c3/src/maintain/smoke.rs:747`, `crates/c3/src/maintain/smoke.rs:753`, `crates/c3-core/src/quota.rs:879`, `crates/c3-core/src/quota.rs:987`, `CHANGELOG.md:549` - A successful Codex smoke can count two turns as proven using switch evidence from only one turn of a shared rollout. Trigger: Two consult rows share a thread whose rollout has two terminal records and recognized switch evidence only for the first turn. Evidence: read-code: The check groups consults by thread, compares completion counts and tests a single rollout-wide evidence flag. The parser retains no association between positive evidence and a turn.; read-code: The shared-thread test rejects too few terminal records. It does not test full terminal counts with incomplete switch evidence. Verify: Add a shared-rollout fixture containing two completed turn segments with recognized evidence in only the first and assert that the smoke returns Unproven. Remedy: Bind rollout evidence to each smoke turn using an identifiable turn boundary and require recognized evidence for each turn before returning Pass. Supersedes: F11-8.
- **F32-3** [major] `crates/c3/src/engines/tree_check.rs:229`, `crates/c3/src/engines/tree_check.rs:262`, `crates/c3/src/consult/orchestrate/tree.rs:404`, `CHANGELOG.md:517` - A known read-only agy tool can discard a complete review because an ordinary filename is classified as a command. Trigger: An allow-listed read_file step reads a file named `notes&&draft.md` using a `path` parameter. Evidence: read-code: The recursive classifier scans strings in path fields and treats any && occurrence as a command. The engine fails a flagged step before considering whether the tree is clean. Verify: Pass a read_file event for `notes&&draft.md` through the parser and tree check with a clean snapshot and assert that the reply remains usable. Remedy: Interpret known reader parameters according to their schemas. Treat declared filename arguments as data and keep conservative handling for unknown tools or ambiguous fields.
- **F32-4** [major] `crates/c3/src/setup/branches.rs:238`, `crates/c3/src/bounded.rs:45`, `crates/c3/src/bounded.rs:129` - The new branch deadline registry can lose a still-running direct Git child, leaving it alive after the hook returns. Trigger: A Git wrapper closes stdout and stderr and continues running until after the deadline. Evidence: read-code: The patch removes the child from the registry as soon as both pipes reach EOF, then waits without the registry. The deadline can only stop children still in the registry.; read-code: The later shared runner retains the same unregister-before-wait sequence. Verify: Run the bounded runner against a fixture process that closes both pipes and waits, then assert that its PID is gone when the deadline returns. Remedy: Keep the direct child under deadline ownership until exit is confirmed, including when both pipes close first. Supersedes: F13-5.
- **F32-5** [note] `crates/c3/src/panel/member.rs:90`, `crates/c3/src/panel/run.rs:2135` - The reviewed patch made large panels fail to start by putting full prior finding records on each member's command line. The requested checkout includes the subsequent file-based fix. Trigger: At 99e2da7, a task has enough open finding text for the serialized member argument to exceed the platform launch limit. Evidence: read-code: The patch adds full finding records to the member spec while retaining base64 command-line transport.; ran-command: The later commit reports Windows launch error 206 and changes members to receive a spec-file path. Verify: Run the large-prior fake-member panel fixture at both revisions on Windows and compare member launch outcomes. Remedy: Retain b9f665a or an equivalent file-backed member-spec transport wherever the reviewed patch ships.

### Prior findings

- F02-1 - not-checked - Outside this handoff.
- F02-2 - not-checked - Outside this handoff.
- F02-3 - not-checked - Outside this handoff.
- F02-4 - not-checked - Outside this handoff.
- F02-5 - not-checked - Outside this handoff.
- F02-6 - not-checked - The broader hook latency claim was outside this handoff.
- F02-7 - not-checked - The full description round trip was outside this handoff.
- F02-8 - not-checked - Outside this handoff.
- F02-9 - not-checked - Outside this handoff.
- F02-10 - not-checked - Outside this handoff.
- F02-11 - not-checked - Outside this handoff.
- F02-12 - not-checked - Outside this handoff.
- F02-13 - not-checked - Outside this handoff.
- F02-14 - not-checked - Outside this handoff.
- F02-15 - not-checked - Outside this handoff.
- F02-16 - not-checked - Outside this handoff.
- F02-17 - not-checked - Outside this handoff.
- F02-18 - not-checked - Outside this handoff.
- F02-19 - not-checked - Outside this handoff.
- F02-20 - not-checked - Outside this handoff.
- F02-21 - not-checked - The full removal-race claim was outside this handoff.
- F03-1 - not-checked - Outside this handoff.
- F03-2 - not-checked - The full removal-race claim was outside this handoff.
- F03-3 - not-checked - Dotted and unusual branch names were outside this handoff.
- F03-4 - not-checked - Outside this handoff.
- F03-5 - not-checked - Outside this handoff.
- F03-6 - not-checked - Outside this handoff.
- F04-1 - not-checked - The primary-worktree index audit was outside this handoff.
- F04-2 - not-checked - Outside this handoff.
- F04-3 - not-checked - Outside this handoff.
- F04-4 - not-checked - The full doctor child audit was outside this handoff.
- F04-5 - not-checked - The Windows unlink-error fixture was outside this handoff.
- F04-6 - not-checked - Outside this handoff.
- F04-7 - not-checked - Outside this handoff.
- F04-8 - not-checked - Outside this handoff.
- F04-9 - not-checked - The config-read error path was outside this handoff.
- F10-1 - still-open - Unknown tools fail closed. An allow-listed tool can still lose evidence during parameter merge, and unlogged writes can remain usable.
- F10-2 - still-open - Stable-index events merge. A changed index or a missing index can leave an unnamed step classified as writing.
- F10-3 - still-open - Staging failures have a typed refusal. A deleted live brief still fails before the committed fallback.
- F10-4 - fixed - New member specs freeze finding records, and store commits take the task write lock.
- F10-5 - fixed - Known MCP multi-agent names count, and opted-in seats return Unproven. Finding #2 covers turn attribution.
- F10-7 - still-open - The panel can recover evidence by handoff, but pending evidence or another member's newer stream can still obscure the affected member's last turn.
- F11-1 - still-open - The integration test explicitly accepts a launcher write with no tool event. Nested updates can also still hide tool evidence.
- F11-2 - fixed - A detected before/after change to a bound input fails the live agy outcome and keeps the reply.
- F11-3 - fixed - The static committed-link trigger is refused. Finding #1 covers replacement after validation.
- F11-4 - still-open - Readable dot-dot paths normalize and staging refusal is typed. The normal entry point still rejects an already-deleted committed brief.
- F11-5 - fixed - Ordinary live-only paths are counted and names containing newlines or backticks are withheld.
- F11-6 - fixed - New panels pass immutable prior records and gate reported IDs against that set.
- F11-7 - fixed - Missing thread IDs and opt-ins yield Unproven. The automatic consumers hold it.
- F11-8 - still-open - Identity, malformed-line and relative-home checks improved. Finding #2 gives the remaining per-turn proof defect.
- F11-9 - still-open - The final poll catches a recorded fast-exit spawn for outcome handling. It runs after exit, and older timestamp-less child rollouts can still match the slack window.
- F11-10 - fixed - The patch covers continuation, read-insist and format repair. The requested checkout also covers denial retry.
- F11-11 - still-open - Single-run stream notes are persisted, but an older same-handoff pending stream still has priority.
- F11-12 - still-open - Raw Markdown partials count now. The breakdown measures recorded preservation and can still count a subsequently deleted sidecar.
- F12-1 - still-open - Windows access denial maps to CannotTell. A Linux procfs permission failure can still map to Gone.
- F12-2 - fixed - The targeted rev-parse command sets GIT_OPTIONAL_LOCKS=0.
- F12-3 - still-open - The message distinguishes a cloud reparse point, but repair still refuses it.
- F12-4 - still-open - The documentation now acknowledges the remaining interval between re-read and write.
- F13-1 - fixed - A static linked root is reported and excluded from doctor removal.
- F13-2 - still-open - Bare stray entries are rejected. HEAD file existence alone can still make a non-repository qualify.
- F13-3 - fixed - A valid calendar date is required to recognize a branch record.
- F13-4 - fixed - Update commands set the marker and the three branch mutators refuse a marked child.
- F13-5 - still-open - Registered children are stopped, but a child can leave the registry before it exits. Finding #4 narrows the trigger.
- F13-6 - still-open - Known older pairs are filtered. The original roster-only case can still rewrite when local date evidence is missing.
- F13-7 - fixed - A failed cherry or ahead query leaves the branch unchecked and counted as such.
- F14-1 - not-checked - Outside this handoff.
- F14-2 - not-checked - Outside this handoff.
- F14-3 - not-checked - The scratch-roster attack fixture was outside this handoff.
- F14-4 - not-checked - Outside this handoff.
- F14-5 - not-checked - Outside this handoff.
- F15-1 - not-checked - Outside this handoff.
- F15-2 - not-checked - Outside this handoff.
- F15-3 - not-checked - The roster rebinding cases were outside this handoff.
- F15-5 - not-checked - Outside this handoff.
- F15-6 - not-checked - Outside this handoff.
- F16-1 - not-checked - Outside this handoff.
- F16-2 - not-checked - Outside this handoff.
- F16-3 - not-checked - Outside this handoff.
- F16-4 - not-checked - Outside this handoff.
- F16-6 - not-checked - Outside this handoff.
- F16-7 - not-checked - Outside this handoff.
- F16-8 - not-checked - Outside this handoff.
- F16-9 - not-checked - Outside this handoff.
- F16-10 - not-checked - Outside this handoff.
- F17-1 - not-checked - Outside this handoff.
- F17-2 - not-checked - Outside this handoff.
- F17-3 - not-checked - Outside this handoff.
- F17-4 - not-checked - Outside this handoff.
- F17-5 - not-checked - Outside this handoff.
- F17-6 - not-checked - The delegation-rule rewrite was outside this handoff.
- F17-7 - not-checked - Outside this handoff.
- F17-8 - not-checked - Outside this handoff.
- F17-9 - not-checked - Outside this handoff.
- F17-10 - not-checked - Outside this handoff.
- F18-1 - not-checked - Outside this handoff.
- F18-2 - fixed - A later checkout change sets --no-optional-locks on the live status call.
- F18-3 - not-checked - Outside this handoff.
- F18-4 - not-checked - Outside this handoff.
- F18-5 - not-checked - The complete staging-source audit was outside this handoff.
- F18-6 - not-checked - Outside this handoff.
- F18-7 - not-checked - Outside this handoff.
- F19-1 - not-checked - Outside this handoff.
- F19-2 - not-checked - Outside this handoff.
- F19-3 - not-checked - Outside this handoff.
- F19-4 - fixed - Creation fails on owner-record error and the six-hour rule for existing ownerless directories is gone.
- F19-5 - not-checked - Outside this handoff.
- F19-6 - not-checked - Outside this handoff.
- F20-1 - not-checked - Outside this handoff.
- F20-2 - fixed - A later checkout change sets --no-optional-locks on the live status call.
- F20-3 - not-checked - Outside this handoff.
- F20-4 - not-checked - Outside this handoff.
- F20-5 - not-checked - The full engine and revision matrix was outside this handoff.
- F20-6 - not-checked - Outside this handoff.
- F24-2 - still-open - A nonempty same-handoff pending path still wins over the persisted turn note.
- F24-4 - still-open - The merge still discards a later populated nested value when the original value is nonempty.
- F27-1 - not-checked - Outside this handoff.
- F27-2 - still-open - The checkout-time walker still opens queued directories by pathname without a containment recheck.
- F27-3 - not-checked - Outside this handoff.
- F28-1 - not-checked - Outside this handoff.
- F28-2 - not-checked - Outside this handoff.
- F28-3 - not-checked - Outside this handoff.
- F29-1 - not-checked - Outside this handoff.
- F29-3 - not-checked - Outside this handoff.
- F30-1 - not-checked - Outside this handoff.
- F30-2 - not-checked - Outside this handoff.
- F30-3 - not-checked - Outside this handoff.
- F30-4 - not-checked - Outside this handoff.
- F30-5 - not-checked - Outside this handoff.
- F30-6 - not-checked - Outside this handoff.
- F31-1 - not-checked - The CI-specific process-tree fixture was outside this handoff.
- F31-2 - not-checked - Outside this handoff.
- F31-3 - not-checked - Outside this handoff.
- F31-4 - not-checked - Outside this handoff.
- F31-5 - not-checked - Outside this handoff.
- F31-6 - not-checked - Outside this handoff.

## Verdict: HOLD

Staging can still write outside the worktree, and the Codex smoke can pass without evidence tied to both turns.

### Blockers

- **F32-1** `crates/c3/src/engines/worktree.rs:248`, `crates/c3/src/engines/worktree.rs:259`, `crates/c3/src/engines/worktree.rs:269`, `crates/c3/src/engines/worktree.rs:275`, `crates/c3/src/engines/worktree.rs:359`, `CHANGELOG.md:500` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there. Verify: Use an injected directory-identity change between the validation and create calls and assert that staging refuses before any write outside the opened worktree. Remedy: Bind directory traversal, file creation, cleanup and publication to verified directory handles and refuse if a platform cannot safely keep those operations within the worktree.
- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, fixed) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, fixed) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, fixed) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.

### Unproven scenarios

- No build, test suite or filesystem mutation was run in the read-only worktree. CI results are from the handoff.
- Windows and Unix staging and removal under concurrent parent replacement, and cleanup under write or deletion failure.
- Actual cross-user Linux process visibility and Windows cloud-sync reparse behavior.
- Real Codex rollout shapes across both smoke turns, older Codex config-key handling, and behavior after a spawn has happened immediately before process exit.
- End-to-end panel reserve re-seating, interrupted panel stream attribution, and live agy behavior with the complete Cascade reader vocabulary.
- Unrelated release waves, the excluded collaboration and eck artifacts, and patch hunks outside the targeted call sites and tests.

### First-run checklist (observable)

_(none)_
