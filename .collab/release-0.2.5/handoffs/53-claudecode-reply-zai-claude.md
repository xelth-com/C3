# Handoff 53 - Claude (claude): reply-zai-claude

Date: 2026-11-24 09:40 local. Author: Claude (claude) (model glm-5.3, effort high), claude-cli 2.1.331.
Reviewer: ZAI-claude :: glm-5.3 [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://api.z.ai/api/anthropic (token from env ZAI_API_KEY); provider fingerprint 944a94557723; harness claude-cli 2.1.331).
Preflight: ok: env ZAI_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 5a8a70c2-ab31-4987-8b16-23d624002abc.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model glm-5.3 --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id 3f52d842-81f9-47eb-a70e-bb50ed6bed03` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-96133c4b1139405aa54f899bc38c78e6` of revision 2ffa02aa435c0942ffb9cba505eef58d358a58a1 (--rev 2ffa02aa435c0942ffb9cba505eef58d358a58a1; removed after the run).
Parent thread: (none - new thread). Result thread: `3f52d842-81f9-47eb-a70e-bb50ed6bed03` (source: events).
Brief: `.collab/release-0.2.5/handoffs/50-claude-diff-review-bridge-debt.md` (sha256 22b36ab257c1). Reviewed: 2ffa02a, base 2ffa02aa435c0942ffb9cba505eef58d358a58a1, tree sha256 946c1867ea9b, 0 changed files. Live HEAD at launch: 2ffa02aa435c0942ffb9cba505eef58d358a58a1.
Bridge outcome: usable reply. Wall time: 689.5 s. Tokens: in 4980568 (cached 4807168), out 51059, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model glm-5.3; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: ACCEPT with 11 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1, F32-1)..
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: ACCEPT - The commit soundly closes the 24 claimed debt items (verified in the patch and the code at 2ffa02a); the residual defects I found are minor prompt-accuracy, disclosure and narrow-race issues, none of which damages a machine, a repository or a consultation record. Findings: 0 blocker, 0 major, 4 minor, 3 note (F53-1..F53-7, tracked in `findings.json`). Structured reply: `handoffs/53-claudecode-reply-zai-claude.reply.json`.
Raw event stream: `handoffs/53-claudecode-reply-zai-claude.events.jsonl`.
Verbatim reply follows.

---

**Q1.** No false withhold from the bridge's own writes or a sibling's patch: the reply prefix and `panel_ignore_prefixes` (`<task>/handoffs/<nn>-`, `tree_check.rs:435`) cover siblings' patches; `compare_collab` compares content hashes, so mtime-only changes are invisible. Two residual paths: a single run whose findings-store read fails at its start snapshots zero priors (`shown_priors`, `mod.rs:2694`) and its reply's prior report is then rejected at finish although the store was readable by then (finding 3); and kimi/claude-HEAD worktree reviewers with a clean-except-collab tree lose the whole isolation paragraph (finding 1). No still-usable-after-change gap found: `bound_inputs_withhold` covers every live-tree engine plus codex's finish re-hash; http reads only its pack. An A→B→A brief edit mid-run is invisible to any end-hash (unproven).

**Q2.** The recursive merge and key roles close F24-4/F32-3; nested keys are re-judged by their own names, unknown tools stay writing, arrays take the key's role. Remaining: any value under a key ending in path/file/dir/uri is never scanned — including a second conflicting value the merge keeps as an array under that key (`{"Path":"a.md"}` then `{"Path":"del b.rs"}`), so a write executed through a path-keyed parameter of an allow-listed reader is classified as a reader (finding 2, minor).

**Q3.** The final name never holds partial bytes (claim-then-rename is atomic), and same-name implies the same sha8 prefix, so a 60 s takeover of a live-but-stalled owner's claim ends in a byte-identical replace. Residuals: `drop_own_claim` checks identity/size and then removes by name, so a sibling completing the name in that microsecond gap loses its patch while its ledger says placed (finding 6); a hard crash leaves the handoffs tmp file and the OS-temp `c3-diff-index-*` index (and its `.lock`) unreaped (finding 4). The temp index never touches the real one: every index-writing git call passes `GIT_INDEX_FILE=tmp` and `git_cmd` sets `GIT_OPTIONAL_LOCKS=0` (`reviewed.rs:749`, `worktree.rs:34`); the test asserts the real index and its skip-worktree flags are untouched.

