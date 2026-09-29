# M11 spec — index federation (opt-in per connection) and local embeddings

Design of record: `docs/DESIGN.md` §7 ("Federation, opt-in", "Embeddings"), §3 (invariants).
Both features are OFF unless the user's configuration turns them on. Both live behind the cargo
feature `index-surreal`. The index is never required for a consultation.

## 1. The configuration file

C3 has no configuration file of its own yet. It gets one, at the USER level (the place where the
reviewer roster already lives): `<codex home>/c3/config.json` (`CODEX_HOME`, else `~/.codex`),
overridable for tests with `C3_CONFIG=<path>`; `C3_CONFIG=none` means "no configuration".

    {
      "config_version": 1,
      "index": {
        "peers": [
          { "name": "xelixir",
            "conn": "ws://127.0.0.1:8000",
            "namespace": "xelixir", "database": "index",
            "projects": ["C:/Users/Dmytro/c3"],
            "use_in_packs": false }
        ],
        "embedder": {
          "url": "http://127.0.0.1:11434/v1/embeddings",
          "model": "nomic-embed-text",
          "dimension": 768,
          "projects": ["C:/Users/Dmytro/c3"]
        }
      }
    }

Validation, fail closed (a file that fails validation is not used at all, and the command says
why in one line): `config_version == 1`; the file at most 256 KiB; at most 16 peers; `name` a slug,
unique; `conn` parsed by the existing `index::parse_conn` and never `none`; `projects` REQUIRED and
non-empty for a peer and for the embedder — absolute paths of repository roots, compared after
canonicalisation and case folding on Windows; there is no wildcard: a connection is allowed for the
projects it names and for no other. Unknown keys are refused (a typo must not silently disable a
restriction). No refusal echoes a value other than a peer `name` that passed the slug check.

Nothing in this file is a secret and nothing in it may be one: no key, no token, no password
field exists. A `conn` with userinfo (`ws://user:pass@host`) is refused.

## 2. Federation v1 — reading a peer

A peer is ANOTHER index in the same schema (another project's store, or the user's hub). v1 is
read-only: C3 never writes into a peer and never syncs.

- `c3 index peers` — lists the peers of the configuration that THIS project may use (name, conn
  with the path or host, `use_in_packs`), and for each whether it opens right now
  (`available`, `busy` — an embedded store held by another process, fail fast per RC1 —
  `unreachable`, `schema mismatch`). Peers configured for other projects only are not listed, and
  their existence is not mentioned.
- `c3 index query <q>... --peer <name>` (repeatable) or `--peers all` — runs the same retrieval on
  the local index and on each named peer, then fuses all result lists by reciprocal rank. Every
  hit carries its source: `local` or `peer:<name>`. Without `--peer` the query is local, exactly
  as today. A peer that does not open is skipped with one line on stderr; the query never fails
  because of a peer.
- An embedded peer (`surrealkv:<path>`) is opened with `open_read`, for the shortest span, and is
  closed before the next one is opened. No retry loop. A `ws://` peer is reached with the
  namespace and database of its entry.
- A peer hit is an EXCERPT from the peer's index (the entity's stored text and its path relative
  to the PEER's repository). C3 never reads a file of another project from disk through
  federation, and never resolves a peer path against the local repository.

### Peers and packs

Content of another project entering a reviewer pack leaves the machine with that pack. So:

- A pack uses a peer only when the peer's entry says `"use_in_packs": true` AND the pack command
  or the consultation was given `--peer <name>` (or `--peers all`). Both are needed. The default
  pack is local.
- Peer excerpts go into their own pack section, `## Peripheral context from other indexes`, each
  one headed `peer:<name> / <path>`; they count against the periphery budget after the local
  periphery, and they pass the single sanitizer like everything else.
- The pack sidecar records `peers: [ { name, hits, excerpts, tokens } ]`. The dry run of a
  consultation prints one line per peer used: `peer        : <name> - <n> excerpts, about <t>
  tokens from another project`.
- The tool-state rule holds for peers too: nothing whose peer path starts with `.collab/`, `.eck/`
  or `.claude/` enters a pack.
- Nothing federated ever reaches the maintainer's server: telemetry carries the COUNT of peers
  used (an integer), never a name, a path or a connection string. Add it to the typed allowlist
  and to the privacy test.

## 3. Embeddings v1 — local only

- The embedder is an OpenAI-compatible `/v1/embeddings` endpoint on THIS machine. The URL must be
  `http://` or `https://` with a loopback host: `127.0.0.1`, `localhost`, `[::1]` — checked on the
  parsed URL, and the resolved address must be loopback too (a `localhost` that resolves
  elsewhere is refused). No userinfo, no query, no fragment. Anything else is refused:
  `a cloud embedder is not supported; the embedder must listen on this machine`. No key is sent:
  there is no key field.
- `c3 index embed` — computes vectors for the entities that have none (or whose text hash
  changed), in batches of 64, with a timeout of 30 s per batch and no redirects followed. The text
  sent is what the index already stores for the entity, after the single sanitizer. `--limit <n>`
  bounds a run. `c3 index build` does NOT embed (a build must stay fast and offline); `c3 index
  stats` shows `vectors: <n> of <m> entities, model <name>, dimension <d>`.
- An unavailable embedder yields NO vector — never a mock, never zeros. A response whose vector
  has the wrong dimension, is not finite, or whose count does not match the batch is discarded
  whole and the batch is reported as failed.
- The vector is stored on the entity with the model name and the dimension. A change of model or
  dimension in the configuration makes the old vectors stale: they are ignored by retrieval and
  replaced by the next `embed`. The HNSW index (`DEFINE INDEX ... HNSW DIMENSION <d> DIST COSINE`)
  is defined on first `embed` and redefined when the dimension changes.
- Retrieval: when the project has an embedder configured, the embedder answers within 2 s, and
  vectors exist, the query is embedded and a fifth leg (nearest neighbours, top 20) joins the
  four BM25 legs in the reciprocal-rank fusion. Rows without a current vector simply do not
  appear in that leg. If the embedder does not answer, retrieval is the four BM25 legs, exactly
  as today, and says nothing unless `C3_INDEX_PROFILE` is set.
- `rebuild` stays an acceptance test: `index rebuild` followed by `index embed` reproduces the same
  entity set and the same vector count.

## 4. What must stay true (tests)

1. With no configuration file, or `C3_CONFIG=none`, every command behaves byte for byte as before
   this milestone: same query output, same pack bytes (extend the existing byte-identity test).
2. A peer configured for another project is invisible: not listed, not queryable, and naming it
   with `--peer` says `no peer named <name> is allowed for this project`.
3. `use_in_packs: false` plus `--peer` on a pack: the peer is not used, and the command says so.
4. A peer hit under `.collab/` never enters a pack; a peer path is never joined to a local root
   (test with a peer path `../../etc/passwd`-like and with an absolute path).
5. The embedder refusals: a public host, a private LAN address, `localhost.evil.example`, userinfo,
   a redirect answer.
6. A wrong-dimension, NaN or short response stores nothing.
7. The seeded-secret test: a secret seeded into a peer's entity text is redacted in the pack; a
   secret seeded into local entity text is redacted before it is sent to the embedder.
8. Two-process behaviour (RC1) holds for an embedded peer: a peer held by another process is
   `busy`, the query goes on without it.

Tests use two temporary stores and an in-process fake embeddings server; no test touches the
network, the user's real configuration, or an index of a real project.
