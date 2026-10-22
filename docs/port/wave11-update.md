# Wave 11 — `c3 update` (self-update check and apply)

C3 checks whether a newer release of itself exists and offers it; it applies only on an explicit
`--apply`. New subcommand `c3 update` plus one extra line in the SessionStart hook.

## Source of truth

The git **tags** of `https://github.com/xelth-com/C3`, read through the unauthenticated tags API
`https://api.github.com/repos/xelth-com/C3/tags` (JSON array `[{"name":"v0.2.3","commit":{"sha":…}},…]`).
The newest release is the highest semver among names of the form `v<major>.<minor>.<patch>`
(anything else is ignored), compared with `env!("CARGO_PKG_VERSION")`.

## Module map

- `crates/c3/src/update/mod.rs` — version parse/compare, the tags API, the cache, install-kind
  detection, the apply plan (`Vec<Step>`, pure and testable) and the thin runner, plus
  `run_check_report` / `run_apply` and `hook_line`.
- `crates/c3/src/cli/update.rs` — the clap surface (`--check` / `--apply`) and dispatch.
- Wiring: `crates/c3/src/lib.rs`, `crates/c3/src/cli/mod.rs`, `crates/c3-cli/src/main.rs`.
- `crates/c3/src/c3config.rs` — two new config keys: `update_check` (bool, default **true**) and
  `source_checkout` (string path).
- `crates/c3/src/hook/mod.rs` — prints the update line last; `plugin/hooks/c3-hook.{ps1,sh}` forward
  a line starting `c3: update available ` (alongside the `codex-consult:` lines they already
  forward).

## Fetch, cache, off switch

- **Fetcher.** Reuses the priors `Fetcher` trait and `UreqFetcher` from
  `crates/c3/src/router/download.rs` (`User-Agent: c3/<version>`, https only, `.redirects(0)`, 3 s
  timeout, no cookies/query). The priors behaviour is unchanged.
- **Cache.** `<codex home>/c3/update-check.json`
  (`{checked_at, attempted_at, latest, latest_sha, installed, source:"github-tags"}`), written
  atomically (temp + rename). `checked_at` is the last SUCCESS, `attempted_at` the last attempt (see
  Wave 11b F97-3 for the throttle and offer windows). A failed fetch keeps the previous success and
  is silent in the hook (reported only by `c3 update`). Clock: `Utc::now()`.
- **Off switch (default ON).** `C3_UPDATE_CHECK=off` or `"update_check": false`. In
  `CODEX_CONSULT_TEST_MODE=1` no network is ever touched: the fetcher reads the tags JSON from
  `C3_UPDATE_TEST_TAGS_FILE` when set, else reports "offline".

## The hook line

```
c3: update available v<latest> (installed <installed>) - review: https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply
```

Printed only when the cache says a newer version exists and the last successful check is younger
than 7 days (Wave 11b F97-3); startup/resume/compact output is otherwise byte-identical. The hook
never exceeds its budget or fails: any error prints no line.

## `c3 update`

- No flags / `--check`: force a check now (ignoring the throttle, still writing the cache), print
  installed vs latest, whether an update exists, the changelog URL, the install kind detected, the
  exact commands `--apply` would run, and `nothing was changed.` Exit `0` up to date, `3` an update
  exists, `1` on a fetch failure (reason: a status code or "offline", never a secret; the cached
  data is still printed with a "refresh failed" note when a prior success exists — Wave 11b F97-3).
- `--apply`: run, streaming child output, stopping at the first failure — (a)
  `cargo install --git <GIT_URL> --rev <latest_sha> --locked c3-cli` (+ `--features index-surreal`
  when the running binary has that feature); (b) a **clean** source checkout on a branch is fetched
  from the canonical URL, verified against `latest_sha`, revalidated, then fast-forwarded to that
  commit (Wave 11b F97-1/F97-2; a dirty tree / detached HEAD / sha or identity mismatch / non-ff is
  printed and skipped, never failing (a)); (c) `claude plugin update c3@c3` when `claude` is on
  PATH, else the command is printed. Refused when up to date (exit 0) or the fetch failed (exit 1).

## Never

Run on its own without `--apply`; touch a dirty checkout; follow a redirect; send anything but the
GET; write outside `<codex home>/c3/`. There is no release signature yet: the install trusts
GitHub's HTTPS transport and the API's release commit.

