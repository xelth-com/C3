# START HERE - handover for a session opened in ~/C3 (written 2026-10-15)

Language: the operator speaks Russian; everything internal (briefs, reviewer prompts, commits, docs) is English.
Detailed state: `.collab/parity-0.6.1-2026-10-08/state.md`, section "Where things stand (handover, 2026-10-15 03:45)".
Memory of the old session lives in `~/.claude/projects/C--Users-Dmytro-claude-codex-consult/memory/` (MEMORY.md index;
read `project-c3-parity.md` and `reference-roster-0.6-claude-first.md`) - a session here has its own, empty memory.

## Where things stand
- main = 735ed89, v0.2.1 tagged. Binary 0.2.1 at ~/.cargo/bin/c3; the c3 plugin is read from ~/C3/plugin (user scope).
- **0.2.2 candidate = 6a462f0 (wave 9, NOT tagged).** Tests 828/0 (+ known flaky `index::embed`), harness-roster 125/0.
  GLM round 5 = ACCEPT (notes F74-1/2, non-blocking). Astra held at round 4 (F72-1/2, fixed in 9d), no re-acceptance yet.
- **Operator decision pending:** tag 0.2.2 now on GLM's ACCEPT (coordinator's recommendation), or wait for Astra after the
  ChatGPT weekly reset (Mon 2026-10-20; the window is at 96 %, no Astra rounds this week).
- Tag procedure: bump `Cargo.toml` line 6, `plugin/.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json` to 0.2.2;
  CHANGELOG `[0.2.2] - <date>` from `[Unreleased]`; commit, tag v0.2.2, push; then update the /C3/ page strings in the
  xelth.com repo (deploy) and run `claude plugin update c3@c3`.
- Next waves (plugin repo ROADMAP R25-R29, week of 2026-10-19): R25 telemetry prerequisites first (intake ignores unknown
  keys, `vocab_version`, `close_event` keeps all event types, roster validator accepts `effort`), then R26 worker
  telemetry, R27 the watch, R28 usage tiers, R29 browser engine. Consilium notes: `.collab/consilium-2026-10-14/`.
- Carry for 0.2.3: harness-detach F11-2 (fails identically on base). Plugin T17 (mirror 8b-8j) is "not now".

## Roster (9 entries, claude-first, `~/.codex/codex-consult-roster-0.6.json`)
Astra (codex, weighty), glm-5.3, mimo-v2.6-pro, gemini-3.8-flash-high (agy), deepseek-v4-1-flash, dola-seed-2-0-pro,
k3-256k, qwen3.8-max (Alibaba monthly quota at 100 % until 2026-10-27, untested), muse-spark-1.3-contributor.
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
- Keep the public plugin generic: no private-project names. Do not restart a killed background wait unasked.
- Codex desktop app and the bridge are different things; the operator closed the app.
