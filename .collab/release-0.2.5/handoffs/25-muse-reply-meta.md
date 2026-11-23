# Handoff 25 - Meta Muse (muse): reply-meta

Date: 2026-11-23 05:20 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: a369c5e3-527d-4c02-92e4-ea015a438b99.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-de5d5763eccb4a48aa1ebc39bf4e907d.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-0997e66fa3f246c28bee7b4c68fb9b4e` of revision b9f665a9ea46e50d1cad137d8072b3258eacc8d7 (--rev b9f665a9ea46e50d1cad137d8072b3258eacc8d7; removed after the run).
Parent thread: (none - new thread). Result thread: `01a2069e-9cd8-78f3-96a9-d22cec6c3455` (source: events).
Brief: `.collab/release-0.2.5/handoffs/21-claude-verify-staging-smoke-doctor.md` (sha256 117034b5c3ae). Reviewed: b9f665a, base b9f665a9ea46e50d1cad137d8072b3258eacc8d7, tree sha256 93a2e63d1804, 0 changed files. Live HEAD at launch: b9f665a9ea46e50d1cad137d8072b3258eacc8d7.
WARNING: the main working tree changed during the review (fingerprint before/after differ); the reviewer read its throwaway worktree, not the live tree.
Bridge outcome: usable reply. Wall time: 155.1 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: ACCEPT with 7 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F19-1).; muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-0997e66fa3f246c28bee7b4c68fb9b4e\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them; the main working tree changed during the run (1 file: crates/c3/src/engines/subprocess.rs) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - All in-scope triggers are closed or narrowed to explicitly documented boundaries; residuals are minor/notes, not acceptance blockers. Findings: 0 blocker, 0 major, 1 minor, 2 note (F25-1..F25-3, tracked in `findings.json`). Structured reply: `handoffs/25-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/25-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

Q1. All in-scope ids read as fixed against the new invariants: F11-1/F10-1/F10-2 (merged params, `agy.rs`/`tree_check.rs`), F11-2 (bound-input fail), F11-3 (dual link checks plus exclusive create and pre-rename re-check in `worktree.rs:233-269`), F11-4/F10-3 (normalised `..`, HEAD materialisation, typed refusal), F11-5 (tracked-only names), F11-6/F10-4 (frozen member record), F11-7/F10-5 (Unproven held like failure, `smoke.rs:28-50,657`), F11-8 (home/session-id/tool-array/terminal checks), F11-9 (post-exit inspection, creation-timestamp gate), F11-10 (reviewer sentence), F11-11/F10-7 (per-turn persisted stream, own-run filter), F11-12 (`reply_kept`), F13-1/F13-8/F12-1 (link/owner/cannot-tell handling, `branches.rs:68-78`), F13-2 (strict `.git` plus `GIT_DIR` refusal, `repo_models.rs:623-630`), F13-3 (real date), F13-4 (update children marked, child branch-write refusal), F13-5 (registered git killed), F13-6 (dated-older refused; undated unchanged by design), F13-7 (failed cherry unchecked), F12-2/F12-3/F12-4 (wording/code corrected). F10-6/F13-9 are accepted non-fix/boundary decisions.

Q2. Legitimate cases: no refusal found for normal dirs, cloud placeholders (now called reparse points), or temp-dirs that are links; hostile link-between-check-and-write is narrowed to the documented re-read-to-rename instant. Residual: that instant, undated AGENTS.md follows, timestamp-less child falls back to slack.

Q3. No leftover on covered errors: failed owner write removes worktree and fails run (`worktree.rs:131-141`); failed stage removes temp (`worktree.rs:263-266`); repair uses sibling temp plus rename; death-record reader uses persisted stream. Untested: stale lock after kill, temp-dir exhaustion.

Q4. No over-broad break seen: agy Cascade names added; mandatory owner fails closed safely; C3_CHILD refusals cover hook repair, doctor fix, branches set/clear/default; hook GIT_DIR silence only skips repair, exit stays 0.