**Q4.** Ignored/excluded content cannot reach the patch (`check-ignore` + `hide_in_index` on the temp index). Untracked content still does by design and is now disclosed: the ten names filter backticks and control characters, but bidi/zero-width format characters are not `is_control()` (finding 5). `--diff-tracked-only` reaches panels (`build_spec`), detached runs (`DetachArgs`) and members — but not MCP callers, and in detached/MCP runs the stderr notice lands in a log, not the operator's terminal (finding 7).

**Q5.** One HEAD resolution (`mod.rs:2712`) decides withholding, `live_head` and the unresolved marker; the handoff and ledger say what was read, and a revision-sourced brief is hashed from the committed bytes with "Taken from the committed revision". No wrong record found; a HEAD that moves after launch is still labelled "at launch", which is true.

**Q6.** `explain` is now pure: skip-preflight election, `no_network`, cache-only `multipliers_for`/`plan_windows`/`entry_reserve_hold`/`member_inputs` (all `network=false` branches verified), PATH-scan launcher, records-only `recorded_availability` — no process, network or writes. `replay` writes nothing but can now exit 1 on a record whose draw block has an invalid row that it previously replayed clean — intended strictness, reported with the reason.

**Q7.** Untested beyond the author's list (http+--rev e2e, live-tree bound-input against real agy/muse/grok CLIs, real no-hardlink FS): `hide_in_index` on a SHA-256 repository or gitlink/submodule hidden paths; the takeover and `drop_own_claim` races on a genuinely stalled filesystem; the clean-tree isolation regression — no test asserts the paragraph survives when the only uncommitted paths are collab paths (the updated kimi test asserts the opposite direction).

---

### Findings

