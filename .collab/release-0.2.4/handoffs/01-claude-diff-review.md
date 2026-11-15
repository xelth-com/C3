# Handoff 01 - Claude: diff-review of the 0.2.4 wave

Date: 2026-11-15. Base commit: `035d386` + uncommitted (the working tree you are in IS the change).

The whole change as one patch: `.collab/release-0.2.4/handoffs/01-claude-diff-review.patch`
(`git diff 035d386` including the new files). You may have no shell: read the patch file and the
files it names with your file tools. Everything is in this repository.

## Question

Is this wave safe to commit and install as 0.2.4? Find defects that would make C3 damage a
machine, a roster or a consultation record - not style.

## Delta since the last review

First review of this task. Four pieces:

- **`c3 maintain`** (new, `crates/c3/src/maintain/`, `crates/c3/src/cli/maintain.rs`, hook call in
  `crates/c3/src/hook/mod.rs`, config key in `crates/c3/src/c3config.rs`): weekly, started detached
  from the SessionStart hook, ON by default. Per engine CLI: installed/latest version; when an update
  exists - smoke the installed version, update, re-read the version, smoke again, roll back on
  failure. Smoke = two real `c3 consult --raw` turns in a throwaway repo, the second must return a
  random token from `token.txt`. Then model lists; a seated roster model is replaced only by a
  same-major, higher-minor successor that passed the smoke, after a roster backup.
- **Grok** (`crates/c3-core/src/effort.rs`, `crates/c3/src/consult/orchestrate/secondary.rs` and
  `mod.rs`, `crates/c3/src/consult/prompt.rs`, `crates/c3/src/engines/grok.rs`): the reply schema now
  travels in the prompt (`prompt-only`) instead of `--json-schema`; a read-insist re-ask is a NEW
  conversation carrying the whole original prompt; a re-ask that read and then died is the recorded
  outcome (not the zero-tool primary); the answer is the text after the last tool call.
- **Budget** (`crates/c3/src/budget/mod.rs`): live quota windows for z.ai (HTTP GET with the plan
  key) and Muse (`muse usage --json`), cached, ahead of the operator's snapshot.
- **Version**: `crates/c3-cli/build.rs` stamps the build commit into `c3 --version`; 0.2.3 -> 0.2.4.

## CURRENT invariants claimed

- A `--dry-run` pass and `--smoke` write no state, take no lock, install nothing.
- The hook never blocks and never spawns under `CODEX_CONSULT_TEST_MODE` unless a test opts in.
- Nothing is installed without a passing smoke of the installed version first; a failed post-update
  smoke rolls back where a pin exists (codex, grok, kimi) and says so where none exists.
- An engine with a live run or process is skipped, never updated.
- A request that carries an API key never follows a redirect; the keyless kimi marker/installer GETs
  follow at most 3 redirects, https only, same site only (`maintain/env.rs::redirect_target`).
- The roster is edited only by `maintain/models.rs::apply_swap`: backup first (once per day),
  exactly one `model` value replaced, the result re-validated before it is written.
- For grok, a zero-tool reply is never usable on a non-raw purpose; an abandoned zero-tool thread is
  never recorded for a later resume.
- A failed live quota read yields no windows, so the snapshot or "no data" applies - never a guess.

## Changed files

| File | Change |
|---|---|
| `crates/c3/src/maintain/{mod,engine,env,smoke,models,version}.rs` | new: the maintenance pass |
| `crates/c3/src/cli/maintain.rs`, `cli/mod.rs`, `crates/c3-cli/src/main.rs` | the `maintain` subcommand |
| `crates/c3/src/hook/mod.rs`, `c3config.rs`, `providers.rs` | hook line + spawn, `maintain` config key, `find_on_path` made crate-visible |
| `crates/c3-cli/tests/maintain.rs`, `tests/fixtures/fake-mnt*` | integration tests with fake CLIs |
| `crates/c3-core/src/effort.rs` | grok schema transport `native` -> `prompt-only` |
| `crates/c3/src/consult/orchestrate/{secondary,mod,render}.rs`, `consult/prompt.rs` | fresh-conversation re-ask, dead re-ask outcome, answer-once paragraph |
| `crates/c3/src/engines/grok.rs`, `crates/c3-core/src/{ledger,handoff}.rs` | final text after the last tool call; `read_insist.new_conversation` |
| `crates/c3-cli/tests/grok_engine.rs`, `tests/fixtures/fake-grok.ps1` | tests for the above |
| `crates/c3/src/budget/{mod,tests}.rs` | z.ai and Muse quota sources |
| `crates/c3-cli/build.rs`, `Cargo.toml`, plugin manifests | build commit in `--version`, 0.2.4 |
| `CHANGELOG.md`, `docs/DESIGN.md`, `plugin/skills/consult/reference.md` | documentation |

## Open findings

_(none open)_

## Evidence

- Unit tests: `cargo test -p c3-core` 281 passed; `cargo test -p c3 --lib` 616 passed. The full
  single-threaded `c3-cli` integration suite is quoted in the prompt line of this consultation.
- Live, this machine: `c3 maintain --run --dry-run` names the right installed/latest version of all
  six CLIs and leaves no state; `c3 maintain --smoke --engine codex --engine kimi` passed (codex on
  `ZAI::glm-5.3-flash`, kimi on `kimi-code/k3.1-pro`).
- Live, grok-4.7 / CLI 1.0.77, one fixture brief: with `--json-schema` 3 of 5 fresh conversations
  ended on a schema-shaped placeholder with 0 tool calls; without it 10 of 10 read the files (6
  direct CLI runs, 4 through `c3 consult --schema-transport prompt-only`).
- NOT exercised live: a real update, a real rollback, the kimi installer run, a real roster swap.

## Questions

- **Q1.** `c3 maintain` pass: name any path that installs without a passing smoke, leaves an engine
  on a version that failed its smoke while a rollback existed, updates an engine in use, loops
  (the hook spawning a pass on every session), or lets a crash leave the lock or state inconsistent.
- **Q2.** Roster swap: can the successor rule or `replace_entry_model` seat a wrong model, touch
  another entry, or leave a roster that does not load? Is the never-swap rule for stamped ids sound?
- **Q3.** Security of the pass: the kimi installer download-and-run, `npm install -g` of a new
  version, child environment, key handling on the model-list GETs, anything a hostile version
  string, model id or HTTP body could inject into a command, a path or the report line.
- **Q4.** Grok: with the schema in the prompt and fresh-conversation re-asks, is there a path where
  a zero-tool or placeholder reply ends up usable, a wrong thread is recorded or resumed, the reply
  file holds the wrong attempt, or the outcome describes the wrong attempt?
- **Q5.** Budget sources: parsing, units (epoch ms, percent vs ratio), window attribution, cache
  and failure behaviour for z.ai and Muse - anything that would feed the panel draw a wrong window.
- **Q6.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
