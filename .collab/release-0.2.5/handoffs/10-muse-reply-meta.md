# Handoff 10 - Meta Muse (muse): reply-meta

Date: 2026-11-22 23:54 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: a2376e5b-9683-42ad-8b50-cac5a3332385.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-17ecfc7a824b45e2aba8247c040688a2.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-83e8a54f5da14f4d8d0616870f58e36d` of revision a115422f2d4f17cb2bd6c2c84b7ad7235cb5f77d (--rev a115422f2d4f17cb2bd6c2c84b7ad7235cb5f77d; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20570-ced9-7403-abeb-cbdb47a1e0b7` (source: events).
Brief: `.collab/release-0.2.5/handoffs/05-claude-diff-review-reliability.md` (sha256 3449d5673caa). Reviewed: e4f42a8, base e4f42a8da3b138e4061a9f8f76d977b1117ecadf, tree sha256 ca80d2a65896, 0 changed files.
Bridge outcome: usable reply. Wall time: 373.3 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-83e8a54f5da14f4d8d0616870f58e36d\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The patch improves reliability but leaves fail-open agy classification holes, a misclassified kimi seat failure, snapshot status races, and smoke/guard bypass paths. Findings: 0 blocker, 6 major, 1 minor, 0 note (F10-1..F10-7, tracked in `findings.json`). Structured reply: `handoffs/10-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/10-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

Q1. agy outcome: the classifier is better than a name list but still wrong both ways. A write whose top-level keys lack the nine marker fragments (nested `{config:{path}}`, a command in a value, fixed-path writers) is judged a reader, so a real write with a tree change warns as 'reply kept' (finding 1). Conversely the ACTIVE/DONE merge keeps the first parameters seen, so a parameter-less ACTIVE event fails a run that only read, throwing away a complete reply (finding 2). A corrupted-in-place reply file is not covered either: own handoff files are excluded from the collab compare. Q2. kimi seat: `stageable_brief_rel` blocks only `..`; symlinks/junctions and absolute paths are not specially handled, and a HEAD-committed but live-unreadable brief passes preflight then fails at staging as a non-preflight capability failure the panel will not re-seat, while the prompt still claims the brief was copied in (finding 3). The refusal wording is preflight-class, but the staging error path is not. Q3. Snapshot and races: `run_open_priors` narrows ids to the shown snapshot yet reads live statuses, so a sibling's mid-panel status flip still changes a member's verdict; concurrent findings writes can last-writer-win (finding 4). Q4. Smoke proof: version/tool-shape changes fail sound updates (fail-safe), but the smoke passes with a real surface via the `mcp__` exemption or an all-opted-in seat; rollout timing races can also flake a sound update (finding 5). Rollback wiring itself was not readable in this pass. Q5. Guard and flags: the launch boundary fixes the F13-1 cases, but a copied-history fork without a `task_started` line falls back to scanning from 0 and false-stops; the two new `-c` keys ship unconditionally on an ignorability claim with no older-CLI test (finding 6); the coordinator sentence is prompt text only and a global AGENTS.md can still outrank it in the reviewer's eyes. Q6. Truthful reporting: death fallback and wall folding work for single detached runs (panel members excluded, finding 7); `reply_outcomes` counts are metadata-only, no new exfiltration seen. CHANGELOG reads true except 'ledger entry and scoreboard mark are written' for reply-kept, which I could not trace read-only. Q7. Untested: no live agy/kimi/grok A1 run, no real fork against the guard, no rollback run, no older-CLI run, no concurrent-writer or Windows rollout test is on record.

---

### Findings

- **F10-1** [major] `crates/c3/src/engines/tree_check.rs:201`, `crates/c3/src/engines/tree_check.rs:219` - The agy step classifier misses real writes whose parameter keys lack the nine marker fragments: only top-level keys are tested, nested objects are ignored, and a command carried in a value (or a writer with fixed-path parameters such as action/destination/text) is judged a reader, so a real write with a tree change becomes a 'reply kept' warning instead of a failure. Trigger: An agy tool step with parameters like {"action": "rm -rf x"}, {"config": {"path": "a.rs"}}, or a fixed-target writer taking only text/content. Evidence: read-code: agy_tool_step_is_writing returns non-writing when parameters are an object with no top-level key containing a marker; markers are only path/file/dir/folder/cwd/command/cmd/target/uri and only obj.keys() are tested.; read-code: engine_tree_outcome fails only on non-empty agy.writing; otherwise a complete reply warns, so a missed write directly flips fail to warned. Verify: Unit-test agy_tool_step_is_writing with nested-path params and a command-in-value step and assert both count as writing. Remedy: Also inspect nested parameter objects and well-known value shapes, or treat unknown tool names with object parameters as writing unless the name is an allow-listed reader; add fixtures with nested-path and value-embedded-command steps.
- **F10-2** [major] `crates/c3/src/engines/agy.rs:470` - The ACTIVE/DONE merge keeps the first logged parameters object per (index, name) and an unnamed or parameter-less ACTIVE step counts as writing, so a read-only step whose DONE event carries the parameters (or a renamed/re-indexed continuation) fails a run that actually only read, discarding a complete reply. Trigger: Agy logs a read_file ACTIVE event without tool_info.parameters and the DONE event with them, or logs the same logical step under a different name or without step_index. Evidence: read-code: Steps merge only on matching index and name and keep the first parameters seen; a step whose parameters were not logged is writing, and an unnamed step is kept with an empty name that the classifier counts as writing.; read-code: Writing steps are collected from merged steps and any non-empty writing list fails the run even when the monitored compare is clean. Verify: Feed a two-event read step (parameter-less ACTIVE, parameterized DONE) to read_agy_events and assert writing_tool_names stays empty. Remedy: Merge parameters across all events of a step (prefer any object seen, not the first), and treat a parameter-less ACTIVE event as unknown only until its DONE/ERROR event is seen.
- **F10-3** [major] `crates/c3/src/engines/kimi.rs:297`, `crates/c3/src/engines/kimi.rs:386`, `crates/c3/src/engines/kimi.rs:508` - A brief committed at HEAD but unreadable in the live tree passes the preflight yet fails later at staging as a capability ProviderFailure, which the panel does not re-seat; meanwhile the isolation paragraph unconditionally claims the brief was copied whenever the ref is stageable. Trigger: Brief committed at HEAD whose live file is deleted, locked, or unreadable; or staging fails (read-only worktree parent, disk error) while the prompt still says it was copied in. Evidence: read-code: isolated_brief_refusal returns None when cat-file HEAD:ref succeeds even if the live source cannot be opened, so the seat launches.; read-code: Staging copies the live brief.src and on error returns a capability ProviderFailure, not a preflight refusal; the prompt line appends the copied-in sentence for every stageable ref without checking staging. Verify: Delete the live brief while it stays committed at HEAD, launch a kimi seat, and check whether the panel re-seats it and what the prompt claims. Remedy: Refuse at preflight only when the seat can actually proceed (fall back to materializing the brief from HEAD), or make the staging failure a preflight-class refusal; gate the copied-in sentence on staging success.
- **F10-4** [major] `crates/c3/src/consult/orchestrate/mod.rs:4391` - The pre-panel snapshot narrows finding ids but still reads every finding's live status, so a sibling that changes a shown finding's status mid-panel changes the member's verdict; concurrent findings-store writes from two members or two c3 processes can also last-writer-win. Trigger: A sibling transitions F02-1 proposed->fixed while a member judged against the pre-panel snapshot is still running, or two members commit findings concurrently. Evidence: read-code: run_open_priors filters by listed ids but takes status from the store as it stands (fresh status), per its own doc comment.; inferred: No locking or merge of the findings store is introduced by this patch; the snapshot is an id list, not a status/version snapshot. Verify: Flip a listed finding's status mid-panel in a fixture and assert the running member's prior-blocker disposition still uses the shown status. Remedy: Snapshot (id, status) pairs per member at panel start and judge against those; serialize findings-store updates with file locking or read-modify-write retries.
- **F10-5** [major] `crates/c3-core/src/quota.rs:877`, `crates/c3/src/maintain/smoke.rs:350` - The smoke can pass while a sub-agent surface exists: tools under any mcp__ namespace are exempt even when named exactly like multi-agent tools, an opted-in seat passes without exercising the switch, and a future CLI using a new namespace or new session_meta field names yields 'unproven' failure on sound updates but any new tool name inside the collaboration namespace is the only future-proofed case. Trigger: A CLI serving spawn_agent through an mcp__ namespace, or a smoke roster whose codex entries all set codex_config to allow sub-agents. Evidence: read-code: is_multi_agent_tool returns false for any namespace starting with mcp__, so collaboration tools wrapped as MCP are neither offered nor called.; read-code: Consults with subagents_allowed true are skipped, and all-opted-in yields Pass with only a stderr note. Verify: Craft a rollout offering collaboration.spawn_agent under namespace mcp__srv and assert subagent_switch_check fails. Remedy: Count exact multi-agent tool names regardless of namespace, fail (not pass) when every consult opted in, and pin the tested CLI version's rollout shape in the failure message.
- **F10-6** [major] `crates/c3/src/consult/orchestrate/mod.rs:194`, `crates/c3-core/src/engine.rs:442` - Every codex review turn now unconditionally passes two new -c keys on the claim that an older CLI ignores unknown keys, with no version gate or fallback; if any supported older CLI rejects unknown -c keys, all codex reviews fail. Trigger: Run a codex review turn on an older codex-cli that errors on unknown -c keys. Evidence: read-code: bridge_config_for appends REVIEW_ISOLATION for every codex review turn minus only user-named keys; no CLI-version check exists.; read-code: The ignorability claim for older CLIs lives only in a comment on REVIEW_ISOLATION; no test exercises an older CLI. Verify: Run bridge_config_for against the oldest supported codex-cli and confirm the turn starts. Remedy: Probe the CLI's supported keys once per version (or retry a rejected turn without the new keys) and document the oldest CLI actually tested.
- **F10-7** [minor] `crates/c3/src/consult/orchestrate/mod.rs:6050` - The death-record fallback covers only single detached runs: panel members never note their stream, and a reader of a dead run passes noted: None, so a panel member that dies in a read-insist re-ask still gets empty events_file/last_event/events_mtime/partial_hint. Trigger: A detached panel whose member dies in its read-insist re-ask after the commit removed the pending record. Evidence: read-code: note_engine_stream is called only when detach is active and panel_member is none; the reader path passes noted: None. Verify: Kill a panel member in its read-insist turn and inspect the reader-written death record's events fields. Remedy: Note the stream for panel members as well (keyed by member) or have the reader fall back to the newest matching events file for the dead turn.

### Prior findings

- F02-1 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-2 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-3 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-4 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-5 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-6 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-7 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-8 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-9 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-10 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-11 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-12 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-13 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-14 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-15 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-16 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-17 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-18 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-19 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-20 - still-open - Out of this range (fixes: none); untouched by this patch.
- F02-21 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-1 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-2 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-3 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-4 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-5 - still-open - Out of this range (fixes: none); untouched by this patch.
- F03-6 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-1 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-2 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-3 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-4 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-5 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-6 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-7 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-8 - still-open - Out of this range (fixes: none); untouched by this patch.
- F04-9 - still-open - Out of this range (fixes: none); untouched by this patch.

## Verdict: HOLD

The patch improves reliability but leaves fail-open agy classification holes, a misclassified kimi seat failure, snapshot status races, and smoke/guard bypass paths.

### Blockers

- **F02-1** (prior, still-open) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, still-open) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, still-open) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, still-open) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, still-open) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, still-open) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.

### Unproven scenarios

- Whether 'reply kept' warned runs actually write the ledger entry and scoreboard mark (finish path not fully traced read-only).
- Whether a smoke failure always reaches and completes the update rollback, and whether rollback can loop or be skipped (--dry-run, partial recording).
- Whether a real Codex update keeps, moves, or renames multi_agent_version/tool lists (future-shape robustness).
- Windows rollout path, line endings, and file-locking behavior under live smoke/guard.
- Whether an older codex-cli ignores or rejects the two new -c keys.

### First-run checklist (observable)

_(none)_
