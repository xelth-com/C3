# Handoff 69 - Codex: acceptance-delta-3

Date: 2026-11-24 22:47 local. Author: Codex (model gpt-6.2-astra, effort high), Codex CLI 0.169.0.
Reviewer: openai :: gpt-6.2-astra (provider from roster, model from roster; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.169.0).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - position 1 of 17.
Effort: high sent (requested high, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: 082fedef-c0a9-4b8c-8f12-1a7a0e2643dc.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: acceptance). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6.2-astra -c features.multi_agent=false -c agents.enabled=false -c features.plugins=false -c skills.include_instructions=false -c model_reasoning_effort="high" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-e99892f0872a4854957322d6ffad2c02.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-28af236d2bc94b23a5b36d4956c24fc5` of revision 8706a2ff7be79521f4915c4070b5ba496034d710 (--rev HEAD; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20f83-a3ba-7a90-9eae-c38118f5994a` (source: events).
Brief: `.collab/release-0.2.5/handoffs/68-claude-acceptance-delta-3.md` (sha256 1c080893ba77). Reviewed: 8706a2f, base 8706a2ff7be79521f4915c4070b5ba496034d710, tree sha256 cde5584cf837, 0 changed files. Live HEAD at launch: 8706a2ff7be79521f4915c4070b5ba496034d710.
Bridge outcome: usable reply. Wall time: 242.3 s. Tokens: in 648793 (cached 571264), out 7974, reasoning 113.
Quota (rollout): 5h 77% until 00:16; wk 95% until Nov 29; this run 648793 input tokens (571264 cached) in 10 calls, max 77980 per call of 258400; sub-agents 0; sub-agents off (features.multi_agent=false, agents.enabled=false); budget 90 % of the 5-hour window.
Warnings: ACCEPT with 12 unchecked prior blocker(s) (F02-1, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1, F32-1, F57-4, F57-5)..
Timeout: 3600 s (the default of purpose acceptance); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - The release blocker is closed, and the remaining exchange-test and documentation issues do not block tagging the reviewed head once all three CI jobs pass on that exact commit. Findings: 0 blocker, 0 major, 1 minor, 0 note (F69-1, tracked in `findings.json`). Structured reply: `handoffs/69-codex-acceptance-delta-3.reply.json`.
Raw event stream: `handoffs/69-codex-acceptance-delta-3.events.jsonl`.
Verbatim reply follows.

---

1. **F67-1 closed.** Both ordinary update and recovery re-read the installed version after a passing smoke. A changed or unreadable identity returns an unresolved action and retains the pending record. The caller clears it only on resolution (`crates/c3/src/maintain/mod.rs:999`, `:1163`, `:1186`, `:1352`). The integration fixtures check the action, journal and subsequent recovery. **F67-2 closed.** Numeric prerelease identifiers compare by significant digit count, then by digits, at arbitrary length (`crates/c3/src/maintain/version.rs:122`). **F67-3 partially closed.** First creation works under the simulated exchange refusal when hard links are available. Existing-file replacement still refuses safely. The tests, however, select by build target at `crates/c3/src/contain.rs:1843` and unwrap at `:1874` and `:2099`. A Linux build on a filesystem that rejects exchange still fails those tests for correct production behavior. **F67-4 closed.** The changelog now identifies which directory writers must be trusted and explicitly addresses shared checkouts (`CHANGELOG.md:940`).

2. **No new blocker.** One new documentation detail merits correction. `CHANGELOG.md:890` promises refusal when a file appears during first creation. An `AlreadyExists` result retries at `crates/c3/src/contain.rs:779`. On an exchange-capable filesystem the retry can publish successfully and preserve the displaced bytes.

3. **ACCEPT**, given green three-job CI for `8706a2ff7be79521f4915c4070b5ba496034d710`. The brief reports Windows test results for `808a1bb` and green CI for earlier commits. I did not independently observe CI for the tag candidate. I read all five changed code and fixture paths, the changelog hunks and the relevant handoff sections. I skipped the coordinator’s logs, stores, sidecar and unchanged implementation outside the cited paths. No files changed. `git diff --check c4a74af HEAD -- CHANGELOG.md crates` passed. I did not run builds or tests in this read-only review.

## Requested checks

- **RC1** — From the repository root on Linux, with workspace-write permission and `TMPDIR` on a writable filesystem known to support hard links and reject `RENAME_EXCHANGE`, run `cargo test -p c3 publish_exchange_`. The current unwrapped refusal confirms the remaining F67-3 test gap. Budget one filtered test run.
- **RC2** — In the repository’s CI run list, with read-only access, inspect the run for the full SHA in answer 3. Require successful completion of all three jobs and the expected test steps. Budget one run inspection.

