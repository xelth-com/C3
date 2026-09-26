# C3 design consilium - reviewer pack 01 (paste this whole file into the chat)

You are one member of a review panel for a software project. Other reviewers (GLM, MiMo, Kimi, Qwen, Muse, and
later GPT) answer the same brief independently; a human coordinator and Claude Code reconcile the answers. Your
job is to reason about the ARCHITECTURE questions below, not to write code. You are read-only: everything you need
is in this document - the brief, then three context digests of the sibling projects, then the current README and
scaffold of C3. Files the brief cites by path (`.collab/c3-design/context/...`, `README.md`, `src/main.rs`) are
included below in full, so cite them by those names.

Rules for your answer:
1. Answer every question Q1..Q8 by number, in Markdown, at most 900 words for the answers section.
2. First challenge the framing itself if you think it is wrong; name options the brief does not list.
3. Say what you would need to know to choose between options. Mark "assumed" where you did not verify.
4. After the answers, add a section `## Findings` with one bullet per concrete defect or risk you assert, in this
   shape: `- [severity: blocker|major|minor|note] claim - trigger: ... - evidence: (read: <where in this doc> |
   inferred | assumed) - verification: <one step the coordinator can run> - remedy: <fix>`. An empty list is a
   valid answer.
5. End with `## Verdict: ADVISE - <one sentence>` and, only if you need something you cannot obtain from this
   document, `## Requested checks` with at most 5 items RC1..RCn, each one runnable command or procedure and the
   observation that would settle it.
6. Do not restate the brief. Do not write code beyond a few lines of illustration. Do not flatter; disagreement
   is the useful signal.

Model you are answering as: (state your model name on the first line of your reply)

---

# PART 1 - The brief (.collab/c3-design/handoffs/01-claude-merge-framing.md)

# Handoff 01 - Claude: C3 merge framing

Date: 2026-09-26. Base commit: `b13aff1` + uncommitted: this `.collab/c3-design/` tree (brief and three context digests).

## Question

