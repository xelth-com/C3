# Handoff 20 - Claude (claude): reply-zai-claude

Date: 2026-11-23 01:24 local. Author: Claude (claude) (model glm-5.3, effort high), claude-cli 2.1.330.
Reviewer: ZAI-claude :: glm-5.3 [claude] (provider from roster, model from roster; engine claude (C:\users\dmytro\.local\bin\claude.exe), endpoint https://api.z.ai/api/anthropic (token from env ZAI_API_KEY); provider fingerprint 944a94557723; harness claude-cli 2.1.330).
Preflight: ok: env ZAI_API_KEY set.
Effort: high sent (requested high, mapping claude-v1, by caps-v1: engine:claude, any model; not confirmed by the provider). Consultation id: 158e5ac1-9472-4c99-a59a-2c8d7125b505.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `claude -p --output-format stream-json --verbose --restricted --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model glm-5.3 --effort high --json-schema "{"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}}" --session-id e7d99b97-ac0d-489d-877e-d2036c347a68` (prompt on stdin).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-97a9b8b72d8a4e60bb828456be4256c7` of revision e4f42a8da3b138e4061a9f8f76d977b1117ecadf (--rev e4f42a8da3b138e4061a9f8f76d977b1117ecadf; removed after the run).
Parent thread: (none - new thread). Result thread: `e7d99b97-ac0d-489d-877e-d2036c347a68` (source: events).
Brief: `.collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.md` (sha256 05e9583d36cf). Reviewed: 4f43074, base 4f430740c2ce1936fdff3649d67ed21d7ca1e5d4, tree sha256 24740c634eea, 0 changed files.
Bridge outcome: usable reply. Wall time: 693.7 s. Tokens: in 3155503 (cached 3042112), out 33881, reasoning 0.
Engine turns: 1 (claude -p, auth endpoint; model glm-5.3; init tools Glob, Grep, Read, StructuredOutput; permission denials 0).
Warnings: the live collab directory changed during the run (1 file: .collab/release-0.2.5/handoffs/09-claude-diff-review-reviewed-revision.patch) - the reviewer ran in a throwaway worktree, the change is not the reviewer's.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The isolation core is sound and well-tested with fakes, but `--diff` on the http engine asks a reviewer to read a patch its pack cannot carry (a consultation record whose premise is false), and one claimed invariant (writes watched "gitignored or not") is not true; both are cheap fixes. Findings: 0 blocker, 1 major, 4 minor, 1 note (F20-1..F20-6, tracked in `findings.json`). Structured reply: `handoffs/20-claudecode-reply-zai-claude.reply.json`.
Raw event stream: `handoffs/20-claudecode-reply-zai-claude.events.jsonl`.
Verbatim reply follows.

---

**Q1. Same revision.** Sound. `--rev` resolves once to a full sha via `rev-parse --verify <rev>^{commit}` (`crates/c3/src/engines/worktree.rs:46`), which refuses blobs/trees and leading dashes (`rev_token_ok`, `crates/c3/src/consult/reviewed.rs:89`); the worktree is added at the resolved sha (`worktree.rs:95`), so a branch moving between resolve and checkout cannot split a run. A panel resolves once and hands every member the full sha (`crates/c3/src/panel/run.rs:813`, `member.rs:244`), so members cannot diverge. Ambiguous names are resolved by git once; the recorded sha is what is checked out. Shallow clones work for any sha git knows; others are refused before anything starts. Findings do stay bound to live HEAD under `--rev` (ledger `base_commit` from the live tree; `worktree.sha` disambiguates) — as claimed; a reader of `base_commit` alone could mis-apply, but the record is truthful.

**Q2. Staging.** Copies the task's `handoffs/` (minus `*.events.jsonl`), `state.md`, and the brief (`reviewed.rs:290-325`). Destination safety is real: `stageable_rel` refuses `..`/absolute (`worktree.rs:482`), `real_parent_inside` requires every component to be a real directory and canonicalizes inside the root (`worktree.rs:206`), a destination that is a link is refused (`worktree.rs:154`) — F11-3's overwrite-through-link is closed. It can overwrite the worktree's tracked copy of the brief (intended). Two read-side notes: a live `handoffs` that is itself a symlink is followed (`is_dir`, `reviewed.rs:299`) and content from outside the repo is staged in; and any untracked file in handoffs is staged by design. Event streams are excluded.

**Q3. `--diff` and its index.** The temp-index path is clean: the real index is only read (`fs::copy`), `GIT_INDEX_FILE` points `ls-files`/`add --intent-to-add`/`diff` at the temp copy, and the unit test byte-compares the real index and confirms the new file stays untracked (`reviewed.rs:880-901`); a concurrent commit only changes a snapshot. Option injection is refused (leading `-`, whitespace; `parse_diff_spec`, `reviewed.rs:416`). Two gaps: the patch carries full contents of every untracked, not-ignored file (`reviewed.rs:639`) — a not-ignored secret lands in handoffs and the prompt (finding #6); and the patch path is shared by brief stem, so a later run or member with different bytes replaces the file an earlier ledger `diff.sha256` binds (finding #3).

**Q4. Cleanup.** Removal is Drop-based on every in-process path including the setup failure (`worktree.rs:244`, `mod.rs:4085`); kill/crash leaves the `c3-kimi-wt-<32hex>` directory with its owner record for `c3 branches`/`doctor --fix` — which still rests on the fail-open liveness and best-effort owner write of F12-1/F13-8, now covering far more worktrees. The removal itself never follows a link and aborts destructiveness on any error (`worktree.rs:290-317`).

**Q5. Classification and races.** Correctly re-scoped: live tree/collab/brief/artifact changes are warnings only (`tree.rs:191-276`), the worktree's own changes decide (`tree.rs:279`, snapshot taken after staging, `reviewed.rs:326`), and a live-tree change no longer skips the timeout continuation (`secondary.rs:399`). kimi fails on its own worktree write, claude/grok/muse warn, codex fails — consistent with prior semantics. Gap: writes to gitignored paths are invisible (`revision.rs:314`), so kimi's fail-on-write and the "gitignored or not" claim overstate (finding #4). Names are uuids — no collisions; Windows reparse points count as links in staging, checking and removal alike.

**Q6. Privacy and entry points.** http+`--rev` warns; http+`--diff` does not, and the http pack holds only brief+artifacts (`http.rs:186`), so its prompt names a patch it cannot read — finding #1. The uncommitted-path list puts up to 20 live-tree filenames, unescaped, into prompts sent to remote models (finding #5, supersedes F11-5). MCP `rev`/`diff` are validated (syntax in `args.rs:645`, existence in `build_context`) before any git call. No new child-mark concerns. CHANGELOG sentences match the code except through finding #4's doc claim.

**Q7. Untested.** Everything against a real reviewer CLI (per the brief); http+`--diff` end to end; the ignored-write escape; concurrent members sharing one patch name; autocrlf fidelity of patch bytes; `doctor --fix` on rev/frozen-head leftovers.

## Requested checks
- RC1 (finding #1): with a stub endpoint logging request bodies, run `c3 consult --engine http --purpose diff-review --diff HEAD --brief <md> --artifact <f>` in a scratch repo (workspace-write for the stub); observe whether the prompt names a patch absent from the pack. 10 min.
- RC2 (finding #4): scratch repo with `ignored.txt` gitignored; fake-kimi consult writing only `ignored.txt` in its worktree; read `tree_check_outcome`. 10 min.
- RC3 (finding #3): 2-member panel with `--diff HEAD`, touch a tracked file between member starts; compare each member's ledger `diff.sha256` with the on-disk patch. 15 min.
- RC4 (finding #2): hold `.git/index.lock` in a scratch repo and run a kimi consult; also compare `.git/index` mtime before/after worktree creation. 10 min.

---

### Findings

- **F20-1** [major] `crates/c3/src/consult/orchestrate/mod.rs:2527`, `crates/c3/src/consult/http.rs:189`, `crates/c3/src/consult/reviewed.rs:712` - `--diff` on the http engine writes a patch and adds "read it first and review that change" to the prompt, but the http reviewer receives only a pack built from the brief and bound artifacts, so it is asked to review a change it cannot see; unlike `--rev`+http there is no warning. Trigger: `c3 consult --engine http --diff <range> --brief <md> --artifact <file>`. Evidence: read-code: build_pack's doc and body: the pack is built from the brief and -Artifact files only; a grep of http.rs finds no handling of `diff`.; read-code: diff_plan/diff_line are built for every engine; only `--rev` has an http-engine warning branch.; read-code: diff_prompt_line instructs the reviewer to read the patch file first. Verify: RC1: run http+--diff against a stub endpoint and inspect the prompt vs the pack. Remedy: Refuse or warn for http+--diff (mirroring the --rev warning), or include the patch bytes in the pack.
- **F20-2** [minor] `crates/c3/src/engines/worktree.rs:91` - ThrowawayWorktree::create runs `git status --porcelain` in the live repository without `--no-optional-locks`, so every kimi run, claude diff-review/acceptance and `--rev` run can refresh (write) the live index and contend for index.lock — the discipline F04-1 established for the read-only commands is missing on this new hot path. Trigger: A consult while another git process holds or creates index.lock, or with a stale index stat cache. Evidence: read-code: create() calls git_capture(repo, ["status", "--porcelain"]) with no --no-optional-locks.; read-code: neighbouring calls (uncommitted_paths, revision_info) do pass --no-optional-locks. Verify: RC4: hold .git/index.lock and run a kimi consult; check .git/index mtime after creation. Remedy: Add `--no-optional-locks` to the status call (or derive main_dirty from uncommitted_paths).
- **F20-3** [minor] `crates/c3/src/consult/reviewed.rs:486`, `crates/c3/src/consult/reviewed.rs:689` - The patch file name is derived from the brief's stem and is shared across runs and panel members; write_patch overwrites the file whenever its bytes differ, so a later run (or a sibling member rendering `--diff HEAD` at a different moment) silently replaces the bytes an earlier ledger `diff {sha256}` record binds. Trigger: Two consultations on one brief with different --diff values, or a panel with `--diff HEAD` while the working tree changes between members' renders. Evidence: read-code: patch_name reuses the brief stem whenever the brief sits in the task's handoffs.; read-code: write_patch overwrites plan.path unless the on-disk sha already matches.; read-code: every member of a panel receives the same --diff value, so members share the path. Verify: RC3: 2-member panel with --diff HEAD and a tree edit between member starts; compare ledger sha256 with the file. Remedy: Make the name unique per range/content (or refuse to clobber a file whose bytes differ from what a ledger record binds).
- **F20-4** [minor] `crates/c3/src/consult/reviewed.rs:359`, `crates/c3/src/consult/revision.rs:314` - ReviewTree::changes() cannot see writes to gitignored paths (git status and the content map exclude ignored files), so a kimi reviewer that writes only inside an ignored path stays usable, while the code doc and the handoff invariant claim coverage "gitignored or not". Trigger: A kimi (or codex) reviewer writes inside a gitignored directory of its throwaway worktree. Evidence: read-code: the fingerprint uses `git status --porcelain=v1 -uall -z` without --ignored, so ignored paths never appear.; read-code: the note literally says "ignored files excluded".; read-code: the doc comment of changes() claims "tracked and untracked files of the checkout ... gitignored or not". Verify: RC2: fake-kimi consult writing only a gitignored file; read tree_check_outcome. Remedy: Include ignored files in the worktree-side compare (e.g. a directory walk) or correct the claim.
- **F20-5** [minor] `crates/c3/src/engines/worktree.rs:593`, `crates/c3/src/consult/orchestrate/mod.rs:2697` - The isolation paragraph lists up to 20 uncommitted live-tree paths (plus a count of the rest) in every worktree-isolated reviewer's prompt — now claude diff-review/acceptance and every `--rev` engine, not only kimi — with filenames passed through unescaped (Markdown delimiters, line breaks), sending private or adversarial names to remote models. Trigger: Any isolated run on a repository with uncommitted files whose names are confidential or crafted for prompt injection. Evidence: read-code: uncommitted_paths/isolation_prompt_line list the whole tree's status paths, take(20), wrapped in backticks only.; read-code: the match on isolation applies isolation_prompt to every non-rev isolated run. Verify: Capture a fake claude diff-review prompt with a sensitive untracked filename outside the reviewed path. Remedy: Restrict the list to the -Range/-Artifact focus, escape names, or make the listing opt-in. Supersedes: F11-5.
- **F20-6** [note] `crates/c3/src/consult/reviewed.rs:639` - A one-revision `--diff` includes the full contents of every untracked, not-ignored file, so a secret not covered by .gitignore is copied into the task's handoffs and handed to the reviewer model. Trigger: `--diff HEAD` (or any single revision) with an untracked secret file in the tree. Evidence: read-code: ls-files --others --exclude-standard feeds intent-to-add into the diff, so untracked non-ignored files' contents enter the patch. Verify: Create an untracked secrets.txt and run --diff HEAD; inspect the patch bytes. Remedy: Document the exposure or exclude dotfile/secret-looking names by default.

### Prior findings

- F02-1 - not-checked - repo_models.rs not read this session.
- F02-2 - not-checked
- F02-3 - not-checked
- F02-4 - not-checked
- F02-5 - not-checked
- F02-6 - not-checked
- F02-7 - not-checked
- F02-8 - not-checked
- F02-9 - not-checked
- F02-10 - not-checked
- F02-11 - fixed - Guard re-read: is_c3_throwaway_worktree requires the system-temp parent and the exact 32-hex name (branches.rs:752-766); the shared prefix is deliberate for leftovers.
- F02-12 - not-checked
- F02-13 - not-checked
- F02-14 - not-checked
- F02-15 - not-checked
- F02-16 - not-checked
- F02-17 - not-checked
- F02-18 - not-checked
- F02-19 - not-checked
- F02-20 - not-checked
- F02-21 - fixed - The hardened removal moved verbatim into worktree.rs:290-317 (abort on any unlink error, no forced removal).
- F03-1 - not-checked
- F03-2 - not-checked - Same hardened removal reused; not re-verified beyond reading worktree.rs.
- F03-3 - not-checked
- F03-4 - not-checked
- F03-5 - not-checked
- F03-6 - not-checked
- F04-1 - still-open - The old sites keep --no-optional-locks, but this diff adds a NEW unlocked `git status --porcelain` on every consult (worktree.rs:91) — finding #2.
- F04-2 - not-checked
- F04-3 - not-checked
- F04-4 - not-checked
- F04-5 - not-checked - Removal abort-on-error reused; residual TOCTOU accepted earlier, unchanged.
- F04-6 - not-checked
- F04-7 - not-checked
- F04-8 - not-checked
- F04-9 - not-checked
- F10-1 - not-checked
- F10-2 - not-checked
- F10-3 - still-open - Narrowed: the preflight passes when the brief is committed at the rev but unreadable live (isolated_brief_refusal returns None), and setup_review_tree then still tries to copy the live brief and capability-fails without the panel re-seating; also the delete-between-preflight-and-stage race remains.
- F10-4 - not-checked
- F10-5 - not-checked
- F10-7 - not-checked
- F11-1 - not-checked
- F11-2 - not-checked
- F11-3 - fixed - Kimi staging rewritten through ThrowawayWorktree::stage_file: dest-link refusal, real-parent walk, canonical containment (worktree.rs:148-240); BriefCopy removed.
- F11-4 - fixed - Preflight refusal carries " is not usable:" (worktree.rs:545) so a panel re-seats; non-stageable (`..`, out-of-repo) refs are refused before launch.
- F11-5 - still-open - Superseded by finding #5: same listing, now emitted for claude diff-review/acceptance and every --rev engine, not only kimi.
- F11-6 - not-checked
- F11-7 - not-checked
- F11-8 - not-checked
- F11-9 - not-checked
- F11-10 - not-checked
- F11-11 - not-checked
- F11-12 - not-checked
- F12-1 - still-open - Unchanged code, broader exposure: every --rev / claude diff-review / acceptance run now creates a worktree whose doctor --fix removal rests on the fail-open pid liveness.
- F12-2 - not-checked
- F12-3 - not-checked
- F12-4 - not-checked
- F13-1 - still-open - Removal path unchanged; now also reached by rev/frozen-head worktrees.
- F13-2 - not-checked
- F13-3 - not-checked
- F13-4 - not-checked
- F13-5 - not-checked
- F13-6 - not-checked
- F13-7 - not-checked
- F13-8 - still-open - record_throwaway_owner is still best-effort (`let _ = write`, branches.rs:791-798); a killed long review without an owner record is stale after 6h — and such runs are now far more common.
- F14-1 - not-checked
- F14-2 - not-checked
- F14-3 - not-checked
- F14-4 - not-checked
- F14-5 - not-checked
- F15-1 - not-checked
- F15-2 - not-checked
- F15-3 - not-checked
- F15-5 - not-checked
- F15-6 - not-checked
- F16-1 - not-checked
- F16-2 - not-checked
- F16-3 - not-checked
- F16-4 - not-checked
- F16-6 - not-checked
- F16-7 - not-checked
- F16-8 - not-checked
- F16-9 - not-checked
- F16-10 - not-checked
- F17-1 - not-checked
- F17-2 - not-checked
- F17-3 - not-checked
- F17-4 - not-checked
- F17-5 - not-checked
- F17-6 - not-checked
- F17-7 - not-checked
- F17-8 - not-checked
- F17-9 - not-checked
- F17-10 - not-checked

## Verdict: HOLD

The isolation core is sound and well-tested with fakes, but `--diff` on the http engine asks a reviewer to read a patch its pack cannot carry (a consultation record whose premise is false), and one claimed invariant (writes watched "gitignored or not") is not true; both are cheap fixes.

### Blockers

- **F02-1** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:149` - Model repair can rewrite an external file through a hard link or through replacement of a checked path component between check and write.
- **F02-2** (prior, not-checked) `crates/c3/src/setup/repo_models.rs:167` - An interrupted or failed repair can leave a truncated Markdown file, and an ordinary concurrent edit can be overwritten.
- **F02-16** (prior, not-checked) `crates/c3/src/maintain/mod.rs:1324` - A smoke can use a different roster entry, endpoint, and quota policy from the entry being edited.
- **F02-19** (prior, not-checked) `crates/c3/src/c3config.rs:329` - Telemetry can disclose a private endpoint model name when a private configured catalog or page supplies that same name.
- **F02-21** (prior, fixed) `crates/c3/src/engines/kimi.rs:210` - Kimi invokes forced removal after an incomplete unlink pass, and the unlink walker can traverse outside through a substituted root or queued directory.
- **F03-1** (prior, not-checked) `crates/c3-core/src/claude.rs:313` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there.
- **F11-3** (prior, fixed) `crates/c3/src/engines/kimi.rs:279` - The Kimi staging path can overwrite a file outside the throwaway worktree before the reviewer starts.
- **F11-7** (prior, not-checked) `crates/c3/src/maintain/smoke.rs:328` - Automatic maintenance can treat an unexercised Codex sub-agent switch as a passing smoke and skip rollback.
- **F13-1** (prior, still-open) `crates/c3/src/setup/branches.rs:825` - c3 doctor --fix can classify a temp path named c3-kimi-wt-<32 hex> as a stale throwaway when that path is a symlink or junction, because staleness follows the target with std::fs::metadata and does not require the path itself to be a real directory. Removal then always runs git worktree remove --force and remove_dir_all after a best-effort unlink, which can delete the link target.

### Unproven scenarios

- No run of --rev, --diff or the HEAD worktree against a real reviewer CLI exists (stated in the brief); all integration tests use fake engines.
- http + --diff end to end (finding #1 is code-reading only).
- Whether a kimi/codex write inside a gitignored path of the worktree really escapes the fail (finding #4, code-read only).
- Concurrent panel members sharing one patch path with differing bytes (finding #3).
- Patch byte fidelity under core.autocrlf / CRLF working trees.
- doctor --fix and c3 branches on leftovers of the new rev/frozen-head worktrees (same name and owner machinery as kimi's, untested for the new reasons).
- A live handoffs directory that is a symlink staging outside-repo content into the worktree (read-follow noted, not exercised).

### First-run checklist (observable)

_(none)_
