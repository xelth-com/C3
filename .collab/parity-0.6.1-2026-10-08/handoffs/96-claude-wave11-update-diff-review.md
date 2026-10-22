Write in English.

# Handoff 96 - claude: diff review of wave 11 - `c3 update` (self-update check, offer, explicit apply)

Date: 2026-10-22. Base commit: `3e8763e` on `main` (0.2.4-dev). The range under review is `191cc15..3e8763e`;
skip `.collab/`. New: `crates/c3/src/update/mod.rs`, `crates/c3/src/cli/update.rs`, `crates/c3-cli/tests/update.rs`,
`docs/port/wave11-update.md`; changed: `c3config.rs` (`update_check`, `source_checkout`), `hook/mod.rs` (one more line),
the plugin hook wrappers, `c3-cli/src/main.rs`, `cli/mod.rs`, README "Updating C3", `plugin/README.md`, CHANGELOG
`[Unreleased / 0.2.4-dev]`. Also a `cargo fmt` pass over four `work/` files (formatting only).

## What it does (operator's ask: "C3 should check for and pull its updates, or at least offer them")

- **Check**: the newest `v<semver>` tag of `github.com/xelth-com/C3` through the tags API (`/repos/xelth-com/C3/tags`,
  unauthenticated GET, `User-Agent: c3/<version>`, https only, no redirects followed, 3 s timeout - the priors
  download `Fetcher`/`UreqFetcher` reused unchanged), at most once per 24 h, cached atomically at
  `<codex home>/c3/update-check.json` `{checked_at, latest, latest_sha, installed, source}`; a failed fetch keeps the
  previous cache. Default ON; off with `C3_UPDATE_CHECK=off` or config `"update_check": false`; under
  `CODEX_CONSULT_TEST_MODE=1` no network (a tags file via `C3_UPDATE_TEST_TAGS_FILE`, else "offline").
- **Offer**: `c3 hook` prints one extra LAST line only when a newer tag exists:
  `c3: update available v<latest> (installed <installed>) - review: https://github.com/xelth-com/C3/blob/v<latest>/CHANGELOG.md ; apply: c3 update --apply`;
  any error = no line; startup/resume/compact output otherwise byte-identical (the wrappers forward `c3:` lines).
- **`c3 update` / `--check`**: forces a check now, prints installed, latest, status, changelog URL, the install kind
  and the exact commands `--apply` would run, "nothing was changed."; exit 0 up to date / 3 update exists / 1 fetch
  failure (reason, never a secret). Live today: latest v0.2.2 on GitHub (v0.2.3 is not pushed), installed 0.2.3 -> "up
  to date", exit 0.
- **`c3 update --apply`**: (a) `cargo install --git https://github.com/xelth-com/C3.git --tag v<latest> --locked c3-cli`
  (+ `--features index-surreal` when the running binary has it) - touches no checkout; (b) only if a source checkout
  is known (config `source_checkout`, env `C3_SOURCE_CHECKOUT`, or the plugin root's parent when it is a git work
  tree with `plugin/.claude-plugin/plugin.json` naming c3) AND `git status --porcelain` is empty AND HEAD is on a
  branch: `git fetch --tags origin` + `git merge --ff-only v<latest>`; dirty / detached / non-ff = printed and
  skipped, (a) stands; (c) `claude plugin update c3@c3` if `claude` is on PATH, else the command is printed. Refused
  when up to date (exit 0) or after a fetch failure (exit 1). The plan is a pure `Vec<Step>` run by a thin streaming
  runner; tests never run cargo or git for real.

## Checks run

| check | command | exit | observation |
|---|---|---|---|
| workspace | `cargo test -j 2 --workspace --no-fail-fast` | 0 | all sets green (worker's run) |
| update | `cargo test -p c3 --lib update`; `-p c3-cli --test update -- --test-threads=1` | 0 | 9 + 8 passed |
| hook / router / c3config | `cargo test -p c3 --lib hook`, `router`, `c3config`; `-p c3-cli hook` | 0 | 6, 7, 6, 2 passed; priors unchanged |
| clippy / fmt | `--all-targets`; `cargo fmt --check` | 0 | 0 warnings; clean |
| live | `cargo run -p c3-cli -- update` | 0 | real GitHub tags path works; cache written |

## Reading budget

No sub-agents; never print a whole file; at most 5 tool calls; the verdict even on an incomplete read. Suggested
order: `crates/c3/src/update/mod.rs` (`plan`, `inspect_checkout`, `run_apply`, the cache write, `hook_line`), then
`crates/c3/src/hook/mod.rs` diff, then `docs/port/wave11-update.md`.

## Questions

- **Q1.** Supply-chain and safety of `--apply`: `cargo install --git --tag` over https with no release signature
  (none exists yet) - acceptable for an explicit operator action, or must `--apply` first verify the tag's commit
  sha against the API's `latest_sha` (and how, given `cargo install --git` hides the checkout)? Anything in the
  checkout steps (b) that can damage an operator's work tree?
- **Q2.** The offer path: can the hook line mislead (stale cache, a tag pushed then deleted, a pre-release tag, a
  downgrade when the installed version is ahead of the newest tag), and is the 24 h / 3 s / default-on choice right
  for a SessionStart hook?
- **Q3.** Verdict: ACCEPT (ship in 0.2.4), HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 200 words.
