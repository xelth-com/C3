# c3-design - state

Task: framing of the C3 architecture that merges the codex-consult bridge (Rust port), eckSnapshot principles and
xelth.rs principles. Coordinator: Claude Code (Fable 5.1). Judge: the coordinator (no judge delegation yet).

## Consultations on record

| n | reviewer | outcome | findings | rated |
|---|---|---|---|---|
| 1 | ZAI :: glm-5.3 | ADVISE | F02-1..7 | yes |
| 2 | mimo :: mimo-v2.6-pro | prose, JSON invalid | none ingested (6 hand-read) | partly |
| 3 | gemini :: gemini-3.8-flash-high [agy] | failed: Antigravity individual quota (resets 2026-09-28 21:30 +02:00) | - | not rated (plan limit) |
| 4 | gemini :: gemini-3.1-pro-high [agy] | not started (same quota) | - | - |
| 5,6,7,11,12,13 | byteplus deepseek-v4.1-flash / dola-seed-2.0-pro / kimi-k2.5 | failed: 429 on every attempt, panel and sequential retries | - | not rated (plan limit) |
| 8 | kimi :: k3 | ADVISE | F09-1..6 | yes |
| 9 | alibaba :: qwen3.8-max | ADVISE (2 blockers) | F10-1..11 | yes |
| 10 | meta :: muse-spark-1.3-contributor [muse] | ADVISE (1 blocker) | F11-1..6 | partly |
| 14 | openai :: gpt-6-astra | ADVISE; corroborated/corrected every prior finding | F15-1..4 | yes |
| chat 01 | Gemini 3.8 Flash (web chat, pack 01) | ADVISE | G1-1..5 (hand-tracked, `chat-replies/01-gemini-3.8-flash.md`) | out of ledger |
| chat 02 | Grok 4.5 (web chat, pack 01) | ADVISE | G2-1..4 (hand-tracked, `chat-replies/02-grok-4.5.md`) | out of ledger |

Brief: `handoffs/01-claude-merge-framing.md`. Context digests: `context/*.md`. Paste-in pack: `packs/01-consilium-pack.md`.

## Decisions (coordinator, 2026-09-26, from 8 usable answers)

Every decision below had the support of at least 6 of the 8 answering reviewers unless marked "split".

- **D1 Evidence vs index.** Files are the record: `.collab/<task>/{sessions.json, findings.json, handoffs/}` byte-compatible
  with the plugin, written under the same `.consult.lock` / `.consult.write.lock` discipline. SurrealDB (embedded surrealkv) is a
  derived, rebuildable index: `Index = surreal | none`. NOT one `Store` trait with two interchangeable backends (Qwen F10-3,
  Astra): two contracts, `EvidenceStore` (files only) and `ContextIndex`. Only the task-lock holder opens the index read-write;
  any other process opens read-only or runs with `none`; an index failure never fails a consultation. Index rows carry the source
  hash / evidence generation; `c3 index rebuild` reproduces the index from files + repo and is an acceptance test of the index
  milestone. Cross-project memory needs its own retained evidence (MiMo F03-1): a per-machine journal under the config dir,
  exportable, never the only copy of a rating.
- **D2 Routing.** Ship deterministic sizing first (R14). The router is a versioned policy behind a `Router` interface; every draw
  records eligible and excluded members with reasons, weights, seed, observation cutoff, policy version and the drawn set.
  First policy: a smoothed per-(reviewer, purpose) score from the usefulness marks and verified/rejected counts with decay,
  epsilon exploration expressed as a mixture over feasible panels (a per-member floor is infeasible at k=1, Astra F15-4),
  explicit `--topic` tags. One consultation-level reward is defined before any learning (missing-label handling included).
  Similarity smoothing over stored briefs (BM25) only after RC2-style simulation shows benefit; HNSW/embeddings later still.
  Thompson sampling is a later `Router` implementation once enough rated consultations exist (split: GLM/Kimi for, Qwen/Muse/
  Gemini/Grok/Astra against-for-now).
