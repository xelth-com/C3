# Handoff 19 - Meta Muse (muse): reply-meta

Date: 2026-11-23 01:18 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (usable reply 35 min ago).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 236ddc88-a9bc-41c0-877a-f57a90f0d28b.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-a5be61283e3b4c8ab9c425617a361fc2.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8091fcd67c534046b08742c0e9e8298a` of revision e4f42a8da3b138e4061a9f8f76d977b1117ecadf (--rev e4f42a8da3b138e4061a9f8f76d977b1117ecadf; removed after the run).
Parent thread: (none - new thread). Result thread: `01a205bd-5931-7092-9285-7a3ffaf9bb2c` (source: events).
Brief: `.collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.md` (sha256 05e9583d36cf). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 332.2 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8091fcd67c534046b08742c0e9e8298a\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them; the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: REJECT - The http --rev path warns but still sends the live tree off machine, alongside patch-content and ledger-binding defects. Findings: 1 blocker, 5 major, 0 minor, 0 note (F19-1..F19-6, tracked in `findings.json`). Structured reply: `handoffs/19-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/19-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

Q1 Same revision: panel resolves once to a full sha and members check out that sha, and single runs pin the sha before worktree add, so a moving branch does not split runs. Blob/tree shas are refused by `^{commit}` plus the 40-hex check. Weak points: prefix-equality in resume_revision_warning can equate distinct shas sharing a 12-char prefix, and findings stay bound to live HEAD while the code read is an older sha (false attribution, finding #6). Shallow clones fail as capability failures, which is safe.

Q2 Staging: copies only task handoffs (minus `.events.jsonl`), `state.md`, and a stageable brief. Source links are skipped, dest links and link-parents refuse, and `..`/absolute brief refs are not staged (unreachable ones are refused). No write through a checked-out link was found. Overwrite of a tracked path happens only inside the throwaway. Hard-link sources are copied by content, not linked.

Q3 Diff/index: range sides reject leading `-` and whitespace, and the range is passed as one argv, so option injection was not found. Temp-index path uses a uuid copy with `GIT_INDEX_FILE`, leaving the real index alone. Gaps: WorkingTree `--diff` sweeps every untracked non-ignored file into the patch (finding #2); same patch path for different ranges races the ledger (finding #3). Binary/file counting via `diff --git` lines understates binaries.

Q4 Cleanup: one worktree per run dropped after the commit covers success/failure/timeout paths, and removal stays link-safe. SIGKILL/detach-kill leaves Drop unrun for doctor, but the owner record is best-effort so a live tree can look stale (finding #4). A failed worktree create after a patch write leaves the patch behind.

Q5 Classification/races: live-tree changes are now warnings for isolated reviewers and worktree writes decide the outcome; sibling worktrees use distinct uuid dirs. Remaining risk is claude/grok/muse writes staying usable by design, and the patch-path race above. No name collision beyond the patch path was found.

Q6 Privacy/entry: http+--rev warns yet sends the live pack off machine (finding #1, blocker). Frozen-claude isolation lines now send up to 20 live uncommitted paths to the model endpoint. MCP forwards rev/diff to CLI flags with only syntax checks, and C3_CHILD=1 does not stop the patch write (finding #5). CHANGELOG overstates link safety by implying no `--force`, while the code runs `worktree remove --force` after the unlink pass.

Q7 Untested: no real-CLI run of --rev/--diff/HEAD worktree is on record; Windows junction/dir-symlink removal, doctor-after-crash discovery, detached-kill cleanup, MCP boundary strings, and C3_CHILD behavior are covered only by fakes or not at all. Prior findings were not re-run; all are reported still-open.

## Requested checks
RC1 Run `c3 consult --rev <old> --engine http` in a scratch repo and confirm which tree the pack contains. Needs workspace-write. Settles finding #1.
RC2 Run `c3 consult --diff HEAD` with an untracked non-ignored sentinel file and inspect the patch bytes. Needs workspace-write. Settles finding #2.
RC3 Run two `--diff` ranges sharing one brief concurrently and compare each ledger `diff.sha256` with the file on disk. Needs workspace-write. Settles finding #3.
RC4 Make `record_throwaway_owner` fail, start a kimi/--rev run, then run `c3 doctor --fix` while it lives. Needs workspace-write. Settles finding #4.
RC5 With `C3_CHILD=1`, run `c3 consult --diff HEAD` and check whether a patch file is created. Needs workspace-write. Settles finding #5.

---

### Findings

- **F19-1** [blocker] `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision. Trigger: c3 consult --rev <old> --engine http (or panel/MCP with rev) in a tree whose live files differ from <old>, including uncommitted secrets. Evidence: read-code: plan_isolation returns Ok(None) for engine http, so no worktree is made; the caller only pushes a WARNING string and continues the run.; read-code: Patch wires rev/diff through for every engine but the --rev isolation is skipped for http with a printed WARNING, not a refusal; the http pack path still reads the live tree. Verify: Run --rev with the http engine and capture which tree the pack is built from; confirm no network send occurs after the warning. Remedy: Refuse --rev with the http engine before anything starts, or build the pack from the resolved sha worktree instead of warning and sending live content.
- **F19-2** [major] `crates/c3/src/consult/reviewed.rs:590` - --diff with one revision (diff against the working tree) includes every untracked non-ignored file, so an unrelated secret file in the checkout is written into the handoff patch and handed to the reviewer, including a remote model. Trigger: c3 consult --diff HEAD in a checkout containing an untracked non-ignored secret file, reviewed by any engine that sends the patch off machine. Evidence: read-code: WorkingTree path lists untracked files with ls-files --others --exclude-standard, marks them intent-to-add in a temp index copy, then diffs the rev; ignored files are excluded but ordinary untracked files are all included.; read-code: No prompt, warning, or path filter narrows the untracked set to review-relevant files before the patch is named in the prompt and sent. Verify: Place an untracked secret-named file in a scratch repo, run --diff HEAD, and inspect whether the patch bytes contain it. Remedy: Limit WorkingTree --diff to tracked changes by default and require an explicit flag to include untracked files, with a warning naming the included untracked paths.
- **F19-3** [major] `crates/c3/src/consult/reviewed.rs:502`, `crates/c3/src/consult/reviewed.rs:675` - Two concurrent runs sharing one brief stem but different --diff ranges write the same patch path, so the loser's ledger sha256/files/bytes can describe bytes the winner wrote. Trigger: Two c3 consult --diff runs (or two panel members with different diffs) on the same task/brief with different ranges at the same time. Evidence: read-code: plan_diff derives the patch name only from the brief stem (or nn/prefix), never from the range; write_patch treats identical bytes as fine but different bytes from a sibling race as fine too after re-checking same().; read-code: Ledger diff record is filled from the in-memory plan after the write, so a same-path overwrite by a sibling between write and commit binds the entry to the wrong file. Verify: Run two --diff specs with the same brief concurrently and compare each ledger diff.sha256 with the sha256 of the file at its patch path. Remedy: Include a short hash of the normalized range in the patch filename, or fail the write when the path already holds different bytes instead of accepting the race.
- **F19-4** [major] `crates/c3/src/engines/worktree.rs:89` - The throwaway worktree owner record is still best-effort, so a live --rev/kimi/claude worktree whose owner write failed can be reaped by doctor --fix while the reviewer is running. Trigger: Owner write fails (read-only admin dir, concurrent cleanup) and doctor --fix runs more than 6 hours after the worktree directory mtime while the reviewer process is alive. Evidence: read-code: ThrowawayWorktree::create calls record_throwaway_owner and discards its result; an unrecorded live worktree is indistinguishable from a stale one to the 6-hour staleness rule. Verify: Make record_throwaway_owner fail in a fixture, start a kimi/--rev run, then run doctor --fix and check the live worktree survives. Remedy: Check the owner-write result and fail the run as a capability failure when it cannot be recorded, or make staleness require positive proof of death rather than a missing owner file. Supersedes: F13-8.
- **F19-5** [major] `crates/c3/src/consult/reviewed.rs:672`, `crates/c3/src/mcp/mod.rs:317` - consult --rev/--diff performs repository writes (patch file, staged worktree metadata, worktree registration) even when C3_CHILD=1, and MCP rev/diff values reach git argument construction through only the CLI syntax check. Trigger: A C3-started child (C3_CHILD=1) runs c3 consult --diff, or an MCP caller supplies rev/diff with unusual but syntax-valid tokens. Evidence: read-code: Search shows no C3_CHILD guard on the consult patch/worktree setup path; prepare_review_inputs writes the patch into the live handoffs before the first turn unconditionally.; read-code: MCP maps rev/diff straight to --rev/--diff CLI flags; the only gate before git is rev_token_ok/parse_diff_spec syntax, with semantic resolve done later in orchestration. Verify: With C3_CHILD=1 run consult --diff in a scratch repo and check whether a patch file is created; pass boundary rev/diff strings via MCP and confirm they are refused before git. Remedy: Honor C3_CHILD=1 on the patch write (refuse or redirect outside the repo) and document the MCP rev/diff contract; add MCP-side length/charset validation mirroring the CLI.
- **F19-6** [major] `.collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.md:54` - Under --rev the review reads an older commit but findings bookkeeping stays bound to live HEAD, so a verdict/findings can be recorded against code that was never reviewed. Trigger: Review an older revision with open findings recorded against current HEAD; inspect which commit the resulting findings attach to. Evidence: read-code: Brief states findings stay bound to the live HEAD even under --rev; the isolation plan pins sha for the worktree while the ledger entry still carries the live base commit.; inferred: No re-binding of open/prior findings to the reviewed sha is described in reviewed.rs or the orchestration hunks; the resume warning only warns on thread reuse. Verify: Open a finding on HEAD, run --rev on an older sha, and read the committed entry's base/findings linkage. Remedy: Bind the findings snapshot to the reviewed sha under --rev (or label the entry unbound), and warn that prior HEAD findings were not re-checked at that revision.

### Prior findings

- F02-1 - still-open - Range holds no fix per brief; not re-run here.
- F02-2 - still-open - Range holds no fix per brief; not re-run here.
- F02-3 - still-open - Range holds no fix per brief; not re-run here.
- F02-4 - still-open - Range holds no fix per brief; not re-run here.
- F02-5 - still-open - Range holds no fix per brief; not re-run here.
- F02-6 - still-open - Range holds no fix per brief; not re-run here.
- F02-7 - still-open - Range holds no fix per brief; not re-run here.
- F02-8 - still-open - Range holds no fix per brief; not re-run here.
- F02-9 - still-open - Range holds no fix per brief; not re-run here.
- F02-10 - still-open - Range holds no fix per brief; not re-run here.
- F02-11 - still-open - Range holds no fix per brief; not re-run here.
- F02-12 - still-open - Range holds no fix per brief; not re-run here.
- F02-13 - still-open - Range holds no fix per brief; not re-run here.
- F02-14 - still-open - Range holds no fix per brief; not re-run here.
- F02-15 - still-open - Range holds no fix per brief; not re-run here.
- F02-16 - still-open - Range holds no fix per brief; not re-run here.
- F02-17 - still-open - Range holds no fix per brief; not re-run here.
- F02-18 - still-open - Range holds no fix per brief; not re-run here.
- F02-19 - still-open - Range holds no fix per brief; not re-run here.
- F02-20 - still-open - Range holds no fix per brief; not re-run here.
- F02-21 - still-open - Range holds no fix per brief; not re-run here.
- F03-1 - still-open - Range holds no fix per brief; not re-run here.
- F03-2 - still-open - Range holds no fix per brief; not re-run here.
- F03-3 - still-open - Range holds no fix per brief; not re-run here.
- F03-4 - still-open - Range holds no fix per brief; not re-run here.
- F03-5 - still-open - Range holds no fix per brief; not re-run here.
- F03-6 - still-open - Range holds no fix per brief; not re-run here.
- F04-1 - still-open - Range holds no fix per brief; not re-run here.
- F04-2 - still-open - Range holds no fix per brief; not re-run here.
- F04-3 - still-open - Range holds no fix per brief; not re-run here.
- F04-4 - still-open - Range holds no fix per brief; not re-run here.
- F04-5 - still-open - Range holds no fix per brief; not re-run here.
- F04-6 - still-open - Range holds no fix per brief; not re-run here.
- F04-7 - still-open - Range holds no fix per brief; not re-run here.
- F04-8 - still-open - Range holds no fix per brief; not re-run here.
- F04-9 - still-open - Range holds no fix per brief; not re-run here.
- F10-1 - still-open - Range holds no fix per brief; not re-run here.
- F10-2 - still-open - Range holds no fix per brief; not re-run here.
- F10-3 - still-open - Range holds no fix per brief; not re-run here.
- F10-4 - still-open - Range holds no fix per brief; not re-run here.
- F10-5 - still-open - Range holds no fix per brief; not re-run here.
- F10-7 - still-open - Range holds no fix per brief; not re-run here.
- F11-1 - still-open - Range holds no fix per brief; not re-run here.
- F11-2 - still-open - Range holds no fix per brief; not re-run here.
- F11-3 - still-open - Range holds no fix per brief; not re-run here.
- F11-4 - still-open - Range holds no fix per brief; not re-run here.
- F11-5 - still-open - Range holds no fix per brief; not re-run here.
- F11-6 - still-open - Range holds no fix per brief; not re-run here.
- F11-7 - still-open - Range holds no fix per brief; not re-run here.
- F11-8 - still-open - Range holds no fix per brief; not re-run here.
- F11-9 - still-open - Range holds no fix per brief; not re-run here.
- F11-10 - still-open - Range holds no fix per brief; not re-run here.
- F11-11 - still-open - Range holds no fix per brief; not re-run here.
- F11-12 - still-open - Range holds no fix per brief; not re-run here.
- F12-1 - still-open - Range holds no fix per brief; not re-run here.
- F12-2 - still-open - Range holds no fix per brief; not re-run here.
- F12-3 - still-open - Range holds no fix per brief; not re-run here.
- F12-4 - still-open - Range holds no fix per brief; not re-run here.
- F13-1 - still-open - Range holds no fix per brief; not re-run here.
- F13-2 - still-open - Range holds no fix per brief; not re-run here.
- F13-3 - still-open - Range holds no fix per brief; not re-run here.
- F13-4 - still-open - Range holds no fix per brief; not re-run here.
- F13-5 - still-open - Range holds no fix per brief; not re-run here.
- F13-6 - still-open - Range holds no fix per brief; not re-run here.
- F13-7 - still-open - Range holds no fix per brief; not re-run here.
- F13-8 - still-open - Superseded by new owner-record finding; underlying race persists.
- F14-1 - still-open - Range holds no fix per brief; not re-run here.
- F14-2 - still-open - Range holds no fix per brief; not re-run here.
- F14-3 - still-open - Range holds no fix per brief; not re-run here.
- F14-4 - still-open - Range holds no fix per brief; not re-run here.
- F14-5 - still-open - Range holds no fix per brief; not re-run here.
- F15-1 - still-open - Range holds no fix per brief; not re-run here.
- F15-2 - still-open - Range holds no fix per brief; not re-run here.
- F15-3 - still-open - Range holds no fix per brief; not re-run here.
- F15-5 - still-open - Range holds no fix per brief; not re-run here.
- F15-6 - still-open - Range holds no fix per brief; not re-run here.
- F16-1 - still-open - Range holds no fix per brief; not re-run here.
- F16-2 - still-open - Range holds no fix per brief; not re-run here.
- F16-3 - still-open - Range holds no fix per brief; not re-run here.
- F16-4 - still-open - Range holds no fix per brief; not re-run here.
- F16-6 - still-open - Range holds no fix per brief; not re-run here.
- F16-7 - still-open - Range holds no fix per brief; not re-run here.
- F16-8 - still-open - Range holds no fix per brief; not re-run here.
- F16-9 - still-open - Range holds no fix per brief; not re-run here.
- F16-10 - still-open - Range holds no fix per brief; not re-run here.
- F17-1 - still-open - Range holds no fix per brief; not re-run here.
- F17-2 - still-open - Range holds no fix per brief; not re-run here.
- F17-3 - still-open - Range holds no fix per brief; not re-run here.
- F17-4 - still-open - Range holds no fix per brief; not re-run here.
- F17-5 - still-open - Range holds no fix per brief; not re-run here.
- F17-6 - still-open - Range holds no fix per brief; not re-run here.
- F17-7 - still-open - Range holds no fix per brief; not re-run here.
- F17-8 - still-open - Range holds no fix per brief; not re-run here.
- F17-9 - still-open - Range holds no fix per brief; not re-run here.
- F17-10 - still-open - Range holds no fix per brief; not re-run here.

## Verdict: REJECT

The http --rev path warns but still sends the live tree off machine, alongside patch-content and ledger-binding defects.

### Blockers

- **F19-1** `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision. Verify: Run --rev with the http engine and capture which tree the pack is built from; confirm no network send occurs after the warning. Remedy: Refuse --rev with the http engine before anything starts, or build the pack from the resolved sha worktree instead of warning and sending live content.
- **F02-1** (prior, still-open) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, still-open) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, still-open) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, still-open) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, still-open) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, still-open) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, still-open) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, still-open) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, still-open) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- Whether c3_core::store::write_text_atomic follows a pre-placed symlink at the patch path.
- Whether a reviewer background process surviving the tree kill can win the removal TOCTOU between the final link sweep and git remove / remove_dir_all.
- Exact git behavior for ambiguous short shas under --verify --quiet across git versions.
- Windows drive-relative (C:foo) and UNC brief refs against stageable_rel and Path::join.

### First-run checklist (observable)

_(none)_