Q5. Unproven is held on every automatic path sampled (`maintain/mod.rs:1202,1609,1753`; `smoke.rs:36-50`). CHANGELOG now says checked not proves, documents remaining windows and undated behaviour; no over-claim found.

Q6. Untested per brief: bound-input rule for muse/grok/claude outside worktree, panel re-seat after staging refusal, agy Cascade reader fixture; plus interleaved-writer, timestamp-less child, and undated AGENTS.md cases above.

---

### Findings

- **F25-1** [note] `crates/c3/src/setup/repo_models.rs`, `crates/c3/src/setup/branches.rs` - A concurrent writer landing between the final re-read and the rename/write is still not seen: repo_models repair, branches --set/--clear and staging re-check immediately before replacing, but the instant between that check and the filesystem rename remains. Trigger: Another process edits the same file/description in the microseconds between the pre-write re-read and the rename/git write. Evidence: read-code: Doc comments state the file/description is re-read immediately before rename and the write is skipped when changed, but the moment between re-read and rename itself remains / is not seen.; read-code: Branches doc says value is read again immediately before write, skipped when changed, with the moment between re-read and git's own write remaining.; read-code: Stage writes to exclusive temp then repeats checks right before rename; any race after that second check is unhandled. Verify: Interleave a writer at the exact write boundary (git-config lock or rename tracer) and inspect whether its value survives. Remedy: Document as decided boundary (already done) or close with atomic compare-and-swap where the store allows it; no change required for acceptance.
- **F25-2** [minor] `crates/c3/src/setup/repo_models.rs:114` - AGENTS.md still follows a numerically higher id when the maintain state dates neither id: without dates for both ids the version rule alone decides, so an undated grok-4.20 still replaces grok-4.7. Trigger: AGENTS.md model grok-4.7 with a sole followable roster entry grok-4.20 and no catalog dates for the pair. Evidence: read-code: Successor dated-not-newer is refused, but without dates for both ids the version rule alone decides, as before.; read-code: CHANGELOG states never follows to a dated-older id, with the undated behaviour unchanged. Verify: Call rewrite_agents_md_model on grok-4.7 with roster [grok-4.20] and empty ModelsState; inspect the returned edit. Remedy: Require dates for any cross-major follow, or refuse undated downgrades explicitly; otherwise keep the documented limitation.
- **F25-3** [note] `crates/c3-core/src/quota.rs` - A child rollout whose head carries no readable timestamp still falls back to the 5s mtime slack, so an old unrelated child written just now can be attributed to the current turn. Trigger: An old child rollout of a resumed thread has its head timestamp stripped and its mtime refreshed during the current turn. Evidence: read-code: Head timestamp at/after launch is required, but a head with no readable timestamp is still decided by mtime slack alone. Verify: Unit-test subagent attribution with a timestamp-less old child modified now and check whether it is attributed. Remedy: Treat a timestamp-less head as unproven rather than falling back to mtime, or record the ambiguity in the smoke/parent-call verdict.

### Prior findings