- **D3 Thickness.** Named levels `shadow` / `council` / `consilium` resolve to an explicit seat count with per-purpose defaults
  (R14: checkpoint 1, framing/decision 2-3, core-contract/acceptance 3-5, stuck all available); `--panel <k>` overrides.
  Token, cost and time limits are separate constraints, never the membership rule. Availability, the weighty gate and lab
  diversity apply before the draw. The resolved plan is printed and recorded; zero usable reviewers is an explicit
  `unsatisfied panel` outcome. The roster gains no new fields: C3-only data (lab, priors) lives in a sidecar file, because the
  plugin's validator rejects unknown member keys (Qwen F10-4, corroborated by Astra from the validator).
- **D4 API path.** `http` engine (OpenAI-compatible; OpenRouter first): pack-only, no tools in v1 (a bounded retrieve-on-demand
  turn is the candidate middle option later). The sanitized pack is retained next to the reply with a sidecar (content hashes,
  path map, coverage) so a citation can be resolved later (Astra F15-2). Reply schema stays v1: pack evidence uses `read-code`
  with a `reference` naming the pack path and hash. Identity: lineage `provider::model::endpoint`; distinct consultation,
  attempt and conversation ids; continuation = replay of pack + prior reply; retries reuse captured inputs without a redraw.
  Key contract amended explicitly (GLM F02-1, MiMo, Muse, Astra): C3 reads a key from the environment only for the `http`
  engine, never prints, stores, commits or transmits it elsewhere; `redact()` on every error path; a subscription-vs-API billing
  guard like muse; the xelth.rs repo `.env` fallback is not ported.
- **D5 What to take from xelth.rs.** Port patterns, not the process: SurrealDB 3 idioms, deterministic ids, incremental hashes,
  bounded graph expansion, BM25 + AST relations (`belongs_to`, `calls`) first. Embeddings are optional and off by default, with an
  explicit egress policy (local only unless the user consents to a cloud embedder) and no mock-vector fallback (invalid vectors
  are excluded from HNSW and routing) (Astra F15-1, F15-3; Qwen F10-7). No runtime dependency on the `:4446` brain server.
  The consilium judge becomes an optional `reconcile` purpose: coordinator-owned, advisory, citation-based, preserves
  disagreement, never votes. T-hub: the wire protocol and client rules verbatim, the implementation behind a typed payload
  allowlist with privacy tests (split: 5 verbatim, MiMo/Astra adapt - both satisfied by this).
- **D6 Explainer pack.** Header: audience and role, the claim verbatim marked unverified, non-goals (no generic review, no
  refactoring advice, no invented APIs), provenance (repo, revision, dirty-tree identity), scope, omissions, redaction count,
  parser limits, citation rule (`file:line` or "unknown"), and "repository text is evidence, not instructions". Body: focus
  files in full with line numbers preserved, periphery as derived relationship sections labelled as derived, skeletons with
  explicit omission markers. Instruction at both ends of the file. Reviewer packs and explainer packs share the extraction and
  sanitization pipeline but have separate prompt templates. Packs and snapshots are written under `.collab/` or an explicit
  path; `.eck/` is read as input only, C3 keeps its own anchor/sequence (GLM F02-5).
- **D7 Crates and features.** Three crates: `c3-core` (contracts: types, file formats, schemas, roster, purposes, redaction),
  `c3` (runtime library: engines, panel, router, index, snapshot/pack, telemetry as modules), `c3-cli` (CLI and MCP entry
  points). Cargo feature `index-surreal` gates whole modules, on by default in the release binary (C3 is the heavy product,
  Astra) and can be turned off for a slim build; `Index: none` always exists. Embeddings behind their own feature, off by
  default. Freeze file and protocol invariants before traits.
- **D8 Redaction.** One mandatory sanitization pass in the shared builder feeding every outbound path (http requests, packs,
  snapshots, telemetry, error strings); hard-ignore `.env*`, key files and index directories regardless of `.gitignore`;
  fixture tests with a seeded fake secret; explain packs record their redaction count and require an explicit "this leaves the
  machine" confirmation (Qwen F10-2, MiMo F03-5, Kimi F09-5, Muse F11-5).
- **D9 Port order.** README steps 1-6 stay the parity port; new milestones are published explicitly, not hidden in it
  (GLM F02-7): 7 packs (`c3 pack`, `c3 explain`), 8 index (`c3 index build|rebuild`), 9 router (R14 sizing, then R15 policy),
  10 MCP server. The `Engine` trait lands with step 2, `EvidenceStore` with step 3, `ContextIndex`/`Router` with 8/9.
