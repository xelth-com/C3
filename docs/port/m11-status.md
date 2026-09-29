# M11 status — index federation (opt-in, read-only) and local embeddings

Milestone 11 (spec `docs/port/m11-spec.md`, DESIGN §7). Both features are OFF unless the user's
configuration turns them on, both live behind `index-surreal`, and the index is never required
for a consultation. Nothing federated ever reaches the maintainer's server.

## The configuration file

C3 gains its only configuration file, at the user level beside the roster:
`<codex home>/c3/config.json` (`CODEX_HOME`, else `~/.codex`), overridable with `C3_CONFIG`;
`C3_CONFIG=none` means "no configuration". Parsing is a manual `serde_json::Value` walk (like
`c3_core::roster_ext`) so no serde type-mismatch error can echo an offending value. Validation
fails closed: `config_version == 1`, at most 256 KiB, at most 16 peers, peer `name` a unique
slug, `conn` parsed by `index::parse_conn` and never `none`/userinfo, `namespace`/`database`
present, `projects` required and non-empty (absolute paths, canonicalised + case-folded on
Windows, no wildcard), `use_in_packs` a bool. Unknown keys are refused (a typo — or a smuggled
`key`/`token`/`password` field — cannot silently disable a restriction). No refusal echoes a
value other than a peer `name` that passed the slug check. Module: `crates/c3/src/c3config.rs`
(feature-independent; builds with `--no-default-features`).

## Federation v1 — reading a peer

Read-only; C3 never writes into a peer and never syncs. `crates/c3/src/index/federation.rs`
holds `PeerSelection`, `ResolvedPeer`, the cross-source reciprocal-rank fusion `fuse_sourced`
(pure, unit-tested), and the feature-gated open/probe/retrieve. A peer is opened with
`open_read` for the shortest span and closed before the next; no retry loop. Every hit carries a
`source` (`local` / `peer:<name>`); a peer path is relative to the PEER's repository and is
never resolved against the local one.

- `c3 index peers` — lists the peers THIS project may use (name, location, `use_in_packs`, and
  availability: `available` / `busy` (RC1 lock) / `unreachable` / `schema mismatch`). Peers
  configured for other projects are not listed and their existence is never revealed.
- `c3 index query <q>… --peer <name>` (repeatable) / `--peers all` — runs the same retrieval on
  the local index and each named peer, fuses by reciprocal rank, tags each hit's source. A peer
  that does not open is skipped with one line on stderr; the query never fails for a peer.
  Without `--peer` the query is local, byte-for-byte as before.

### Peers and packs

