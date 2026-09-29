# M8c status — index performance and the index-fed reviewer pack

> M11 (see `m11-status.md`) builds on this: opt-in read-only federation (`c3 index peers`,
> `c3 index query --peer`, `c3 pack --peer`) and a local-only embedding leg (`c3 index embed`,
> a fifth NN leg in retrieval). RC1 was confirmed absolute at the process level: an embedded
> surrealkv store holds its OS lock for the whole process, so a store PATH opens at most once
> per process (a re-open after drop still returns os error 33). Distinct paths are independent,
> so opening the local index plus a peer store in one command is fine; only re-opening the same
> path in one process is not (which is why M11's tests build a peer store in a child process).


Milestone 8c: make a no-change `index build` touch no rows, cut `index query` latency, and let
the index feed the reviewer-pack periphery (with index-off packs unchanged). Feature
`index-surreal`; embedded surrealkv is single-process/fail-fast (RC1, DESIGN §7).

## Part 1 — no-change rebuild touches no rows

`index build` on an unchanged tree used to `DELETE`+re-`INSERT` all ~21.7k relation edges every
run (the 10 s the earlier "Measured" paragraph records). The relation rewrite is now incremental:

- The incremental path (`do_index`, store non-empty) compares each file's content hash to the
  stored `file_hash` row. It computes `changed` (hash differs or new) and `deleted` (stored path
  gone).
- **No file changed** → the derived entities and every edge are identical to what is stored, so
  the build returns after reading the hashes (and the generation), touching no `entity`, edge,
  `file_hash` or dir row. The one exception is the `meta:generation` row, updated only when the
  generation string itself differs.
- **A changed/deleted file** → only its rows move: the edges that start or end in any dirty
  entity are deleted (an edge can only change if a referenced name appeared, disappeared or moved,
  which makes one endpoint's file dirty), the dirty files' entity and hash rows are deleted, the
  changed files' current entities are re-inserted, directory entities are diffed against the
  current file set, and exactly the edges touching a dirty file are re-inserted.
- The from-scratch bulk load (`index_from_scratch`) is unchanged in spirit: base schema, drop BM25
  indexes, chunked bulk `INSERT`/`INSERT RELATION`, rebuild BM25 in one pass.

Acceptance: `crates/c3/tests/index_integration.rs::incremental_matches_rebuild` proves a no-change
build reports identical counts and a changed-file build lands the same entity/edge counts as a
full `rebuild` of the new file set (including a caller's now-dangling call target being dropped).

## Part 2 — query latency

Baseline: each query 3.2–3.6 s. The retrieval work was tiny; the cost was per-row round trips and
schema `DEFINE` on open. Changes:

- **No `DEFINE` on a read open.** `open_read` connects and selects ns/db only. `open` (build) runs
  the cheap base schema (tables + analyser, `IF NOT EXISTS`); the expensive BM25 full-text
  `DEFINE INDEX` runs once in the from-scratch load, never on an incremental build or a read.
- **Four BM25 legs in one round trip** (`bm25_legs`): one four-statement query, one bound search
  text; a secondary `id` sort makes ties deterministic.
- **Batched fetch/expansion.** Primary rows in one `WHERE id IN $ids`; neighbour edges in one query
  per relation table (`WHERE in IN $ids ORDER BY src, dst`); expansion rows in one `IN $ids` fetch.
  Round trips per query fell from ~76 (4 legs + 12 primary fetches + 36 neighbour queries + up to
  24 neighbour fetches) to 6.
- **Shared open for several queries.** `c3 index query <q1> <q2> …` and `--queries-file` retrieve
  each query on one open; each further query pays only its retrieval, not another open.
- RC1 preserved: still one fail-fast exclusive open, no retry, no second open, no long-held handle.

Profile (`C3_INDEX_PROFILE=1`, this repo, preliminary/under-load): retrieval is `bm25_legs` 0.032 s
+ `fetch_primary` 0.031 s + `expansion` 0.158 s ≈ **0.22 s total**; the remaining single-query cost
is the surrealkv store open. Multi-query proves the shared open: five queries ≈ one open + 5×0.22 s.

<!-- FINAL NUMBERS pending "load rule lifted"; table filled then. -->

## Part 3 — index-fed reviewer-pack periphery

`pack::reviewer::build` now selects the periphery from the index when one is configured, opens and
is non-empty: it retrieves on a code-first query (below), maps the hits to the periphery candidate
set in retrieval order, dedupes, and renders them in the existing skeleton/redaction/budget loop.
The index handle is dropped before the pack is assembled, well before any reviewer launches.

Periphery candidate set (one filter, both the index-fed and the lexical path draw from it): the
`discover` result for this run, minus focus files, minus tool-state paths. So the guarantees hold
for both paths:

- **Reviewer independence.** Tool-state documents never enter the periphery: anything whose first
  path component is `.collab`, `.eck` or `.claude` is excluded (`is_tool_state_path`), so another
  reviewer's replies and the coordinator's state reach a reviewer only through the pack's dedicated
  sections (prior findings), never as neighbourhood context. A tool-state path the focus set names
  explicitly is a focus file and is shown regardless. Tests: `lexical_periphery_excludes_tool_state`
  (1c), `tool_state_path_kept_when_focused` (1b), `tool_state_paths_are_recognised`, and
  `index_hits_outside_the_discovered_periphery_are_dropped` (1a).
- **Filtered by discovery + validated against the root.** A hit is used only if its path is in the
  set `pack::discover` yields for this run, so `.gitignore`, the hard-ignore list and the secret-file
  rules apply even to a stale index row, and every path is repo-relative and inside the root (both
  from `discover`). Test: `index_hits_outside_the_discovered_periphery_are_dropped` (an undiscovered
  `target/` row and a `.collab/` row are both dropped; a legitimate neighbour is kept).
- **Never required.** No conn, `none`, a bad conn, an absent store, a held/failed open, an empty
  index, a query with no terms, or a feature-off build all fall back to the lexical neighbourhood,
  producing the pack byte-for-byte. Test: `index_off_pack_is_byte_identical_to_lexical`.
- **`--conn` never creates a store.** The embedded store is opened only when its path already exists
  (checked before any open); a pack build on a repo without an index creates nothing. Test:
  `pack_does_not_create_index_store` (item 4).
- **Code-first query narrowing.** `brief_query_terms` builds the retrieval query from the focus
  files' paths and stems and the brief's code-like tokens (carrying `_ : . / -`, a digit, or mixed
  case); only if fewer than three such tokens exist does it fall back to the brief's plain words
  minus a small stop list. First-occurrence order, deduped, capped at 32 terms (deterministic). This
  matters because SurrealDB's `@@` match is conjunctive; the shared `retrieve` splits a query into
  terms and OR-fuses `(term × field)` legs by RRF (a single-term query is unchanged). Test:
  `brief_query_terms_are_code_first`.
- **Same sanitizer, deterministic.** Each body still passes `redact::redact`; BM25 legs order by
  `s DESC, id ASC`, RRF and expansion are stable, the shared-identifier header is computed as the
  lexical path computes it.
- **Provenance.** The `.pack.json` sidecar records `index: { used, source, hits, files_added }`.
- `c3 pack` defaults `--conn` to the embedded store `<collab>/.c3/index` (used when present);
  `--conn none` opts out.

## Files changed

- `crates/c3/src/index/surreal.rs` — incremental relation/entity diff; `open_read`; base-schema-only
  open; batched retrieval; multi-term OR tokenization; profiling.
- `crates/c3/src/cli/index.rs` — multi-query + `--queries-file`; read opens use `open_read`; profiling.
- `crates/c3/src/pack/reviewer.rs` — index-fed periphery; `is_tool_state_path` (both paths);
  `brief_query_terms` code-first tokenizer; `map_hits_to_periphery`; `conn` in `PackOpts`; `index`
  sidecar block.
- `crates/c3/src/cli/pack.rs` — `--conn` (defaults to the embedded store; opened only if it exists).
- `crates/c3/tests/index_integration.rs` — `incremental_matches_rebuild`.

## Verification

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --all-targets -- -D warnings` and `--no-default-features`: clean.
- `cargo test`: 234 lib + 4 index integration + rest, all pass; `cargo test --no-default-features`:
  all pass.

## Measurements (this repo, the installed release binary)

Final medians of three, 2026-09-29, the LTO release binary built from 6d88822, measured on the
clean release worktree into a scratch store (249 files, 2707 entities, 2658 `belongs_to`, 25420
`calls`, 4 `relates_to`). "Before" is the DESIGN section 7 paragraph of 2026-09-28 (230 files).

| Metric | Before | After |
|---|---|---|
| cold build | 14.0 s | 19.4 s (one shot; 8 % more files, 20 % more entities) |
| no-change build | 10.0 s | **2.62 s** (2.40 / 2.62 / 2.63) |
| rebuild | 16.8 s | 19.6 s (17.8 / 19.6 / 21.3) |
| stats | 2.7 s | 1.15 s |
| one query (open + retrieve) | 3.2-3.6 s | **0.95-1.17 s** over five queries |
| five queries in one open | about 17 s | **1.45 s** (about 0.1 s per further query) |
| store on disk | 128.5 MiB | 180.7 MiB |

Single-query breakdown (`C3_INDEX_PROFILE=1`): `open_read` 0.89 s, the rest is process start and
retrieval. The store open is the floor: everything avoidable (schema definition on a read, per-row
round trips, a second open) is gone. All three targets are met: no-change build under 3 s, one
query under 1.5 s, a further query well under 0.5 s.

The figures taken under load during development (no-change 2.66 s, one query 1.44-2.36 s with an
`open_read` of 1.77 s) are kept here only as a warning: on this machine a parallel build doubles
the store open.
