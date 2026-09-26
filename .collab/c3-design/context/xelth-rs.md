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