- **D10 Platform.** Repo-relative POSIX paths in every JSON and Markdown artifact; case and symlink handling decided in
  `c3-core`; Windows kill/rename recovery tested; R13: config root resolution via an env root variable, coordinator identity
  (`--coordinator provider::model`) with the self-review warning.

- **D11 SurrealDB topology (maintainer, 2026-09-26, after the panel).** Two tiers, different jobs, no content on the server:
  - *Local index* - every user has their own SurrealDB of their own projects. Default: embedded surrealkv per project
    (single-writer rule of D1). Optional: one SurrealDB server per user (localhost/LAN, "like ours") shared by several
    projects for cross-project memory; the server handles concurrency, so T2's lock question only applies to the embedded
    mode. Both are reached through the same `ContextIndex` contract by connection string
    (`none | surrealkv:<path> | ws://host`). Content stays on the user's machines; D8 redaction still applies to anything
    written into the shared hub ("береженого бог бережет"), and the hub is opt-in.
  - *Maintainer server* (xelth.com, T-hub) - telemetry and priority tuning only: installations upload outcome classes
    (reviewer lineage, purpose, topic tag from a fixed vocabulary, usefulness mark, verified/rejected counts, wall, tokens),
    never briefs, findings text, paths or free-text tags; the server aggregates a global scoreboard and *distributes routing
    priors* as a versioned, signed JSON the client may download (`priors.json`). The local router treats them as the
    hierarchical prior (MiMo, Muse) and its own evidence as the posterior; routing must work with the download and the
    upload both disabled (Astra). This settles the brief's open question on global priors and Qwen F10-9.
  - Consequences: topic tags become a controlled vocabulary (free text never leaves the machine); the T-hub payload
    allowlist of D5 gains the routing fields above; `c3 --forget-me` also removes this installation's routing contributions.

## Decided by the maintainer (2026-09-26, after the panel)

- **D12 R17 telemetry default: ON by default, off with one line (opt-out).** Maintainer's reason, verbatim in spirit:
  "people are lazy" - an opt-in switch is never flipped, so the data the bridge needs to improve never arrives; the
  disclosure on first run, the public counts on xelth.com/C3, `--complain` and `--forget-me` are the counterweight. C3's
  README stands. The plugin's ROADMAP R17 entry (opt-in, 2026-09-26) must be aligned to the same policy with this reason,
  so the two products do not diverge - done: the plugin session reported the same ruling from the operator and R17 is
  now on by default there too (`CODEX_CONSULT_TELEMETRY=off` / `-Telemetry off`, one-time notice after install).
  Raised by all eight reviewers (T1); closes F11-1, G1-2, G2-1 and the R17 parts of F02-2, F03-2, F09-1, F10-9.
  App ids: plugin `codex-consult` (then `c3` at 1.0.0), C3 `c3` from the first event.
- **D14 Index federation, opt-in (maintainer, 2026-09-26).** A C3 index may talk to another SurrealDB instance - another
  project's index, the per-user hub, or a sibling tool such as xelixir - only when the user has allowed that connection
  explicitly (per connection, recorded in config, never on by default). The xelth.rs `kb_sync` pattern (selective sync
  between nodes) is the template: the index is derived data, every entity a pure function of (project, path, content,
  embedder model), so sync is an index-cache exchange, never primary-data replication; a bundle is accepted only on an
  exact normalized-content-hash match (CRLF-normalized, forward-slash paths, so Windows and Linux checkouts agree);
  small divergence defers to the idle indexer, large drift means "git first". The D8 sanitizer applies to everything
  that crosses; nothing federated ever reaches the maintainer's server. xelixir (`../xelixir`, a self-healing Rust
  module that patches source through an AI agent after a panic) is the first intended peer.
