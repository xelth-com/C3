You are a coding worker inside this repository. Do only what the brief asks, touch only the files it names or clearly implies, never run git, never leave the repository, and never commit. Finish with a final report: what you changed (file list), the exact test commands you ran and their result counts, any deviation from the brief and why, and what you could not do.

# Task: `c3 work` run B — integration tests and docs

CONTEXT BUDGET RULE: never read a file longer than 300 lines whole; Grep first, then Read with
offset/limit (≤ 150 lines). Do not read `crates/c3/src/consult/orchestrate.rs`.

`c3 work` is implemented (design `docs/work-mode.md`, read it fully): `crates/c3-core/src/claude.rs`
(`ClaudeWorkArgs`, `work_args`, the preamble), `crates/c3/src/work/{mod,record,detach}.rs`,
`crates/c3/src/cli/work.rs`, dispatch in `crates/c3-cli/src/main.rs`. Unit tests exist (c3-core 245,
c3 lib 434, all green). Do not change the implementation unless a test you write exposes a real bug;
if you do, say so in the report.

## Deliverables

1. `crates/c3-cli/tests/work.rs` — integration tests through the built binary (follow the pattern of
   `crates/c3-cli/tests/claude_engine.rs`: grep there for how a fake launcher is created and how the
   binary is invoked with a temporary repo and a temporary roster/`CLAUDE_CONFIG_DIR`):
   - a fake launcher (cmd stub on Windows, sh elsewhere, `#[cfg]`-gated like the existing stubs) that
     ignores stdin, echoes three stream-json lines (an `init` event, an assistant text event, a
     `result` event with `num_turns` and a `usage` object) and creates one file `worker-touched.txt`
     in the current directory; a temporary git repository with one committed file; a temporary roster
     with one worker entry whose `env_key` variable is set to a dummy value for the child;
     run `work --task t --brief <brief> --provider <label>` and assert: exit 0; the record directory
     `.collab/t/work/01-<provider>-<model>/` holds `brief.md`, `events.jsonl`, `report.md`, `diff.txt`,
     `run.json`; `brief.md` starts with the preamble and contains the brief text; `report.md` = the
     assistant text; `diff.txt` lists `worker-touched.txt` as new; `run.json` has `status: "done"`,
     `files_changed == ["worker-touched.txt"]`, `turns` from the result, no token value anywhere
     (grep the record dir for the dummy token value → absent);
   - refusals: missing `--brief` file; `--allow "Bash(git:*)"`; a non-worker label (message lists the
     worker entries); `env_key` unset; each exit 1 and no record directory;
   - `--status` over synthetic `run.json` files: all done → 0; one died → 1; one running → 2; `--id`
     narrowing;
   - a second `work` on a task with a `running` record is refused (exit 1).
2. README.md: a "Worker runs" section after the consult/panel sections (what `c3 work` is for, that
   it is not a consultation — no evidence check, no findings, no rating — the three first workers,
   the record layout, the refusals, the live-member rule not applying).
3. CHANGELOG.md: an entry under an "Unreleased / 0.2.3-dev" heading: "`c3 work` — worker runs".
4. `plugin/skills/coordinate/SKILL.md`: one paragraph in the "bridge's own background means"
   section: `c3 work --detach/--status/--wait`, a worker run counts as running work for the wait
   rule, and the live-member rule does not apply to it (a worker is expected to change the tree).

Verification: `cargo test` (whole workspace) green, `cargo clippy --all-targets -- -D warnings`
clean, `cargo fmt`. Do not run git in the repository itself (the tests may run git inside their own
temporary repositories). Do not commit.

## Final report (your last message)

Test counts per crate; which tests you could not write and why; any implementation bug found and
how you fixed it; the files changed.