---

### Findings

- **F69-1** [minor] `CHANGELOG.md:890`, `crates/c3/src/contain.rs:779`, `crates/c3/src/contain.rs:758` - The changelog promises refusal if the destination appears during first creation, but publication can retry and successfully exchange the newly arrived file. Trigger: The first exchange sees a missing destination, a different file appears before the hard link, and the next exchange succeeds. Evidence: read-code: AlreadyExists continues the three-attempt loop. A successful subsequent exchange returns Publication::Exchange and processes the displaced file.; read-code: The parenthetical describes refusal for this race with or without exchange support. Verify: In a publication fixture on an exchange-capable filesystem, insert a distinct file between the Missing result and the hard-link call. Inspect the returned publication mode and the displaced bytes. Remedy: Describe the retry and its two outcomes. An available exchange can preserve the intervening edit and complete publication. An unsupported exchange refuses replacement.

### Prior findings

- F02-1 - not-checked - Outside this delta.
- F02-2 - fixed - Handoff 67 closure retained. Existing-file exchange still preserves displaced edits.
- F02-3 - not-checked - Outside this delta.
- F02-4 - not-checked - Outside this delta.
- F02-5 - not-checked - Outside this delta.
- F02-6 - not-checked - Outside this delta.
- F02-7 - not-checked - Outside this delta.
- F02-8 - not-checked - Outside this delta.
- F02-9 - not-checked - Outside this delta.
- F02-10 - not-checked - Outside this delta.
- F02-11 - not-checked - Outside this delta.
- F02-12 - not-checked - Outside this delta.
- F02-13 - not-checked - Outside this delta.
- F02-14 - not-checked - Outside this delta.
- F02-15 - not-checked - Outside this delta.
- F02-16 - not-checked - Outside this delta.
- F02-17 - not-checked - Outside this delta.
- F02-18 - not-checked - Outside this delta.
- F02-19 - not-checked - Outside this delta.
- F02-20 - not-checked - Outside this delta.
- F02-21 - not-checked - Outside this delta.
- F03-1 - not-checked - Outside this delta.
- F03-2 - not-checked - Outside this delta.
- F03-3 - not-checked - Outside this delta.
- F03-4 - not-checked - Outside this delta.
- F03-5 - not-checked - Outside this delta.
- F03-6 - not-checked - Outside this delta.
- F04-1 - not-checked - Outside this delta.
- F04-2 - not-checked - Outside this delta.
- F04-3 - not-checked - Outside this delta.
- F04-4 - not-checked - Outside this delta.
- F04-5 - not-checked - Outside this delta.
- F04-6 - not-checked - Outside this delta.
- F04-7 - not-checked - Outside this delta.
- F04-8 - not-checked - Outside this delta.
- F04-9 - not-checked - Outside this delta.
- F10-1 - not-checked - Outside this delta.
- F10-2 - not-checked - Outside this delta.
- F10-3 - not-checked - Outside this delta.
- F10-4 - not-checked - Outside this delta.
- F10-5 - not-checked - Outside this delta.
- F10-7 - not-checked - Outside this delta.
- F11-1 - not-checked - Outside this delta.
- F11-2 - not-checked - Outside this delta.
- F11-3 - not-checked - Outside this delta.
- F11-4 - not-checked - Outside this delta.
- F11-5 - not-checked - Outside this delta.
- F11-6 - not-checked - Outside this delta.
- F11-7 - not-checked - Outside this delta.
- F11-8 - not-checked - Outside this delta.
- F11-9 - not-checked - Outside this delta.
- F11-10 - not-checked - Outside this delta.
- F11-11 - not-checked - Outside this delta.
- F11-12 - not-checked - Outside this delta.
- F12-1 - not-checked - Outside this delta.
- F12-2 - not-checked - Outside this delta.
- F12-3 - not-checked - Outside this delta.
- F12-4 - not-checked - Outside this delta.
- F13-1 - not-checked - Outside this delta.
- F13-2 - not-checked - Outside this delta.
- F13-3 - not-checked - Outside this delta.
- F13-4 - not-checked - Outside this delta.
- F13-5 - not-checked - Outside this delta.
- F13-6 - not-checked - Outside this delta.
- F13-7 - not-checked - Outside this delta.
- F13-8 - not-checked - Outside this delta.
- F13-9 - not-checked - Outside this delta.
- F14-1 - not-checked - Outside this delta.
- F14-2 - not-checked - Outside this delta.
- F14-3 - not-checked - Outside this delta.
- F14-4 - not-checked - Outside this delta.
- F14-5 - not-checked - Outside this delta.
- F15-1 - not-checked - Outside this delta.
- F15-2 - not-checked - Outside this delta.
- F15-3 - not-checked - Outside this delta.
- F15-5 - not-checked - Outside this delta.
- F15-6 - not-checked - Outside this delta.
- F16-1 - fixed - Handoff 67 closure carried forward. Catalog trust is outside this delta.
- F16-2 - not-checked - Outside this delta.
- F16-3 - not-checked - Outside this delta.
- F16-4 - not-checked - Outside this delta.
- F16-5 - not-checked - Outside this delta.
- F16-6 - not-checked - Outside this delta.
- F16-7 - not-checked - Outside this delta.
- F16-8 - not-checked - Outside this delta.
- F16-9 - not-checked - Outside this delta.
- F16-10 - not-checked - Outside this delta.
- F17-1 - not-checked - Outside this delta.
- F17-2 - not-checked - Outside this delta.
- F17-3 - not-checked - Outside this delta.
- F17-4 - not-checked - Outside this delta.
- F17-5 - fixed - Handoff 67 closure carried forward. Catalog trust is outside this delta.
- F17-6 - not-checked - Outside this delta.
- F17-7 - not-checked - Outside this delta.
- F17-8 - not-checked - Outside this delta.
- F17-9 - not-checked - Outside this delta.
- F17-10 - not-checked - Outside this delta.
- F18-1 - not-checked - Outside this delta.
- F18-2 - not-checked - Outside this delta.
- F18-3 - not-checked - Outside this delta.
- F18-4 - not-checked - Outside this delta.
- F18-5 - not-checked - Outside this delta.
- F18-6 - not-checked - Outside this delta.
- F18-7 - not-checked - Outside this delta.
- F19-1 - not-checked - Outside this delta.
- F19-2 - not-checked - Outside this delta.
- F19-3 - not-checked - Outside this delta.
- F19-4 - not-checked - Outside this delta.
- F19-5 - not-checked - Outside this delta.
- F19-6 - not-checked - Outside this delta.
- F20-1 - not-checked - Outside this delta.
- F20-2 - not-checked - Outside this delta.
- F20-3 - not-checked - Outside this delta.
- F20-4 - not-checked - Outside this delta.
- F20-5 - not-checked - Outside this delta.
- F20-6 - not-checked - Outside this delta.
- F24-1 - not-checked - Outside this delta.
- F24-2 - not-checked - Outside this delta.
- F24-4 - not-checked - Outside this delta.
- F27-1 - not-checked - Outside this delta.
- F27-2 - not-checked - Outside this delta.
- F27-3 - not-checked - Outside this delta.
- F28-1 - not-checked - Outside this delta.
- F28-2 - not-checked - Outside this delta.
- F28-3 - not-checked - Outside this delta.
- F29-1 - not-checked - Outside this delta.
- F29-2 - not-checked - Outside this delta.
- F29-3 - not-checked - Outside this delta.
- F30-1 - not-checked - Outside this delta.
- F30-2 - not-checked - Outside this delta.
- F30-3 - not-checked - Outside this delta.
- F30-4 - not-checked - Outside this delta.
- F30-5 - not-checked - Outside this delta.
- F30-6 - not-checked - Outside this delta.
- F31-1 - not-checked - Outside this delta.
- F31-2 - not-checked - Outside this delta.
- F31-3 - not-checked - Outside this delta.
- F31-4 - not-checked - Outside this delta.
- F31-5 - not-checked - Outside this delta.
- F31-6 - not-checked - Outside this delta.
- F32-1 - not-checked - Outside this delta.
- F32-2 - not-checked - Outside this delta.
- F32-3 - not-checked - Outside this delta.
- F32-4 - not-checked - Outside this delta.
- F32-5 - not-checked - Outside this delta.
- F36-1 - not-checked - Outside this delta.
- F36-2 - not-checked - Outside this delta.
- F36-3 - not-checked - Outside this delta.
- F36-4 - not-checked - Outside this delta.
- F36-5 - not-checked - Outside this delta.
- F36-6 - not-checked - Outside this delta.
- F36-7 - not-checked - Outside this delta.
- F36-8 - not-checked - Outside this delta.
- F38-1 - not-checked - Outside this delta.
- F38-2 - not-checked - Outside this delta.
- F39-1 - not-checked - Outside this delta.
- F39-2 - not-checked - Outside this delta.
- F39-4 - not-checked - Outside this delta.
- F43-1 - not-checked - Outside this delta.
- F43-2 - not-checked - Outside this delta.
- F43-3 - not-checked - Outside this delta.
- F43-5 - not-checked - Outside this delta.
- F43-6 - not-checked - Outside this delta.
- F43-7 - not-checked - Outside this delta.
- F43-8 - not-checked - Outside this delta.
- F44-1 - not-checked - Outside this delta.
- F44-2 - not-checked - Outside this delta.
- F44-3 - not-checked - Outside this delta.
- F45-1 - not-checked - Outside this delta.
- F45-2 - not-checked - Outside this delta.
- F45-3 - not-checked - Outside this delta.
- F45-4 - not-checked - Outside this delta.
- F45-5 - not-checked - Outside this delta.
- F45-6 - not-checked - Outside this delta.
- F45-7 - not-checked - Outside this delta.
- F45-8 - not-checked - Outside this delta.
- F45-9 - not-checked - Outside this delta.
- F46-1 - not-checked - Outside this delta.
- F46-2 - not-checked - Outside this delta.
- F46-3 - not-checked - Outside this delta.
- F46-4 - not-checked - Outside this delta.
- F46-5 - not-checked - Outside this delta.
- F46-6 - not-checked - Outside this delta.
- F46-7 - not-checked - Outside this delta.
- F46-8 - not-checked - Outside this delta.
- F47-1 - not-checked - Outside this delta.
- F47-2 - not-checked - Outside this delta.
- F47-3 - not-checked - Outside this delta.
- F47-4 - not-checked - Outside this delta.
- F48-1 - not-checked - Outside this delta.
- F48-2 - not-checked - Outside this delta.
- F48-3 - not-checked - Outside this delta.
- F48-4 - not-checked - Outside this delta.
- F48-5 - not-checked - Outside this delta.
- F51-1 - not-checked - Outside this delta.
- F51-2 - not-checked - Outside this delta.
- F51-3 - not-checked - Outside this delta.
- F51-4 - not-checked - Outside this delta.
- F51-5 - not-checked - Outside this delta.
- F51-6 - not-checked - Outside this delta.
- F52-1 - not-checked - Outside this delta.
- F52-2 - not-checked - Outside this delta.
- F52-4 - not-checked - Outside this delta.
- F53-1 - not-checked - Outside this delta.
- F53-2 - not-checked - Outside this delta.
- F53-3 - not-checked - Outside this delta.
- F53-4 - not-checked - Outside this delta.
- F53-5 - not-checked - Outside this delta.
- F53-6 - not-checked - Outside this delta.
- F53-7 - not-checked - Outside this delta.
- F57-1 - fixed - Handoff 67 closure carried forward. Cached page provenance is outside this delta.
- F57-2 - fixed - Handoff 67 closure carried forward. Tool inventory parsing is outside this delta.
- F57-3 - fixed - Recovery still measures exact identity after the smoke through the shared helper.
- F57-4 - not-checked - Outside this delta.
- F57-5 - not-checked - Outside this delta.
- F57-6 - fixed - Handoff 67 closure carried forward for admission-time validation.
- F57-7 - still-open - Handoff 67 accepted the Unix checkout timing limit as nonblocking. The changelog now states the trusted-temp assumption.
- F57-8 - not-checked - Outside this delta.
- F57-9 - not-checked - Outside this delta.
- F57-10 - fixed - Existing-file publication still preserves a displaced edit or refuses an unsupported exchange.
- F60-1 - not-checked - Outside this delta.
- F60-2 - not-checked - Outside this delta.
- F60-3 - not-checked - Outside this delta.
- F61-1 - not-checked - Outside this delta.
- F61-3 - fixed - Handoff 67 closure carried forward. Path resolution is outside this delta.
- F62-1 - fixed - Recovery still uses exact release identity. Its post-smoke comparison is now shared with ordinary updates.
- F62-2 - fixed - Handoff 67 closure carried forward for textual path normalization.
- F62-3 - fixed - Handoff 67 closure carried forward for collection-time catalog trust.
- F62-4 - fixed - Handoff 67 closure carried forward for the numbered backup collision.
- F62-5 - fixed - Handoff 67 closure carried forward for nested collaboration namespaces.
- F62-6 - fixed - Handoff 67 closure carried forward for bounded patch stems.
- F65-1 - fixed - Handoff 67 closure carried forward for the removed rename fallback.
- F65-2 - fixed - A third installed release still takes the same-pass recovery smoke.
- F65-3 - fixed - Handoff 67 records consult dry-run coverage of the shared resolver.
- F65-4 - fixed - Handoff 67 records exact-identity update, rollback and release-date fixtures.
- F65-5 - fixed - Handoff 67 records the corrected test comment.
- F65-6 - fixed - Handoff 67 records binary coverage of the legacy built-in catalog.
- F67-1 - fixed - After a passing ordinary smoke, exact identity is measured again. Drift returns unresolved and leaves the recovery record intact.
- F67-2 - fixed - Significant digit length and byte ordering cover both overflow examples without integer conversion.
- F67-3 - still-open - First creation and target-level guards are fixed. Runtime filesystem refusal on a supported build target still reaches unconditional test unwraps at crates/c3/src/contain.rs:1874 and :2099.
- F67-4 - fixed - The changelog explicitly requires trusted directory writers and explains the shared-checkout limitation.