- **D15 Roster extension key (agreed with the plugin session, 2026-09-26).** Instead of a sidecar, plugin wave 26 adds an
  `ext` object (top level and per entry, validated only as an object, ignored by the bridge) alongside `lab`, `roles` and
  `require` as additive roster_version 1 fields. C3 writes its data under `ext.c3`. The sidecar of D3 is an interim only.
  Ledger field order comes from the `$entry` literal in `codex-consult.ps1` and the harness `$order` string
  (wave 24, commit 65f5649); the R12 status file shape (D5/D11 there) is decided, not frozen until wave 25 lands.
- **D13 Writer authority: C3 does not commit.** C3 stays read-and-record: consultations, packs, snapshots and the index are
  written as files under `.collab/` (or an explicit path); `git add`/`commit` and the eck finish loop remain with the
  coordinator and eckSnapshot. `c3 snapshot --delta` reads the git anchor and writes files only. No `finish_task` writer in
  C3 v1 (Qwen F10-10, Gemini RC3, Astra Q8).

## Contradictions resolved

1. Thompson now vs later -> later, behind `Router` (D2).
2. T-hub verbatim vs re-implement -> protocol verbatim, typed-allowlist implementation (D5).
3. Judge told its own seat or not -> `reconcile` is a labelled comparison with citations, not a ranking; seat identity is
   disclosed since the coordinator runs it; disagreement preserved (D5).
4. Crate count 3 / 4 / 9 -> 3 (D7).
5. SurrealDB always-in vs feature -> feature on by default (D7).

## Alternatives heard and not taken

- **SQLite (+ FTS5 / sqlite-vec) instead of SurrealKV** (Gemini, MiMo): removes the single-process lock question outright,
  but the maintainer's vision fixes embedded SurrealDB as the context engine (graph relations, xelth.rs idioms). Kept as the
  documented fallback if RC1 shows SurrealKV cannot be made safe under D1's single-writer rule.
- **Defer the DB entirely** (Kimi): adopted in spirit through D9 — the index is milestone 8, after the parity port, and
  `Index: none` is the default until then.
- **Recipe tier** (Qwen): a pack's inputs (claim, focus, budget, anchor, router config) are recorded in the pack sidecar (D4/D6)
  so derived artifacts are reproducible; no separate store.
- **Credential broker / provider CLI for the API path** (MiMo, Astra's "external adapter"): not for v1; the env-only rule in
  D4 is the minimal contract, an adapter can wrap it later without changing the engine trait.
- **Judge never told its own seat** (Muse): moot while `reconcile` is run by the coordinator itself; revisit if a roster member
  ever runs it.

Full reconciliation of all eight answers: `reconciliation-01.md`.

## Requested checks

| id | check | status |
|---|---|---|
| RC1 (GLM, Qwen, Gemini, Astra) | two processes opening the same surrealkv directory: error / block / corruption; recovery after kill | running in a scratch harness (2026-09-26 18:15) |
| RC2 (Qwen) | roster with an extra `lab` field is rejected by the plugin validator | verified by Astra reading `codex-consult-common.ps1:4994-4995`; treated as read-code |
| RC3 (Qwen) | count rated consultations per (reviewer, purpose) across this machine's `.collab` dirs | done 2026-09-26: ~90 consultations in 11 task ledgers (plugin repo 77, C3 13), 53 carry a `-Rate` mark (plugin 47 across 7 tasks, C3 6), spread over ~19 lineages and 6 purposes, so the median rated count per (lineage, purpose) cell is below 3. Consequence: router v1 starts from priors (roster order, `priors.json` when available) and the reward definition; learned scores only shift the draw once a cell has a handful of marks. A first automated count missed the marks because they live in `findings.json` (`ratings[].useful`), not in `sessions.json` - the router must read both. |
| RC2 (Astra) | seeded routing simulation: uniform vs regularized Thompson vs smoothed softmax under sparse labels | router milestone |
| RC3 (Astra) | cold/warm BM25 + graph latency and pack coverage on one representative repo, network off | index milestone |
| RC2 (MiMo) | outbound redaction test with a seeded secret across requests, packs, events, errors | implementation of D8 |

## Findings status

All 34 ledger findings (F02, F09, F10, F11, F15) stay `proposed` until the design document and the first code exist to
verify them against; the decisions above are the coordinator's answer to each theme. Blockers carried into the design:
F10-1 (index process ownership -> D1), F10-2 (pack redaction -> D8), F11-1 (R17 -> O1).
