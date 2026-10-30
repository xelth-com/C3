The temp dir removal is blocked by the sandbox (outside the working directory) — I'll note it in the report instead of pushing on it.

---

# Final report — tests for `c3 work --role tests`, `--of`, `--scope`, `patch.diff`

## Files changed (test code only)

- `crates/c3/src/work/record.rs` — `#[cfg(test)]`: new `git_repo()` helper + 4 tests
- `crates/c3/src/work/mod.rs` — `#[cfg(test)]`: 1 test (`scope_exit`)
- `crates/c3/src/work/detach.rs` — `#[cfg(test)]`: extended `the_wire_round_trips` with the new assertions
- `crates/c3-cli/tests/work.rs` — new `write_tests_file_launcher` fake + 5 integration tests, plus `cargo fmt` wraps in those tests

## Numbered case list and coverage

| # | Case | Covered by (before → after) |
|---|------|------------------------------|
| 1–4 | `file_patch`: new file (`--- /dev/null`), deleted file (`+++ /dev/null`), unchanged → `""`, header names the path never blob ids | none → `file_patch_shapes_for_new_deleted_modified_and_unchanged` (record.rs) |
| 5 | `run_patch`: pre-dirty file edited further → only the worker's hunk (baseline blob *written* to the object store — the content never existed in a commit) | none (the existing `files_changed_uses_content_identity` covers only the listing) → `run_patch_diffs_from_the_baseline_content_and_expands_directories` (record.rs) |
| 6 | untracked directory entry (`?? dir/`) expands (patch + scope) | none → same test + `scope_violations_empty_defaults_violation_and_skip` |
| 7 | `worktree_patch` leaves the skip/collab prefix out; includes it with no skip | none → `worktree_patch_skips_the_collab_directory` (record.rs) |
| 8–10 | `scope_violations`: empty scope → none; the five test-path defaults cover test files; unmatched path listed, path under skip never counts | none → `scope_violations_empty_defaults_violation_and_skip` (record.rs) |
| 11–14, 18 | `prepare` refusals before any record: `--role x`, `--of` without `--role tests`, bad `--scope` glob, muse + tests (even `--unrestricted`), nothing-to-test on a clean tree | none → `the_tests_role_guards_refuse_before_any_record_exists` (work.rs) |
| 15–17 | `--of` a not-done run, a done run without `patch.diff`, an unknown id | none → `of_refuses_a_not_done_run_a_run_without_patch_and_an_unknown_id` (work.rs) |
| 19 | tests-run prompt order: work preamble → tests preamble → `<author-change>` patch → brief | none → `a_tests_run_inlines_the_working_tree_change_and_lands_within_the_defaults` (work.rs) |
| 20 | `run.json` records `role`/`of`/`scope` (defaults joined for tests; own globs otherwise) | none → same test + `an_out_of_scope_run_exits_one_and_lists_the_violation` |
| 21 | `patch.diff` written for the run's change; collab dir left out | none → same two tests |
| 22 | out-of-scope finished run exits 1, lists the violation (stdout+stderr), records it; in-scope run exits 0 | none → `scope_exit_overrides_the_status_code_only_on_violations` (mod.rs, pure fn) + `an_out_of_scope_run_exits_one_and_lists_the_violation` |
| 23 | a tests run whose change falls under the defaults exits 0 | none → `a_tests_run_inlines_…` |
| 24 | `--of <done id>`: the *author's* `patch.diff` inlined (not the tree's later change — asserted negatively), `of` recorded, same-provider warning | none → `a_tests_run_of_a_done_run_inlines_the_authors_patch` |
| 25 | `--status` prints `role tests of …` and `OUT OF SCOPE:` | none → folded into the three run tests |
| 26 | wire round-trips role/of/scope | partial (fields set, never asserted) → assertions added to `the_wire_round_trips` (detach.rs) |

## Verification (exact commands and results)

- `cargo test -j 2 -p c3 work::` — **30 passed, 0 failed** (5 new + 1 extended)
- `cargo test -j 2 -p c3-cli --test work -- --test-threads=1` — **27 passed, 0 failed** (5 new); re-run after `cargo fmt`, still 27/27
- `cargo clippy -j 2 -p c3 -p c3-cli --all-targets -- -D warnings` — clean
- `cargo fmt --check` — failed on 5 over-long asserts in my new tests → `cargo fmt -p c3-cli` (the pre-check showed no diffs outside my additions) → clean

## Deviations from the brief

- The three cargo commands that link `c3.exe` (integration test, clippy) were run with `--target-dir C:/Users/Dmytro/AppData/Local/Temp/c3tgt-worktest`: an external, periodically respawning `c3.exe` process (coordinator-side tooling, not mine) kept locking `target\debug\c3.exe`, failing the relink with os error 5 (access denied). Same sources and flags otherwise. That scratch target dir (~a fresh debug build) is left in `%TEMP%\c3tgt-worktest` — deleting it was blocked by the session sandbox; it can be removed manually. While probing the lock I renamed `target\debug\c3.exe` → `c3.exe.probe` → back (byte-identical, no content change).

## Where I believe the change itself is wrong (not fixed)

1. **The scope exit-1 contract does not hold on resume/detach paths.** `scope_exit` is applied only at the end of the initial foreground `run_prepared` (`mod.rs:837`). `run_alarm` (a detached run's quota resume) and `do_resume_foreground` (`--resume`/`--kick`) return `exit_for(&status)` directly, so a finished run with recorded `scope_violations` exits **0** there. `run.json` and `--status` still surface the violations, but the documented "exits 1, whatever its status" holds only for the never-resumed foreground run.
2. **`--status` ignores violations in its exit code** (`exit_of` in `detach.rs`): a `done` run with `scope_violations` still status-exits 0; the violation is only a printed line. May be intended (post-hoc, informational), but it weakens scripting on the violation.
3. **Renames lose the deletion side.** `path_of_status_line` takes the new path of `R old -> new`; `worktree_patch` then finds no `head_blob(new)` and emits a new-file diff — the old path's deletion never appears in `patch.diff`/`worktree_patch`, so a tests run would not see removed code in a renamed file.
4. Minor: in practice `files_changed` never lists `.collab` paths anyway (the baseline captures the untracked directory with an unhashable `""` hash, and directories hash identically before/after), so the `collab_rel` prefix filter in `refresh_artifacts` is mostly redundant — harmless, but it is doing less than it appears to.