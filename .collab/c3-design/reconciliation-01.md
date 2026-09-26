# c3-design - reconciliation 01 (eight answers to handoff 01)

Produced 2026-09-26 by a fresh-context worker (Opus) from handoffs 02, 03, 09, 10, 11, 15 and chat replies 01-02, then
reviewed by the coordinator. Decisions drawn from it are in `state.md`; this file is the evidence trail.

Panel: GLM glm-5.3 (F02), MiMo mimo-v2.6-pro (F03, JSON invalid, findings hand-extracted), Kimi k3 (F09), Qwen qwen3.8-max
(F10), Muse muse-spark-1.3 (F11), Gemini 3.8 Flash (G1, web chat), Grok 4.5 (G2, web chat), Astra gpt-6-astra (F15, ran after
the panel with the others' open findings). All eight verdicts: ADVISE. Nobody attacked the fixed vision; every challenge
targets the proposed shape or its hidden assumptions.

## 1. Framing challenges and unlisted options

| Reviewer | Challenge | Unlisted option |
|---|---|---|
| GLM | Two vision items silently amend the spec (OpenRouter vs "no keys"; R17); "port order stays" contradicts a shape full of subsystems absent from README | Ship R14 deterministic sizing first, defer the probabilistic panel until a reward exists |
| MiMo | Brief merges three separable products (bridge / snapshot / GraphRAG) | SQLite FTS5 + AST edges for the light path; append-only event log + content-addressed packs; credential via OS broker |
| Kimi | "files vs DB-as-truth" hides a third axis | Defer SurrealDB entirely: steps 1-4 files-only, routing on the JSON scoreboard |
| Qwen | `Store{files,surreal}` interchangeability asserts the DB can be the record | `RecordStore` + `Index{surreal,none}`; a Recipe tier; an escalation panel |
| Muse | Merges three conflicting trust models (deterministic vs probabilistic; files vs scored memory; sandbox- vs C3-enforced read-only) | Sidecar not merge; event-sourced files; no judge by default; snapshot-as-protocol without `.eck/` sprawl |
| Gemini | Swappable `Store` trait is flawed; nine crates before step 1 works invites deadlock | Two-tier (lean CLI + optional GraphRAG); embedded SQLite + sqlite-vec to dodge the SurrealKV lock |
| Grok | "One binary, many crates" sound only if trait boundaries freeze before any impl | Hybrid: DB primary for scoreboard/priors only, files authoritative for artifacts |
| Astra | Conflates a compatibility port with several new products; evaluate additions separately | `EvidenceStore` vs `ContextIndex`; short-lived serialized DB sessions; external credential-bearing adapter; lexical retrieval before embeddings |

Recurring meta-challenge (GLM, Kimi, Muse, Astra, Gemini, Grok): do not freeze `Store`/`Engine` traits until R17, the key
contract, the lock model, the rebuild contract and the reward function are decided.

## 2. Per-question consensus and splits

- **Q1** consensus 8/8: files are the authoritative record, the DB a rebuildable index; DB-as-truth rejected. Splits: storage
  engine (Gemini, MiMo push SQLite); one trait vs two contracts (Qwen, Astra: two); defer the DB outright (Kimi).
- **Q2** split: Qwen, Gemini, Grok, Muse say Thompson is premature at tens (softmax / epsilon-greedy first); GLM and Kimi
  defend Thompson as right for the small-sample regime. Astra reframes: the undefined reward is the defect, sparse data is
  uncertainty, not invalidity. Consensus 8/8 on explicit `-Topic` tags over similarity priors and on logging seed, weights,
  candidate set and draw.
- **Q3** consensus 8/8: named levels with R14 per-purpose defaults plus numeric overrides; the resolved plan, not the level
  name, lands in the ledger. Nuances: budget as a separate constraint (all) vs a USD cap (Gemini); escalation (Qwen);
  unsatisfied-panel outcome (Astra).
- **Q4** consensus 8/8: pack-only first, tool loop deferred, read-only by construction. Qwen's bounded `needs:[]` middle
  option; Astra and MiMo insist the "never handles a key" wording must change.
- **Q5** consensus 7/8: port patterns, never depend on the `:4446` server; judge = optional coordinator-owned `reconcile`,
  never voting; embeddings/candle behind a feature. Split on T-hub: six copy verbatim, MiMo and Astra re-implement behind a
  typed allowlist with consent policy.
- **Q6** consensus 8/8: role + verbatim claim + "answer only from the slice or say unknown" + no generic review + provenance +
  trailing anti-drift prompt; focus files in full, periphery as relationship sections. Astra adds: claim marked unverified,
  parser-coverage limits, edges labelled derived.
- **Q7** split on count: GLM, MiMo, Qwen, Gemini, Astra want 3-4 crates; Muse ~7; Grok accepts nine if traits freeze first.
  Consensus 8/8 on `surrealdb` behind a cargo feature; Astra: keep it in the release since C3 is the heavy product.
- **Q8** consensus: http lineage/continuation undefined; R17; R13; Windows paths; central redaction. Singletons: roster
  `lab` field breaks the plugin validator (Qwen, confirmed by Astra); `finish_task` is a writer (Qwen, Astra); env-prefix
  compatibility and MAX_PATH (Qwen); C3 state must stay off `.eck/` anchors (GLM, Astra).

## 3. Findings, de-duplicated

| # | Theme | Merged claim | Max severity | Raised by | Converged remedy |
|---|---|---|---|---|---|
| T1 | R17 telemetry default | plugin decided opt-in 2026-09-26, C3 README says on by default | blocker (F11-1) | F02-2, F03-2, F09-1, F10-9, F11-1, G1-2, G2-1, F15 | decide once, align both docs before telemetry ships |
| T2 | DB concurrency / process ownership | embedded SurrealKV is single-process; detach + MCP + CLI can open one repo's DB | blocker (F10-1, G1-1) | F02-3, F03-6, F09-4, F10-1, F11-6, G1-1, F15 | only the lock holder writes; others read-only or `none`; short-lived opens |
| T3 | Redaction / secret egress | redacting error strings only is insufficient; secrets leave via packs, snapshots, telemetry | blocker (F03-5, F10-2) | F03-5, F09-5, F10-2, F11-5, F15-1 | one outbound sanitizer in the shared builder; hard-ignore `.env*`; "leaves the machine" gate |
| T4 | HTTP lineage / continuation | no substrate for "one continuation on the same thread" over stateless HTTP | major | F03-4, F09-6, F10-8, G1-3, G2-3, F15 | `provider::model::endpoint` lineage, C3-owned transcript, pack hash + reply retained |
| T5 | Routing method / reward undefined | the reward (marks, decay, missing labels) is undefined so the ledger shape cannot freeze | major | F02-6, F09-3, F10-5, F11-3, G2-4, F15 | define the reward first; `Router` seam; Thompson later; Astra contradicts "statistically empty" |
| T6 | Rebuildability / staleness | no rebuild command, hash check or write order; cross-project priors not file-reconstructible | major | F02-4, F03-1, F09-2, F11-2, G2-2, F15 | files first, rows carry source hash, `c3 index rebuild` tested in CI; Astra contradicts F11-2 as categorical |
| T7 | `Store` trait mismatch | interchangeable impls assert the DB can be the record | major | F10-3, G1, F15 | split `EvidenceStore` / `ContextIndex`; Astra: role separation, not logical necessity |
| T8 | "never handles a key" vs HTTP | the native engine must read a credential; new exfil surfaces | major | F02-1, F03-3, F11-4, G2, F15 | R-numbered divergence; re-word to never persist/print/commit/transmit elsewhere |

Singletons: roster schema compatibility (F10-4, F15: verified against the validator); embedding stack bloat and mock-vector
fallback (F10-7 superseded by F15-3); pack evidence provenance (F10-6 superseded by F15-2: schema v1 suffices, retain the pack
and source map); `finish_task` writer inside a read-only MCP (F10-10, F15); nine crates premature (F10-11, G1-4); `.eck/`
collision (F02-5, conditional per Astra); Windows path normalization (G1-5); embedding egress policy (F15-1).

## 4. Contradictions

1. Thompson vs softmax: Astra breaks the tie - neither is invalid, the reward is missing; decide by simulation (RC2/RC3).
2. T-hub verbatim vs re-implement: six vs MiMo + Astra.
3. Storage engine: six assume SurrealKV, Gemini and MiMo argue SQLite.
4. Pack-evidence schema bump: Qwen F10-6 vs Astra F15-2 (Astra read the schema; v1 suffices).
5. Mock-vector "silent": Qwen F10-7 vs Astra F15-3 (rate-limited warnings exist; contamination risk stands).
6. Crate count: Grok alone accepts nine; five want 3-4; Astra keeps SurrealDB in the release.

## 5. Requested checks (attributed)

- GLM RC1: grep `Surreal::init|kv-surrealkv` in xelth.rs db.rs/main.rs - per-command or long-lived DB open.
- GLM RC2: grep `consult\.lock|write\.lock|pending-` in `codex-consult-common.ps1` - lock semantics to honour.
- MiMo RC1: two concurrent `c3 consult --task t`, delete the index, rebuild - defined lock and deterministic rebuild.
- MiMo RC2: `cargo test --test outbound_redaction` - no seeded secret in requests, packs, events, errors.
- MiMo RC3: compare README with plugin R17/R13; record one telemetry default and key threat model.
- Qwen RC1: `xelth mcp` holding `.eck/db` plus a second reader - DB ownership rule.
- Qwen RC2: roster with `"lab"` through `codex-providers.ps1` - accept vs reject.
- Qwen RC3: group ratings by (reviewer, purpose) across `.collab/*/findings.json`; median n below 20 means softmax first.
- Qwen RC4: grep `SecretScanner` in eckSnapshot src - redaction only in createSnapshot.
- Qwen RC5: grep thread resume/continuation in the plugin - state an http engine must persist.
- Gemini RC1: two processes on one surrealkv dir. Gemini RC2: OpenRouter `response_format` support per model. Gemini RC3: ask
  the coordinator whether C3 auto-commits.
- Astra RC1: disposable SurrealKV harness, second process opens the same path, then after termination. Astra RC2: seeded
  routing simulation (uniform / regularized Thompson / smoothed softmax) reporting regret and sensitivity. Astra RC3: retrieval
  spike, cold/warm BM25 + graph latency and pack coverage, network off.
- Grok: none.

## 6. Worker's own read (judgment)

- T1, T2, T3 are the real blockers and are read-code grounded; T2 and T3 outrank T1 in engineering risk because they change
  trait shapes, while R17 is a one-line documentation decision.
- On Q2 Astra is best supported: the digests say no usefulness formula exists yet, so the method debate is premature.
- Qwen F10-6 looks wrong and Astra's supersession is right; F10-7's "silent" is overstated, the contamination risk stands.
- Gemini/MiMo's SQLite alternative deserves a hearing: if the graph is deferred, SQLite removes T2 entirely.
- F11-2 is too categorical; F03-1 (cross-project retention) is the sharper version.
- F10-4 is under-corroborated but concretely verified against the live validator; rank it above its count.
- F02-5 (`.eck/` collision) is the weakest risk: a naming convention, not a blocker.

## 7. Usefulness

GLM yes; MiMo partly (strong substance, invalid JSON); Kimi yes; Qwen yes (deepest, two overreaches corrected by Astra);
Muse partly; Gemini yes (SQLite option, thread-state remedy); Grok partly (thinnest, echoes consensus); Astra yes, highest
signal-to-noise (adjudicated the others from source, added the embedding-egress finding).
