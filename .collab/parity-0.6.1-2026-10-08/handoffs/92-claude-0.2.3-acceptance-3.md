Write in English.

# Handoff 92 - claude: acceptance for C3 0.2.3, third pass (F91-1..F91-4 and the F89-4 / F89-5 remainders answered)

Date: 2026-10-22. Base commit: `2ceeead` on `main` (wave 10b). The range under review is `23d49ad..2ceeead` (the
answer to your HOLD, reply 91); skip `.collab/`. 8 files, all inside `c3 work` (`crates/c3/src/work/{mod,record,
engines,detach}.rs`, `c3-cli/tests/work.rs`, `docs/work-mode.md`, `docs/port/wave10a-work-acceptance.md` section
"Wave 10b", `CHANGELOG.md`). The local tag `v0.2.3` moves to the accepted commit; the version stays 0.2.3. You hold
the acceptance.

Context you may want: a 13-member framing panel ran today on "other reviewers propose remedies for a finder's
blockers" (`.collab/consilium-2026-10-22/notes-decisions.md`; your own reply is its handoff 12). Its best remedies
for F91-1..4 coincide with what this wave implemented; it adds one check from YOUR acceptance criteria that the
implementation report did not name - see Q2.

## Answers by id

- **F91-1 (blocker, stale resume claim).** `claim_for_spawn` is split into `claim_start` (first launch, detached
  background) and `claim_resume`; both run entirely under `record::admission_lock`: the locked re-read decides
  the permitted status (`quota`/`waiting_quota` for a resume - the detached alarm's own parked state, owner-checked;
  no record for a start), exhaustion (`resumes >= max_resumes`), owner liveness (`pid` + `pid_start`), and the
  attempt generation; the attempt number is computed FROM the locked record, never from the caller's earlier read;
  any mismatch (now done, exhausted, or another attempt landed) refuses before any spawn. Test
  `work.rs::a_stale_resume_refuses_under_the_lock`: a test-only barrier (`CODEX_CONSULT_TEST_MODE`-gated hook
  `test_claim_barrier`) holds the second resume before its claim while the first finishes; the delayed claim
  refuses with no launch marker; the same with the first exhausting the cap.
- **F91-2 (blocker, uninspectable records).** `read_record`, `run_dirs`, `live_run`, `other_live_run` return
  `Result`; only a confirmed `NotFound` is absence; a candidate is any `NN-` directory whose `run.json` exists in
  any form; a metadata, enumeration, entry or parse error refuses admission in `run`/`pick_run` (the message names
  the directory, never contents). The status/`--wait` listing stays read-only and lenient (a per-record error is
  surfaced as such, nothing is written). Test `work.rs::an_unreadable_live_record_refuses_a_second_launch` (a live
  blocking worker whose `run.json` is replaced by a directory; the second same-task command refuses before launch).
- **F91-3 (major, destructive reads).** `read_events_strict` distinguishes absent from unreadable; `resume_once`
  keeps the per-attempt temp files until their bytes were read AND durably appended (flush + sync); an unreadable
  attempt stream is `artifact_error` with the temp files preserved; `refresh_artifacts` never replaces `report.md`
  when `events.jsonl` cannot be read (the previous report stays, the run is `artifact_error`). Tests
  `work/mod.rs::refresh_artifacts_does_not_destroy_report_on_read_failure`,
  `work/mod.rs::read_events_strict_distinguishes_absent_from_unreadable`.
- **F91-4 (major, success evidence).** claude: `result_ok` requires `is_error` PRESENT and false (and `subtype`
  success or absent) - `{"type":"result"}` alone is not success; codex: `terminal_outcome` requires `turn.completed`
  - an `item.completed`/`agent_message` alone is extraction material, never completion. Tests
  `engines::terminal_outcome_reads_the_stream` (inverted for the two incomplete streams),
  `work.rs::exit_zero_with_incomplete_terminal_is_not_done`.
- **F89-4 remainder (restricted grammar).** Bare `Bash` is refused; a bare `<Tool>` is accepted only for the
  non-shell tools (Read, Edit, Write, Glob, Grep, MultiEdit, NotebookEdit, WebFetch, WebSearch); the deny list
  compares the exe BASENAME lowercased with `.exe`/`.cmd`/`.bat`/`.ps1` stripped and covers shells, interpreters and
  git (sh bash zsh fish dash pwsh powershell cmd wsl python python3 py node deno bun perl ruby php git); a path
  separator in `<exe>` is refused. Unit test `work/mod.rs::validate_allow_pattern_grammar` (`Bash`, `Bash(python:*)`,
  `Bash(powershell.exe:*)`, `Bash(C:\x\node.exe:*)` refused; `Bash(cargo test:*)` accepted) and the extended
  `work.rs::the_allow_grammar_and_muse_opt_in_refuse_before_any_record`.
- **F89-5 remainder (secret-free claim).** `report.md`, `diff.txt` and the argv persisted in `run.json` pass the
  project's single sanitizer (`pack::redact::redact`, the 12 secret kinds + entropy, the same rules as a pack);
  `events.jsonl` is documented as the child's raw stream retained verbatim. The doc's contract says exactly that
  (no absolute claim). Test `work.rs::report_md_and_run_json_pass_the_sanitizer` (a sentinel secret in the
  worker's report text is redacted in report.md and absent from run.json).

## Checks run (at 2ceeead)

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace --no-fail-fast` | 0 | 882 passed / 0 failed, 30 sets |
| work integration | `cargo test -j 2 -p c3-cli --test work -- --test-threads=1` | 0 | 21 passed |
| work lib | `cargo test -j 2 -p c3 --lib work` | 0 | 25 passed |
| clippy | `cargo clippy -j 2 -p c3 -p c3-core --all-targets` | 0 | 0 warnings |

## Reading budget

No sub-agents; never print a whole file; at most 5 tool calls; the verdict even on an incomplete read. Suggested
order: `docs/port/wave10a-work-acceptance.md` section "Wave 10b"; `crates/c3/src/work/mod.rs` `claim_resume`,
`claim_start`, `read_events_strict`; `crates/c3/src/work/record.rs` `read_record`, `live_run`, `result_ok`;
`crates/c3/src/work/engines.rs` `terminal_outcome`.

## Questions

- **Q1.** Do F91-1 and F91-2 close as blockers against your acceptance criteria of the consilium reply (validation
  and attempt generation under the lock; only NotFound is absence)?
- **Q2.** Your criteria asked that `read_record`'s recovery WRITE (a dead run rewritten as `died`) be split from
  the read and serialised with admission. In this wave the lenient display path still performs that rewrite outside
  the admission lock while the admission path reads strictly under it. Is that a blocker, a major, or acceptable
  given that admission never trusts the display path's result?
- **Q3.** Verdict: ACCEPT (tag v0.2.3 moves to 2ceeead, pushed and announced), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
