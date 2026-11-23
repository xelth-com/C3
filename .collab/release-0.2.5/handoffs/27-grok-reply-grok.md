# Handoff 27 - Grok (grok): reply-grok

Date: 2026-11-23 05:55 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.77.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.77).
Preflight: ok: signed in (usable reply 21 min ago).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 0b9d2820-7dae-4c67-b93b-0b0c1d1f3b49.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-f50feef72e054498af65ca246d52e30b.txt --output-format streaming-json -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Isolation: the reviewer ran in a throwaway git worktree `C:\Users\Dmytro\AppData\Local\Temp\c3-kimi-wt-119d55c16f654b33a394c5e8453c93c4` of revision fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f (--rev fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f; removed after the run).
Parent thread: (none - new thread). Result thread: `01a206b4-1cc9-7c40-8067-b4c093e5f546` (source: events).
Brief: `.collab/release-0.2.5/handoffs/22-claude-verify-maintain-rev-diff.md` (sha256 2526f15c8fe2). Reviewed: fdf193a, base fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f, tree sha256 77688b041006, 0 changed files. Live HEAD at launch: fdf193a2bb3b3d85fbaf5abb1e45b115ea860a5f.
Bridge outcome: usable reply. Wall time: 815.9 s. Tokens: in 191563 (cached 4189056), out 58893, reasoning 47018.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Timeout: 2400 s (the default of purpose diff-review); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - Most of the maintain and --rev/--diff fixes hold, but an endpoint-only roster edit is still smoked, the new public-address gate still allows non-global targets, and the new link walk can leave the worktree. Findings: 0 blocker, 2 major, 1 minor, 1 note (F27-1..F27-4, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/27-grok-reply-grok.reply.json`.
Raw event stream: `handoffs/27-grok-reply-grok.events.jsonl`.
Verbatim reply follows.

---

## Q1

Maintain:

- F15-1 **fixed** at `maintain/env.rs:273` and `:303`, peer check `:422`, fetch refusal `maintain/web.rs:279`. IP literals, `localhost`, RFC1918, loopback, link-local, ULA and multicast never supply a body. Residual: finding #1.
- F14-1 **fixed** at `maintain/env.rs:203`, `:221`, `:379`. ureq 2.12 starts `timeout_connect` before DNS, so a slow lookup consumes that window rather than adding another 15s. The leftover thread is finding #4.
- F14-2 / F15-2 **fixed** at `maintain/models.rs:83`: a pair whose seated id is still active is never evicted.
- F15-3 **partly** at `maintain/smoke.rs:335`. Provider or seated-model changes fail closed. An endpoint or plan change that keeps both still clones the new object (`:351`).
- F14-3 **fixed** at `maintain/smoke.rs:476` (`create_dir` plus `create_new` inside it).
- F14-4 **fixed** at `maintain/smoke.rs:318` (`validate_roster`).
- F14-5 **fixed** at `maintain/models.rs:520` (`created > now+600` never qualifies; `+23h` is announce-only; exactly `+600s` still qualifies).
- F15-5 **fixed** at `maintain/mod.rs:1278` (fresh `installed_version`, no older release).
- F15-6 **fixed** at `maintain/mod.rs:630`, before `acquire_lock` (`:639`), which is what creates the directory. `state_dir` (`:80`) does not.
- F16-1 / F17-5 **fixed**. Catalog identity is the full URL; `maintain/models.rs:810` trusts only an exact source match. Legacy `web:<host>` matches nothing.

`--rev` / `--diff`:

- F19-1 / F20-1 **fixed**. `consult/reviewed.rs:135` gives http a worktree only under `--rev`. `consult/http.rs:226` packs that tree or refuses. `pack/reviewer.rs:350` binds the patch and refuses a budget miss before any request.
- F19-2 / F18-4 / F20-6 **fixed** at `consult/reviewed.rs:1055` (redactor) plus ignore / `.eckignore` exclusion. Untracked files that are not ignored stay in.
- F19-3 / F20-3 **fixed** at `consult/reviewed.rs:970`: `<stem>.<sha8>.patch`, exclusive create, no overwrite.
- F18-1 / F19-6 **fixed**. `consult/orchestrate/mod.rs` `reviewed_info` names the worktree sha; `:2636` withholds priors when that sha is not HEAD.
- F18-2 / F20-2 **fixed** at `engines/worktree.rs:33` and `:121` (`GIT_OPTIONAL_LOCKS=0` and `--no-optional-locks`).
- F18-3 **fixed** at `consult/orchestrate/mod.rs:3999` (snapshot after the patch) and `consult/orchestrate/tree.rs:220`. Live agy is untested. The prefix is finding #3.
- F18-5 **fixed** at `engines/worktree.rs:836`: a linked brief refuses; other links are skipped.
- F18-6 **fixed** for a link present at checkout (`engines/worktree.rs:612` unlinks outside and dangling targets before the reviewer). Finding #2 is a race the older walker already closed.
- F20-4 **fixed** at `consult/revision.rs:165` (`--ignored` with `-uall`).
- F18-7 **fixed** at `engines/worktree.rs:1102`: handoffs are named only after a real stage. `events.jsonl` is skipped.
- F19-5 git-option half **fixed** at `consult/reviewed.rs:106` and `:872`, and `engines/worktree.rs:72`. The `C3_CHILD` half still writes, by decision.
- Agy denial retry **fixed** at `consult/orchestrate/secondary.rs:102`.
- F19-4 **still holds**: `setup/branches.rs:894` fails the run if the owner record cannot be written; `engines/worktree.rs:131` removes that worktree.
- F20-5 **still holds**: `engines/worktree.rs:1034` counts untracked names and shows only prompt-safe tracked paths.

## Q2

A public name with a public A or AAAA record is fetched. A same-host redirect is followed and checked again. An IP-literal URL is refused on purpose, including a public address (`env.rs:303`). Any one non-public address in the lookup fails the whole name. ureq is given no proxy, so `HTTP_PROXY` is not used. A catalog clock more than 10 minutes fast makes a new id announce-only. `--rev main` when `main` is HEAD does not withhold: both sides are full shas (`mod.rs:2636`). A brief in a normal directory is staged.

`0.0.0.1` and `100.64.0.1` still pass (finding #1). After the resolver returns, ureq connects to those addresses and `remote_addr` is checked again, so a later rebinding to 127.0.0.1 does not. A `--rev` / `--diff` token with a space, a control character, a leading `-`, or more than 256 characters is refused before git. `ls-tree` and `cat-file` (`worktree.rs:744`, `:830`) omit `--end-of-options`; they see a sha or a token that already failed the dash check.

## Q3

`resolve_within` (`env.rs:242`) leaves the DNS thread running after the wait. A no-hard-link patch write that fails mid-`write_all` can leave a short file at the final name (`reviewed.rs:970`); a crash can also leave `<stem>.<uuid>.tmp`. The smoke scratch dir is removed on drop and is not deleted if `create_dir` fails; a kill can leave it. If outside-link removal fails and the follow-up removal fails, the worktree stays (`worktree.rs:149`). `MODELS=off` does not take the lock.

## Q4

Without `--rev`, http still packs the live tree. `live_head` is recorded only for `--rev`. Kimi and claude isolation without `--rev` now also set `GIT_OPTIONAL_LOCKS=0` and may gain the outside-link sentence. An old `web:<host>` record loads and is not trusted until a refetch, so telemetry can drop names 0.2.4 would have sent. `telemetry_parity` was updated for the URL key. An http run with no patch omits the patch section.

## Q5

`pack/redact.rs:14` misses a low-entropy password, a split secret, and any value that is not secret-shaped. Those still ride in a one-revision patch when the file is untracked and not ignored. The ledger sha256 is of the redacted bytes `place_patch` writes, and `redactions` is that replacement count. `from_utf8_lossy` (`reviewed.rs:1055`) can change a non-UTF-8 diff before the hash, so the file matches the hash and is not git's raw diff. Withholding uses full shas: it does not fire for another spelling of live HEAD, and it does fire when the commit differs. `Some(0)` still drops invented priors.

## Q6

Untested here: the live-tree agy half of F18-3, an https http `--rev` round trip, a real stalled resolver against the 15s clock, and the directory-swap in finding #2. The suite was not executed. Not read in full: `c3config` host checks, the rest of `build_with_extras`, and every test body.

## Requested checks

RC1. Working directory `crates/c3`. Read-only. Assert `is_public_address` on `0.0.0.1` and `100.64.0.1` (a one-off test or the existing address test). Both are `true` today, which confirms finding #1. Budget: 5 minutes.

RC2. Working directory `crates/c3`. Read-only unit call of `bound_roster` where provider and model match the chosen seat but `endpoint.base_url` changed (F15-3). `Ok` plus the new URL in the one-entry JSON confirms the residual. Budget: 5 minutes.

RC3. Disposable repo, workspace-write, Windows. During `unlink_outside_links`, replace a real subdirectory with a junction to a sentinel that holds a symlink, and see whether that outside symlink is removed (finding #2). Budget: 15 minutes.


---

### Findings

- **F27-1** [major] `crates/c3/src/maintain/env.rs:273`, `crates/c3/src/maintain/env.rs:257`, `crates/c3/src/maintain/env.rs:422` - is_public_address treats 0.0.0.0/8 (except 0.0.0.0) and 100.64.0.0/10 as public, and the IPv6 check does not cover deprecated site-local fec0::/10. A catalog hostname that resolves to 0.0.0.1 or 100.64.0.1 is looked up, connected, and its body is read. On Linux, 0.0.0.0/8 is a local route; 100.64.0.0/10 is non-global (CGNAT, and overlays such as Tailscale). Trigger: A configured https catalog or page hostname resolves to 0.0.0.1 or 100.64.0.1, or a same-host redirect does, and c3 maintain fetches it. Evidence: read-code: IPv4 rejection is only is_loopback, is_private (RFC1918), is_link_local, is_unspecified and is_multicast. 0.0.0.1 and 100.64.0.1 match none of those. IPv6 checks loopback, unspecified, multicast, fc00::/7 and fe80::/10 only.; read-code: The unit list of refused addresses does not include 0.0.0.1 or 100.64.0.1, and 172.32.0.1 is asserted public.; read-code: Both the pre-connect lookup and check_peer use is_public_address, so an address that function allows is connected and its body is read. Verify: RC1: assert is_public_address is false for 0.0.0.1 and 100.64.0.1. Today both are true. Remedy: Reject 0.0.0.0/8, 100.64.0.0/10, broadcast, and the other non-global IPv4 ranges, plus IPv6 site-local fec0::/10, in is_public_address, and add them to the address test.
- **F27-2** [major] `crates/c3/src/engines/worktree.rs:612`, `crates/c3/src/engines/worktree.rs:643`, `crates/c3/src/engines/worktree.rs:541` - unlink_outside_links lists a directory and later descends into it without the re-check that unlink_links_inside_with performs. If that directory is replaced by a junction or directory symlink before read_dir, the walk follows it and can unlink links in the target, which may be outside the worktree. canonicalize on a link also follows the link to decide whether it is inside. Trigger: During ThrowawayWorktree::create, after symlink_metadata has classified a child as a real directory, that path becomes a junction to another tree that contains a symlink. Evidence: read-code: The loop calls read_dir on a queued path with no real_dir_canonical inside-check. A non-link directory is pushed onto the stack. A link is classified with std::fs::canonicalize, which follows it.; read-code: The older walker re-checks that the directory still canonicalizes inside the worktree before read_dir and again before acting, and aborts otherwise. The new walker does not. Verify: RC3: swap a subdirectory for a junction to a sentinel during the walk and see whether a symlink in the sentinel is removed. Remedy: Use the same inside() re-check as unlink_links_inside_with before every read_dir, and abort the worktree if a directory no longer resolves inside it.
- **F27-3** [minor] `crates/c3/src/consult/orchestrate/tree.rs:220` - The collab tree check ignores every live path under <task>/handoffs/<patch-stem>. so an agy or muse reviewer can write another file with that prefix and the run still looks clean. The bridge's own patch and its temp file are a narrower set. Trigger: A live-tree agy run with --diff writes a file such as handoffs/<stem>.notes.md in the task directory and changes nothing else. Evidence: read-code: own_prefixes gains format!("{}/handoffs/{}.", task, stem), which matches any later suffix, not only .<sha8>.patch and the write temp. Verify: Run a fake agy consult with --diff that writes only handoffs/<stem>.notes.md and read tree_check_outcome. Remedy: Ignore only the written patch path and the <stem>.<uuid>.tmp pattern, not every name that shares the stem prefix.
- **F27-4** [note] `crates/c3/src/maintain/env.rs:242`, `crates/c3/src/maintain/env.rs:219` - When a public lookup exceeds the deadline, fetch returns, but the c3-maintain-dns thread is left blocked in the OS resolver until that lookup finishes. A redirect chain can leave one such thread per hop. The thread does not connect. Trigger: A catalog name lookup stalls longer than the time remaining in the 15s deadline. Evidence: read-code: The comment says the thread is left to finish on its own. On recv_timeout failure the function returns TimedOut and drops the receiver; the spawned lookup keeps running. Verify: Point a catalog at a resolver that sleeps 30s, time the fetch (it should error at 15s), and list c3-maintain-dns threads after the error. Remedy: Document the leftover as a bound, or detach the lookup so a late result cannot outlive the pass by more than the OS DNS timeout you accept. Killing the thread is not available.

### Prior findings

- F02-1 - not-checked - Outside this maintain/--rev/--diff review.
- F02-2 - not-checked - Outside this maintain/--rev/--diff review.
- F02-3 - not-checked - Outside this maintain/--rev/--diff review.
- F02-4 - not-checked - Outside this maintain/--rev/--diff review.
- F02-5 - not-checked - Outside this maintain/--rev/--diff review.
- F02-6 - not-checked - Outside this maintain/--rev/--diff review.
- F02-7 - not-checked - Outside this maintain/--rev/--diff review.
- F02-8 - not-checked - Outside this maintain/--rev/--diff review.
- F02-9 - not-checked - Outside this maintain/--rev/--diff review.
- F02-10 - not-checked - Outside this maintain/--rev/--diff review.
- F02-11 - not-checked - Outside this maintain/--rev/--diff review.
- F02-12 - not-checked - Outside this maintain/--rev/--diff review.
- F02-13 - not-checked - Outside this maintain/--rev/--diff review.
- F02-14 - not-checked - Outside this maintain/--rev/--diff review.
- F02-15 - not-checked - Outside this maintain/--rev/--diff review.
- F02-16 - not-checked - Outside this maintain/--rev/--diff review.
- F02-17 - not-checked - Outside this maintain/--rev/--diff review.
- F02-18 - not-checked - Outside this maintain/--rev/--diff review.
- F02-19 - not-checked - Outside this maintain/--rev/--diff review.
- F02-20 - not-checked - Outside this maintain/--rev/--diff review.
- F02-21 - not-checked - Outside this maintain/--rev/--diff review.
- F03-1 - not-checked - Outside this maintain/--rev/--diff review.
- F03-2 - not-checked - Outside this maintain/--rev/--diff review.
- F03-3 - not-checked - Outside this maintain/--rev/--diff review.
- F03-4 - not-checked - Outside this maintain/--rev/--diff review.
- F03-5 - not-checked - Outside this maintain/--rev/--diff review.
- F03-6 - not-checked - Outside this maintain/--rev/--diff review.
- F04-1 - not-checked - Outside this maintain/--rev/--diff review.
- F04-2 - not-checked - Outside this maintain/--rev/--diff review.
- F04-3 - not-checked - Outside this maintain/--rev/--diff review.
- F04-4 - not-checked - Outside this maintain/--rev/--diff review.
- F04-5 - not-checked - Outside this maintain/--rev/--diff review.
- F04-6 - not-checked - Outside this maintain/--rev/--diff review.
- F04-7 - not-checked - Outside this maintain/--rev/--diff review.
- F04-8 - not-checked - Outside this maintain/--rev/--diff review.
- F04-9 - not-checked - Outside this maintain/--rev/--diff review.
- F10-1 - not-checked - Outside this maintain/--rev/--diff review.
- F10-2 - not-checked - Outside this maintain/--rev/--diff review.
- F10-3 - not-checked - Outside this maintain/--rev/--diff review.
- F10-4 - not-checked - Outside this maintain/--rev/--diff review.
- F10-5 - not-checked - Outside this maintain/--rev/--diff review.
- F10-7 - not-checked - Outside this maintain/--rev/--diff review.
- F11-1 - not-checked - Outside this maintain/--rev/--diff review.
- F11-2 - not-checked - Outside this maintain/--rev/--diff review.
- F11-3 - not-checked - Outside this maintain/--rev/--diff review.
- F11-4 - not-checked - Outside this maintain/--rev/--diff review.
- F11-5 - not-checked - Outside this maintain/--rev/--diff review.
- F11-6 - not-checked - Outside this maintain/--rev/--diff review.
- F11-7 - not-checked - Outside this maintain/--rev/--diff review.
- F11-8 - not-checked - Outside this maintain/--rev/--diff review.
- F11-9 - not-checked - Outside this maintain/--rev/--diff review.
- F11-10 - not-checked - Outside this maintain/--rev/--diff review.
- F11-11 - not-checked - Outside this maintain/--rev/--diff review.
- F11-12 - not-checked - Outside this maintain/--rev/--diff review.
- F12-1 - not-checked - Outside this maintain/--rev/--diff review.
- F12-2 - not-checked - Outside this maintain/--rev/--diff review.
- F12-3 - not-checked - Outside this maintain/--rev/--diff review.
- F12-4 - not-checked - Outside this maintain/--rev/--diff review.
- F13-1 - not-checked - Outside this maintain/--rev/--diff review.
- F13-2 - not-checked - Outside this maintain/--rev/--diff review.
- F13-3 - not-checked - Outside this maintain/--rev/--diff review.
- F13-4 - not-checked - Outside this maintain/--rev/--diff review.
- F13-5 - not-checked - Outside this maintain/--rev/--diff review.
- F13-6 - not-checked - Outside this maintain/--rev/--diff review.
- F13-7 - not-checked - Outside this maintain/--rev/--diff review.
- F13-8 - not-checked - Outside this maintain/--rev/--diff review.
- F14-1 - fixed - One deadline covers lookup wait, connect and body chunks. The DNS thread can outlive the return (finding #4).
- F14-2 - fixed - merge_older no longer evicts a pair whose seated id is still active.
- F14-3 - fixed - Scratch directory is exclusively created; the roster file is create_new inside it.
- F14-4 - fixed - bound_roster uses validate_roster, so non-integer reserve and non-bool on_request refuse.
- F14-5 - fixed - A stamp later than now+600s does not qualify. +600s still does.
- F15-1 - fixed - Stated classes and post-connect peer checks hold. 0.0.0.0/8 and 100.64.0.0/10 still pass (finding #1).
- F15-2 - fixed - Protected older pairs survive the 500 cap while the seated id stays active.
- F15-3 - still-open - Model and provider changes are refused. An endpoint or plan edit that keeps both is still smoked.
- F15-5 - fixed - Recovery compares the vendor release with the version installed after rollback.
- F15-6 - fixed - MODELS=off returns before the lock that creates the state directory.
- F16-1 - fixed - Trust is the full catalog URL. A private URL on a host that also has a public URL is not trusted.
- F16-2 - not-checked - Outside this maintain/--rev/--diff review.
- F16-3 - not-checked - Outside this maintain/--rev/--diff review.
- F16-4 - not-checked - Outside this maintain/--rev/--diff review.
- F16-6 - not-checked - Outside this maintain/--rev/--diff review.
- F16-7 - not-checked - Outside this maintain/--rev/--diff review.
- F16-8 - not-checked - Outside this maintain/--rev/--diff review.
- F16-9 - not-checked - Outside this maintain/--rev/--diff review.
- F16-10 - not-checked - Outside this maintain/--rev/--diff review.
- F17-1 - not-checked - Outside this maintain/--rev/--diff review.
- F17-2 - not-checked - Outside this maintain/--rev/--diff review.
- F17-3 - not-checked - Outside this maintain/--rev/--diff review.
- F17-4 - not-checked - Outside this maintain/--rev/--diff review.
- F17-5 - fixed - Same URL-key fix as F16-1. Legacy host keys are not published.
- F17-6 - not-checked - Outside this maintain/--rev/--diff review.
- F17-7 - not-checked - Outside this maintain/--rev/--diff review.
- F17-8 - not-checked - Outside this maintain/--rev/--diff review.
- F17-9 - not-checked - Outside this maintain/--rev/--diff review.
- F17-10 - not-checked - Outside this maintain/--rev/--diff review.
- F18-1 - fixed - Reviewed line, ledger and new findings use the worktree revision; live HEAD is a side field.
- F18-2 - fixed - Worktree git calls set GIT_OPTIONAL_LOCKS=0 and status uses --no-optional-locks.
- F18-3 - fixed - Collab snapshot is after the patch write. Live agy was not tested. Prefix is broader than the patch (finding #3).
- F18-4 - fixed - Ignored and .eckignore paths are omitted. Other untracked files remain, then pass the redactor.
- F18-5 - fixed - A linked brief refuses the run. Linked handoffs and state.md are skipped, not followed.
- F18-6 - fixed - Links present at checkout that lead outside or dangle are unlinked before the reviewer. A swap during the walk is finding #2.
- F18-7 - fixed - The prompt names handoffs only when files were staged. events.jsonl is not staged.
- F19-1 - fixed - http --rev packs the revision worktree and refuses when none was made.
- F19-2 - fixed - Same patch redaction and ignore exclusion as F18-4 / F20-6.
- F19-3 - fixed - Patch path includes the content hash and is not overwritten.
- F19-4 - fixed - Owner-record failure still removes the worktree and fails the run. This commit did not weaken it.
- F19-5 - still-open - Option injection is refused before git. A C3_CHILD consult still writes its handoff, left open by decision.
- F19-6 - fixed - Same bookkeeping as F18-1. Open findings are withheld when the reviewed sha is not live HEAD.
- F20-1 - fixed - The --diff patch is a pack section. A patch that misses the budget refuses the run before a request.
- F20-2 - fixed - Same optional-lock guard as F18-2 on worktree setup.
- F20-3 - fixed - Same content-addressed exclusive patch file as F19-3.
- F20-4 - fixed - Worktree before/after status includes ignored files.
- F20-5 - fixed - Untracked names are counted, not printed. Prompt-safe tracked paths are still shown. This commit did not put raw names back.
- F20-6 - fixed - Same as F19-2: ignored and .eckignore files stay out; remaining text is redacted.
- F24-2 - not-checked - Outside this maintain/--rev/--diff review.
- F24-4 - not-checked - Outside this maintain/--rev/--diff review.

## Verdict: HOLD

Most of the maintain and --rev/--diff fixes hold, but an endpoint-only roster edit is still smoked, the new public-address gate still allows non-global targets, and the new link walk can leave the worktree.

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
- **F19-1** (prior, fixed) `crates/c3/src/consult/reviewed.rs:103` - c3 consult --rev with the http engine warns but still sends a pack of the live working tree to a remote model, while the operator asked for a frozen revision.

### Unproven scenarios

- The live-tree agy half of F18-3 was not executed; the snapshot order was only read.
- No https end-to-end http --rev run was inspected; the pack is built in a unit test only.
- A real stalled resolver was not timed. The 15s bound is from the code and ureq 2.12 connect_host, which starts timeout_connect before DNS.
- Whether 0.0.0.1 connects to a local service on Windows was not run. The classifier allows it on every OS.
- The directory-swap race in finding #2 was not executed.
- Tests in the patch were read in part and not run.

### First-run checklist (observable)

_(none)_