## Wave 11b — Astra diff-review fixes (handoff 97)

Three findings against wave 11 (committed `3e8763e`). Each shipped with a closing test that fails on
`3e8763e`'s logic and passes now.

### F97-2 (major) — bind the install and the sync to the API's release commit

The tag NAME is never what gets built. `--apply` step (a) is now
`cargo install --git <GIT_URL> --rev <latest_sha> --locked c3-cli` (+ features). Step (b) fetches
the release tag from the **canonical** URL into a dedicated ref
(`git fetch <GIT_URL> refs/tags/v<latest>:refs/c3-update/v<latest>` — never the checkout's
`origin`), peels it (`git rev-parse refs/c3-update/v<latest>^{commit}`) and compares to
`latest_sha`; a mismatch prints "release tag does not match the GitHub API commit" and skips (b)
entirely (no merge, no origin fallback); a match fast-forwards to the **sha**
(`git merge --ff-only <latest_sha>`). The temporary ref is deleted afterwards (best effort).
- Change: `Step::CargoInstall { rev, .. }`, `Step::GitSync { id, tag, rev }`, `sync_checkout`.
- Proving tests: `plan_for_each_install_kind` (the cargo step text is `--rev <sha>`, no `--tag`);
  `sync_merges_on_matching_commit_and_skips_on_mismatch` (a temp "canonical" repo with a v-tag:
  matching sha fast-forwards, a mismatched recorded sha is skipped and HEAD does not move).
- Fails-before: on `3e8763e` the cargo step rendered `--tag v<latest>`, and GitSync ran
  `git fetch --tags origin` + `git merge --ff-only v<latest>` with no sha check. I reproduced the old
  logic in a scratch repo: `git merge --ff-only v9.0.0` on a clone one commit behind the release
  succeeds and moves HEAD to the tag commit regardless of any API sha — so the mismatch assertion
  (HEAD unmoved) fails under the old logic.

### F97-1 (major) — carry the inspected identity into the plan and revalidate before the merge

`inspect_checkout` now returns the full identity `{path (git work-tree root), branch, HEAD sha}`;
the plan's `GitSync` stores it (`CheckoutId`); the runner, AFTER the fetch and immediately BEFORE
the merge, re-inspects and requires the same path, branch, HEAD sha and a still-clean tree
(`revalidate`). Any difference prints which precondition changed and skips (b); (a) stands.
- Proving test: `sync_revalidates_identity_before_merge` — a clean checkout on main is inspected,
  then a **nonconflicting** untracked file is added (or the branch switched) before the sync runs;
  the merge is skipped, HEAD is unchanged, and the message names the precondition.
- Fails-before: `3e8763e` never rechecked. I confirmed in a scratch repo that `git merge --ff-only
  v9.0.0` with a nonconflicting untracked file present, and on a switched branch, both succeed and
  move HEAD — so the "HEAD unchanged" assertions fail under the old logic. (A *conflicting* dirty
  file is blocked by git itself, which is why the test uses a nonconflicting one.)

### F97-3 (minor) — separate attempt state from the last successful response

The cache gains `attempted_at` (every attempt, success or failure) beside `checked_at` (last
SUCCESS). Throttle: no automatic attempt within 1 h of any attempt, nor within 24 h of a success;
`c3 update` by hand still forces. Offer policy: the hook offers only from a success younger than 7
days (older: the release may be gone, so no line); `c3 update` on a forced-refresh failure prints
the cached data annotated `(checked <checked_at>, refresh failed: <reason>)` and exits 1.
- Change: `UpdateCache.attempted_at` (serde default), `should_attempt`, `offerable`,
  `cache_attempt_failed`, the `hook_line`/`run_check_report` rewrites.
- Proving tests: unit `attempt_state_throttle_and_offer_windows`; integration
  `hook_suppresses_a_stale_offer_on_fetch_failure`, `hook_still_offers_from_a_fresh_success_on_fetch_failure`,
  `hook_backs_off_after_a_failed_attempt`, `update_prints_cached_data_when_a_forced_refresh_fails`.
- Fails-before: on `3e8763e` the cache had no `attempted_at`, no backoff, and offered from any cache
  regardless of age; a stale (8-day) newer cache would still print a hook line and every hook would
  re-attempt on failure — so the suppression and the unchanged-`attempted_at` assertions fail under
  the old logic.
