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
