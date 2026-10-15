# START HERE - handover for a session opened in ~/C3 (written 2026-10-15)

Language: the operator speaks Russian; everything internal (briefs, reviewer prompts, commits, docs) is English.
Detailed state: `.collab/parity-0.6.1-2026-10-08/state.md`, section "Where things stand (handover, 2026-10-15 03:45)".
Memory of the old session lives in `~/.claude/projects/C--Users-Dmytro-claude-codex-consult/memory/` (MEMORY.md index;
read `project-c3-parity.md` and `reference-roster-0.6-claude-first.md`) - a session here has its own, empty memory.

## Where things stand (updated 2026-10-15 14:05)
- **main = 7416e04, v0.2.2 tagged and pushed** (wave 9a-9i). Astra ACCEPT at pass 7 (n=49, handoff 87; F84-1 closed, no new
  findings), conditional gate met: four fake-claude c3-cli binaries single-threaded one at a time 15/17/5/12 + c3-core 201/0,
  harness-roster 125/0 on Windows PowerShell 5.1 (pwsh 7 shows 3 TIMEONLY shape-only failures - `ConvertFrom-Json` datetime
  conversion - always run that harness with powershell 5.1). Details: state.md entries 11:50-14:05.
- Binary 0.2.1 still at ~/.cargo/bin/c3 (a 298ed07 debug copy at %TEMP%\c3-rc2\c3-298ed07.exe); `cargo install --path crates/c3-cli`
  (release, -j 2, machine otherwise idle) brings 0.2.2 when wanted. Plugin c3@c3 updated to 0.2.2 (restart applies it).
- xelth.com /C3/ page strings -> 0.2.2 (nine locales) committed by a worker; deploy pending the operator's go.
- Carry for 0.2.3: harness-detach F11-2 (fails on base); fake-claude binaries need a serialized single-threaded gate
  (they starve each other under parallel spawn - not a product bug); the 500-anchor capacity limit of the shared health file.
- Next waves (plugin repo ROADMAP R25-R29, week of 2026-10-19): R25 telemetry prerequisites, R26 worker telemetry, R27 the
  watch, R28 usage tiers (incl. the 10 % muse-wonder guard), R29 browser engine; R30 compaction rule; R31 cloud workers
  (opt-in, measured 2026-10-15: a cloud probe costs ~2 USD from the cloud credit, not the weekly quota; `claude --cloud` needs
  an operator TTY). Candidate wave after the tag: the muse engine records tokens (`muse export`).
- Tag procedure (for 0.2.3): bump `Cargo.toml` line 6, `plugin/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`;
  `cargo metadata --offline` refreshes Cargo.lock; CHANGELOG `[x] - <date>` from `[Unreleased]`; commit, tag, push; xelth.com
  locales `c3p.chip_rust|cta_rust|install_rust_note|rust_status` in nine files; `claude plugin update c3@c3`.

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
