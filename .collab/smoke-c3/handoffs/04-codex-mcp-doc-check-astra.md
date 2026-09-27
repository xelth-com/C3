# Handoff 04 - Codex: mcp-doc-check-astra

Date: 2026-09-27 20:08 local. Author: Codex (model gpt-6-astra, effort medium), Codex CLI 0.155.1.
Reviewer: openai :: gpt-6-astra (provider from -Provider, model from -Model; endpoint (default), wire_api: (default); provider fingerprint 56d97b6ece36; harness codex-cli 0.155.1).
Preflight: ok: Logged in using ChatGPT.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 1 of 10 for -Provider openai (nothing applied).
Effort: medium sent (requested medium, mapping openai, by caps-v1: builtin:openai, any model; not confirmed by the provider). Consultation id: d945dd42-934e-48ef-b7d7-27aa88a5c009.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: checkpoint). Argv: `codex exec --sandbox read-only --color never --json -m gpt-6-astra -c model_reasoning_effort="medium" -c model_provider="openai" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-37a397fdea904dc1bebd543a7805bc31.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json -` (prompt on stdin).
Parent thread: (none - new thread; no thread of lineage openai :: gpt-6-astra in this task's ledger; other lineage(s): ZAI :: glm-5.3). Result thread: `01a0e40a-9a1e-7f20-af50-3ae08a874be4` (source: events).
Brief: `.collab/smoke-c3/handoffs/01-claude-mcp-doc-check.md` (sha256 b5d8ce37ca26). Reviewed: 9d9ccdf, base 9d9ccdf43fe317a6898afdbb387e6aced70ddae9, tree sha256 f3ee73ccc59e, 0 changed files.
Bridge outcome: usable reply. Wall time: 206.7 s. Tokens: in 394049 (cached 337536), out 3126, reasoning 63.
Timeout: 1800 s (-TimeoutSec); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The documented tool surface matches, but path guarantees, side-effect descriptions, and byte-identity claims need correction. Findings: 0 blocker, 0 major, 3 minor, 0 note (F04-1..F04-3, tracked in `findings.json`). Structured reply: `handoffs/04-codex-mcp-doc-check-astra.reply.json`.
Raw event stream: `handoffs/04-codex-mcp-doc-check-astra.events.jsonl`.
Verbatim reply follows.

---

1. **Q1:** Checked current HEAD `9d9ccdf`. All 11 tool names, listed arguments, required fields, array notation, CLI mappings, `.collab` default, and timeout values match. The document does not enumerate all JSON types, but gives no conflicting types. The registration snippet correctly invokes `c3 mcp`; actual Claude Code registration was not exercised.

   - **F03-1 corroborated:** containment is lexical; an accepted path through a symlink/junction can reach outside the working directory.
   - **F03-2 corroborated with qualification:** the document already rejects absolute paths, but the `brief` schema description explicitly permits absolute in-repository paths. Runtime rejects those and **every** `..` component, including `foo/../bar.md`.
   - **F03-3 corroborated:** `index` is implemented and dispatched by the CLI, although absent from MCP. “Not built” is stale.
   - **F03-4 corroborated:** telemetry status creates the installation salt when absent, contradicting its schema’s “Read-only” description.
   - Additional drift: using the same executable guarantees the same implementation, not byte-identical artifacts across runs; pack sidecars contain the current timestamp. The snapshot schema also understates writes: full snapshots update anchor/sequence state.

2. **Q2:** More than the advertised tool list: `build_argv` explicitly allowlists commands and rejects unknown tool names, so callers cannot directly invoke a commit/finish command. However, `run_cli` adds no sandbox or Git prohibition. The absolute “never commits” guarantee is therefore not enforced across all descendants; engine tree checks explicitly treat content-preserving HEAD changes as informational. Scope the claim to C3’s own implemented operations, or enforce it across reviewer execution.

This was a read-only code review; no files were changed and no mutating reproductions were run.

---

### Findings

- **F04-1** [minor] `docs/port/mcp.md:17`, `crates/c3/src/mcp/mod.rs:10`, `crates/c3/src/pack/reviewer.rs:276` - The blanket guarantee of byte-identical MCP and CLI artifacts is false across separate runs because generated artifacts include run-dependent data. Trigger: Generate the same reviewer pack through MCP and CLI at different times. Evidence: read-code: The runner invokes current_exe with mapped CLI arguments.; read-code: The sidecar's generated field uses chrono::Local::now().to_rfc3339(). Verify: Generate equivalent packs at different times and compare their sidecar generated fields. Remedy: Describe implementation and format parity instead of unconditional byte identity.
- **F04-2** [minor] `crates/c3/src/mcp/mod.rs:724`, `crates/c3/src/pack/snapshot.rs:351` - The snapshot tool description says it writes only the snapshot file, but full snapshots also update persistent anchor and sequence state. Trigger: Run a full snapshot in a repository with a HEAD commit. Evidence: read-code: The tool description promises snapshot-file-only writes.; read-code: The full-snapshot branch calls write_anchor and reset_seq. Verify: Compare <collab>/.c3 state before and after a full snapshot in a disposable repository. Remedy: Include anchor and sequence updates in the tool's side-effect description.
- **F04-3** [minor] `docs/port/mcp.md:86`, `crates/c3/src/mcp/mod.rs:484`, `crates/c3/src/engines/tree_check.rs:10` - The no-commit invariant is not a transitive enforcement boundary for reviewer descendants: MCP restricts command dispatch, but does not prohibit Git writes, and engine content checks do not reject content-preserving commits. Trigger: A reviewer descendant changes HEAD without changing working-tree contents. Evidence: read-code: Unknown tools are refused, but allowed child commands are spawned without an MCP-level sandbox.; read-code: Content-preserving commits are explicitly treated as informational revision movement rather than content violations. Verify: Inspect the engine comparison result for identical content fingerprints with different HEAD values; confirm it does not flag content tampering. Remedy: Document the invariant as C3 not initiating commits, or add preventive restrictions and explicit Git-state validation for descendants.

### Prior findings

- F03-1 - still-open - Corroborated by code: contained_join checks components only; reviewer::write uses ordinary create_dir_all/fs::write through accepted paths. No symlink or junction reproduction was run.
- F03-2 - still-open - Corroborated with qualification: docs/port/mcp.md:21 already prohibits absolute paths, whereas mcp/mod.rs:596 permits them in the brief description. task_slug.rs:130-132 rejects all ParentDir, RootDir and Prefix components.
- F03-3 - still-open - Corroborated: crates/c3-cli/src/main.rs:62,120 declares and dispatches Index; crates/c3/src/cli/index.rs implements build, rebuild, query and stats. MCP does not expose it.
- F03-4 - still-open - Corroborated: cli/telemetry.rs:125 calls instance_id; telemetry/mod.rs:175-196 reads or creates salt using create_dir_all and fs::write in the telemetry home. No first-use invocation was run.

## Verdict: ADVISE

The documented tool surface matches, but path guarantees, side-effect descriptions, and byte-identity claims need correction.

### Blockers

_(none)_

### Unproven scenarios

- Actual Claude Code registration and protocol interoperability were not exercised.
- Symlink/junction escape and first-use telemetry writes were established from code, not runtime reproduction.
- No reviewer was induced to commit; the review establishes the absence of a transitive enforcement guarantee.

### First-run checklist (observable)

_(none)_
