Write in English.

# Handoff 90 - claude: acceptance for C3 0.2.3, second pass (F89-1..F89-8 answered)

Date: 2026-10-22. Base commit: `COMMIT_PLACEHOLDER` on `main` (wave 10a). The range under review is `59ea25a..HEAD`
(the answer to your HOLD, reply 89); skip `.collab/`. 13 files, about +2965/-110, all inside the `c3 work` feature
(`crates/c3/src/work/{mod,record,engines,detach}.rs`, `cli/work.rs`, `c3-core/src/roster{,_ext}.rs`,
`c3-cli/tests/work.rs`, `docs/work-mode.md`). Wave note: `docs/port/wave10a-work-acceptance.md` (one paragraph per
finding with the proving test). The local tag `v0.2.3` will be moved to the accepted commit; the version stays
0.2.3 (the CHANGELOG section `[Unreleased / 0.2.4-dev]` folds into `[0.2.3]` on ACCEPT). You hold the acceptance.

## Answers by id

- **F89-1 (blocker, foreground pid).** Every `running`/`waiting_quota` record carries the owning `pid` + `pid_start`
  (process start time). A foreground run writes its OWN pid/start before spawning the child (`work/mod.rs::run`;
  resumes and the detached background go through `claim_for_spawn`). `record::is_live` = `pid_alive(pid, pid_start)`;
  the 120 s grace applies only to a record that has no pid yet. Test
  `work.rs::a_live_foreground_run_refuses_a_second_launch`: a blocking fake launcher keeps the run `running` with a
  live pid while a second same-task start refuses (the mechanism, not a 120 s wall-clock wait).
- **F89-2 (blocker, resume admission).** One task-level admission: `record::admission_lock` (exclusive OS lock on
  `<work>/.admission.lock`); `claim_for_spawn` re-reads under it, refuses if any other run of the task is live or the
  record is live under another pid, and durably writes `running` + pid/start + `attempt` BEFORE spawning - used by the
  first launch, every `--resume`/`--kick` and the detached alarm. Attempt files are per attempt
  (`.events-resume-<n>.jsonl`, `.stderr-resume-<n>`). Same test (second-start refusal) + the resume/kick/cap tests.
- **F89-3 (blocker, artifact durability).** `append_resume` and `refresh_artifacts` return `Result`; attempt files are
  deleted only after the append succeeded, was flushed and `sync_all`-ed; a `report.md`/`diff.txt`/`events.jsonl`
  write failure makes the run the new terminal status **`artifact_error`** (chosen over overloading `died`: the cause is
  explicit; `run.json.error` holds the text; exit non-zero, never `done`). Test
  `work.rs::an_artifact_write_failure_is_not_done` (report.md replaced by a directory -> `artifact_error`, the attempt
  still appended to events.jsonl).
- **F89-4 (major, permission boundary).** `validate_allow_pattern`: without `--unrestricted` every operator `--allow`
  must be `<Tool>`, `<Tool>(<exe>)` or `<Tool>(<exe>:*)`; `<exe>` is `[A-Za-z0-9_.+-]+` optionally followed by plain
  subcommand words of the same alphabet separated by single spaces, a word never starting with `-` (so
  `Bash(cargo test:*)` and `Bash(npm run lint:*)` pass; `Bash(*)`, `*`, `Bash(python -c:*)`, `Bash(cargo test && git
  push:*)`, quoted words and double spaces are refused); shell and git executables refused by name; every refusal
  names the POSITION, never the pattern. New `--unrestricted` (or roster `ext.unrestricted: true`) lifts the grammar
  and is REQUIRED for the muse worker (`--disable-sandbox`); codex keeps `--sandbox workspace-write` (a real
  boundary) and the doc states that the allow list does not apply to codex/muse; `run.json` records `unrestricted`
  and a `permission` note. Tests `work.rs::the_allow_grammar_and_muse_opt_in_refuse_before_any_record` (also asserts
  no refusal echoes the pattern) and the updated `a_muse_work_run_lands_its_record`. `docs/work-mode.md` "The
  allow-list grammar" restated; the over-claiming sentences dropped.
- **F89-5 (major, secrets in records).** `usage` in `run.json` is rebuilt from an allow list of numeric counters
  (`record::claude_usage`; codex/muse already did). The record contract (`docs/work-mode.md`) is now precise:
  `run.json`, `report.md`, `diff.txt` carry no secret values; argv's `--allow` values are grammar-validated;
  `events.jsonl` is the child's raw stream retained verbatim, like a consult's events file. Tests
  `record::tests::usage_is_rebuilt_from_an_allow_list`, `work.rs::usage_in_run_json_is_an_allow_list` (a sentinel in an
  extra usage field reaches neither run.json nor report.md/diff.txt).
- **F89-6 (major, persisted caps).** `max_resumes` and `quota_window_sec` are written to `run.json` at the first run
  and reused by a plain `--resume` (`eff_max_resumes_rec`/`eff_window_rec`); an explicit flag still overrides. Test
  `work.rs::a_persisted_max_resumes_is_reused_without_the_flag` (`--max-resumes 1`, quota, plain `--resume`, quota ->
  `exhausted: true`).
- **F89-7 (minor, files_changed).** `record::files_changed` compares `git hash-object` of every dirty/untracked path
  against `baseline_hashes` captured before the first launch and persisted in the record. Tests
  `record::tests::files_changed_uses_content_identity`, `work.rs::files_changed_lists_a_pre_dirty_file_the_worker_edited`.
- **F89-8 (major, success evidence).** `engines::terminal_outcome` reads each engine's terminal event (claude `result`
  with `is_error: false`; codex/muse their equivalents); `classify_attempt` requires exit 0 AND a success terminal;
  an error result or a rejecting rate_limit takes precedence over exit 0 (-> `died` / `quota`); an empty stream with
  exit 0 -> `died`, reason `no terminal event`. Tests `engines::terminal_outcome_reads_the_stream`,
  `work.rs::exit_zero_without_a_success_terminal_is_not_done`.

## Checks run (at HEAD)

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace` | 0 | SUITE_PLACEHOLDER |
| work integration | `cargo test -j 2 -p c3-cli --test work -- --test-threads=1` | 0 | 17 passed / 0 failed (6 new) |
| work lib | `cargo test -j 2 -p c3 --lib work` | 0 | 22 passed |
| clippy | `cargo clippy -j 2 -p c3 -p c3-core --all-targets` | 0 | 0 warnings |

Not run: the pinned plugin harnesses (no consult-path change in this range). The installed binary is rebuilt after
your verdict.

## Reading budget

No sub-agents; never print a whole file; at most 5 tool calls; the verdict even on an incomplete read. Suggested
order: `docs/port/wave10a-work-acceptance.md`; then `crates/c3/src/work/mod.rs` `claim_for_spawn`, `run` (the
pre-spawn record write) and `classify_attempt`; then `crates/c3/src/work/record.rs` `is_live`, `admission_lock`,
`claude_usage`; then `validate_allow_pattern` in `mod.rs`.

## Questions

- **Q1.** Do F89-1, F89-2 and F89-3 close as blockers (pid + start-time ownership before spawn, one admission lock
  across start/resume/alarm, artifact errors terminal and never `done`)?
- **Q2.** Is the allow-list grammar with plain subcommand words (never a flag-shaped word) a real boundary for the
  claude worker, and is `--unrestricted` as the explicit opt-in for muse acceptable?
- **Q3.** Verdict: ACCEPT (tag v0.2.3 moves to this commit, pushed and announced), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
