# Worker brief 01 - tests for `c3 work --role tests`, `--of`, `--scope` and `patch.diff`

The change you test is inlined above (the working tree against HEAD). In short:

- `crates/c3/src/work/record.rs`: `baseline_hashes` now also WRITES the baseline content to git's
  object store (`hash-object -w`); new `file_patch`, `run_patch`, `worktree_patch`,
  `scope_violations` (+ private `git_raw`, `store_blob`, `empty_blob`, `head_blob`,
  `expand_path`); new `RunRecord` fields `role`, `of`, `scope`, `scope_violations`.
- `crates/c3/src/work/mod.rs`: `Options`/`Prepared` fields; in `prepare()` the role/of/scope
  guards, the tests block (`TESTS_PREAMBLE` + `<author-change>` with the patch) in the prompt, the
  muse refusal for tests runs; `refresh_artifacts` writes `patch.diff` and the violations;
  `scope_exit` makes a finished run with violations exit 1; `print_run_line` prints OUT OF SCOPE.
- `crates/c3/src/work/detach.rs`: the wire carries role/of/scope; `--status` prints them.
- `crates/c3/src/cli/work.rs`: the `--role`, `--of`, `--scope` flags.

## Where tests may go

- `#[cfg(test)]` modules of `crates/c3/src/work/record.rs`, `crates/c3/src/work/mod.rs`,
  `crates/c3/src/work/detach.rs`.
- `crates/c3-cli/tests/work.rs` - the integration tests through the real binary with the fake
  claude launcher; follow its existing helpers and env hygiene exactly.

Nothing else. Tests that need git build a scratch repository in a temp dir (`git init`, a commit)
- look for an existing helper first.

## Hints for your case list (find your own too)

patch content for a new file, a deleted file, a pre-dirty file the worker edits FURTHER (only the
worker's hunk may appear, not the earlier dirt), an untracked directory; the collab directory left
out of `worktree_patch` and of `patch.diff`; `scope_violations` with an empty scope, the defaults,
a violation, a path under the skip prefix; every refusal of `prepare()` (`--role x`, `--of` without
`--role tests`, a bad glob, muse + `--role tests`, `--of` naming a run that is not done or has no
patch.diff, an empty change); the prompt of a tests run holds the preamble and the patch; a run
that edits a file outside its scope exits 1 and lists it; the wire round-trips role/of/scope.

## Verify

- `cargo test -j 2 -p c3 work::`
- `cargo test -j 2 -p c3-cli --test work -- --test-threads=1`
- `cargo clippy -j 2 -p c3 -p c3-cli --all-targets -- -D warnings`
- `cargo fmt --check` (run `cargo fmt` if not clean)

One cargo command at a time; never the full workspace suite.

## Report

Files changed, your numbered case list with coverage, the commands with pass counts, and every
place you think the change is wrong (do not fix non-test code).
