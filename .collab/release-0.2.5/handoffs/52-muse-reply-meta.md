# Handoff 52 - Meta Muse (muse): reply-meta

Date: 2026-11-24 09:31 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (usable reply 22 min ago).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: 12e69c3d-928b-4306-af7b-42f40e426678.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-764f08cb982844efadd13d36e632366b.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8f64f73aadfd4a5fb7ca44cae9c08d91` of revision 2ffa02aa435c0942ffb9cba505eef58d358a58a1 (--rev 2ffa02aa435c0942ffb9cba505eef58d358a58a1; removed after the run).
Parent thread: (none - new thread). Result thread: `01a20cab-1a81-72d1-8134-bcf5cca1e035` (source: events).
Brief: `.collab/release-0.2.5/handoffs/50-claude-diff-review-bridge-debt.md` (sha256 22b36ab257c1). Reviewed: 2ffa02a, base 2ffa02aa435c0942ffb9cba505eef58d358a58a1, tree sha256 946c1867ea9b, 0 changed files. Live HEAD at launch: 2ffa02aa435c0942ffb9cba505eef58d358a58a1.
Bridge outcome: usable reply. Wall time: 138.6 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: muse: warning: rules file at \\?\C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-8f64f73aadfd4a5fb7ca44cae9c08d91\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Residual classifier bypass and patch-claim/anchor races remain; otherwise the bridge-debt fixes read sound. Findings: 0 blocker, 2 major, 1 minor, 1 note (F52-1..F52-4, tracked in `findings.json`). Structured reply: `handoffs/52-muse-reply-meta.reply.json`.
Raw event stream: `handoffs/52-muse-reply-meta.events.jsonl`.
Verbatim reply follows.

---

## Q1. Outcomes
Bound-input rule now covers live-tree muse/grok/claude/kimi/codex plus agy; worktree reads are exempt and codex re-hashes after last turn. Residual: timestamp-only change still withholds (hash unchanged? no — mtime-only does not change hash, so no false withhold seen). Bridge writes are narrowly exempt (own patch + tmp, own handoff prefixes); sibling patch with same stem but different diff can still collide via shared patch name — ledger sha binds file written last.

## Q2. agy classifier
Recursive merge keeps both conflicting values; path/search keys are data. Residual bypass is `command_shaped`: first-word allow-list misses `sudo/env/nohup/su/doas/start`, pipe `|`/`$()`/backticks, so `sudo rm -rf x` under unknown key stays reader (finding #1).

## Q3. Patch placement
Handle-based handoffs dir, empty-claim 60 s takeover, own-claim-only removal, temp-index for excludes all read sound. Residual: 60 s takeover can steal a live stalled owner; outside-repo collab anchor is `create_dir_all` by path (finding #2–3).

## Q4. What leaves the machine
One-rev patch still carries untracked contents by default; stderr + ledger list up to ten names, backtick/control names counted-not-shown; `--diff-tracked-only` leaves them out and is plumbed to detached + panel members. No ignored-content path seen; redaction is still value-shaped only.

## Q5. `--rev` records
One HEAD resolution decides withhold + `live_head`; unresolvable withholds with `live_head_unresolved`; brief-from-revision skips live re-hash and handoff says so. No wrong-revision record seen; mtime-tie in death record is the residual (finding #4).

## Q6. Router
`explain` now sets `no_network`, cache-only budgets, recorded-health availability, panel-order flag + `seats:` line; replay treats any invalid draw row as absent. No spawn/network path seen in read code.

## Q7. Untested
Author notes http+`--rev` unit-only, live-tree half of bound rule vs real agy/muse/grok, no-hardlink FS via hook only. Add: wrapper-command classifier, slow-owner claim race, outside-collab anchor link, coarse-mtime death tie.

## Requested checks
RC1: unit-test `agy_params_show_write` on `sudo rm -rf x` under unknown key (read-only, seconds). RC2: two `place_without_link` on one name with first held >60 s at `patch-claim`, then compare survivor bytes (workspace-write, minutes). RC3: `--diff` with `--collab-dir` outside repo under a link parent, list where handoffs lands (workspace-write, minutes).

---

### Findings

- **F52-1** [major] `crates/c3/src/engines/tree_check.rs:306` - Agy reader classification misses commands whose first word is a launcher wrapper (sudo, env, nohup, su, doas, start) and pipe-only chains: command_shaped requires the first whitespace word to be in AGY_COMMAND_WORDS and only treats &&, ||, ;, > as chaining, so a Scanned string such as 'sudo rm -rf x' or 'cat f | sh' under an unknown key is judged not-writing. Trigger: An allow-listed reader logs a parameter under an unknown key with value 'sudo rm -rf x' or 'cat f | sh' and the tree changes. Evidence: read-code: command_shaped checks first_word against AGY_COMMAND_WORDS (rm, git, powershell, ...) and chaining only for &&, ||, ' > ', '>>', ';' + command word; sudo/env/nohup absent, single '|' absent. Verify: Unit-test agy_params_show_write on those two strings under an unknown key and assert true. Remedy: Normalize launcher prefixes (strip sudo/env/nohup/doas/su/runas/start) before first-word test and treat '|'/$()/backticks as chaining, or scan every whitespace word for a command word.
- **F52-2** [major] `crates/c3/src/consult/reviewed.rs:1125`, `crates/c3/src/consult/reviewed.rs:1258` - No-hardlink patch placement can take over a live owner's claim: an EMPTY claim unchanged for 60 s is Abandoned and removed/re-claimed, but a live owner stalled >60 s between exclusive create and rename (AV, suspend, slow FS) still holds its tmp; the taker's rename then wins and the owner's later rename replaces or fails against the taker's file. Trigger: Two concurrent --diff placements on one patch name where the first stalls >60 s between claim create and rename. Evidence: read-code: reuse_patch returns Abandoned for empty file with mtime age >= claim_stale_after (60 s); place_without_link claims via create_new then rename; drop_own_claim only protects the failing renamer, not the stalled owner whose rename runs second. Verify: Hold patch-claim race point >60 s in one process, start a second placement, then release the first and inspect which bytes survive. Remedy: Hold the claim open (keep the create_new handle alive until rename) or re-check identity after rename; extend the 60 s window and log both pids.
- **F52-3** [minor] `crates/c3/src/consult/reviewed.rs:1319` - open_patch_dir creates the outside-repo collab anchor by pathname before any handle check: when the collab root lies outside the repo, create_dir_all(collab_root) follows links above it, so a link above the anchor defeats the contained-from-handle guarantee that covers the in-repo case. Trigger: --collab-dir outside the repo whose parent chain contains a symlink/junction when a --diff patch is placed. Evidence: read-code: open_patch_dir: in-repo branch anchors on repo_root; outside branch does std::fs::create_dir_all(collab_root) by path, then Root::open_anchor(collab_root); only components below the anchor are opened without following. Verify: Point --collab-dir at an outside path under a swapped link parent and trace where the handoffs dir is created. Remedy: Resolve/create the outside anchor through handles too (create missing parents via Root or refuse when the anchor or a parent is a link).
- **F52-4** [note] `crates/c3/src/consult/detach.rs:1206` - Death-record newest-stream rule decides by filesystem mtime with second-granularity ties broken toward the note: on filesystems with coarse mtime or equal timestamps the pending main stream can still win over a newer continuation, and a missing noted file falls back to the note even when the pending file is the newer evidence. Trigger: Main and continuation streams created within one mtime tick, or the noted continuation file already pruned. Evidence: read-code: noted_stream_wins compares mtime(pending) vs mtime(noted), n>=p wins; (Some,None)=>false, _=>true; tie goes to note; test sleeps 50 ms to separate mtimes. Verify: Create both streams with identical mtimes and check which events_file the death record keeps. Remedy: Compare (mtime, size/hash) or sequence numbers from the status note instead of mtime alone; document tie behavior.

### Prior findings

- F02-1 - not-checked - Read-only review, no commands run.
- F02-2 - not-checked - Read-only review, no commands run.
- F02-3 - not-checked - Read-only review, no commands run.
- F02-4 - not-checked - Read-only review, no commands run.
- F02-5 - not-checked - Read-only review, no commands run.
- F02-6 - not-checked - Read-only review, no commands run.
- F02-7 - not-checked - Read-only review, no commands run.
- F02-8 - not-checked - Read-only review, no commands run.
- F02-9 - not-checked - Read-only review, no commands run.
- F02-10 - not-checked - Read-only review, no commands run.
- F02-11 - not-checked - Read-only review, no commands run.
- F02-12 - not-checked - Read-only review, no commands run.
- F02-13 - not-checked - Read-only review, no commands run.
- F02-14 - not-checked - Read-only review, no commands run.
- F02-15 - not-checked - Read-only review, no commands run.
- F02-16 - not-checked - Read-only review, no commands run.
- F02-17 - not-checked - Read-only review, no commands run.
- F02-18 - not-checked - Read-only review, no commands run.
- F02-19 - not-checked - Read-only review, no commands run.
- F02-20 - not-checked - Read-only review, no commands run.
- F02-21 - not-checked - Read-only review, no commands run.
- F03-1 - not-checked - Read-only review, no commands run.
- F03-2 - not-checked - Read-only review, no commands run.
- F03-3 - not-checked - Read-only review, no commands run.
- F03-4 - not-checked - Read-only review, no commands run.
- F03-5 - not-checked - Read-only review, no commands run.
- F03-6 - not-checked - Read-only review, no commands run.
- F04-1 - not-checked - Read-only review, no commands run.
- F04-2 - not-checked - Read-only review, no commands run.
- F04-3 - not-checked - Read-only review, no commands run.
- F04-4 - not-checked - Read-only review, no commands run.
- F04-5 - not-checked - Read-only review, no commands run.
- F04-6 - not-checked - Read-only review, no commands run.
- F04-7 - not-checked - Read-only review, no commands run.
- F04-8 - not-checked - Read-only review, no commands run.
- F04-9 - not-checked - Read-only review, no commands run.
- F10-1 - not-checked - Read-only review, no commands run.
- F10-2 - not-checked - Read-only review, no commands run.
- F10-3 - not-checked - Read-only review, no commands run.
- F10-4 - not-checked - Read-only review, no commands run.
- F10-5 - not-checked - Read-only review, no commands run.
- F10-7 - not-checked - Read-only review, no commands run.
- F11-1 - not-checked - Read-only review, no commands run.
- F11-2 - not-checked - Read-only review, no commands run.
- F11-3 - not-checked - Read-only review, no commands run.
- F11-4 - not-checked - Read-only review, no commands run.
- F11-5 - not-checked - Read-only review, no commands run.
- F11-6 - not-checked - Read-only review, no commands run.
- F11-7 - not-checked - Read-only review, no commands run.
- F11-8 - not-checked - Read-only review, no commands run.
- F11-9 - not-checked - Read-only review, no commands run.
- F11-10 - not-checked - Read-only review, no commands run.
- F11-11 - not-checked - Read-only review, no commands run.
- F11-12 - not-checked - Read-only review, no commands run.
- F12-1 - not-checked - Read-only review, no commands run.
- F12-2 - not-checked - Read-only review, no commands run.
- F12-3 - not-checked - Read-only review, no commands run.
- F12-4 - not-checked - Read-only review, no commands run.
- F13-1 - not-checked - Read-only review, no commands run.
- F13-2 - not-checked - Read-only review, no commands run.
- F13-3 - not-checked - Read-only review, no commands run.
- F13-4 - not-checked - Read-only review, no commands run.
- F13-5 - not-checked - Read-only review, no commands run.
- F13-6 - not-checked - Read-only review, no commands run.
- F13-7 - not-checked - Read-only review, no commands run.
- F13-8 - not-checked - Read-only review, no commands run.
- F13-9 - not-checked - Read-only review, no commands run.
- F14-1 - not-checked - Read-only review, no commands run.
- F14-2 - not-checked - Read-only review, no commands run.
- F14-3 - not-checked - Read-only review, no commands run.
- F14-4 - not-checked - Read-only review, no commands run.
- F14-5 - not-checked - Read-only review, no commands run.
- F15-1 - not-checked - Read-only review, no commands run.
- F15-2 - not-checked - Read-only review, no commands run.
- F15-3 - not-checked - Read-only review, no commands run.
- F15-5 - not-checked - Read-only review, no commands run.
- F15-6 - not-checked - Read-only review, no commands run.
- F16-1 - not-checked - Read-only review, no commands run.
- F16-2 - not-checked - Read-only review, no commands run.
- F16-3 - not-checked - Read-only review, no commands run.
- F16-4 - not-checked - Read-only review, no commands run.
- F16-5 - not-checked - Read-only review, no commands run.
- F16-6 - not-checked - Read-only review, no commands run.
- F16-7 - not-checked - Read-only review, no commands run.
- F16-8 - not-checked - Read-only review, no commands run.
- F16-9 - not-checked - Read-only review, no commands run.
- F16-10 - not-checked - Read-only review, no commands run.
- F17-1 - not-checked - Read-only review, no commands run.
- F17-2 - not-checked - Read-only review, no commands run.
- F17-3 - not-checked - Read-only review, no commands run.
- F17-4 - not-checked - Read-only review, no commands run.
- F17-5 - not-checked - Read-only review, no commands run.
- F17-6 - not-checked - Read-only review, no commands run.
- F17-7 - not-checked - Read-only review, no commands run.
- F17-8 - not-checked - Read-only review, no commands run.
- F17-9 - not-checked - Read-only review, no commands run.
- F17-10 - not-checked - Read-only review, no commands run.
- F18-1 - not-checked - Read-only review, no commands run.
- F18-2 - not-checked - Read-only review, no commands run.
- F18-3 - not-checked - Read-only review, no commands run.
- F18-4 - not-checked - Read-only review, no commands run.
- F18-5 - not-checked - Read-only review, no commands run.
- F18-6 - not-checked - Read-only review, no commands run.
- F18-7 - not-checked - Read-only review, no commands run.
- F19-1 - not-checked - Read-only review, no commands run.
- F19-2 - not-checked - Read-only review, no commands run.
- F19-3 - not-checked - Read-only review, no commands run.
- F19-4 - not-checked - Read-only review, no commands run.
- F19-5 - not-checked - Read-only review, no commands run.
- F19-6 - not-checked - Read-only review, no commands run.
- F20-1 - not-checked - Read-only review, no commands run.
- F20-2 - not-checked - Read-only review, no commands run.
- F20-3 - not-checked - Read-only review, no commands run.
- F20-4 - not-checked - Read-only review, no commands run.
- F20-5 - not-checked - Read-only review, no commands run.
- F20-6 - not-checked - Read-only review, no commands run.
- F24-1 - not-checked - Read-only review, no commands run.
- F24-2 - not-checked - Read-only review, no commands run.
- F24-4 - not-checked - Read-only review, no commands run.
- F27-1 - not-checked - Read-only review, no commands run.
- F27-2 - not-checked - Read-only review, no commands run.
- F27-3 - not-checked - Read-only review, no commands run.
- F28-1 - not-checked - Read-only review, no commands run.
- F28-2 - not-checked - Read-only review, no commands run.
- F28-3 - not-checked - Read-only review, no commands run.
- F29-1 - not-checked - Read-only review, no commands run.
- F29-2 - not-checked - Read-only review, no commands run.
- F29-3 - not-checked - Read-only review, no commands run.
- F30-1 - not-checked - Read-only review, no commands run.
- F30-2 - not-checked - Read-only review, no commands run.
- F30-3 - not-checked - Read-only review, no commands run.
- F30-4 - not-checked - Read-only review, no commands run.
- F30-5 - not-checked - Read-only review, no commands run.
- F30-6 - not-checked - Read-only review, no commands run.
- F31-1 - not-checked - Read-only review, no commands run.
- F31-2 - not-checked - Read-only review, no commands run.
- F31-3 - not-checked - Read-only review, no commands run.
- F31-4 - not-checked - Read-only review, no commands run.
- F31-5 - not-checked - Read-only review, no commands run.
- F31-6 - not-checked - Read-only review, no commands run.
- F32-1 - not-checked - Read-only review, no commands run.
- F32-2 - not-checked - Read-only review, no commands run.
- F32-3 - not-checked - Read-only review, no commands run.
- F32-4 - not-checked - Read-only review, no commands run.
- F32-5 - not-checked - Read-only review, no commands run.
- F36-1 - not-checked - Read-only review, no commands run.
- F36-2 - not-checked - Read-only review, no commands run.
- F36-3 - not-checked - Read-only review, no commands run.
- F36-4 - not-checked - Read-only review, no commands run.
- F36-5 - not-checked - Read-only review, no commands run.
- F36-6 - not-checked - Read-only review, no commands run.
- F36-7 - not-checked - Read-only review, no commands run.
- F36-8 - not-checked - Read-only review, no commands run.
- F38-1 - not-checked - Read-only review, no commands run.
- F38-2 - not-checked - Read-only review, no commands run.
- F38-3 - not-checked - Read-only review, no commands run.
- F38-4 - not-checked - Read-only review, no commands run.
- F38-5 - not-checked - Read-only review, no commands run.
- F39-1 - not-checked - Read-only review, no commands run.
- F39-2 - not-checked - Read-only review, no commands run.
- F39-4 - not-checked - Read-only review, no commands run.
- F43-1 - not-checked - Read-only review, no commands run.
- F43-2 - not-checked - Read-only review, no commands run.
- F43-3 - not-checked - Read-only review, no commands run.
- F43-4 - not-checked - Read-only review, no commands run.
- F43-5 - not-checked - Read-only review, no commands run.
- F43-6 - not-checked - Read-only review, no commands run.
- F43-7 - not-checked - Read-only review, no commands run.
- F43-8 - not-checked - Read-only review, no commands run.
- F44-1 - not-checked - Read-only review, no commands run.
- F44-2 - not-checked - Read-only review, no commands run.
- F44-3 - not-checked - Read-only review, no commands run.
- F45-1 - not-checked - Read-only review, no commands run.
- F45-2 - not-checked - Read-only review, no commands run.
- F45-3 - not-checked - Read-only review, no commands run.
- F45-4 - not-checked - Read-only review, no commands run.
- F45-5 - not-checked - Read-only review, no commands run.
- F45-6 - not-checked - Read-only review, no commands run.
- F45-7 - not-checked - Read-only review, no commands run.
- F45-8 - not-checked - Read-only review, no commands run.
- F45-9 - not-checked - Read-only review, no commands run.
- F46-1 - not-checked - Read-only review, no commands run.
- F46-2 - not-checked - Read-only review, no commands run.
- F46-3 - not-checked - Read-only review, no commands run.
- F46-4 - not-checked - Read-only review, no commands run.
- F46-5 - not-checked - Read-only review, no commands run.
- F46-6 - not-checked - Read-only review, no commands run.
- F46-7 - not-checked - Read-only review, no commands run.
- F46-8 - not-checked - Read-only review, no commands run.
- F47-1 - not-checked - Read-only review, no commands run.
- F47-2 - not-checked - Read-only review, no commands run.
- F47-3 - not-checked - Read-only review, no commands run.
- F47-4 - not-checked - Read-only review, no commands run.
- F48-1 - not-checked - Read-only review, no commands run.
- F48-2 - not-checked - Read-only review, no commands run.
- F48-3 - not-checked - Read-only review, no commands run.
- F48-4 - not-checked - Read-only review, no commands run.
- F48-5 - not-checked - Read-only review, no commands run.

## Verdict: HOLD

Residual classifier bypass and patch-claim/anchor races remain; otherwise the bridge-debt fixes read sound.

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

- Whether sibling same-stem different-diff collision is reachable in a real panel given per-member reply names.
- Whether coarse-mtime death-record tie occurs on the release filesystems.
- Whether a live owner really stalls >60 s between claim and rename.

### First-run checklist (observable)

_(none)_