- **F53-1** [minor] `crates/c3/src/engines/worktree.rs:1168`, `crates/c3/src/consult/orchestrate/mod.rs:2878` - A worktree reviewer whose live tree is clean outside the collab directory now gets no isolation paragraph at all: the notice that its working directory is a throwaway worktree of HEAD (not the live checkout) and that the brief was copied in disappear, because the collab paths that used to make the paragraph non-empty are skipped. Trigger: A kimi or claude diff-review/acceptance run in a repository whose only uncommitted or untracked paths are under the collab directory (the common case: untracked .collab or a modified tracked state.md). Evidence: read-code: isolation_prompt returns String::new() when uncommitted_paths_outside is empty; before this commit the untracked .collab directory made new_paths >= 1 so the paragraph always printed.; read-code: the updated test asserts the paragraph no longer mentions 'path(s) that are not in HEAD' when only .collab was untracked, and no test covers the fully-clean case. Verify: Run a fake-kimi consult in a clean scratch repo whose only untracked path is .collab/ and check whether the prompt contains the 'Isolation: your working directory is a throwaway git worktree' sentence. Remedy: Print the isolation sentence (worktree-of-HEAD and brief-copied) whenever a throwaway worktree of HEAD is used, regardless of the uncommitted-path counts; keep the collab skip only for the path list and counts.
- **F53-2** [minor] `crates/c3/src/engines/tree_check.rs:265`, `crates/c3/src/engines/agy.rs:429` - A command-shaped value under a key whose folded name ends in a path/file/dir/uri word is never scanned, including a second conflicting value that the recursive merge keeps as an array under that key, so a writing step executed through a path-keyed parameter of an allow-listed reader is classified as a reader. Trigger: An agy read_file step logs ACTIVE parameters {"Path":"a.md"} and DONE parameters {"Path":"del b.rs"} (or any command string under a key ending in file/path/dir/uri) while the tree changes. Evidence: read-code: agy_param_role maps any key ending in an AGY_DATA_PARAM_SUFFIXES word to Data, and walk only scans strings whose role is Scanned.; read-code: merge_parameters keeps two different non-empty scalars as an array under the same key, so the conflicting value inherits the key's Data role.; read-code: write markers are checked before data suffixes, so keys like OverwriteFile still write; the hole is only a command value under a data-suffixed key. Verify: Add the table row ("read_file", [("ACTIVE", "{\"Path\":\"a.md\"}"), ("DONE", "{\"Path\":\"del b.rs\"}")], false) to agy_steps_merge_at_every_depth and observe the classifier reports the step as not writing. Remedy: For an allow-listed reader, scan command-shaped strings under a data-role key when they arrive as conflicting merged values (arrays with 2+ entries) while keeping single values data, or at minimum count a merged conflicting value under a path key as writing.
- **F53-3** [minor] `crates/c3/src/consult/orchestrate/mod.rs:2694` - A single run whose findings-store read fails at its start (file locked or unreadable) takes an empty shown_priors snapshot, so a reply that reports a real prior finding is rejected at finish as not reportable, although the store was readable by then; before this change the finish re-read the live store. Trigger: The task's findings.json is transiently unreadable exactly when build_context reads it (another process holding it, antivirus), and the reviewer's reply names an open prior finding. Evidence: read-code: shown_priors for a single run is store.read_findings(&task).ok().flatten().map(listed_findings).unwrap_or_default() - a read error becomes an empty immutable snapshot.; read-code: member_prior_view now judges reportable ids against ctx.shown_priors, never a finish-time store read. Verify: Make findings.json unreadable (exclusive open) only during build_context of a scratch single run whose reply reports an open finding, and observe the reply fail prior-report validation. Remedy: Distinguish a failed store read from an absent store: on read error, refuse the run's start (or fall back to the finish-time live read for a single run) instead of freezing an empty snapshot.
- **F53-4** [note] `crates/c3/src/consult/reviewed.rs:1033`, `crates/c3/src/consult/reviewed.rs:1038` - A hard crash leaves debris that nothing reaps: the temporary index c3-diff-index-<uuid> (and its .lock) stays in the system temp directory, and a crash between the tmp write and the hard link leaves a full <stem>.<sha8>.<uuid>.tmp file in the task's handoffs (remove_temp_index and remove_tmp run only on handled error paths). Trigger: The process is killed (or the machine crashes) during render_patch_with, or between the temp-file write and the placement; no sweep exists for either location. Evidence: read-code: remove_temp_index/remove_tmp are called only on the function's normal and error returns; a killed process skips both, and no later run removes foreign c3-diff-index-* or *.tmp names. Verify: Kill a --diff run mid render and list %TEMP%/c3-diff-index-* and the task handoffs/*.tmp afterwards. Remedy: Reap c3-diff-index-* files older than a day in the OS temp dir at the start of a --diff render, and reap stale <uuid>.tmp files under the task's handoffs when placing a patch.
- **F53-5** [note] `crates/c3/src/consult/reviewed.rs:1540` - A printed or ledger-recorded untracked file name may contain non-control format characters (bidi overrides such as U+202E, zero-width joiners): is_control() is false for category Cf, so the name is shown on stderr in backticks and stored in the ledger's untracked list, allowing visual spoofing. Trigger: An untracked file whose name carries a bidi override or zero-width character is carried by a one-revision --diff patch. Evidence: read-code: untracked_named filters only chars equal to '`' or is_control(); Rust's char::is_control covers Cc only, not Cf. Verify: Create an untracked file named with a leading U+202E and run --diff HEAD; observe the name printed verbatim in the notice and stored in the ledger. Remedy: Also withhold names containing category-Cf characters (count them instead), matching the CI-title treatment of earlier waves.
- **F53-6** [note] `crates/c3/src/consult/reviewed.rs:1224` - drop_own_claim reads the file's identity and size on a handle and then removes by name, so a sibling that completes the patch name in the window between the check and the remove_file loses its finished patch while its ledger records it as placed. Trigger: Two concurrent no-hardlink placements on the same patch name where this call's rename fails and a sibling finishes the name in the microseconds between file_state and remove_file. Evidence: read-code: the identity+size check is immediately followed by dir.remove_file(leaf) by name; the removal is not tied to the checked handle.; inferred: the same name implies the same sha8 prefix, so the removed file would generically hold identical bytes; the composite race is extremely narrow. Verify: Interleave a sibling rename exactly between drop_own_claim's file_state and remove_file (race-point hook) and confirm the sibling's completed patch survives. Remedy: Remove the claim through the handle it was checked on (open, verify size 0, delete via the handle where the platform allows) or re-check size after acquiring the right to remove.
- **F53-7** [minor] `crates/c3/src/mcp/mod.rs:318`, `crates/c3/src/consult/reviewed.rs:1564` - The untracked-content disclosure does not reach MCP callers or detached-run operators: the MCP surface has no diff_tracked_only parameter, and the untracked notice is printed on the run process's stderr, which for a detached run or an MCP server is a log file rather than the operator's terminal - so an MCP-triggered one-revision diff still ships untracked file content with no operator-visible notice. Trigger: A consult started through the MCP tool with diff set to a single revision while the checkout holds untracked non-ignored files. Evidence: read-code: the MCP consult tool accepts diff (syntax-checked) but exposes no tracked-only option.; read-code: the notice is emitted with eprintln in prepare_review_inputs, i.e. on whatever process runs the bridge. Verify: Call the MCP consult tool with a one-revision diff in a scratch repo with an untracked file and inspect where the notice appears and whether the patch bytes contain the file. Remedy: Add a diff_tracked_only field to the MCP consult tool's input, and surface the untracked count in the MCP tool's reply text (or the detached run's status) in addition to stderr.

### Prior findings

- F02-1 - not-checked - outside this commit's delta; not re-verified
- F02-2 - not-checked - outside this commit's delta; not re-verified
- F02-3 - not-checked - outside this commit's delta; not re-verified
- F02-4 - not-checked - outside this commit's delta; not re-verified
- F02-5 - not-checked - outside this commit's delta; not re-verified
- F02-6 - not-checked - outside this commit's delta; not re-verified
- F02-7 - not-checked - outside this commit's delta; not re-verified
- F02-8 - not-checked - outside this commit's delta; not re-verified
- F02-9 - not-checked - outside this commit's delta; not re-verified
- F02-10 - not-checked - outside this commit's delta; not re-verified
- F02-11 - not-checked - outside this commit's delta; not re-verified
- F02-12 - not-checked - outside this commit's delta; not re-verified
- F02-13 - not-checked - outside this commit's delta; not re-verified
- F02-14 - not-checked - outside this commit's delta; not re-verified
- F02-15 - not-checked - outside this commit's delta; not re-verified
- F02-16 - not-checked - outside this commit's delta; not re-verified
- F02-17 - not-checked - outside this commit's delta; not re-verified
- F02-18 - not-checked - outside this commit's delta; not re-verified
- F02-19 - not-checked - outside this commit's delta; not re-verified
- F02-20 - not-checked - outside this commit's delta; not re-verified
- F02-21 - not-checked - outside this commit's delta; not re-verified
- F03-1 - not-checked - outside this commit's delta; not re-verified
- F03-2 - not-checked - outside this commit's delta; not re-verified
- F03-3 - not-checked - outside this commit's delta; not re-verified
- F03-4 - not-checked - outside this commit's delta; not re-verified
- F03-5 - not-checked - outside this commit's delta; not re-verified
- F03-6 - not-checked - outside this commit's delta; not re-verified
- F04-1 - not-checked - outside this commit's delta; not re-verified
- F04-2 - not-checked - outside this commit's delta; not re-verified
- F04-3 - not-checked - outside this commit's delta; not re-verified
- F04-4 - not-checked - outside this commit's delta; not re-verified
- F04-5 - not-checked - outside this commit's delta; not re-verified
- F04-6 - not-checked - outside this commit's delta; not re-verified
- F04-7 - not-checked - outside this commit's delta; not re-verified
- F04-8 - not-checked - outside this commit's delta; not re-verified
- F04-9 - not-checked - outside this commit's delta; not re-verified
- F10-1 - not-checked - outside this commit's delta; not re-verified
- F10-2 - not-checked - outside this commit's delta; not re-verified
- F10-3 - fixed - committed_brief accepted in build_context and materialised from HEAD/rev; handoff says 'Taken from the committed revision' (render.rs)
- F10-4 - not-checked - outside this commit's delta; not re-verified
- F10-5 - not-checked - outside this commit's delta; not re-verified
- F10-7 - fixed - noted_stream_wins prefers the newer events file (detach.rs:1375)
- F11-1 - not-checked - outside this commit's delta; not re-verified
- F11-2 - not-checked - outside this commit's delta; not re-verified
- F11-3 - not-checked - outside this commit's delta; not re-verified
- F11-4 - fixed - brief committed at HEAD or the reviewed revision accepted when a worktree is planned; committed bytes hashed
- F11-5 - not-checked - outside this commit's delta; not re-verified
- F11-6 - not-checked - outside this commit's delta; not re-verified
- F11-7 - not-checked - outside this commit's delta; not re-verified
- F11-8 - not-checked - outside this commit's delta; not re-verified
- F11-9 - not-checked - outside this commit's delta; not re-verified
- F11-10 - not-checked - outside this commit's delta; not re-verified
- F11-11 - fixed - covered by noted_stream_wins (F24-2)
- F11-12 - fixed - KeptReplyFiles counts a kept reply as failed_reply_gone when its file is gone (providers.rs)
- F12-1 - not-checked - outside this commit's delta; not re-verified
- F12-2 - not-checked - outside this commit's delta; not re-verified
- F12-3 - not-checked - outside this commit's delta; not re-verified
- F12-4 - not-checked - outside this commit's delta; not re-verified
- F13-1 - not-checked - outside this commit's delta; not re-verified
- F13-2 - not-checked - outside this commit's delta; not re-verified
- F13-3 - not-checked - outside this commit's delta; not re-verified
- F13-4 - not-checked - outside this commit's delta; not re-verified
- F13-5 - not-checked - outside this commit's delta; not re-verified
- F13-6 - not-checked - outside this commit's delta; not re-verified
- F13-7 - not-checked - outside this commit's delta; not re-verified
- F13-8 - not-checked - outside this commit's delta; not re-verified
- F13-9 - not-checked - outside this commit's delta; not re-verified
- F14-1 - not-checked - outside this commit's delta; not re-verified
- F14-2 - not-checked - outside this commit's delta; not re-verified
- F14-3 - not-checked - outside this commit's delta; not re-verified
- F14-4 - not-checked - outside this commit's delta; not re-verified
- F14-5 - not-checked - outside this commit's delta; not re-verified
- F15-1 - not-checked - outside this commit's delta; not re-verified
- F15-2 - not-checked - outside this commit's delta; not re-verified
- F15-3 - not-checked - outside this commit's delta; not re-verified
- F15-5 - not-checked - outside this commit's delta; not re-verified
- F15-6 - not-checked - outside this commit's delta; not re-verified
- F16-1 - not-checked - outside this commit's delta; not re-verified
- F16-2 - not-checked - outside this commit's delta; not re-verified
- F16-3 - not-checked - outside this commit's delta; not re-verified
- F16-4 - not-checked - outside this commit's delta; not re-verified
- F16-5 - not-checked - outside this commit's delta; not re-verified
- F16-6 - not-checked - outside this commit's delta; not re-verified
- F16-7 - not-checked - outside this commit's delta; not re-verified
- F16-8 - not-checked - outside this commit's delta; not re-verified
- F16-9 - not-checked - outside this commit's delta; not re-verified
- F16-10 - not-checked - outside this commit's delta; not re-verified
- F17-1 - not-checked - outside this commit's delta; not re-verified
- F17-2 - not-checked - outside this commit's delta; not re-verified
- F17-3 - not-checked - outside this commit's delta; not re-verified
- F17-4 - not-checked - outside this commit's delta; not re-verified
- F17-5 - not-checked - outside this commit's delta; not re-verified
- F17-6 - not-checked - outside this commit's delta; not re-verified
- F17-7 - not-checked - outside this commit's delta; not re-verified
- F17-8 - not-checked - outside this commit's delta; not re-verified
- F17-9 - not-checked - outside this commit's delta; not re-verified
- F17-10 - not-checked - outside this commit's delta; not re-verified
- F18-1 - not-checked - not re-verified against this commit's records; live_head now comes from the one start resolution
- F18-2 - fixed - verified: ThrowawayWorktree::create runs git status with --no-optional-locks (worktree.rs:137-139)
- F18-3 - fixed - collab_before is taken after the patch write and the run's own patch is exempted via is_run_patch_file
- F18-4 - fixed - untracked files named on stderr, recorded in the ledger, and --diff-tracked-only leaves them out
- F18-5 - not-checked - outside this commit's delta; not re-verified
- F18-6 - not-checked - outside this commit's delta; not re-verified
- F18-7 - not-checked - outside this commit's delta; not re-verified
- F19-1 - not-checked - outside this commit's delta; not re-verified
- F19-2 - fixed - disclosure + --diff-tracked-only (same item as F18-4)
- F19-3 - fixed - patch names are content-addressed (<sha8>) and place_patch never overwrites other bytes
- F19-4 - not-checked - outside this commit's delta; not re-verified
- F19-5 - not-checked - outside this commit's delta; not re-verified (MCP rev/diff unchanged)
- F19-6 - not-checked - outside this commit's delta; not re-verified
- F20-1 - not-checked - outside this commit's delta; not re-verified
- F20-2 - fixed - verified: create uses --no-optional-locks status (worktree.rs:137-139)
- F20-3 - fixed - content-addressed name plus never-overwrite placement binds the ledger sha to the file
- F20-4 - not-checked - outside this commit's delta; not re-verified
- F20-5 - fixed - collab paths are left out of the isolation list and both counts
- F20-6 - fixed - same as F18-4
- F24-1 - not-checked - outside this commit's delta; not re-verified
- F24-2 - fixed - noted_stream_wins picks the newer stream of the run's own handoff
- F24-4 - fixed - merge_parameters is recursive and keeps conflicting values; roles read nested keys by name
- F27-1 - not-checked - outside this commit's delta; not re-verified
- F27-2 - not-checked - outside this commit's delta; not re-verified
- F27-3 - fixed - only the run's own patch file and its 32-hex tmp name are exempt (is_run_patch_file)
- F28-1 - fixed - the no-hardlink fallback claims, renames and cleans up under the handle; final name never partial
- F28-2 - fixed - excluded paths go through a temporary index (hide_in_index), no path limit; test with 501 paths
- F28-3 - fixed - lossy sequences counted, ledger lossy_utf8, prompt line says the patch is not byte-exact
- F29-1 - not-checked - outside this commit's delta; not re-verified
- F29-2 - not-checked - outside this commit's delta; not re-verified
- F29-3 - fixed - one HEAD resolution (rev_live_head) decides withholding and live_head; unresolvable withholds
- F30-1 - fixed - explain: skip-preflight election, no_network, cache-only budget, no process
- F30-2 - fixed - tier_missed_warning warns when the required entries fill every seat
- F30-3 - not-checked - outside this commit's delta; not re-verified
- F30-4 - fixed - an invalid ext.c3.draw row makes the whole block Invalid, treated as absent
- F30-5 - not-checked - outside this commit's delta; not re-verified
- F30-6 - not-checked - outside this commit's delta; not re-verified
- F31-1 - not-checked - outside this commit's delta; not re-verified
- F31-2 - not-checked - outside this commit's delta; not re-verified
- F31-3 - not-checked - outside this commit's delta; not re-verified
- F31-4 - not-checked - outside this commit's delta; not re-verified
- F31-5 - not-checked - outside this commit's delta; not re-verified
- F31-6 - fixed - explain starts nothing: recorded health only, [availability not checked], seat-order line
- F32-1 - not-checked - outside this commit's delta; not re-verified
- F32-2 - not-checked - outside this commit's delta; not re-verified
- F32-3 - fixed - path/file/dir/uri-keyed values of an allow-listed reader are data; residual hole filed as a new finding
- F32-4 - not-checked - outside this commit's delta; not re-verified
- F32-5 - not-checked - outside this commit's delta; not re-verified
- F36-1 - not-checked - outside this commit's delta; not re-verified
- F36-2 - not-checked - outside this commit's delta; not re-verified
- F36-3 - not-checked - outside this commit's delta; not re-verified
- F36-4 - not-checked - outside this commit's delta; not re-verified
- F36-5 - not-checked - outside this commit's delta; not re-verified
- F36-6 - not-checked - outside this commit's delta; not re-verified
- F36-7 - not-checked - outside this commit's delta; not re-verified
- F36-8 - not-checked - outside this commit's delta; not re-verified
- F38-1 - not-checked - outside this commit's delta; not re-verified
- F38-2 - not-checked - outside this commit's delta; not re-verified
- F38-3 - not-checked - outside this commit's delta; not re-verified
- F38-4 - not-checked - outside this commit's delta; not re-verified
- F38-5 - not-checked - outside this commit's delta; not re-verified
- F39-1 - not-checked - outside this commit's delta; not re-verified
- F39-2 - not-checked - outside this commit's delta; not re-verified
- F39-4 - not-checked - outside this commit's delta; not re-verified
- F43-1 - fixed - open_patch_dir reaches handoffs through handles from the repo/collab anchor; a link above refuses
- F43-2 - fixed - empty claim unchanged 60 s is taken over; residual stall-vs-takeover benign (same sha8 name)
- F43-3 - fixed - drop_own_claim removes only the identity-checked empty claim; a check-then-remove microsecond window remains (filed as a note)
- F43-4 - not-checked - outside this commit's delta; not re-verified
- F43-5 - not-checked - outside this commit's delta; not re-verified
- F43-6 - not-checked - outside this commit's delta; not re-verified
- F43-7 - still-open - crash temp-index and handoffs tmp leftovers still have no reaper, and Root::open_anchor still follows the repo root; the new takeover removes only the empty claim
- F43-8 - not-checked - outside this commit's delta; not re-verified
- F44-1 - fixed - the 60 s empty-claim takeover resolves the abandoned-claim block
- F44-2 - not-checked - outside this commit's delta; not re-verified
- F44-3 - not-checked - outside this commit's delta; not re-verified
- F45-1 - not-checked - outside this commit's delta; not re-verified
- F45-2 - not-checked - outside this commit's delta; not re-verified
- F45-3 - not-checked - outside this commit's delta; not re-verified
- F45-4 - not-checked - outside this commit's delta; not re-verified
- F45-5 - not-checked - outside this commit's delta; not re-verified
- F45-6 - not-checked - outside this commit's delta; not re-verified
- F45-7 - not-checked - outside this commit's delta; not re-verified
- F45-8 - not-checked - outside this commit's delta; not re-verified
- F45-9 - not-checked - outside this commit's delta; not re-verified
- F46-1 - not-checked - outside this commit's delta; not re-verified
- F46-2 - not-checked - outside this commit's delta; not re-verified
- F46-3 - not-checked - outside this commit's delta; not re-verified
- F46-4 - not-checked - outside this commit's delta; not re-verified
- F46-5 - not-checked - outside this commit's delta; not re-verified
- F46-6 - not-checked - outside this commit's delta; not re-verified
- F46-7 - not-checked - outside this commit's delta; not re-verified
- F46-8 - not-checked - outside this commit's delta; not re-verified
- F47-1 - not-checked - outside this commit's delta; not re-verified
- F47-2 - not-checked - outside this commit's delta; not re-verified
- F47-3 - not-checked - outside this commit's delta; not re-verified
- F47-4 - not-checked - outside this commit's delta; not re-verified
- F48-1 - not-checked - outside this commit's delta; not re-verified
- F48-2 - not-checked - outside this commit's delta; not re-verified
- F48-3 - not-checked - outside this commit's delta; not re-verified
- F48-4 - not-checked - outside this commit's delta; not re-verified
- F48-5 - not-checked - outside this commit's delta; not re-verified

## Verdict: ACCEPT

The commit soundly closes the 24 claimed debt items (verified in the patch and the code at 2ffa02a); the residual defects I found are minor prompt-accuracy, disclosure and narrow-race issues, none of which damages a machine, a repository or a consultation record.

**WARNING: ACCEPT with 11 unchecked prior blocker(s) (F02-1, F02-2, F02-16, F02-19, F02-21, F03-1, F11-3, F11-7, F13-1, F19-1, F32-1).**

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, not-checked) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, not-checked) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, not-checked) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.
- **F19-1** (prior, not-checked) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.
- **F32-1** (prior, not-checked) `crates/c3/src/engines/worktree.rs:248` - Stage creation and publication use pathnames after validation, so a replaced directory component can send staged bytes outside the worktree and leave a temporary file there.

### Unproven scenarios

- A brief or artifact changed to other bytes and back during a live-tree run is invisible to the end-of-run re-hash (no engine records intermediate reads).
- The live-tree bound-input rule against real agy/muse/grok CLIs (author's own NOT-covered list; fakes only).
- A real file system without hard links for patch placement (only the test hook forces that path).
- An end-to-end http engine run under --rev (unit level only, per the brief).
- hide_in_index on a SHA-256 repository (64-hex zero padding) and with gitlink/submodule modes among the hidden paths.
- The composite takeover / drop_own_claim races on a genuinely stalled filesystem (finding on drop_own_claim).
- Router explain on a real multi-entry roster: no process or network spawn was verified statically only, not by tracing a live run.

### First-run checklist (observable)

_(none)_
