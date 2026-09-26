# Digest: eckSnapshot (sibling Node.js tool, C:\Users\Dmytro\eckSnapshot) — read-only recon, 2026-09-26

## Layout / entry points
- `index.js` → `src/cli/cli.js: run()`; global bin `eck-snapshot`. Single JSON-payload router (`{"name": ..., "arguments": {...}}`)
  plus human shorthands (`snapshot`, `update`, `scout`, `fetch`, `profile`, ...).
- `src/cli/commands/*.js` — one per tool: `createSnapshot.js` (full, 1131 lines), `updateSnapshot.js` (delta), `recon.js` (scout/fetch),
  `detectProject.js`, `doctor.js`, `generateProfileGuide.js`, `importProfiles.js`, `setupMcp.js`, `trainTokens.js`, `claudeSettings.js`.
- `src/core/snapshotBuilder.js` — unified pipeline: `resolveEffectiveConfig → discoverFiles → renderFileAtDepth → computeArtifactMetrics`.
- `src/core/depthConfig.js`, `src/core/skeletonizer.js` — depth scale 0–9 and function-body stripping (Babel + Tree-sitter).
- `src/utils/` — `fileUtils.js` (discovery, binary sniff, ignore rules), `gitUtils.js` (anchor + diff), `aiHeader.js` (Architect prompt header),
  `projectDetector.js`, `telemetry.js`, `tokenEstimator.js`, `claudeMdGenerator.js`, `opencodeAgentsGenerator.js`,
  `eckProtocolParser.js` (parses `<eck_task>` / `<profile>` tags in LLM replies).
- `scripts/mcp-eck-core.js` — MCP server: `eck_finish_task`, `eck_fail_task`, `eck_manifest_edit`. `scripts/mcp-glm-zai-worker.mjs` — GLM worker fleet.
- `.claude/skills/eck-scout`, `eck-fetch` — thin wrappers around `eck-snapshot scout/fetch` (explore/fetch an external repo).
- `eck-telemetry/` — an existing Rust component inside the repo (prior art for a Rust telemetry client).

## What a SNAPSHOT is
One Markdown file: AI-facing header (project overview, role instructions, Architect prompt, `.eck/` manifest digest, journal summary)
+ optional directory tree + every included file's full text under `--- File: /path ---` markers. Every snapshot ENDS with an embedded,
mode-specific Architect/Coder prompt (`aiHeader.js`, from `setup.json` templates); for ChatGPT the README warns to paste that trailing
prompt as the first message or the model reverts to generic code review. README frames it as giving a chat model
"a university degree in your codebase" — paste into Gemini/ChatGPT/any large-context LLM.

Sizing: `estimateProjectTokens()` pre-scan with an adaptive polynomial estimate; final metrics (bytes, ~tokens = len/4) embedded in the
filename (`..._<sizeKB>kb.md`). `--skeleton` strips function bodies; depth 0–9 trades fullness for size in scout/link/notebook modes.
Filters: global `setup.json` rules merged with per-project-type rules → `.gitignore` → ignore globs → content-aware binary sniff
(magic bytes / null bytes); ML model files skipped by default (`--ml` = header-only metadata); hard-ignore globs for logs/dumps/swap.

Full vs delta: a full snapshot writes the commit hash to `.eck/anchor`. `eck_update` reads the anchor, auto-commits a dirty tree,
`git diff --name-only <anchor> HEAD`, renders only changed files with `update-prompt.template.md` (tells the LLM to discard prior
"Update" snapshots; new bodies are authoritative; deleted files marked `[FILE DELETED]`). `.eck/update_seq` = `<anchor7>:<n>`;
filenames `eck<repo><timestamp>_<anchor7>_up<n>_<sizeKB>kb.md`; `--base <file-or-hash>` = one-off diff (`_upcustom`).
Output: `.eck/snapshots/` (gitignored); the newest also mirrored into `.eck/lastsnapshot/` as the "active" one.

## `.eck/` manifests
`CONTEXT.md` (architecture/overview), `ROADMAP.md` / `TECH_DEBT.md` (checklists edited via `eck_manifest_edit` or the `[SYNC]` audit),
`JOURNAL.md` (never read wholesale — `aiHeader.js` summarises: latest entry full + 5 prior headers + count), `OPERATIONS.md` (commands),
`ENVIRONMENT.md` (`project_type`/`agent_id`), `ARCHITECTURAL_AUDIT.md`, `anchor`, `update_seq`, `lastsnapshot/` (+ `AnswerToSA.md`
agent report, `.lock` dir mutex), `telemetry_queue.json`, `token-training.json`. `[STUB]` markers in generated placeholders; stripped
from snapshot headers. `eck_manifest_edit(file, action: append_to_section|replace_text, content, section_header?, target_text?)`
patches by header/regex anchor without reading the whole file — pure token economy.

## Architect/Coder ("Royal Court" / Swarm) protocol
Senior Architect (a large-context web LLM reading a pasted snapshot) → Junior Architects local (Sonnet/Opus/GLM via OpenCode, or the
"Fable" tri-tier court) → GLM worker fleet via a separate MCP. Formal tasks arrive as `<eck_task id="repo:short-desc">...</eck_task>`;
only tagged work auto-triggers `eck_finish_task`. Finish loop (`mcp-eck-core.js`): write `.eck/lastsnapshot/AnswerToSA.md` → `git add .`
+ commit → `eck_update_auto` delta snapshot (embeds AnswerToSA once, lockdir-guarded; prepends to JOURNAL.md). `eck_fail_task` writes a
BLOCKED report and an emergency non-committing snapshot. Injected coder protocols: Context Hygiene, Proactive Tech Debt, Boy Scout Rule,
Zero-Broken-Windows (tests pass before finish). The protocol text is duplicated across claudeMdGenerator / OpenCode templates / aiHeader.

## Telemetry
`.eck/telemetry_queue.json`: `instanceId` (random UUID), `feedback[]`, per-tool `usage` counts, `errors[]`. `src/utils/telemetry.js`
parses AnswerToSA.md for model_name / agent_role / task_scope / status / error_summary and POSTs to hardcoded `https://xelth.com/T/report`
after each update; `telemetry disable/enable`. No source code sent.

## Token-economy principles
Never re-read JOURNAL wholesale; blind manifest edits; delta-only updates against a fixed anchor; skeleton/depth to trade fidelity for size;
pre-flight token estimate; content-aware binary filtering; the delegation ladder (expensive reasoning reserved, execution pushed down-tier).

## Rough edges
`createSnapshot.js` duplicates parts of `discoverFiles`/`renderFileAtDepth` (drift risk, noted in ARCHITECTURAL_AUDIT §1); secret redaction
only in createSnapshot's output path, not in the shared builder; skeletonizer depth-6 JSDoc bug; HUMAN_SHORTHANDS double-parse;
telemetry endpoint hardcoded with no visible auth — AnswerToSA content could leak if not redacted first.
