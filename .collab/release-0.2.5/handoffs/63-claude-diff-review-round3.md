# Handoff 63 - Claude: diff review of one commit, the fixes for the second acceptance review

Date: 2026-11-24. A NEW thread. You run in a throwaway checkout of commit `f78901b`
(`--rev HEAD`); the change is handed to you as a patch (`--diff ea30cce..HEAD`). The patch is
large: read it by file, and read the surrounding code only where a question needs it.

## What the commit is

The standing acceptance reviewer rejected the release at `ea30cce` with five blockers and one
minor (`.collab/release-0.2.5/handoffs/62-codex-acceptance-delta.md`, findings F62-1..F62-6 -
read her "### Findings" entries, not the long "Prior findings" list). This commit is the answer.
She will read it again; find what she would find.

| Finding | The fix, in one line | Where |
|---|---|---|
| F62-1 | a CLI version carries its exact release identity (`1.0.80-rc.1` is not `1.0.80`) and a separate semver ordering; the recovery of a pending update and "did the update take" / "is the previous back" / "up to date" compare the identity | `crates/c3/src/maintain/version.rs`, `maintain/mod.rs`, `crates/c3-cli/tests/maintain.rs` |
| F62-2 | the collab-root resolver resolves `.` and `..` by text before its link check and its canonical form | `crates/c3/src/providers.rs` (`resolve_collab_root_with`), `state_dir.rs` (`lexical`), `state_dir/tests.rs`, `crates/c3-cli/tests/work.rs` |
| F62-3 | a cached CATALOG list records `url` and `public` (was it public when read) like a page record; an id is published only when both that and today's exact-URL trust hold; a legacy custom record publishes nothing | `crates/c3/src/maintain/web.rs`, `maintain/models.rs`, `telemetry/classes.rs`, `crates/c3-cli/tests/telemetry_parity.rs` |
| F62-4 | the copy of a displaced hand edit gets its name by a hard link (fails when the name exists, then `-2`, `-3` ...), never by a replacing rename | `crates/c3/src/contain.rs` (`place_displaced`) |
| F62-5 | a tool namespace nested inside `collaboration` keeps that ancestry at any depth | `crates/c3-core/src/quota.rs` (`walk_tool_list`) |
| F62-6 | the readable stem of a patch name is cut to 120 bytes at a character boundary | `crates/c3/src/consult/reviewed.rs` (`bounded_stem`) |
| her answer 3 | where the file system has no atomic exchange a hand-edited file is NOT written (the rename fallback is gone) | `crates/c3/src/contain.rs` (`publish_exchange`) |

Known and decided, do not report: a run's later writes go by path after the start check; on
Linux and macOS a swapped worktree directory is caught after `git worktree add`, not prevented.
Not run locally (disk and memory were short), CI runs them: the c3-cli test files other than
`maintain`, `telemetry_parity`, `work`.

## Questions

1. **F62-1.** Is there still a comparison in `maintain/` that decides "same release" by numbers
   (grep the callers of the version type)? Can the new parser read two DIFFERENT releases as one
   identity, or the same release as two (what it strips; a `v`; a trailing `.`; build metadata)?
   Does "did the update take" needing the exact target break a real engine's update (codex and
   kimi through npm, grok through its own updater, agy)?
2. **F62-3.** Is there a path on which a cached id is published without the collection-time
   status being checked (every caller of the provenance function; the legacy rule for the
   built-in catalog; a record whose `url` and name disagree)? Did a fixture change make a test
   pass that should fail?
3. **F62-2 / F62-4 / F62-5 / F62-6 and the refusal without an exchange.** For each: does the fix
   do what her remedy says, and does it open anything new (the hard link on a file system
   without hard links; a stem cut in the middle of a meaningful name; a caller of
   `publish_exchange` that relied on the fallback)?
4. Any test in this diff that does not fail without its fix.

Cite `path:line` for every finding. Say plainly when you find nothing. Keep it under 600 words.
