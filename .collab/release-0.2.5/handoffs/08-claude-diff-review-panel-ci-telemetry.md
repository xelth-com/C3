# Handoff 08 - Claude: diff-review of 0.2.5, part 4 of 5 - panel seating, CI and hook, telemetry

Date: 2026-11-22. Range: `8e6fd3e..08ae5d5` (base `8e6fd3e`, head `08ae5d5`; one commit).

You run in a throwaway checkout of the head commit of this range; the change itself is the patch
file named in your prompt (`.collab/` and `.eck/` left out). You may have no shell: read the patch
and the files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, send something private off the machine, or report a
result that is not true (a failed reviewer shown as usable, a usable one shown as failed) - not
style.

## Delta since the last review

A first panel reviewed `v0.2.4..ba43ff6` and returned 36 findings. This range holds (a) the fixes
for the findings on telemetry and the kimi worktree removal (F02-19, F02-20, F02-21, F03-2, F03-5,
F04-5, F04-8 for the Claude-id shape) and (b) the tech-debt items C1 and C2. One commit
(`08ae5d5`), three pieces:

- **C1. One vendor per panel** (`crates/c3/src/panel/plan.rs`, `crates/c3/src/panel/routing.rs`
  (new), `crates/c3/src/panel/run.rs`, `crates/c3/src/cli/router.rs`,
  `crates/c3/src/router/replay.rs`, `crates/c3-core/src/ledger.rs`). A panel seats at most one
  roster entry per lab; on a weighty purpose one seat goes to a `weighty` entry; the plan and
  `c3 router explain` name a skipped "lab already seated"; the ledger records the rule.
- **C2. CI, hook, finish** (`.github/workflows/ci.yml`, `crates/c3/src/ci_check.rs` (new),
  `crates/c3/src/cli/finish.rs`, `crates/c3/src/hook/mod.rs`, `crates/c3/src/setup/rules.rs` (new),
  `plugin/hooks/c3-hook.ps1`, `plugin/hooks/c3-hook.sh`). `c3 finish` and the hook name a red CI
  run through `gh`; `c3 doctor --fix` appends a missing section to `02-delegation.md`; both hook
  scripts forward every `c3: `, `codex-consult: ` and `CI: ` line of `c3 hook` (before, four
  whitelisted lines) and print a waiting rule.
- **P4. Telemetry and kimi removal** (`crates/c3/src/telemetry/classes.rs`,
  `crates/c3/src/engines/kimi.rs`, `crates/c3/src/maintain/{models,web}.rs`). A private model name
  is no longer sent by name; the kimi worktree is removed without acting on a link.

## CURRENT invariants claimed

- A panel seats one entry per lab (the entry's `lab`, a claude endpoint's host, else the model
  id's prefix; an unknown one is a lab of its own). A size above the lab count is cut with a
  warning; every `--require`d entry is still seated (two of one lab with a warning); `--panel-all`
  warns once; the routed bare default and `C3_PLUGIN_COMPAT=1` keep the old draw.
- On framing, decision, core-contract, acceptance and stuck, a panel of two or more seats gives one
  seat to an eligible `weighty` entry unless a required entry already is one.
- `c3 finish` and the hook print `CI: the latest run on <default> failed - ...` only for a failed
  run; cancelled or skipped decides nothing; never a refusal; `gh` is killed past its deadline
  (10 s finish, 3 s hook). The hook asks at most once a day per repository, never in a C3 child.
- `doctor --fix` appends exactly the missing section through a temp file and a rename; a link, a
  hard-linked file or a concurrent edit is refused; a user-rewritten file is only warned about.
- Both hook scripts forward each `c3: ` / `codex-consult: ` / `CI: ` line once, in order, at most
  10, each cut to 400 characters, then the waiting rule; the two print byte-identical text.
- Telemetry sends a model id by name only if it is in the vendor's closed list, or published by
  the built-in catalog (openrouter.ai) or a configured source declared `"public": true`, read at
  send time, for the vendor class its source attributes it to. A Claude-shaped id goes out only
  from `CLAUDE_MODELS` or that set; an endpoint's or engine's list never confers it.
- The kimi worktree removal treats every reparse point as a link, never descends into it, and
  after any error, left link or linked root runs no `worktree remove`, prune or `remove_dir_all`.

## Evidence

- Local suite after `08ae5d5`: fmt, clippy `-D warnings`, every test binary of c3-core, c3 and
  c3-cli passed. CI green (run 45069617073, second attempt); in the first attempt the Windows job
  was cancelled inside the `work` test `a_delayed_status_recovery_cannot_overwrite_a_live_resume_claim`
  and passed on rerun; the cause was not found.
- Tests: telemetry end to end through `c3 telemetry --backfill-ratings` (11 reviewer cases); a
  junction to an outside sentinel; both hook scripts run with a fake `c3`.
- Known and left open: a new Claude model stays `other` in telemetry until a release; the removal
  walk's re-check is by path; the `.sh` script cuts non-ASCII lines by bytes.
- NOT exercised live: the CI check against a real GitHub run, the hook scripts in a real plugin
  session, a seating change on a real panel, a telemetry send to the real server. Nothing in this
  part was exercised against a real reviewer CLI.

## Questions

- **Q1. Seats (C1).** Can the one-lab-per-panel rule leave a panel short of seats without a clear
  warning, or silently drop a required reviewer (two of one lab, a reserve-held entry, a weighty
  entry that duplicates a lab)?
- **Q2. Labs and records (C1).** Name any entry whose lab resolves wrongly (two makers merged, one
  split, an unknown id, a claude endpoint host), and any case where `panel.routing`,
  `c3 router replay` or `router explain` shows other seats than the real ones. Does a CHANGELOG
  sentence claim more than the code does?
- **Q3. CI check (C2).** Can the check hang, run a process other than `gh`, write outside the
  codex home, race a second session on `ci-check.json`, run in a C3 child, or print something
  untrue (a cancelled run as failed, another branch, a stale cache)? A run title can come from a
  stranger: can it carry control characters or instructions into a session?
- **Q4. Hook scripts (C2).** Can forwarding drop a line that matters (the cap of 10), duplicate
  one, cut a multi-byte character, make the `.ps1` and `.sh` outputs differ, or fail a session
  start?
- **Q5. File operations (C2, P4).** Name any path where the delegation-rule edit or the kimi
  removal writes or deletes outside its target (a symlink, junction, `..`, hard link, a link made
  between check and act), loses line endings, or leaves a temp file.
- **Q6. Telemetry privacy (P4).** Name any path where a private endpoint model name, a model of an
  unknown vendor or another private string is sent by name: trust read at send time, case folding,
  the `[1m]` suffix, the Claude-shape rule, a slug bound to the wrong class.
- **Q7.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
