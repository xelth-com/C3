# Chat reply 01 - Gemini 3.8 Flash (web chat, out of ledger)

Provenance: the user pasted `.collab/c3-design/packs/01-consilium-pack.md` into the Gemini web chat (model: Gemini 3.8 Flash)
on 2026-09-26 and pasted the reply back to the coordinator. Not run through the bridge (Antigravity quota was exhausted),
so it has no ledger entry and no finding ids; the coordinator tracks its findings by hand as G1-1..G1-5 below.
Reply verbatim follows.

---

Model: Gemini

## Answers

### Framing Challenge
The framing attempts a forced three-way fusion between a read-only consultation bridge (`codex-consult`), an autonomous auto-committing task loop (`eckSnapshot`'s `eck_finish_task`), and an embedded GraphRAG brain (`xelth.rs`). Imposing this onto an unbuilt 9-crate workspace before step 1 (provider preflight) even functions invites architectural deadlock. Crucially, defining `Store` as a swappable trait (`files` vs `surreal`) is fundamentally flawed: SurrealDB cannot substitute for committable, human-auditable Git files without destroying the bridge's core value proposition.

*Unlisted alternatives:*
- **Two-tier architecture:** C3 remains a lean consultation CLI/MCP using committable `.collab/` files and SQLite for scoreboard/routing metadata; GraphRAG context assembly is an optional secondary module.
- **Embedded SQLite (with `sqlite-vec`) instead of SurrealKV:** Bypasses SurrealKV's single-process exclusive lock pitfalls while providing zero-friction metadata and vector queries.

*Needed to decide:* Whether C3 must execute git commits (`eck_finish_task`) or remain strictly read-only. *Assumed:* maintainer mandates read-only reviewers, with Claude Code remaining the sole committer.

### Q1. Storage Split
Files as record, SurrealDB as rebuildable cache/index is the only valid split.
- **DB as source of truth breaks:** Git branching (binary databases cannot merge across branches), multi-seat PR workflows, and non-blocking background panels (`-Detach`), because embedded SurrealKV takes an exclusive process lock on its directory.
- **Files as source of truth preserves:** Committable evidence, plugin compatibility, and crash recovery (`c3 reindex` rebuilds the DB from `.collab/` and Git). If the DB corrupts or locks, consultation data remains completely intact.

### Q2. Participation Policy
Thompson sampling over Beta posteriors is unsound for tens of consultations:
1. Outcomes are not binary Bernoulli trials; they are multi-class (verified, rejected, superseded) with ternary ratings (Y/P/N).
2. With small $N < 50$, uninformative Beta priors oscillate erratically, and a single rejection unfairly starves a model.

*Better:* Temperature-scaled softmax over an empirical usefulness score:
$$S = \frac{\text{verified} + 0.5 \cdot Y + 1}{\text{verified} + \text{rejected} + Y + N + 2}$$
with a fixed epsilon-exploration floor ($\epsilon = 0.15$).

*Topic similarity:* Brief text similarity via BM25/HNSW is brittle because phrasing varies widely. Use explicit `-Topic <tags>` as the primary bucket; fall back to BM25 brief similarity only when tags are absent.

*Auditability:* Deterministic PRNG seeded by `sha256(task_id + brief_sha + roster_sha + timestamp_minute)`. Record seed, candidate weights, draw probabilities, and drawn panel in `sessions.json`.

### Q3. Expressing Thickness
Use named levels (`shadow`: 1 cheap companion; `council`: 2–3 diverse lab models; `consilium`: 4–6 or full panel) as primary, with per-purpose defaults matching R14 (e.g., `checkpoint` $\to$ `shadow`, `framing`/`decision` $\to$ `council`, `stuck` $\to$ `consilium`).

Provide `--panel <k>` as a numeric override and `--budget <usd|tokens>` as a hard spending cap for API calls. Named levels preserve policy intent (enforcing R14 lab diversity) without requiring users to micromanage counts.

### Q4. API Path
Context-pack-only first; reject the tool loop for v1.
1. **Safety:** CLI engines (`codex exec`, `agy`) are sandboxed by their host runtimes. If C3 hosts file/grep tools for API models, C3 becomes the security boundary, exposing developers to prompt injection and path traversal.
2. **Determinism & cost:** Tool loops over OpenRouter introduce unpredictable multi-turn latency (30–120s) and token expense.
3. One-shot completion guarantees physical read-only safety (no execution capabilities) and feeds identical context to all panel members.

### Q5. Port vs Depend on xelth.rs
- **Schema & retriever:** Port a minimal subset into C3: SurrealDB BM25 + `ra_ap_syntax` structural AST relations (`belongs_to`, `calls`). Do not port Candle or local JinaBERT embeddings; they introduce massive build times and binary bloat for marginal retrieval gains.
- **Consilium judge:** Port the prompt schema, but expose it as an optional coordinator step (`c3 reconcile`) executed by Claude or the local coordinator, rather than an auto-invoked external call.
- **T-hub client:** Copy `docs/t-hub/client.rs` verbatim as an internal module. Do not depend on xelth.rs over HTTP `:4446` (fragile daemon coupling).

### Q6. Explainer Pack
- **Header:** Must contain strict role anchoring ("Explain the codebase strictly using the provided context slice. Do not critique code, suggest refactorings, or invent unlisted APIs. Ground every claim in explicit file paths and line numbers.") plus the target claim.
- **Slice:** Hybrid. Embed full file bodies *only* for explicitly targeted focus files (`--focus`). For surrounding context, embed only AST signatures and graph relations (`calls`, `relates_to`). Full bodies for secondary files quickly exhaust context budgets.

### Q7. Crate Layout & Dependencies
Nine crates is excessive for a greenfield rewrite with zero functional code today. Consolidate to four crates:
1. `c3-core` (DTOs, schemas, ledger, findings, roster).
2. `c3-engine` (process engines `codex`/`agy`/`muse` and `http` client).
3. `c3-index` (snapshots, AST parsing, SurrealDB graph).
4. `c3-cli` (CLI commands, MCP server, panel orchestration, telemetry).

Put `surrealdb` behind an optional `heavy` cargo feature. Steps 1–4 of the README roadmap must compile swiftly without database bloat.

### Q8. Expensive to Change Later
1. **HTTP thread identity:** CLI engines support session continuation. OpenRouter is stateless. C3 must persist conversation arrays in `.collab/<task>/threads/<lineage>.json` to support timeout continuation and format repair.
2. **SurrealKV process locking:** Embedded SurrealKV holds exclusive locks. Background detached runs (R12) will crash foreground CLI inspection unless concurrency is isolated.
3. **R17 telemetry default:** README specifies ON by default, contradicting the plugin roadmap's opt-in decision. Rectify this before public release.
4. **POSIX path normalization:** Enforce forward slashes (`/`) in all JSON finding locations and manifests across Windows and Unix to avoid Git churn.

---

## Findings

- G1-1 [severity: blocker] Embedded SurrealKV exclusive file locking breaks R12 non-blocking detached runs - trigger: Invoking `c3 -Status` or a concurrent consultation while a background panel (`-Detach`) holds the database open - evidence: read: `01-claude-merge-framing.md` (R12 non-blocking panel + embedded surrealkv) and inferred from SurrealKV storage architecture - verification: Launch a detached process holding open an embedded SurrealKV instance, then attempt to open the same SurrealKV path from a second CLI process - remedy: Treat SurrealDB strictly as an offline re-indexable cache with graceful connection fallback to raw files, or use SQLite in WAL mode for shared index state.
- G1-2 [severity: major] Unresolved contradiction between C3 README and plugin ROADMAP on telemetry default - trigger: Releasing C3 with telemetry ON by default when users expect opt-in per the maintainer's 2026-09-26 R17 roadmap decision - evidence: read: `README.md` ("Telemetry (on by default, off with one line)") vs read: `.collab/c3-design/context/codex-consult.md` ("R17 telemetry (0.5): decided opt-in on 2026-09-26 in the plugin ROADMAP") - verification: Diff `README.md` telemetry section against the maintainer's R17 notes in `.collab/c3-design/context/codex-consult.md` - remedy: Harmonize the spec before implementation: either adopt opt-in or formally record the override rationale in the plugin repo.
- G1-3 [severity: major] Stateless HTTP reviewers break timeout continuation and format repair protocols - trigger: An OpenRouter reviewer exceeds timeout limits or returns unparseable Markdown requiring a repair turn per the consult contract - evidence: read: `.collab/c3-design/context/codex-consult.md` ("Timeout -> one continuation turn on the same thread; format repair: one recorded turn") vs read: `01-claude-merge-framing.md` ("API path: http reviewers get no tools... one call") - verification: Mock a malformed JSON response from an HTTP provider and assert whether C3 can execute a repair turn against the same conversation context - remedy: Implement a thread state file under `.collab/<task>/threads/<lineage>.json` that buffers message arrays for multi-turn repair before final write.
- G1-4 [severity: minor] Premature workspace decomposition into nine crates slows port velocity - trigger: Managing cross-crate trait definitions and dependency trees across nine crates before step 1 (`src/main.rs`) has functional preflight logic - evidence: read: `01-claude-merge-framing.md` (Proposed shape lists 9 crates) vs read: `README.md` (Plan lists port steps 1-6) - verification: Measure cargo dependency graph resolution and compilation overhead after scaffolding 9 crates with circular trait needs - remedy: Consolidate into 4 logical crates (`c3-core`, `c3-engine`, `c3-index`, `c3-cli`), deferring further decomposition until boundaries harden.
- G1-5 [severity: minor] Windows path separators in findings schemas create platform-dependent git diffs - trigger: Recording finding location paths using Windows native backslashes (`\`) - evidence: read: `.collab/c3-design/context/codex-consult.md` (finding locations schema) and assumed Windows host environment - verification: Generate a finding on Windows and inspect `findings.json` for un-normalized backslashes - remedy: Normalize all file paths to repo-relative POSIX forward slashes (`/`) in `c3-core` before serializing JSON or Markdown.

---

## Verdict: ADVISE - Decouple the committable file ledger from the embedded SurrealDB cache, collapse the workspace from nine crates to four, and resolve the R17 telemetry default contradiction before starting step 1 of the port.

---

## Requested checks

- RC1: `cargo run --example test_surrealkv_concurrency` (opening the same `surrealkv` directory from two processes simultaneously) to verify whether SurrealKV supports concurrent read-only access or throws an exclusive lock error.
- RC2: Review OpenRouter model capabilities for endpoints in scope to confirm whether strict JSON schema enforcement (`response_format: { type: "json_object" }`) is universally supported or if prompt-based format repair remains mandatory.
- RC3: Query the project coordinator to confirm whether C3 must implement Git auto-committing mutations (`eck_finish_task`) or if all commits remain exclusively in Claude Code's hands.