- F02-1 - not-checked - Outside this part; not reviewed.
- F02-2 - not-checked - Outside this part; not reviewed.
- F02-3 - not-checked - Outside this part; not reviewed.
- F02-4 - not-checked - Outside this part; not reviewed.
- F02-5 - not-checked - Outside this part; not reviewed.
- F02-6 - not-checked - Outside this part; not reviewed.
- F02-7 - not-checked - Outside this part; not reviewed.
- F02-8 - not-checked - Outside this part; not reviewed.
- F02-9 - not-checked - Outside this part; not reviewed.
- F02-10 - not-checked - Outside this part; not reviewed.
- F02-11 - not-checked - Outside this part; not reviewed.
- F02-12 - not-checked - Outside this part; not reviewed.
- F02-13 - not-checked - Outside this part; not reviewed.
- F02-14 - not-checked - Outside this part; not reviewed.
- F02-15 - not-checked - Outside this part; not reviewed.
- F02-16 - not-checked - Outside this part; not reviewed.
- F02-17 - not-checked - Outside this part; not reviewed.
- F02-18 - not-checked - Outside this part; not reviewed.
- F02-19 - not-checked - Outside this part; not reviewed.
- F02-20 - not-checked - Outside this part; not reviewed.
- F02-21 - not-checked - Outside this part; not reviewed.
- F03-1 - not-checked - Outside this part; not reviewed.
- F03-2 - not-checked - Outside this part; not reviewed.
- F03-3 - not-checked - Outside this part; not reviewed.
- F03-4 - not-checked - Outside this part; not reviewed.
- F03-5 - not-checked - Outside this part; not reviewed.
- F03-6 - not-checked - Outside this part; not reviewed.
- F04-1 - not-checked - Outside this part; not reviewed.
- F04-2 - not-checked - Outside this part; not reviewed.
- F04-3 - not-checked - Outside this part; not reviewed.
- F04-4 - not-checked - Outside this part; not reviewed.
- F04-5 - not-checked - Outside this part; not reviewed.
- F04-6 - not-checked - Outside this part; not reviewed.
- F04-7 - not-checked - Outside this part; not reviewed.
- F04-8 - not-checked - Outside this part; not reviewed.
- F04-9 - not-checked - Outside this part; not reviewed.
- F10-1 - fixed - Merged params, unlisted tool writes, Cascade readers allow-listed; fake-agy tests updated.
- F10-2 - fixed - Bound-input drift path covered by empty-ACTIVE merge fix.
- F10-3 - fixed - Unreadable HEAD brief materialised from HEAD; staging failure is typed pre-start refusal.
- F10-4 - fixed - Frozen per-member record; prompt and prior-blocker use it.
- F10-5 - fixed - Unproven held like failure; any-namespace tool counting.
- F10-7 - fixed - Stream persisted per turn; reader uses persisted stream then newest run events.
- F11-1 - fixed - All-event param merge; command-in-value counts as writing; unattributed warning.
- F11-2 - fixed - Bound brief/artifact change fails outcome as permission, reply kept; new test.
- F11-3 - fixed - Revision-tree plus on-disk link checks, exclusive temp plus pre-rename re-check.
- F11-4 - fixed - Dot-dot normalised, HEAD materialisation, typed refusal with reserve re-seat.
- F11-5 - fixed - Only tracked HEAD paths named as data; untracked counted.
- F11-6 - fixed - Immutable member record; unknown ids rejected.
- F11-7 - fixed - Unproven on missing thread/ledger/opt-in/unreadable rollout; rollback and no verify.
- F11-8 - fixed - Absolute Codex home, session-id, tool-array, terminal-record and retry checks.
- F11-9 - fixed - Post-exit re-inspection plus creation-timestamp attribution; bare head still slack (finding #3).
- F11-10 - fixed - Reviewer sentence on continuation, format-repair and read-insist turns.
- F11-11 - fixed - Per-turn stream notes, own-run filtering, persisted fallback.
- F11-12 - fixed - reply_kept sidecar/partial/none recorded and counted.
- F12-1 - fixed - Cannot-check liveness left alone; owner record mandatory; 6h rule gone.
- F12-2 - fixed - rev-parse now under GIT_OPTIONAL_LOCKS=0; sentence literal.
- F12-3 - fixed - Non-link reparse point worded as reparse point.
- F12-4 - fixed - Re-read guard documented with remaining window (finding #1).
- F13-1 - fixed - symlink_metadata classification; link reported, never removed.
- F13-2 - fixed - HEAD/gitdir strict root test plus GIT_DIR refusal.
- F13-3 - fixed - YYYY-MM-DD required; foreign note kept byte-for-byte.
- F13-4 - fixed - update children marked; branches set/clear/default refuse in child.
- F13-5 - fixed - Registered git children killed and reaped on deadline.
- F13-6 - fixed - Dated-older successor refused; undated still version-only (finding #2).
- F13-7 - fixed - Failed cherry/rev-list stays unchecked and counted.
- F13-8 - fixed - No-owner worktree left alone until prunable; owner write failure fails run.
- F14-1 - not-checked - Outside this part; not reviewed.
- F14-2 - not-checked - Outside this part; not reviewed.
- F14-3 - not-checked - Outside this part; not reviewed.
- F14-4 - not-checked - Outside this part; not reviewed.
- F14-5 - not-checked - Outside this part; not reviewed.
- F15-1 - not-checked - Outside this part; not reviewed.
- F15-2 - not-checked - Outside this part; not reviewed.
- F15-3 - not-checked - Outside this part; not reviewed.
- F15-5 - not-checked - Outside this part; not reviewed.
- F15-6 - not-checked - Outside this part; not reviewed.
- F16-1 - not-checked - Outside this part; not reviewed.
- F16-2 - not-checked - Outside this part; not reviewed.
- F16-3 - not-checked - Outside this part; not reviewed.
- F16-4 - not-checked - Outside this part; not reviewed.
- F16-6 - not-checked - Outside this part; not reviewed.
- F16-7 - not-checked - Outside this part; not reviewed.
- F16-8 - not-checked - Outside this part; not reviewed.
- F16-9 - not-checked - Outside this part; not reviewed.
- F16-10 - not-checked - Outside this part; not reviewed.
- F17-1 - not-checked - Outside this part; not reviewed.
- F17-2 - not-checked - Outside this part; not reviewed.
- F17-3 - not-checked - Outside this part; not reviewed.
- F17-4 - not-checked - Outside this part; not reviewed.
- F17-5 - not-checked - Outside this part; not reviewed.
- F17-6 - not-checked - Outside this part; not reviewed.
- F17-7 - not-checked - Outside this part; not reviewed.
- F17-8 - not-checked - Outside this part; not reviewed.
- F17-9 - not-checked - Outside this part; not reviewed.
- F17-10 - not-checked - Outside this part; not reviewed.
- F18-1 - not-checked - Outside this part; not reviewed.
- F18-2 - not-checked - Outside this part; not reviewed.
- F18-3 - not-checked - Outside this part; not reviewed.
- F18-4 - not-checked - Outside this part; not reviewed.
- F18-5 - not-checked - Outside this part; not reviewed.
- F18-6 - not-checked - Outside this part; not reviewed.
- F18-7 - not-checked - Outside this part; not reviewed.
- F19-1 - not-checked - Outside this part; not reviewed.
- F19-2 - not-checked - Outside this part; not reviewed.
- F19-3 - not-checked - Outside this part; not reviewed.
- F19-4 - not-checked - Outside this part; not reviewed.
- F19-5 - not-checked - Outside this part; not reviewed.
- F19-6 - not-checked - Outside this part; not reviewed.
- F20-1 - not-checked - Outside this part; not reviewed.
- F20-2 - not-checked - Outside this part; not reviewed.
- F20-3 - not-checked - Outside this part; not reviewed.
- F20-4 - not-checked - Outside this part; not reviewed.
- F20-5 - not-checked - Outside this part; not reviewed.
- F20-6 - not-checked - Outside this part; not reviewed.

## Verdict: ACCEPT

All in-scope triggers are closed or narrowed to explicitly documented boundaries; residuals are minor/notes, not acceptance blockers.

**WARNING: ACCEPT with 7 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F19-1).**

### Blockers

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

- Whether the hook kills grandchildren spawned by git hooks on all platforms.
- Whether every smoke consumer outside maintain/mod.rs treats Unproven as hold.
- Concurrent --diff patch sharing across panel members after the rename.

### First-run checklist (observable)

_(none)_
