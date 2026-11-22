# Handoff 03 - Meta Muse (muse): diff-review-meta

Date: 2026-11-22 13:41 local. Author: Meta Muse (muse) (model muse-spark-1.3-contributor, effort high), muse-cli 1.4.25-R9361.1.
Reviewer: meta :: muse-spark-1.3-contributor [muse] (provider from roster, model from roster; engine muse (C:\Users\Dmytro\AppData\Local\Programs\muse\muse.cmd); provider fingerprint 1c6f62bb040d; harness muse-cli 1.4.25-R9361.1).
Preflight: ok: signed in (~/.config/muse/auth.json: providers.meta, mechanism oauth).
Effort: high sent (requested high, mapping muse-v1, by caps-v1: engine:muse, muse-spark-1.3-contributor; not confirmed by the provider). Consultation id: b879cabc-52f2-474f-ba41-75ce041a5250.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `muse exec --json --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-05a512a7c29345bdb2cd7b9e715bfb57.txt --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json --model muse-spark-1.3-contributor --reasoning-effort high --no-foreign-personal-context --disable-web-tools --disable-write --disable-shell --approval-mode never` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a20343-8bce-73a2-9b82-40ac3fdc9cd6` (source: events).
Brief: `.collab/release-0.2.5/handoffs/01-claude-diff-review.md` (sha256 f8d09494b40e). Reviewed: ba43ff6, base ba43ff616de46a514092c1d6afa9e5d343943bed, tree sha256 1234329a6cc8, 0 changed files.
Bridge outcome: usable reply. Wall time: 176.1 s. Tokens: not reported by muse.
Engine turns: 1 (each one a Muse Code subscription prompt); MSP schema_version 1.
Warnings: routing: no lab known for #16 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab"; muse: warning: rules file at \\?\C:\Users\Dmytro\C3\AGENTS.md exists, but the workspace is untrusted, so it is skipped for this session; restart with --trust-workspace to load project rules; muse: warning: project MCP servers from .mcp.json, if any, are not loaded because the workspace is untrusted; restart with --trust-workspace to load them.
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - One blocker breaks the read-only reviewer guarantee on the claude engine, plus worktree-removal and branch-record risks need hardening before 0.2.5. Findings: 1 blocker, 2 major, 2 minor, 1 note (F03-1..F03-6, tracked in `findings.json`). Structured reply: `handoffs/03-muse-diff-review-meta.reply.json`.
Raw event stream: `handoffs/03-muse-diff-review-meta.events.jsonl`.
Verbatim reply follows.

---

## Q1 (repair)
No. Finding #1 is a blocker: the `claude` child allow-list has no `C3_CHILD`, so `child_environment()` drops the mark the hook depends on — a `claude`-engine reviewer/worker is not suppressed. Otherwise the repair is careful: linked `.claude`/`agents` dirs and files are skipped via `symlink_metadata`, only one `model:` line is rewritten, CRLF/BOM handling is tested. Gaps: trailing comments/quotes are normalized away (#4); `model :` spacing evades both doctor and repair; `C3_CHILD=0` counts as "inside a child".

## Q2 (branches)
`--set` keeps the foreign part below the record and `--clear` only removes C3's line, but #3 is real: the branch name is interpolated into `branch.<name>.description`, so dotted/odd names can hit the wrong subsection, and a foreign note starting with `c3: ` is misread as a record. Read paths are not strictly read-only (#6: index refresh, lock files). Detached HEAD / no-upstream degrade to `None`/empty rather than corrupt. `default_branch` fallback order (`origin/HEAD`, `main`, `master`, current) can mislabel the default when `origin/HEAD` is stale. `git cherry` cost is per-branch and uncapped; the hook correctly avoids it. The kimi throwaway exclusion is by dirname prefix, so a user path with that prefix is wrongly hidden and a renamed throwaway is wrongly counted.

## Q3 (catalog security)
Sound as read: https-only, same-host redirects via `redirect_target`, 15s / 4MB / 5000-id caps, `valid_model_id` shape gate (no newline/quote/backslash), keyless GET with `&[]` headers plus only a `c3/<version>` User-Agent. No key/header/cookie/env identity is attached in the code read. Page regexes come from local config, not the remote body; Rust `regex` avoids backtracking DoS. No roster/command/path injection path was found: swaps re-validate the shape and splice one JSON value.

## Q4 (catalog correctness)
The intended guards (date evidence, undated/page ids announced-only, protected/on-request never smoked/edited, `--dry-run` no smoke/edit/state, `--models` leaves `next_due`/engines alone, major-crossing successor) match the prose and the tested helpers, but I did not read the pass-orchestration call sites, so I do not assert the wiring — see RC1/RC2. Vendor-prefixed candidates (`qwen/qwen4-max` vs seated `qwen4-max`) need a check that the comparison strips the vendor (RC1).

## Q5 (telemetry privacy)
Sound as read: private endpoint lists never enter the published set (`web:`/`page:` only), lookup is lowercase on both sides, closed-list spelling is sent, `[1m]` ids cannot enter via catalogs (shape rejects brackets). One accuracy bug (#5): uppercase `[1M]` misses the strip and yields `other`. The Claude-shape open door correctly still withholds private names. Effort-family acceptance was not read — RC3.

## Q6 (kimi worktree)
Order is unlink-then-remove and symlinks are not followed by the walk, but #2 stands: junctions reporting as directories would be descended, and the sweep→`remove --force`→`remove_dir_all` sequence has a race plus an unbounded fallback. Needs a Windows junction/dir-symlink test.

## Q7 (untested)
As the brief says: real roster swap, `model_pages`, hook repair of `AGENTS.md` across a major, `--set` over a foreign description; plus: claude-child `C3_CHILD` propagation, dotted/odd branch names, detached/no-upstream repos, many-branch cost, uppercase `[1M]`, junction-as-directory, concurrent roster edit during smoke, and `--dry-run`/`next_due` wiring.

## Requested checks
RC1 (read-only): `rg -n "strip_vendor|successor\(|candidates" crates/c3/src/maintain/models.rs` — does the swap compare vendor-stripped ids; observation: exact call lines. Budget: minutes.
RC2 (read-only): `rg -n "dry_run|next_due|protected|on_request|smoke" crates/c3/src/maintain/mod.rs crates/c3/src/cli/maintain.rs` — confirm dry-run/models guards; observation: guard lines. Budget: minutes.
RC3 (read-only): `rg -n "effort|vocabulary|prefix.*suffix" crates/c3/src crates/c3-core/src` — locate effort-family rule and test uppercase/lowercase; observation: rule lines. Budget: minutes.

---

### Findings

- **F03-1** [blocker] `crates/c3-core/src/claude.rs:313`, `crates/c3/src/engines/mod.rs:61` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there. Trigger: A review or worker turn runs on the claude engine (SessionStart hook fires inside the repository under review); the hook then rewrites .claude/agents pins or AGENTS.md in a tree that must stay read-only, or pollutes a worker patch. Evidence: read-code: CHILD_ENV_NAMES allow-list (SystemRoot, PATH, TEMP, CLAUDE_CONFIG_DIR, proxies, etc.) contains no C3_CHILD and no prefix matching it; child_environment() only forwards allowed names and puts the rest in removed.; read-code: child_environment() builds the env from the allow-list plus DISABLE_AUTOUPDATER and endpoint vars only; C3_CHILD set by scrub_host_markers never passes through.; read-code: scrub_host_markers() sets C3_CHILD=1 on generic subprocess children, and hook_line() relies on in_c3_child() to stay read-only inside reviewers/workers. Verify: Unit-test child_environment() with C3_CHILD=1 in vars and assert it survives; then run a claude-engine child and check the hook does nothing. Remedy: Add C3_CHILD to the claude child environment explicitly (set C3_CHILD=1 alongside DISABLE_AUTOUPDATER=1 in child_environment/probe paths, or allow-list it), and add a test asserting a claude reviewer child sees it.
- **F03-2** [major] `crates/c3/src/engines/kimi.rs:227`, `crates/c3/src/engines/kimi.rs:206` - unlink_links_inside() can descend into a reparse point that reports as a directory rather than a symlink, and the remove path has a TOCTOU plus a remove_dir_all fallback that can act outside the worktree. Trigger: A kimi worktree contains a junction or directory symlink into the main checkout (e.g. node_modules) that does not report is_symlink, or a new link appears after the sweep; forced removal then empties content outside the worktree. Evidence: read-code: Walk uses symlink_metadata; only ft.is_symlink() is unlinked, ft.is_dir() is pushed and descended. Correctness for junctions depends on them reporting is_symlink, which is platform/behavior dependent.; read-code: remove() unlinks first, then runs git worktree remove --force, then unconditionally falls back to std::fs::remove_dir_all(&path); a link created between the sweep and the removes, or a descended target, is removed with force. Verify: In a fixture worktree with a junction and a dir symlink, run the removal and confirm targets outside survive and nothing is descended. Remedy: Treat directory reparse points/mount points as links (do not descend without a link check that covers junctions), re-sweep or open handles safely, drop or bound the remove_dir_all fallback, and add a junction/dir-symlink test on Windows.
- **F03-3** [major] `crates/c3/src/setup/branches.rs:835`, `crates/c3/src/setup/branches.rs:126` - c3 branches --set/--clear interpolates the branch name into a git config key, so names with dots or unusual characters can address the wrong subsection or fail; a foreign note starting with 'c3: ' is misclassified as C3's record. Trigger: --set on a branch named e.g. foo.bar or a name needing quoting, or a branch whose foreign description happens to start with 'c3: '. Evidence: read-code: set_state/clear_state build key branch.{branch}.description by string interpolation and pass it to git config; parse_descriptions strips branch./.description symmetrically, which breaks down for dotted/odd names.; read-code: is_c3_description is a plain starts_with('c3: ') on the first line, so a user's note beginning with 'c3: ' is treated as a record and --set discards its first line. Verify: Try --set/--clear on dotted and odd branch names and on a foreign note starting with 'c3: ', then inspect git config and the preserved text. Remedy: Quote/escape the config subsection (or use git config --fixed-value / branch.<name> via lower-level ref handling with validation), refuse or delimit ambiguous names, and require a full record parse before treating a first line as C3's.
- **F03-4** [minor] `crates/c3/src/setup/repo_models.rs:72`, `crates/c3/src/setup/doctor.rs:89` - The repair normalizes more than the model value (drops quotes and trailing comments) and misses 'model :' with a space before the colon in both detection and repair. Trigger: An agent file with model: claude-opus-4-8 # comment, quoted ids, or 'model :' spacing. Evidence: read-code: agent_model trims and strips quotes; replace_frontmatter_model_value matches line.strip_prefix('model:') at column 0 and rewrites the whole line as 'model: <alias>', discarding an inline comment.; read-code: agent_model uses line.strip_prefix('model:') as well, so an indented or spaced key is never detected as a pin. Verify: Run doctor and --fix on such files and diff byte-for-byte beyond the value. Remedy: Preserve the original quoting/comment (replace only the value span) or document the normalization; also handle the spaced/odd-key forms consistently between doctor and repair.
- **F03-5** [minor] `crates/c3/src/telemetry/classes.rs:329` - Telemetry lowercases then strips only lowercase '[1m]', so an uppercase suffix such as 'opus[1M]' misses the closed list and is sent as 'other'. Trigger: A roster or event model spelled 'opus[1M]' or 'CLAUDE-...' variants. Evidence: read-code: model_token_closed lowercases the model but strips strip_suffix('[1m]') case-sensitively, while the claude table lookup treats the suffix case-insensitively elsewhere. Verify: Call the token function with opus[1M] and check the result is the table spelling, not other. Remedy: Strip the suffix case-insensitively (or compare case-insensitively) to match the roster rule.
- **F03-6** [note] `crates/c3/src/setup/branches.rs:468`, `crates/c3/src/setup/branches.rs:543` - The 'read-only' branch paths still touch the repository (index refresh, lock files), and the full listing cost is one git cherry plus one rev-list per unmerged branch with no cap. Trigger: A repository with many unmerged branches, or a caller that assumes zero writes from a read path. Evidence: read-code: Full scan runs git cherry and rev-list --count per unmerged branch when opts.content is set; hook uses content:false but the listing pays it.; read-code: Dirty count runs git status --porcelain, which refreshes the index stat info; config/worktree reads take locks. Verify: Time c3 branches on a many-branch fixture and watch index mtime/lock files during read commands. Remedy: Document the index/lock side effects instead of claiming read-only, and cap or parallelize the per-branch cherry work or warn on large repos.

### Prior findings

_(none)_

## Verdict: HOLD

One blocker breaks the read-only reviewer guarantee on the claude engine, plus worktree-removal and branch-record risks need hardening before 0.2.5.

### Blockers

- **F03-1** `crates/c3-core/src/claude.rs:313`, `crates/c3/src/engines/mod.rs:61` - The claude engine's allow-listed child environment drops C3_CHILD, so a reviewer/worker on the claude engine does not see C3_CHILD=1 and the hook repair is not suppressed there. Verify: Unit-test child_environment() with C3_CHILD=1 in vars and assert it survives; then run a claude-engine child and check the hook does nothing. Remedy: Add C3_CHILD to the claude child environment explicitly (set C3_CHILD=1 alongside DISABLE_AUTOUPDATER=1 in child_environment/probe paths, or allow-list it), and add a test asserting a claude reviewer child sees it.

### Unproven scenarios

- Whether the maintain pass compares vendor-prefixed catalog ids after strip_vendor before successor().
- Whether --dry-run truly performs no smoke/roster/state write and --models leaves next_due and engine records alone (call sites not read).
- Whether the effort-family acceptance matches only same prefix/suffix with version not lower and refuses all other unlisted ids.
- Whether every launcher path (reviewer, worker, probe, detached maintain) sets C3_CHILD=1, especially through the claude allow-list.
- Behavior on detached HEAD, missing upstream, non-main default, odd branch names, and many-branch cost.
- Whether Windows junctions always report is_symlink (kimi unlink safety depends on it).

### First-run checklist (observable)

_(none)_
