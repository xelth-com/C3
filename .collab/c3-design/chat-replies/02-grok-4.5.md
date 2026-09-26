# Chat reply 02 - Grok 4.5 (xAI, web chat, out of ledger)

Provenance: the user pasted `.collab/c3-design/packs/01-consilium-pack.md` into the Grok web chat (model: Grok 4.5) on
2026-09-26 and pasted the reply back to the coordinator. Not run through the bridge, so it has no ledger entry and no
finding ids; the coordinator tracks its findings by hand as G2-1..G2-4 below. Reply verbatim follows.

---

Grok 4.5 (xAI)

## Answers

**Q1.** Files-as-record + SurrealDB-as-rebuildable-index is the right split. It preserves the plugin's `.collab` contract, committable evidence, and the "bridge never handles a key / files are the source of truth" invariant (codex-consult.md). Making the DB source of truth breaks recovery (corrupt DB loses history), two-tool coexistence on one repo, and the PowerShell plugin's ability to read/write the same task. Locks remain file-level (`.consult.lock`); DB is advisory cache only. Rebuild path from files + repo must be deterministic and tested early. Alternative not listed: hybrid with DB primary for scoreboard/priors only, files still authoritative for every consultation artifact.

**Q2.** With tens of consultations Thompson sampling over Beta posteriors is over-parameterized and high-variance; a simpler softmax over scoreboard HIT% (or Y/P/N usefulness) + epsilon-greedy exploration is preferable until sample sizes grow. Topic similarity: start with explicit `-Topic` tags as override + BM25 over stored brief text (no hand taxonomy); full HNSW only after embeddings are proven stable. Auditability: ledger must record the full posterior/weight vector, random seed, and selected set (R15 already requires this). Assumed: scoreboard HIT% definition remains stable.

**Q3.** Prefer named levels (`shadow`/`council`/`consilium`) with per-purpose defaults in R14 spirit, plus `--panel k` and `--budget <tokens>` as overrides. Named levels keep the coordinator's mental model cheap; pure token budget is opaque when models differ in cost/latency. Thickness should still filter by availability and weighty-gate before draw.

**Q4.** Context-pack-only first for `http` reviewers. Tool loop later as opt-in. Read-only still holds because C3 itself is the only agent that touches the filesystem; the remote model never receives tools. Context-pack keeps the key out of any model context and matches the deterministic, sandbox-free path of CLI engines. Tool loop would re-introduce the "who enforces read-only" question and raise token cost.

**Q5.** Port schema + retriever patterns (entity, BM25x4, HNSW, RRF, multi-hop) into `c3-context`; do not depend on xelth.rs runtime (it is mid-refactor). Consilium judge is useful as an optional `reconcile` stage run by the coordinator's own model, not as a hard dependency. T-hub client: copy the ~35-line reference client verbatim (redact, spool, 3 s, instance_id). Assumed: SurrealDB 3.0.5 idioms remain stable.

**Q6.** Header must contain: explicit role ("you are answering a grounded question about this exact codebase"), the claim, a hard instruction to use only the supplied slice and to refuse generic advice, and a closing "cite file:line or say unknown". Prefer graph relationship sections + focus files over full bodies when budget is tight; full bodies only for the explainer's focus set. Same generator as the `http` pack reduces drift.

**Q7.** Workspace as proposed is fine if `c3-store` and `c3-engines` traits are fixed in step 1; fewer crates risks the light/heavy boundary becoming porous. `surrealdb` (and candle/embeddings) must be a cargo feature `heavy` so the light path stays small and builds without the heavy deps. Always-in would punish the PowerShell-compatible light users.

**Q8.** Missed items expensive later: (1) lineage/thread identity for pure-HTTP engines (no CLI thread id; need synthetic stable ids); (2) lock model when DB is present - file lock must remain primary, DB must never be required for a single consultation; (3) R17 contradiction (plugin opt-in vs C3 ON-by-default) must be resolved in the first telemetry commit; (4) R13 host-invariance (`$CODEX_CONSULT_ROOT` resolution) and path separators across Windows/macOS/Linux; (5) redaction surface for every error path that could contain an API key. Framing challenge: the "one binary, many crates" preference is sound only if the trait boundaries are frozen before any engine or store implementation lands.

## Findings
- G2-1 [severity: major] R17 telemetry opt-in vs ON-by-default contradiction remains open - trigger: first telemetry landing - evidence: (read: codex-consult.md R17 + README.md Telemetry) - verification: compare plugin ROADMAP decision text with C3 README "on by default" paragraph - remedy: pick one policy and document the switch in both places before any send path ships.
- G2-2 [severity: major] No rebuild-from-files guarantee for SurrealDB yet specified - trigger: DB corruption or clean checkout - evidence: (inferred from proposed shape) - verification: design a `c3 store rebuild` that produces identical indexes from `.collab` + repo - remedy: make rebuild a required acceptance test for step 3.
- G2-3 [severity: minor] Lineage identity for pure-HTTP engines undefined - trigger: OpenRouter path - evidence: (read: codex-consult.md "Lineage = provider + model + endpoint") - verification: define synthetic lineage string and thread-id scheme - remedy: extend lineage rules before `http` engine lands.
- G2-4 [severity: note] Thompson sampling may be premature with sparse data - trigger: R15 implementation - evidence: (inferred) - verification: simulate with current scoreboard sizes - remedy: start with softmax + epsilon, promote to Beta only after >=50 rated consultations per (m,p).

## Verdict: ADVISE - Accept the files-as-record / DB-as-index split and context-pack-first API path, freeze Store/Engine traits early, resolve R17 before telemetry, and keep SurrealDB behind a heavy feature.
