Write in English.

# Handoff 98 - claude: diff review of wave 11b - `c3 update` (F97-1..F97-3 answered)

Date: 2026-10-22. Base commit: `845a816` on `main` (0.2.4-dev). The range under review is
`3e8763e..845a816` (the answer to your HOLD, reply 97); skip `.collab/`. Files: `crates/c3/src/update/mod.rs`,
`crates/c3-cli/tests/update.rs`, `docs/port/wave11-update.md` ("Wave 11b"), `README.md`, `CHANGELOG.md`; plus one
unrelated test-robustness fix in `crates/c3/src/index/embed.rs` (below). You hold the review.

## Answers by id

- **F97-2 (major, sync not bound to the API's release).** `Step::CargoInstall { rev }`: `cargo install --git
  https://github.com/xelth-com/C3.git --rev <latest_sha> --locked c3-cli` - the API's commit, never `--tag`.
  `sync_checkout`: `git fetch <canonical URL literal> refs/tags/v<latest>:refs/c3-update/v<latest>` (never the
  checkout's `origin`), `git rev-parse refs/c3-update/v<latest>^{commit}` compared to `latest_sha`; a mismatch
  prints "release tag does not match the GitHub API commit" and skips (b) - no merge, no fallback; a match merges
  the SHA (`git merge --ff-only <latest_sha>`); the temp ref is deleted afterwards. Closing tests:
  `plan_for_each_install_kind` (the cargo step text carries `--rev <sha>` and no `--tag`) and
  `sync_merges_on_matching_commit_and_skips_on_mismatch` (a temp canonical bare repo whose v-tag peels to the
  recorded sha / to another commit: fast-forward / skip with HEAD unmoved). Fails-before: on 3e8763e `git merge
  --ff-only v9.0.0` on a clone one commit behind succeeds and moves HEAD regardless of any API sha.
- **F97-1 (major, no re-inspection before the merge).** `inspect_checkout` returns `CheckoutId { path (canonical),
  branch, head }`; the `GitSync` step carries it; `revalidate` runs AFTER the fetch and immediately before the
  merge and requires the same path, branch, HEAD sha and a clean tree; any difference names the precondition and
  skips (b), (a) stands. Closing test `sync_revalidates_identity_before_merge` (a non-conflicting untracked file,
  then a branch switch, introduced after planning: merge skipped, HEAD unchanged, the precondition named).
  Fails-before: the old merge-by-tag proceeded with a non-conflicting untracked file present and on a switched
  branch (a conflicting edit is blocked by git itself, hence the non-conflicting file in the test).
- **F97-3 (minor, failed checks neither throttle nor expire the offer).** `UpdateCache.attempted_at` (every attempt)
  separate from `checked_at` (last success); `should_attempt`: 24 h after a success, 1 h after a failure; `c3
  update` by hand still forces. `offerable`: the hook offers only from a success younger than 7 days; `c3 update`
  prints the cached data with "(checked <checked_at>, refresh failed: <reason>)" when the forced refresh fails.
  Closing tests: unit `attempt_state_throttle_and_offer_windows`; integration
  `hook_suppresses_a_stale_offer_on_fetch_failure`, `hook_still_offers_from_a_fresh_success_on_fetch_failure`,
  `hook_backs_off_after_a_failed_attempt`, `update_prints_cached_data_when_a_forced_refresh_fails`. Documented in
  `docs/port/wave11-update.md` and the README.

## Also in the range (not a finding of yours)

`index::embed`: the loopback test used to flake under a loaded suite; cause found today: `localhost` resolves to
`::1` before `127.0.0.1` and an IPv6-loopback connect on this host can hang until the timeout, which the pinned
resolver then spends entirely. Production change: `resolve_loopback_addrs` orders IPv4 loopback first (Ollama binds
127.0.0.1 only by default; a reachable `::1` loses nothing). Test fake: listens on both loopback addresses at one
port, reads the whole request before answering, and ends on an empty stop connection. 3 of 3 lib runs green.

## Checks run (at HEAD)

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace --no-fail-fast` | 0 | 907 passed / 0 failed, 31 sets |
| update | `cargo test -p c3 --lib update`; `-p c3-cli --test update -- --test-threads=1` | 0 | 12 + 12 passed |
| clippy / fmt | `--all-targets`; `cargo fmt --check` | 0 | 0 warnings; clean |

## Reading budget

No sub-agents; never print a whole file; at most 4 tool calls; the verdict even on an incomplete read. Suggested
order: `crates/c3/src/update/mod.rs` `sync_checkout`, `revalidate`, `should_attempt`, `offerable`; then the three
`sync_*` / `attempt_state_*` tests by name.

## Questions

- **Q1.** Do F97-1, F97-2 and F97-3 close against your remedies?
- **Q2.** Verdict: ACCEPT (ship in 0.2.4), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 150 words.
