# Wave 10a — `c3 work` acceptance fixes (handoff 89, F89-1..F89-8)

Astra's 0.2.3 acceptance pass (`.collab/parity-0.6.1-2026-10-08/handoffs/89-codex-codex-0.2.3-acceptance-1.reply.json`)
held the release on three blockers and five more findings in the `c3 work` feature. This wave
resolves all eight. The structured findings carry the exact locations, triggers and remedies; this
note records, per finding, what changed and which test proves it.

**F89-1 (blocker) — foreground worker liveness.** Every `running`/`waiting_quota` record now
carries the owning process's `pid` and `pid_start` (start time). A foreground run writes its OWN
pid + start time before it spawns the child (`work/mod.rs::run` for the first launch,
`claim_for_spawn` for resumes/detached); `record::is_live` checks `pid_alive(pid, pid_start)` and
the 120 s start grace applies only to a record with no pid yet. So a foreground run outliving the
grace stays `running` and still blocks a second launch. Proof: `work.rs::a_live_foreground_run_refuses_a_second_launch`
(a blocking launcher keeps the run `running` with a live pid while a second start refuses), plus
`record::tests::the_record_round_trips_and_allocates`.

**F89-2 (blocker) — one task-level admission.** `record::admission_lock` is an exclusive OS lock on
`<work>/.admission.lock`; `claim_for_spawn` (used by the first launch, every resume and the
detached background) re-reads under it, refuses if another run of the task is live or if the record
is already live under a different pid, and durably writes `running` + pid/start + attempt before
spawning. Resume attempt files are per-attempt (`.events-resume-<n>.jsonl`). Proof: the second-start
refusal in `a_live_foreground_run_refuses_a_second_launch`; the existing resume/kick/cap tests still
green.

**F89-3 (blocker) — artifact durability.** `append_resume` and `refresh_artifacts` return `Result`;
the resume's per-attempt temp files are deleted only after the append succeeded and was flushed
(`f.flush()` + `sync_all`); a `report.md`/`diff.txt`/`events.jsonl` write failure makes the run the
new terminal status `artifact_error` (chosen over overloading `died`, so the cause is explicit)
with the error text in `run.json` and a non-zero exit. Proof: `work.rs::an_artifact_write_failure_is_not_done`
(report.md replaced by a directory → `artifact_error`, the attempt still appended to events.jsonl).

**F89-4 (major) — real allow-list grammar + unrestricted opt-in.** `validate_allow_pattern`
enforces `<Tool>`, `<Tool>(<exe>)` or `<Tool>(<exe>:*)` with `<exe>` a plain executable name and not
a shell/git; the refusal names the position, never the pattern. The built-in defaults are exempt.
The new `--unrestricted` flag (or roster `ext.unrestricted`) lifts the grammar and is required for
the muse worker's `--disable-sandbox`; codex keeps `--sandbox workspace-write`; `run.json` records
`unrestricted` and a `permission` note; `docs/work-mode.md` restated. Proof:
`work.rs::the_allow_grammar_and_muse_opt_in_refuse_before_any_record` and the updated
`a_muse_work_run_lands_its_record`.

**F89-5 (major) — usage allow list + record contract.** `summarize_events` rebuilds `usage` from an
allow list of numeric counters (`record::claude_usage`); the codex/muse summaries already rebuilt
theirs. The doc's record contract is precise: events.jsonl is the raw stream retained verbatim
(like a consult's), run.json/report.md/diff.txt carry no secret values, argv's `--allow` values are
grammar-validated. Proof: `record::tests::usage_is_rebuilt_from_an_allow_list` and
`work.rs::usage_in_run_json_is_an_allow_list` (a sentinel in an extra usage field reaches neither
run.json nor report.md/diff.txt).

**F89-6 (major) — persisted resume caps.** The effective `max_resumes` and `quota_window_sec` are
written to `run.json` at the first run and reused by a later plain `--resume` (`eff_max_resumes_rec`
/ `eff_window_rec`); an explicit flag still overrides. Proof:
`work.rs::a_persisted_max_resumes_is_reused_without_the_flag` (`--max-resumes 1`, then a plain
`--resume` exhausts).

**F89-7 (minor) — content-identity files_changed.** `record::files_changed` now compares each dirty
path's `git hash-object` against a baseline hash map captured before the first launch
(`baseline_hashes`, persisted in the record), so a pre-dirty file the worker edited further is
listed. Proof: `record::tests::files_changed_uses_content_identity` and
`work.rs::files_changed_lists_a_pre_dirty_file_the_worker_edited`.

**F89-8 (major) — terminal-event success.** `engines::terminal_outcome` reads each engine's terminal
verdict from its stream; `classify_attempt` requires exit 0 AND a successful terminal for `done`, an
error terminal or rejecting rate-limit takes precedence over exit 0, and an empty stream with exit 0
is `died` with reason `no terminal event`. Proof: `engines::tests::terminal_outcome_reads_the_stream`
and `work.rs::exit_zero_without_a_success_terminal_is_not_done`.

## Note for the supervisor

The allow-list grammar's `<exe>` accepts plain subcommand words after the executable (`cargo test`,
`npm run lint`) separated by single spaces - the alphabet stays `[A-Za-z0-9_.+-]` and a word never
starts with `-` (`python -c`, `node -e` stay refused), so no wildcard,
quote, pipe or redirection can pass; the shell/git executables are refused by name. A custom
`--allow "Bash(cargo test:*)"` is therefore the same shape as the built-in defaults. (The supervisor
relaxed the worker's first, space-free version of the grammar for this usability reason.)
