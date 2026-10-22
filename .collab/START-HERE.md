# START HERE - handover for a session opened in ~/C3 (written 2026-10-15)

Language: the operator speaks Russian; everything internal (briefs, reviewer prompts, commits, docs) is English.
Detailed state: `.collab/parity-0.6.1-2026-10-08/state.md`, section "Where things stand (handover, 2026-10-15 03:45)".
Memory of the old session lives in `~/.claude/projects/C--Users-Dmytro-claude-codex-consult/memory/` (MEMORY.md index;
read `project-c3-parity.md` and `reference-roster-0.6-claude-first.md`) - a session here has its own, empty memory.

## Where things stand (updated 2026-10-22 10:50)
- **main = a680564 (+ the CHANGELOG tidy), v0.2.3 tagged locally, NOT pushed.** 0.2.3 = c3 work v1.1 (codex + muse worker
  engines, quota resume/alarm), R30 compaction pieces, tech-debt audit, Phase 2 switch, waves 10a-10c (Astra F89-1..8,
  F91-1..4, F93-1). Astra ACCEPT at pass 4 (n=53, handoff 95; one minor F95-1 carried to 0.2.4). Suite 883/0 (30 sets).
- Binary: `cargo install --path crates/c3-cli --locked --force` (the binary crate is c3-cli; `--path crates/c3` finds no
  binary) - 0.2.3 release install from a680564 started 10:50. Plugin c3@c3 reads `plugin/` of this checkout (0.2.3).
- Phase 2 switched 2026-10-22: `codex-consult` disabled, `astra`/`workers` skills -> `c3:*`; rollback `docs/port/phase2-switch.md` (e).
- xelth.com /C3/ page strings -> 0.2.3 (nine locales, xelth.com commit f87fcdd) deployed 2026-10-22 08:03, health ok.
- Consilium consilium-2026-10-22 (10 of 13 usable): remedy proposals by other reviewers - reshaped (test as verifier,
  finder's frozen acceptance criteria + closing test first, remedy panel second, solver router dimension credited only on
  verified); `.collab/consilium-2026-10-22/notes-decisions.md`; ROADMAP "Next".
- Carry for 0.2.4: F95-1 (barrier-entry signal in the F93-1 regression); harness-detach F11-2; the 500-anchor limit of the
  shared health file; the muse engine records tokens (`muse export`).
- Tag procedure (for 0.2.4): bump `Cargo.toml` line 6, `plugin/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`;
  `cargo update -w --offline` refreshes Cargo.lock; CHANGELOG `[x] - <date>` from `[Unreleased]`; commit, tag, push; xelth.com
  locales `c3p.chip_rust|install_rust_note|rust_status` in nine files (`crates/server/locales/`), `deploy.sh`.

## Roster (10 entries, claude-first, `~/.codex/codex-consult-roster-0.6.json`; `codex-consult-roster.json` must stay its copy)
Astra (codex, weighty), glm-5.3, mimo-v2.6-pro, gemini-3.8-flash-high (agy), deepseek-v4-1-flash, dola-seed-2-0-pro,
k3-256k, qwen3.8-max (Alibaba monthly quota at 100 % until 2026-10-27, untested), muse-spark-1.3-contributor (plan meta),
muse-wonder-1.0 (weighty, since 2026-10-15).
Third-party plans expose no usage API; roster `out until` is a manual input.

## c3 command forms
`c3 consult --task <t> --status|--wait --id <id>`, `--panel --panel-all`, `--budget`, `--detach`,
`c3 providers [--short] --provider <p>`. Astra acceptance:
`c3 consult --task parity-0.6.1-2026-10-08 --provider openai --mode new --purpose acceptance --budget 90 --detach`
(check `c3 providers --provider openai` and run `date` first).

## Standing rules (binding)
- Keys are never created, printed, read back, echoed or committed. Never read `~/.codex/auth.json`.
- The coordinator keeps the final word; Astra holds acceptance authority. Rate every consultation that got a reply.
- Live-member rule: no writes or git under `.collab` or the tree while an agy or muse run is live.
- Never `-SkipPreflight` past a real refusal. Workers never modify `~/.codex/*.json`; the plugin checkout is READ-ONLY for C3 workers.
- Standing permission to delete build artifacts when disk runs out (never `.codex/sessions`).
- Harnesses run one at a time (`%TEMP%\codex-consult-tests\HARNESS.lock`). Run `date` before any time-gated action.
- Never fork heavy threads: `--mode new`, budget-aware. Plan Astra rounds against the weekly window too (5 rounds ~ 25 %).
- Never compact the session or start a heavy build while a detached acceptance run is live (pass 7's first attempt died that way).
- Keep the public plugin generic: no private-project names. Do not restart a killed background wait unasked.
- Codex desktop app and the bridge are different things; the operator closed the app.