## Verdict: ACCEPT

The release blocker is closed, and the remaining exchange-test and documentation issues do not block tagging the reviewed head once all three CI jobs pass on that exact commit.

**WARNING: ACCEPT with 12 unchecked prior blocker(s) (F02-1, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1, F32-1, F57-4, F57-5).**

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, fixed) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, not-checked) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.
- **F57-1** (prior, fixed) `crates/c3/src/maintain/web.rs:209` - Changing a page's URL and public flag while retaining its name can publish IDs from the previously non-public page in telemetry.
- **F57-2** (prior, fixed) `crates/c3-core/src/quota.rs:941` - An uninterpretable nested tool inventory counts as positive switch evidence for a Codex smoke turn.
- **F57-3** (prior, fixed) `crates/c3/src/maintain/mod.rs:740` - Pending recovery can record the pending target version as verified after smoking a different installed version.
- **F57-4** (prior, not-checked) `crates/c3/src/state_dir.rs:745` - A live lock creator can return success after another process reclaimed its still-empty file and acquired the same move lock.
- **F57-5** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:1451` - Two different patches can publish successfully at one short-hash path, leaving one consultation's full digest bound to the other consultation's bytes.
- **F57-6** (prior, fixed) `crates/c3/src/state_dir.rs:1544` - A linked task directory below a real project collab directory can redirect handoff directory creation and consultation output.
- **F62-1** (prior, fixed) `crates/c3/src/maintain/version.rs:79` - Pending recovery can mark a final CLI release verified when both installed-version probes and the smoke used only a prerelease.
- **F62-2** (prior, fixed) `crates/c3/src/state_dir.rs:1883` - Consult, panel and detach can classify a repository-relative collab argument as an external anchor because the link check and canonicalization interpret `..` differently.
- **F62-3** (prior, fixed) `crates/c3/src/maintain/web.rs:133` - Declaring a formerly private catalog URL public can publish cached IDs that appeared only in the earlier private response.
- **F62-4** (prior, fixed) `crates/c3/src/contain.rs:817` - The operation that preserves a displaced edit can overwrite an earlier displaced edit and report the same recovery path for both.
- **F62-5** (prior, fixed) `crates/c3-core/src/quota.rs:956` - A nested namespace can erase its enclosing collaboration namespace during inventory traversal, causing an offered tool to count as clean switch evidence.
- **F67-1** (prior, fixed) `crates/c3/src/maintain/mod.rs:1052` - The ordinary target-install path can record a passing smoke against the wrong release and discard the only pending recovery record.

### Unproven scenarios

- The exact tag candidate's three-job CI result was unavailable in this review. Windows results in handoff 68 are coordinator-reported for the code commit.
- The exchange tests were not executed on a scratch filesystem that supports hard links but rejects exchange. First creation on a filesystem without hard links also remains outside the success case.
- Version probes bracket the smoke. A release switch followed by a switch back between probes is undetectable. A release whose smoke attribution is unresolved awaits the next maintenance pass.
- The changelog's path guarantees assume trusted writers after admission. Runtime containment throughout a run in a shared checkout was outside this review.

### First-run checklist (observable)

- [ ] Confirm that the CI run identifies 8706a2ff7be79521f4915c4070b5ba496034d710 and shows all three jobs and their expected test steps completed successfully.
- [ ] For an ordinary successful update, correlate the selected target, the passing smoke and the final installed-version probe. Require the same exact release before accepting the target's `ok` action and cleared pending record.
- [ ] In the drift fixture, observe `1.0.80 -> 1.0.81` or `1.0.80 -> unknown` in the unresolved action, the original pending target still present, and no target success or rollback. Exit code 0 is expected even in this unresolved state.
- [ ] On the next pass of the readable-drift fixture, observe a passing smoke attributed to 1.0.81, the statement that 1.0.80 was not verified and an empty pending journal.
- [ ] For exchange refusal, inspect the fixture bytes and directory listing. The existing file must retain its original bytes. On a hard-link-capable filesystem the absent leaf must contain the complete new bytes after a first write.
