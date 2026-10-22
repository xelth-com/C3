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
  (`{checked_at, latest, latest_sha, installed, source:"github-tags"}`), written atomically
  (temp + rename). A check is attempted at most once per 24 h; a failed fetch keeps the previous
  cache and is silent in the hook (reported only by `c3 update`). Clock: `Utc::now()`.
- **Off switch (default ON).** `C3_UPDATE_CHECK=off` or `"update_check": false`. In
  `CODEX_CONSULT_TEST_MODE=1` no network is ever touched: the fetcher reads the tags JSON from
  `C3_UPDATE_TEST_TAGS_FILE` when set, else reports "offline".

## The hook line

```
c3: update available v<latest> (installed <installed>) - review: https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply
```

Printed only when the (24 h) cache says a newer version exists; startup/resume/compact output is
otherwise byte-identical. The hook never exceeds its budget or fails: any error prints no line.

## `c3 update`

- No flags / `--check`: force a check now (ignoring the 24 h age, still writing the cache), print
  installed vs latest, whether an update exists, the changelog URL, the install kind detected, the
  exact commands `--apply` would run, and `nothing was changed.` Exit `0` up to date, `3` an update
  exists, `1` on a fetch failure (reason: a status code or "offline", never a secret).
- `--apply`: run, streaming child output, stopping at the first failure — (a)
  `cargo install --git … --tag v<latest> --locked c3-cli` (+ `--features index-surreal` when the
  running binary has that feature); (b) a **clean** source checkout on a branch is
  `git fetch --tags origin` + `git merge --ff-only v<latest>` (a dirty tree / detached HEAD / non-ff
  is printed and skipped, never failing (a)); (c) `claude plugin update c3@c3` when `claude` is on
  PATH, else the command is printed. Refused when up to date (exit 0) or the fetch failed (exit 1).

## Never

Run on its own without `--apply`; touch a dirty checkout; follow a redirect; send anything but the
GET; write outside `<codex home>/c3/`. There is no release signature yet: `cargo install --git
--tag` trusts GitHub's HTTPS transport and the tag.