C3 is the Rust rewrite of the `codex-consult` bridge (the plugin's README is its spec, port order in `README.md`).
The maintainer now wants C3 to also absorb the principles of two sibling projects: **eckSnapshot** (repo snapshots for a
chat model's window, `.eck/` manifests, Architect/Coder finish-task loop, an MCP server) and **xelth.rs** (SurrealDB
GraphRAG context assembly, an OpenRouter consilium with a judge, the T-hub telemetry client). I need the architecture
that merges them without losing what makes the bridge trustworthy: read-only reviewers, files as evidence next to the
code, the bridge never handling a key. The shape is expensive to change later because the port order, the store and
the engine abstraction all hang on it.

## Vision (from the maintainer - fixed, not up for debate)

- The coordinator stays Claude Code (Fable as lead). The panel is mandatory but variable in thickness: from a
  "narrow accompaniment" (one cheap reviewer shadowing checkpoints) to a "thick consilium" (every available reviewer
  on a framing or decision).
- Participation is probabilistic and telemetry-driven: a reviewer that was more useful on a purpose/topic before is
  drawn with higher probability; an exploration share keeps everyone in rotation; new models start neutral (plugin R15).
- Subscriptions stay the primary path, but an API path must exist: OpenRouter as the concentrator, for people who will
  not juggle subscriptions and rarely consult.
- Snapshots: (a) carry a slice of the project into any chat model's window (eckSnapshot); (b) "online explanations" -
  a prompt file with a claim/announcement plus a curated slice that anyone pastes into any chat to get a grounded
  explanation of one aspect of the program (to interest someone in a project fast).
- The light PowerShell plugin remains for light use. C3 is the heavy one, on embedded SurrealDB, which should assemble
  context instantly the way xelth.rs's retriever does.
- Astra (gpt-6-astra) joins on Monday; this framing runs without it.

## Task state

- Done: scaffold (`src/main.rs`: `--version`/`--help`, no dependencies), README with port order and telemetry terms.
- Open: everything else. No `.collab` history in this repository before this brief.
- Not in scope: the plugin's own roadmap items being shipped in PowerShell; UI; hosting.

## Evidence (all inside this repository - read these, they are the digest of the three projects)

- `README.md` - C3's port order: providers/preflight -> one consultation -> ledger/findings -> roster/panel/purposes/peaks
  -> telemetry -> Claude Code packaging; telemetry terms (ON by default, one-line off).
- `.collab/c3-design/context/codex-consult.md` - the bridge as it is: files written per consultation, reply schema,
  roster (`panel: always|weighty`, no numeric weight), deterministic roster-order selection, scoreboard that feeds
  nothing back, and the maintainer's already-taken decisions R12 (detach/status file), R13 (`$CODEX_CONSULT_ROOT`,
  self-review guard), R14 (panel size by stakes, `lab` diversity), R15 (weighted draw, `-Topic`, decay, neutral prior,
  draw recorded in the ledger), R17 (decided opt-in in the plugin vs ON by default in C3's README - open contradiction).
- `.collab/c3-design/context/eckSnapshot.md` - snapshot anatomy (AI header prompt + tree + file bodies with
  `--- File: ---` markers, trailing role prompt), anchor + delta (`update_seq`), depth 0-9 / skeleton, binary sniff,
  redaction, `.eck/` manifests and `[STUB]`, `eck_finish_task` loop (report -> commit -> delta snapshot), MCP tools.
- `.collab/c3-design/context/xelth-rs.md` - SurrealDB 3.0.5 embedded (surrealkv): `entity` table, four BM25 indexes +
  HNSW, relations `relates_to`/`belongs_to`/`calls`; `retrieve_context` = HNSW + BM25 fused by RRF then multi-hop
  graph; `consilium.rs` = OpenRouter panel + Claude judge with `redact()`; T-hub client (`instance_id`, spool, 3 s).

## Proposed shape (my current preference - challenge it)

1. **One binary `c3`, one cargo workspace**: `c3-core` (brief, consultation, finding, ledger entry, roster, purpose,
   effort, lineage), `c3-store` (trait `Store`: `files` = byte-compatible `.collab/<task>/{sessions.json,findings.json,
   handoffs/}`; `surreal` = embedded SurrealKV), `c3-engines` (trait `Engine`: `codex`, `agy`, `muse` as subprocess
   engines; `http` = native OpenAI-compatible client for OpenRouter and any direct API), `c3-snapshot` (discovery,
   filters, redaction, depth, full + delta vs anchor, the explainer pack), `c3-context` (SurrealDB index + retrieval in
   the xelth.rs shape; feeds the `http` engine, which has no tools), `c3-panel` (participation policy, scheduler,
   detach/status per R12), `c3-telemetry` (T-hub client + spool), `c3-mcp` (stdio MCP: consult, panel, findings,
   snapshot, explain, finish_task), `c3-cli`.
2. **Files stay the record of a consultation** (committable, plugin-compatible). **SurrealDB is the index and the
   memory**: scoreboard, participation priors, the code graph, cross-project reviewer history. The DB is rebuildable
   from files + repo; losing it loses no evidence.
3. **Participation policy**: for purpose p and the brief's topic, each roster member m gets a Beta posterior from its
   Y/P/N marks and verified/rejected counts on similar past briefs (similarity = SurrealDB BM25/HNSW over stored
   briefs, so no hand taxonomy; `-Topic` tags as an override); the coordinator sets a thickness k or a token budget;
   draw k by Thompson sampling with a floor epsilon per member and a neutral prior for unseen (m, p); `weighty` keeps its
   purpose gate; R14's `lab` diversity applies; the ledger records probabilities, seed and draw.
4. **API path**: `http` reviewers get no tools; C3 assembles their context deterministically (graph-selected slice
   within a budget + the brief), one call, same reply schema. The key stays an env var; C3 never prints or stores it;
   `redact()` on every error string.
5. **Explainer pack**: `c3 explain --claim "<text>" --focus <paths|tags> --budget <tokens>` -> one `.md`: role and
   answer-style prompt, the claim, the `.eck` digest, the graph-selected slice, a closing instruction. Same generator
   as the `http` reviewer context.
6. Port order stays as in README, but `Store` and `Engine` traits are fixed in step 1 so `surreal` and `http` slot in
   without a rewrite; the MCP server lands after step 4.

## Alternatives weighed

1. **DB as the single source of truth, files exported on demand** - cleaner queries; breaks committable evidence and
   the plugin's `.collab` contract; a corrupt DB loses history.
2. **Tool loop for `http` reviewers** (C3 offers `read_file`/`grep` tools) - parity with CLI engines; more tokens and
   code; read-only is then enforced by C3, not by a sandbox. Could follow as opt-in.
3. **One crate, modules only** - faster start; the light/heavy boundary is harder to keep honest.
4. **Depend on xelth.rs as a service** (its brain server on :4446) instead of porting the schema - less code; a
   runtime dependency on a mid-refactor project.

Current preference: the six points above.

## Questions

- **Q1.** Files = record, SurrealDB = rebuildable index/memory: right split, or should C3 commit to the DB as source
  of truth? What breaks in each case (locks, recovery, two tools on one repo)?
- **Q2.** Participation policy: is Thompson sampling over Beta posteriors sound with tens of consultations, or is a
  softmax over the scoreboard hit-rate with epsilon-exploration better? How should topic similarity be derived
  (stored-brief similarity vs explicit tags)? How is the draw kept auditable and reproducible?
- **Q3.** How should thickness be expressed: `--panel k`, `--budget <tokens>`, or named levels (`shadow` / `council` /
  `consilium`) with per-purpose defaults in R14's spirit?
- **Q4.** API path: context-pack-only for `http` reviewers first, or the tool loop first? Does read-only still hold?
- **Q5.** Which xelth.rs pieces to port vs depend on: schema + retriever, the consilium judge (as an optional
  `reconcile` stage run by the coordinator's own model?), the T-hub client (copy verbatim)?
- **Q6.** Explainer pack: what must its header contain so a stranger's chat model answers grounded and does not drift
  into generic code review? Embed file bodies, or graph relationship sections plus focus files only?
- **Q7.** Crate layout and dependency budget: the workspace as proposed or fewer crates? Should `surrealdb` be a cargo
  feature (`heavy`) so the light path builds small, or always in?
- **Q8.** What did I miss that is expensive to change later (lineage/thread identity for `http` engines, the lock
  model with a DB present, R17 opt-in vs on-by-default, R13 host invariance, Windows/macOS/Linux paths)?

Answer by number. Keep it under 900 words. Cite the context files by path when you rely on them; mark "assumed"
where you did not verify.

---

# PART 2 - Context digest: .collab/c3-design/context/codex-consult.md

# Digest: claude-codex-consult (the PowerShell plugin C3 ports; C:\Users\Dmytro\claude-codex-consult, plugin 0.5.0) — read-only recon, 2026-09-26

## Layout / entry points
- `plugins/codex-consult/scripts/codex-consult.ps1` — one consultation or a panel.
- `scripts/codex-consult-common.ps1` (6574 lines) — roster loading/validation, panel member selection, effort caps, peak windows, ledger/findings writers.
- `scripts/codex-providers.ps1` — reviewer availability (config.toml providers + roster engines). `scripts/codex-findings.ps1` — findings.json (list/status/rate/stats).
- `scripts/codex-scoreboard.ps1` — read-only report over sessions.json + findings.json per reviewer × purpose.
- `scripts/codex-consult-hook.ps1` + `hooks/hooks.json` — one SessionStart hook printing a one-line availability summary.
- `skills/consult-codex/SKILL.md`, `skills/setup-providers/SKILL.md` — the two Claude-facing procedures.
- `schemas/consult-reply.schema.json` — the structured reply contract. `templates/brief-framing.md`, `brief-review.md`. `evals/`. `examples/codex-consult-roster.json`.

## One consultation
1. Coordinator writes a one-page brief to `.collab/<task>/handoffs/<NN>-claude-<slug>.md`.
2. `codex-consult.ps1 -Task <id> -Purpose <p> -Brief <path> -Prompt "<ask>" -ReplyName <slug>`. Engine: `codex exec` (default) or per roster entry `agy` (Gemini via Google Antigravity CLI) / `muse` (Meta Muse Code CLI). Prompt on stdin; exec options before the subcommand; read-only sandbox by default.
3. Written: `handoffs/<NN>-<engine>-<slug>.md` (header + verbatim reply + rendered findings), `.reply.json` (raw structured reply), `.events.jsonl`, optional `.original.md` (after a format repair), `.partial.md` (after a timeout kill). Task-level `findings.json` and `sessions.json` (append-only ledger; one object per run: reviewer/lineage/preflight/mode/bridge_outcome/verdict/findings/usage/wall time/peak window).
4. Reply schema (draft-07, additionalProperties:false): `schema_version`, `verdict` (ACCEPT|HOLD|REJECT|ADVISE), `verdict_reason`, `reply_markdown`, `findings[]` (severity, locations[], claim, trigger, evidence[] {kind: read-code|ran-command|inferred|assumed}, verification, remedy, supersedes[]), `prior_findings[]`, `unproven[]`, `first_run_checklist[]`.
5. `findings.json` entries: id `F<NN>-<k>`, status proposed|implemented|verified|rejected|wontfix|superseded, history[], `ratings[]` (from `-Rate n -Useful yes|partly|no`).
6. Purposes: framing, decision, checkpoint, core-contract, acceptance, diff-review, stuck, chore — each with default effort (low..xhigh), max words, timeout. Weighty purposes: framing, decision, core-contract, acceptance, stuck. Effort caps per endpoint ("caps-v1"), never inferred from the model. Peak windows: `CODEX_CONSULT_PEAK_<PROVIDER>` schedules; warn or refuse (`-OffPeakOnly`).
7. Timeout → one continuation turn on the same thread; else `.partial.md` + a printed resume command. Format repair: one recorded turn when prose came back instead of JSON (`format_retry.drift[]`).
8. Lineage = provider + model + endpoint; never fork/resume across lineages. Thread ids per repo; project isolation via `<repo>/.collab/<task>/`.

## Panel today
- Roster `{roster_version:1, reviewers:[{provider, model, engine?, codex_config?[], auth?, panel?: "always"|"weighty"}], parallel?: {label:n}}`. No numeric weight field.
- `Select-PanelMembers`: deterministic, roster order; every entry whose preflight passes runs unless it is `weighty` and the purpose is not weighty (or `-PanelAll`). Single-reviewer selection = first available entry (first-fit).
- Members run in parallel across endpoints, sequential within one endpoint; each is a full independent consultation with its own lineage, reply file and ledger entry (`panel` field); all see the same pre-panel open-findings snapshot. Panel holds the task lock; a failing member does not stop the rest; exit 0 only when all usable.
- Reconciliation: none automated — the coordinator merges by hand. Explicit non-goal: no majority voting on verdicts. Cross-wave linking (`-Link` corroborates/contradicts/duplicates, canonical issue id) deferred (R9).
- Effectiveness: `codex-scoreboard.ps1` reports per (lineage, purpose): consults/usable/failed/raised/verified/rejected/hit%/verdict letters/Y-P-N usefulness marks/median wall time/tokens. It feeds NOTHING back into selection. R15 (telemetry routing: weighted draw by scoreboard usefulness + topic + exploration share, neutral prior for new models) is planned 0.5, depends on R14, zero code.

## Providers / engines
- `[model_providers.<name>]` tables in `~/.codex/config.toml` with `env_key`; `codex login status` for the builtin openai provider. `agy`/`muse` are roster-declared engines, not config providers. Any Responses-API provider works through the same env_key mechanism; OpenRouter is not special-cased.
- README line 17: "Never create, print or paste an API key." — the bridge never handles keys (never in config.toml, roster, brief, state.md or a commit). Muse refuses to run when `META_API_KEY`/`MODEL_API_KEY` are set (guards against per-token billing instead of the subscription).

## Telemetry
Not implemented in the plugin today (R17, planned 0.5): one anonymised event per consultation to `https://xelth.com/T/v2/...`, NDJSON spool under `<codex home>/telemetry-spool/`, background send 3 s, retry, 7-day drop; payload = app_id, version, salted instance id, event_type, severity, details (engine/provider/model/purpose/outcome/wall/tokens/findings counts/panel size/OS); never task names, prompts, briefs, paths, thread ids, finding text, keys. `CODEX_CONSULT_TELEMETRY=off`. `-Complain` previews then sends and prints a public_ref. The roadmap text is inconsistent (opt-in vs on-by-default) — C3's README says ON by default.

## ROADMAP (open)
R9 partial (linking deferred); R10 `claude` engine open (T7 agy account binding); R11 parallel panel built (doc status inconsistent about a live run); **R12 non-blocking panel** (`-Detach`, `.panel-<id>.status.json`, `-Status`) open; **R13 host invariance** (decouple from `.claude-plugin`/hooks/`${CLAUDE_PLUGIN_ROOT}`; self-review guard: warn when a roster lineage equals the coordinator's own model) open; R14 adaptive companions (panel size/diversity by stakes, `-PanelSize`, `lab` field) planned; **R15 telemetry routing** planned; R16 companion roles idea; **R17 telemetry intake** planned. TECH_DEBT: T5 auth-window reset, T7 agy lineage/account binding, T8 agy write-detection gaps.
Design docs by the maintainer already exist in the plugin repo: `.collab/nonblocking-2026-09-26/handoffs/01-claude-r12-design.md`, `05-claude-r12-decisions.md`; `.collab/host-2026-09-26/handoffs/01-claude-r13-design.md`.

## Maintainer decisions already taken in the plugin repo (read 2026-09-26; C3 should not diverge without saying so)
- **R12 non-blocking** (`.collab/nonblocking-2026-09-26/handoffs/05-claude-r12-decisions.md` D1–D12): `-Detach` (solo or with `-Panel`), `-Status [<id8>]`, `-Wait [-WaitTimeoutSec n]`, `-Prune` (7 days). Status file `<task>/.consult.detached-<id8>.status.json` with states starting|running|done. Foreground writes `starting` and takes no lock; the background process holds `.consult.lock` and writes `{state: running, pid, start_time}`. Launch returns in ~5 s; the task lock blocks a second consult on that task until done. Rejected: any daemon, queue or notification channel beyond the status file and hooks.
- **R13 host invariance** (`.collab/host-2026-09-26/handoffs/01-claude-r13-design.md`): skills resolve `$CODEX_CONSULT_ROOT` (env > `${CLAUDE_PLUGIN_ROOT}` > the skill's own dir); `install-codex-host.ps1` copies skills into `~/.codex/skills/`; README gets Claude Code / Codex CLI / any-shell tabs. Self-review guard: optional `CODEX_CONSULT_COORDINATOR=<provider>::<model>` — a reviewer whose lineage matches it WARNS ("second opinion from the same model"), never blocks; the ledger records `coordinator`. Rejected: a second packaging format; the plugin id stays `codex-consult` until 1.0.0, then renamed to C3.
- **R14 adaptive companions (0.5)**: panel size by stakes — checkpoint 1, framing/decision 2–3, core-contract/acceptance 3–5, stuck = all available; diversity rule prefers a distinct `lab` before a second model of the same lab; floor of 1 companion on framing/decision; `-PanelSize <n>` overrides.
- **R15 telemetry routing (0.5, after R14)**: scoreboard hit-rate feeds a weighted draw (usefulness + topic-tag match via `-Topic <tags>`); an exploration share keeps under-used/new models in rotation (new model id = neutral prior; old marks decay); availability is filtered BEFORE the draw; the ledger records the routing weights and the draw.
- **R16 companion roles** (idea): edge-case hunter, security reviewer, test designer (failing tests as text), docs checker — as brief templates / purpose presets; members stay read-only.
- **R17 telemetry (0.5)**: decided **opt-in** on 2026-09-26 in the plugin ROADMAP — while C3's README says ON by default with a one-line off switch. This contradiction is open.
- **R11**: a live 8-member panel ran on 2026-09-26 (≤7 concurrent, 30 min wall vs ~107 min summed).
- **Scoreboard metrics** per (reviewer, purpose): CONSULTS, USABLE, PROSE, FAILED; RAISED/VERIFIED/REJECTED/WONTFIX/SUPERSEDED/OPEN; verdict counts A/H/R/D; rating marks Y/P/N; MEDIAN_S; input/output tokens; `HIT% = verified / (verified + rejected)` (null when both zero). No aggregate usefulness formula exists yet — that is R15's router.
- **Reply schema** (`schemas/consult-reply.schema.json`): top level all required — `schema_version` ("1"), `verdict`, `verdict_reason`, `reply_markdown`, `findings[]`, `prior_findings[]`, `unproven[]`, `first_run_checklist[]`; finding: `severity` (blocker|major|minor|note), `locations[{path,line}]`, `claim`, `trigger`, `evidence[{kind, reference, observation}]`, `verification`, `remedy`, `supersedes[]`.

---

# PART 3 - Context digest: .collab/c3-design/context/eckSnapshot.md

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

---

# PART 4 - Context digest: .collab/c3-design/context/xelth-rs.md

# Digest: xelth.rs (sibling Rust project, C:\Users\Dmytro\xelth.rs) — read-only recon, 2026-09-26

## What it is
Single-crate Rust CLI `xelth` (edition 2021), mid-refactor toward a pure "brain" service. Two historical purposes:
(A) a **GraphRAG data engine** — parses Rust/Markdown into a SurrealDB knowledge graph and does hybrid search
for LLM context; (B) a "Zero-Cost Automation Loop" (browser bridge / Chrome extension / WS bridge orchestrating
Claude Code) that was split out into a standalone `gemini-mcp` project on 2026-08-31 and is gone from this repo.
Active today: `mcp` (JSON-RPC/HTTP :4446 brain server), `consilium` (multi-model panel + Claude judge),
indexing/search/watch, `orchestrate`.

Modules in `src/` (18 files):
- `main.rs` CLI dispatch; `cli.rs` clap definitions
- `db.rs` DTOs + SurrealDB schema setup
- `parser.rs` `ra_ap_syntax` CST parsing (Rust) + Markdown header chunking
- `indexer.rs` parse → embed → store → graph edges
- `retriever.rs` hybrid search + graph traversal → LLM context
- `embedder.rs` embedding abstraction (Gemini cloud / local Candle); `jina_code_model.rs` JinaBERT v2 candle model; `llm.rs` Gemini embedding client
- `orchestrator.rs` Gemini/GLM router for the retired loop; `terminal.rs` spawns `claude -p` sessions
- `consilium.rs` multi-model panel via OpenRouter + Claude judge  ← directly relevant to C3
- `mcp_server.rs` / `mcp_stdio.rs` HTTP and stdio MCP servers (hand-rolled JSON-RPC, no MCP crate)
- `context.rs` / `acd.rs` token-budget context window + "Aggressive Context Dehydration" (page stale messages to disk with LLM summaries)
- `kb_sync.rs` selective knowledge-base sync between xelth nodes; `embed_http.rs` loopback HTTP embedder endpoint
- `scanner.rs` secret scanner (redaction), ported from eckSnapshot; `util.rs`

## Key dependencies
tokio 1.36 (full), surrealdb 3.0.5 (feature `kv-surrealkv`, embedded), serde/serde_json, candle 0.10,
ra_ap_syntax 0.0.326, reqwest 0.12 (json, rustls-tls), anyhow, dotenvy, clap 4.5 derive, ignore 0.4,
hmac/sha2/base64, hf-hub + tokenizers, async-trait, notify 6.1, tracing stack. No axum, no MCP crate.
`[profile.dev.package."*"] opt-level = 3` because debug Candle inference is 20-100x slower.

## SurrealDB usage (embedded surrealkv, no server)
Schema in `src/db.rs::setup_database_schema`: one `entity` table (SCHEMALESS on purpose), `embedding: option<array<float>>`;
a custom `code_analyzer` backs four BM25 fulltext indexes (`code_search`, `name_search`, `path_search`, `summary_search`)
plus an HNSW index on `embedding`. Three RELATION tables: `relates_to` (semantic, from `@relates` docstring tags),
`belongs_to` (structural, AST impl/mod nesting), `calls` (execution, AST call expressions) — a 3-layer graph.
Also `training_data` and `file_hash` tables (incremental indexing; deterministic entity ids `path::name`, UPSERT, hash skip).

"Assemble context" = `retriever.rs::retrieve_context`: HNSW cosine top-5 + BM25 fulltext fused via RRF, then multi-hop
graph traversal in 4 directions (->relates_to->, ->calls->, ->belongs_to->, <-belongs_to<-), formatted into labeled
relationship sections for the LLM. `.eck/CONTEXT.md` records SurrealDB v3 idioms: `INSERT INTO` not `CREATE`;
`record::id(id) AS id`; `FULLTEXT ANALYZER ... BM25`; `type::record('table', $id)` in RELATE; inline FTS literals; `<|K, EF|>` HNSW syntax.

## Telemetry / T-hub (from C:\Users\Dmytro\xelth.com\docs\t-hub\)
Base `https://xelth.com/T`. Rules: on by default with one env off-switch disclosed once; never blocks (NDJSON spool under
config dir, background send 3 s timeout, retry next run, drop after 7 days); payload never logged except debug;
complaints shown before sending; `instance_id = sha256(random salt on disk + machine name)`; server scrubs secret-named keys.
Endpoints: `POST /v2/events` (single or batch ≤100, ≤64 KiB) → `{ok, accepted, event_ids, scrubbed}`; 403 = app_id not
allow-listed; 429 + Retry-After. `POST /v2/complaints` → `{ok, complaint_id, public_ref}` (e.g. `T-7KQ4-M2XZ`, quotable on `/F/p/<app_id>`).
`DELETE /v2/instances/<instance_id>?public_ref=...` = forget-me. Reference client `docs/t-hub/client.rs` (~35 lines: serde_json,
sha2, hex, ureq blocking 3 s, uuid): `instance_id()`, `send_event(app_id, app_version, event_type, severity, details)` on a
background thread, `send_complaint(...)` with a confirm callback. Event shape: `app_id, app_version, instance_id, event_type,
severity, title, details, client_time, os, runtime, tags`.

## LLM / provider integration
- Gemini embeddings (`llm.rs`): key in `x-goog-api-key` header, model `gemini-embedding-2-preview`, 768-dim.
- **Consilium** (`consilium.rs`): fans one question to a panel of frontier models in parallel over **OpenRouter**
  (`chat/completions`) plus a Claude subscription seat via `claude -p` (no API cost), then a **Claude judge** ranks all
  labeled answers, states consensus/disagreement, and is told which seat is its own. Panel resolution: `--models` override →
  `api_panel` in `scripts/consilium-panel.json` (checked against live OpenRouter `/models`) → `--panel auto`
  (`pick_default_panel`: newest flagship per lab, deny-list for cheap/mini/flash). `OPENROUTER_API_KEY` from env else `.env`;
  `redact()` strips the key and `sk-or-...` shaped tokens from every error string (unit-tested). GLM rides OpenRouter as `z-ai/glm-5.3`.
  Each seat's answer → `<out_dir>/R_<NN>_<model>.md`, verdict → `VERDICT.md`.
- GLM orchestrator (`orchestrator.rs`): ZhipuAI API with HMAC-SHA256 JWT, global mutex for 1 concurrent request.

## Conventions worth copying
`anyhow::Result` with context everywhere; `redact()` before any error surfaces; secret scanner; config via dotenvy + explicit
env checks; structured docstring tags (`@concept`, `@relates`, `@reasoning`) at the top of every module — human docs AND the
GraphRAG semantic-edge source; `tracing` to file+console; deterministic ids / idempotent UPSERT flagged in comments;
`.eck/` manifests kept current and dated, retired sections marked rather than deleted.

## Directly reusable for C3
`consilium.rs` (panel + judge shape, panel resolution, redaction, per-seat files); the T-hub client verbatim; the SurrealDB
schema/query idioms and the embedded SurrealKV + BM25 + HNSW combo as the template for "assemble context instantly";
`acd.rs`/`context.rs` token-budget pattern for long panel threads.

## Open questions
Whether "SurrealDB assembles context from xelth.rs" means (a) C3 embeds/reuses the same graph engine as a library/service,
or (b) C3 borrows the schema/query patterns into its own separate store. `scripts/consilium-panel.json` and
xelth.com `crates/telemetry/README.md` (server side) were not read.

---

# PART 5 - C3 as it is today

## README.md

```markdown
# C3 — claude-codex-consult in Rust

**Status: early port, not functional yet.** The working implementation today is the
PowerShell plugin [`codex-consult`](https://github.com/xelth-com/claude-codex-consult)
(install in Claude Code: `/plugin marketplace add xelth-com/claude-codex-consult`, then
`/plugin install codex-consult@claude-codex-consult`). C3 is that bridge rewritten in Rust
from a finished copy of it; the plugin stays, and its README is the specification this port
follows section by section. Page: <https://xelth.com/C3/>.

## What it is

A coordinator — your Claude Code, Codex CLI or any shell session — asks a **roster of
reviewers from different labs** to look at a one-page brief. Reviewers are read-only:
nothing is edited by them. Every consultation is recorded as files next to the code (the
brief, the reviewer's reply verbatim, a JSON ledger, **findings tracked by id** that you
verify and move yourself), and a **panel** runs several reviewers in parallel and
reconciles them. Reviewers are subscriptions the user already has, never API keys the
bridge handles: the ChatGPT plan through the Codex CLI, z.ai GLM, Xiaomi MiMo or any
Responses-API provider through a `[model_providers.<name>]` table, Gemini through Google's
Antigravity CLI `agy`, Meta Muse through the Muse Code CLI.

## Why a Rust rewrite

One static binary instead of PowerShell 5.1/7 on three OSes; a typed ledger and findings
store; the same behaviour under Claude Code, Codex CLI and a plain shell without a host
adapter (plugin ROADMAP R13, host invariance); and a place to make the bridge fast enough
to run a panel non-blocking (R12).

## Plan — port order (each step lands with tests and the plugin's evals as the oracle)

1. **Providers and preflight** — the Codex config walk (`[model_providers.*]`, `env_key`),
   `codex login status`, the roster file, the availability verdicts of `codex-providers.ps1`.
2. **One consultation** — brief → `codex exec` (and the `agy` / `muse` engines), the reply
   file with its header lines, the events stream, the `-DryRun` output byte for byte.
3. **Ledger and findings** — `sessions.json`, `findings.json`, ids, statuses, ratings,
   the atomic write order and the lock/recovery rules.
4. **Roster, panel, purposes, effort vocabularies, peak windows.**
5. **Telemetry and complaints** (plugin ROADMAP R17) — the terms below, the spool, `--complain`,
   `--forget-me`.
6. Claude Code packaging (skill, hook, evals) as a thin layer over the binary.

## Telemetry (on by default, off with one line)

Installing C3 means accepting these terms. C3 reports **one anonymous event per
consultation** to the maintainer's intake, <https://xelth.com/T/>, so the bridge can be
improved when someone the maintainer never hears from uses it. Everything collected is shown
in the open, as counts, on <https://xelth.com/C3/>.

* **Sent:** `app_id` (`c3`), the C3 version, an instance id
  (`sha256(salt file + machine name)` — stable per installation, meaningless elsewhere),
  `event_type: consultation`, a severity, and `details`: engine, provider label, model,
  purpose, outcome class (`usable`, `failed:<class>`), wall seconds, token counts, findings
  counts, structured or not, a format retry or not, panel size, OS, runtime.
* **Never:** task names, prompts, briefs, paths, thread ids, finding texts, user names, keys.
  The server drops secret-named keys on arrival and reports how many; C3 treats a non-zero
  count as its own bug.
* **Off:** `CODEX_CONSULT_TELEMETRY=off`, or `--telemetry off` per run. The first run after an
  install prints this notice once; the dry run says whether it is on.
* **Never blocks a run:** events queue in a local spool (NDJSON under the config dir), are sent
  in the background with a 3 s timeout, retried on the next run and dropped after 7 days.
* **Complaints:** `c3 --complain "<text>"` prints the exact payload (your text plus the last
  ledger entry's summary), asks before sending, and prints a public reference like
  `T-7KQ4-M2XZ` to quote on the forum section <https://xelth.com/F/p/c3> or in a mail.
* **Delete my data:** `c3 --forget-me` (or
  `DELETE https://xelth.com/T/v2/instances/<instance_id>?public_ref=<ref>`) removes every
  event and complaint this installation ever sent.

Client rules and a 30-line Rust reference client:
[xelth.com/docs/t-hub](https://github.com/xelth-com/xelth.com/tree/main/docs/t-hub).

## Build

```sh
cargo build --release          # target/release/c3
./target/release/c3 --version
```

No dependencies yet; each ported piece brings its own when it needs one.

## License

MIT — see [LICENSE](LICENSE).
```

## Cargo.toml

```toml
[package]
name = "c3"
version = "0.1.0"
edition = "2021"
description = "C3 — claude-codex-consult in Rust: a coordinator asks a roster of read-only reviewers from different labs, records every consultation as files, tracks findings by id, runs a panel."
authors = ["xelth-com"]
license = "MIT"
repository = "https://github.com/xelth-com/C3"
homepage = "https://xelth.com/C3/"
readme = "README.md"
keywords = ["codex", "code-review", "consultation", "second-opinion", "claude-code"]
categories = ["command-line-utilities", "development-tools"]

[[bin]]
name = "c3"
path = "src/main.rs"

[dependencies]
# Deliberately none yet. The port adds a dependency only when a ported piece
# needs it (serde/serde_json for the ledger and findings, sha2/hex for the
# telemetry instance id, ureq for the intake) — see README "Plan".

[profile.release]
strip = true
lto = true
codegen-units = 1
```

## src/main.rs

```rust
//! C3 — claude-codex-consult in Rust.
//!
//! The consultation bridge: a coordinator (Claude Code, Codex CLI, any shell)
//! asks a roster of read-only reviewers from different labs to look at a
//! one-page brief, records the consultation as files next to the code (the
//! brief, the reply verbatim, a JSON ledger, findings tracked by id) and can
//! run several reviewers as a panel.
//!
//! This is the Rust rewrite of the PowerShell plugin `codex-consult`
//! (github.com/xelth-com/claude-codex-consult), made from a finished copy of
//! it. The plugin stays and is the working implementation until this binary
//! reaches parity; README "Plan" lists the port order. Nothing below is
//! functional yet beyond `--version` and `--help`.

const HELP: &str = "\
c3 — claude-codex-consult in Rust (early port, not functional yet)

USAGE:
    c3 --version | -V        print the version
    c3 --help    | -h        this text

The working implementation today is the PowerShell plugin:
    /plugin marketplace add xelth-com/claude-codex-consult
    /plugin install codex-consult@claude-codex-consult

Page, install, roster, telemetry in the open:  https://xelth.com/C3/
Questions and complaints (forum section c3):    https://xelth.com/F/p/c3
";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => println!("c3 {}", env!("CARGO_PKG_VERSION")),
        Some("--help") | Some("-h") | None => print!("{HELP}"),
        Some(other) => {
            eprintln!("c3: unknown argument `{other}` — the port is not functional yet; run `c3 --help`");
            std::process::exit(2);
        }
    }
}
```


---

# Now answer

Everything above is the complete input. Begin with your model name, then `## Answers` (Q1..Q8 by number, at most
900 words), then `## Findings`, then `## Verdict: ADVISE - ...`, then `## Requested checks` only if needed.
Do not summarise the documents back to me; the coordinator has read them.
