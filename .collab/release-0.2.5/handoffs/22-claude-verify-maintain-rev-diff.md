# Handoff 22 - Claude: verification of the 0.2.5 review fixes, part 2 of 3 - maintain, --rev, --diff

Date: 2026-11-23. Range: `99e2da7..5d92ca7` (base `99e2da7`, head `5d92ca7`; one commit).

You run in a throwaway checkout of the current head of the repository, which contains this commit
and the later ones; the change to verify is the patch file named in your prompt (`.collab/` and
`.eck/` left out). You may have no shell: read the patch and the files it names with your file
tools. Everything is in this repository.

## Question

Are the findings listed below really fixed by this commit, and did a fix introduce a new defect? A
finding is fixed only when the failure its trigger describes can no longer happen - not when a
comment or a CHANGELOG sentence says so.

## Delta since the last review

A first panel read the changes after tag `v0.2.4` in five parts. This commit fixes two: the `c3
maintain` part (F14-n, F15-n, plus F16-1, F17-5) and the `--rev` / `--diff` part (F18-n, F19-n,
F20-n). One line per item: ids - defect - fix - main files.

`c3 maintain` (`crates/c3/src/maintain/`):

- F15-1 - catalog fetch could reach a local or private address - IP-literal and `localhost` hosts
  refused; private peer discarded before the body, also at lookup and every hop (`web.rs`)
- F14-1 - the 15 s deadline checked only between hops - one deadline over connect and body chunks
- F15-2, F14-2 - "older" refusals evicted at the cap - kept while seated (`models.rs`)
- F15-3 - smoke ran on an entry edited since it was chosen - `bound_roster` checks provider AND
  model (`smoke.rs`)
- F14-3 - one-entry roster written at a predictable path - own scratch directory, `create_new`
- F14-4 - protection fields read narrower than the router - the router's `validate_roster`
- F14-5 - a future-dated candidate could qualify - a stamp later than now + 600 s never does
- F15-5 - pending recovery could install an older release - installed version re-read afterwards
  (`mod.rs`)
- F15-6 - `C3_MAINTAIN_MODELS=off` still wrote a state directory - switch read before the lock
- F16-1, F17-5 - telemetry trusted a host - trust per full catalog URL, legacy host keys untrusted

`--rev` / `--diff` (`consult/reviewed.rs`, `engines/worktree.rs`, `consult/http.rs`):

- F19-1, F20-1 (blocker) - http engine sent the LIVE tree under `--rev`, no patch in its pack - a
  worktree of the revision feeds the pack; the patch is a bound part of it
- F19-2, F18-4, F20-6 - a patch could carry secrets or ignored files - redactor, ignore rules
- F19-3, F20-3 - runs shared one patch name - `<stem>.<sha8>.patch`, exclusive create
- F18-1, F19-6 - records named the live HEAD - they name the revision read, `live_head` beside it,
  open findings withheld when it differs (`orchestrate/ledger.rs`, `consult/prompt.rs`)
- F18-2, F20-2 - set-up could take the live index lock - `GIT_OPTIONAL_LOCKS=0`
- F18-3 - the bridge's own patch counted as a tree change - snapshot after the patch
- F18-5 - staging read through links - a brief link refuses, other links skipped and counted
- F18-6 - a link of the revision led out of the worktree - such links unlinked after checkout
- F20-4 - the worktree check ignored ignored paths - `--ignored`
- F18-7 - prompt claimed handoffs it did not stage - claim follows the staging
- F19-5 (second half) - `--rev` / `--diff` values could become git options - one validation for
  CLI and MCP, `--end-of-options` (`reviewed.rs`, `mcp/mod.rs`)
- (no id) - the agy denial-retry prompt lacked the reviewer-role sentence - added
- F19-4, F20-5 - already fixed by the previous commit (owner record; isolation paragraph); confirm.

Not fixed by decision:

- F15-4 (roster compare-then-rename window) - decided boundary: passes are serialised by a lock;
  what remains needs a hand edit inside it.
- F19-5, first half (a consult started in a C3 child writes its records) - rejected: writing its
  own handoff is what a consultation does.

## CURRENT invariants claimed

- The public catalog fetch never connects to an IP literal or `localhost`, discards an answer from
  a local, private or unknown peer before its body, and ends at 15 s overall.
- An id is sent by name only when its source URL is the built-in catalog or declared public.
- An http reviewer under `--rev` reads a pack built from the revision's worktree; with no worktree
  nothing is sent, never the live tree.
- Every `--diff` patch passes the redactor; the ledger's sha256 is of the file as written; a patch
  file is never overwritten and never written through a link.
- Under `--rev` the `Reviewed:` line, ledger `base_commit` and each finding's `source.base_commit`
  name the revision read; open findings are offered for re-checking only when it is the live HEAD.
- Staging never reads through a link; links in the worktree that lead out or dangle are unlinked;
  the worktree check includes ignored paths.
- Review set-up, checks and the patch never take the live index lock.
- `rev` and `diff` values are one token, not leading `-`, at most 256 characters, checked before
  anything starts, passed to git after `--end-of-options`.
- A seated model's "older" refusal is kept; the smoke runs only on the entry it was chosen for;
  recovery installs only a release newer than what is installed now; `MODELS=off` writes nothing.

## Evidence

- Local checks: fmt, clippy; c3-core all green; c3 lib 816 of 817 (the failure is a 5-second
  wall-clock test: 9.7 s under load, 1.5 s alone); c3-cli `mcp_integration` 2,
  `telemetry_parity` 11, `kimi_engine` 20, `claude_engine` 18. c3-cli `maintain` was not verified
  locally (another session's real reviewer process runs there).
- CI for `5d92ca7`: green on Linux and Windows, `maintain` included.
- NOT covered by a test: the live-tree agy half of the own-patch item (F18-3); an end-to-end http
  run (`--base-url` must be https).

## Questions

- **Q1.** For each finding id in the table: `fixed`, `still-open` or `partly`, with `path:line` of
  the code that decides it. Do not report on ids outside this part.
- **Q2.** Can a new refusal fire for a legitimate case (a public catalog behind a CDN or IPv6
  address, a redirect hop, a proxy; a revision spelled differently from HEAD; a brief inside a
  normal directory), or can a hostile one still pass (rebinding between lookup and connect, a
  value that reaches git before validation)?
- **Q3.** Does an error path leave something behind: a lookup thread past the deadline, a patch or
  its temp hard link, a scratch directory, a worktree, a lock?
- **Q4.** Does a fix change behaviour for runs it did not mean to touch: a run without `--rev`
  (prior findings, ledger keys), older state files, `telemetry_parity`, an http run without a patch?
- **Q5.** Can redaction miss a secret or mangle the patch so the recorded sha256 or count is wrong?
  Can the withheld-findings rule fire for the live HEAD, or fail to fire for another revision?
- **Q6.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
