Write in English.

# Handoff 88 - claude: acceptance for C3 0.2.3, first pass

Date: 2026-10-22. Base commit: `59ea25a` on `main`, tagged `v0.2.3` locally (not pushed, not announced - the tag
waits for your verdict; a HOLD moves the tag). The range under review is `v0.2.2..v0.2.3` = `7416e04..59ea25a`
(commits 25d1beb, f1a68f0, 59ea25a); skip `.collab/`. 32 files, +4878/-57, almost all of it new code under
`crates/c3/src/work/`. You hold the acceptance (0.2.2 was your pass 7, handoff 87).

## What 0.2.3 adds (CHANGELOG `[0.2.3] - 2026-10-22`, `docs/work-mode.md`)

1. **`c3 work` - worker runs.** A roster entry with role `"worker"` is run as an ordinary coding worker on the
   tree: it edits and reports; NOT a consultation (no evidence check, no findings, no rating, no ledger entry). The
   prompt is a fixed preamble (never git, never commit, final report) + the brief + `--prompt`. The child runs with
   write tools under an allow list (`Bash(cargo test|check|clippy|fmt:*)` by default, `--allow` replaces it; git
   refused). Record: `<collab>/<task>/work/<NN>-<provider>-<model>/` with `brief.md`, `events.jsonl`, `report.md`,
   `diff.txt` (git status/diff computed by c3, never by the worker) and `run.json` (status, turns, tool calls,
   files changed, usage - no secret values). One live run per task; `--detach`/`--status`/`--wait` as the consult
   flags (status exit 0 done / 1 died / 2 running). Code: `crates/c3/src/work/mod.rs` (1357 lines),
   `work/record.rs`, `work/detach.rs`, `cli/work.rs`; tests `crates/c3-cli/tests/work.rs` (1003 lines, fake
   claude/codex/muse binaries).
2. **Quota, resume and the alarm.** A worker that hits its provider's usage limit is classified `quota` (rejecting
   `rate_limit_event` or the limit wording via the engine's shared `quota_evidence`); `run.json` records
   `quota {text, reset_at, resume_at, resumes, exhausted}`; the SAME child session is resumed (`claude --resume`,
   `codex exec resume <thread>`, muse `--session-id`) when the window reopens - automatically in a detached run
   (parks `waiting_quota`, 30 s slices until `resume_at`, a kick file wakes it) or by `c3 work --resume <id>` /
   `--kick`. Foreground: prints the resume time and command, exit 3. `--max-resumes` (default 3) then `exhausted:
   true`. Window without a provider reset time: `--quota-window-sec` (default 18000) or roster `ext.quota_window_sec`.
3. **v1.1 engines.** The worker may drive `codex` (`codex exec --sandbox workspace-write`, prompt on stdin, thread
   from the stream) and `muse` (`--approval-mode never --trust-workspace --disable-sandbox`, minted `--session-id`)
   beside `claude`; `select_worker` accepts the three, the refusal names them. Code: `crates/c3/src/work/engines.rs`
   (574 lines), `crates/c3-core/src/claude.rs`, `roster.rs`/`roster_ext.rs` (`ext.quota_window_sec`).
4. **R30 compaction pieces (59ea25a).** After `c3 consult --detach` / `--panel --detach` one extra stderr line
   names the `/compact` wave boundary and the `--status` command (`consult/detach.rs`, end of
   `start_detached_run`; not in dry run, not in the background child, not in a panel member). SessionStart source
   `compact` (hooks.json matcher `startup|resume|compact`; `c3 hook --source compact`, hidden flag) prints one
   re-hydration line from `CODEX_CONSULT_COORDINATOR` resolved against the roster (`hook/mod.rs`
   `compaction_line`; no network, nothing written; startup/resume output unchanged).
5. **Small.** `c3_core::engine::SubprocessEngine::{run, continue_turn}` return `EngineError::PlanOnly` instead of
   `unimplemented!()` (`c3-core/src/engine.rs`); `muse-wonder-1.0` in the effort vocabulary and the telemetry vendor
   table (f1a68f0); `.gitattributes` `* text=auto`.

Invariants claimed unchanged: files are the record; reviewers read-only (a worker is not a reviewer - it is the
one path that writes the tree, by design, under the allow list); C3 never commits; keys only from the environment;
one sanitizer for outbound paths; no secret value in any record.

## Checks run (all at 59ea25a)

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace` (default features) | 0 | 865 passed / 0 failed, 30 sets |
| clippy | `cargo clippy -j 2 -p c3 -p c3-core --all-targets` | 0 | 0 warnings |
| installed binary | `cargo install --path crates/c3-cli --locked --force` | 0 | `c3 0.2.3`; `--dry-run` writes nothing; `c3 hook --source compact` prints the line |
| live | codex worker proven with `gpt-6-astra` (`gpt-6.1-sol` not enabled on the account); muse worker proven live; quota resume proven on a detached claude run (2026-10-21) | - | per `docs/work-mode.md` |

Not run for 0.2.3: the pinned plugin harnesses (no consult-path change except the one added stderr line after a
detached launch, which no harness asserts on; `harness-detach` compares the status/wait lines, unchanged).

## Reading budget

No sub-agents; never print a whole file; at most 5 tool calls; the verdict even on an incomplete read. Suggested
order: `CHANGELOG.md` section `[0.2.3]`; then in `crates/c3/src/work/mod.rs` the allow-list / git-refusal
construction and what goes into `run.json` (search `allow`, `git`, `run.json`, `usage`); then
`work/engines.rs` for the codex and muse launch lines (search `workspace-write`, `disable-sandbox`); then
`consult/detach.rs` end of `start_detached_run`.

## Questions

- **Q1.** Worker safety: can the allow list be widened by the worker itself (a `--allow` value, a brief, or the
  roster `ext`) into git or a shell escape, and does `run.json`/`diff.txt` ever carry a secret value?
- **Q2.** Quota resume: is resuming the SAME child session after `resume_at` sound (no double run, no lost
  report), including the `--max-resumes` exhaustion and the kick file?
- **Q3.** Is the muse `--disable-sandbox` launch acceptable as a WORKER (it must write the tree) given the
  reviewer-path muse stays sandboxed, or must it be opt-in?
- **Q4.** Verdict: ACCEPT (tag v0.2.3 stands, pushed and announced), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