`pack::reviewer::build_with_peers(opts, peers)` adds a `## Peripheral context from other indexes`
section AFTER the local periphery, each excerpt headed `peer:<name> / <path>`, counted against
the same periphery budget, every excerpt through the single sanitizer, tool-state peer paths
(`.collab/`, `.eck/`, `.claude/`) excluded. The sidecar records
`peers: [{name, hits, excerpts, tokens}]`. With no peers the pack is byte-identical to the local
one (`build` is unchanged and delegates to `build_inner(opts, &[])`; `PackOpts`/`ReviewerPack`
are unchanged, so the consult path's constructors are untouched). Telemetry gains one integer,
`details.peers_used` (a COUNT only), set by the consult path; the ledger alone leaves it 0.

## Embeddings v1 — local only

`crates/c3/src/index/embed.rs` (feature-independent URL rule + batch client). The embedder must
be an `http`/`https` loopback endpoint (`127.0.0.1`, `localhost`, `[::1]`), no userinfo/query/
fragment, and its resolved address loopback too; anything else is refused with a fixed message
that never echoes the URL. No key is ever sent (there is no key field). No redirect is followed.

- `c3 index embed` — embeds entities with no current vector (missing, or a changed model/
  dimension/content-hash) in batches of 64, 30 s per batch. A batch the embedder cannot answer,
  or whose count/dimension/finiteness is wrong, stores NOTHING and is reported failed. `--limit`
  bounds a run. `index build` does not embed. `index stats` prints
  `vectors: <n> of <m> entities, model <name>, dimension <d>`.
- Vectors are stored on the entity (`embedding`, `emb_model`, `emb_dim`, `emb_hash`); the model/
  dimension are recorded in `meta:embedding`. A model/dimension change makes old vectors stale —
  ignored by retrieval, replaced by the next `embed`.
- Retrieval: when an embedder is configured and answers within 2 s, a fifth leg (nearest
  neighbours, top 20 over CURRENT vectors) joins the four BM25 legs in the RRF. If it does not
  answer, retrieval is the four BM25 legs exactly as before. `rebuild` then `embed` reproduces
  the same entity set and vector count.

## What must stay true (spec §4) — tests

1. No config / `C3_CONFIG=none` behaves as before: `c3config::tests::missing_file_and_none_are_no_config`;
   pack byte-identity `index_federation::no_peers_pack_is_byte_identical_to_local` (plus the
   existing `reviewer::tests::index_off_pack_is_byte_identical_to_lexical`).
2. A peer for another project is invisible / naming it refused:
   `index_federation::peer_configured_for_another_project_is_invisible`.
3. `use_in_packs:false` + `--peer` on a pack is refused/unused:
   `index_federation::use_in_packs_false_is_not_used_in_a_pack`.
4. Peer `.collab/` excluded; a peer path never joined to a local root (traversal + absolute):
   `index_federation::peer_tool_state_excluded_and_paths_never_joined_to_local_root`.
5. Embedder refusals (public host, LAN address, `localhost.evil.example`, userinfo, bad scheme,
   query/fragment): `index::embed::tests::non_loopback_urls_refused`; redirect discarded in
   `index::embed::tests::parse_batch_*` / the client's `redirects(0)`; and the config surfaces it
   in `c3config::tests::cloud_embedder_refused_without_echo`.
6. Wrong-dimension / NaN / short response stores nothing:
   `index::embed::tests::parse_batch_validates_count_dim_and_finite` and
   `index_embeddings::wrong_dimension_response_stores_nothing`.
7. Seeded-secret redaction — peer excerpt in a pack, and local text before the embedder:
   `index_federation::peer_tool_state_excluded_and_paths_never_joined_to_local_root` (peer side,
   AWS key redacted) and `index_embeddings::a_seeded_secret_is_redacted_before_it_reaches_the_embedder`.
8. RC1 two-process: a held embedded peer is `busy`, the query goes on without it:
   `index_federation::a_held_peer_is_busy_and_the_query_goes_on_without_it`.

Fifth-leg + fallback and the rebuild+embed acceptance:
`index_embeddings::embedding_leg_contributes_and_missing_embedder_falls_back`,
`index_embeddings::rebuild_then_embed_reproduces_the_vector_count`. Telemetry field privacy:
`tests/telemetry_peers.rs`.

## The consult entry point

The single function the consult path calls to bring peers into a consultation pack:

    pack::reviewer::resolve_pack_peers(repo_root: &Path, selection: &index::PeerSelection)
        -> Result<Vec<index::ResolvedPeer>, String>

It loads the configuration, applies the project + `use_in_packs` gates, and returns the peers to
hand to `build_with_peers`. Contract: a missing configuration with peers requested is a one-line
refusal (echoing only a requested name); with none requested it returns an empty list (a local
pack); a `--peer` naming a peer this project may not use in packs is refused. The consult dry-run
line is `pack::reviewer::peer_dry_run_line(&PeerPackStat)`; the per-peer stats come back from
`build_with_peers`. The consult path also sets `telemetry Details.peers_used` to the count.

## Choices where the spec was silent

- **Peer retrieval is BM25 only** (no per-peer query embedding): "the same retrieval" is read as
  the shared BM25+RRF+1-hop path; a peer is not embedded on our side.
- **Cross-source fusion id**: `source · path · name · line · rank`, so two peers sharing a path
  stay distinct; ties break deterministically by that id.
- **`schema mismatch`** is a best-effort probe (the `entity` table answers a count); a store that
  is not a C3 index makes it fail.
- **HNSW define is best-effort**: if the store rejects the HNSW definition, `embed` still stores
  vectors and retrieval uses a brute-force cosine over current vectors, so the fifth leg still
  works (see §"impossible as written").
- **`c3 index embed` exit code**: returns non-zero when work remained but every batch failed (the
  embedder was unavailable — no vector, never a mock).
- **Config load error on a plain local query**: the reason is printed and the file is not used
  (local proceeds); a `--peer` that cannot be honoured is a hard error.
- **`meta:embedding`** holds the model + dimension for stats and change detection.

## Anything impossible as written

- **HNSW / the `<|K|>` KNN operator**: rather than assume the exact SurrealDB 3.0.5 HNSW
  behaviour, retrieval tries the HNSW KNN operator and, on ANY store error, falls back to a
  brute-force cosine over the current-vector rows in Rust. Correctness of the fifth leg therefore
  does not depend on HNSW being available in this build; the `DEFINE INDEX … HNSW …` is issued as
  specified but is not load-bearing for retrieval. (If HNSW is fully supported the KNN path is
  used; the integration test passes either way.)
- The embedded store's RC1 single-writer rule means a peer held by another process is `busy`;
  `probe`/`retrieve` classify the OS lock violation (os error 32/33) as `busy`, everything else
  as `unreachable`.
