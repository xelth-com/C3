# Handoff 03 - Codex: mcp-doc-check

Date: 2026-09-27 20:03 local. Author: Codex (model glm-5.3, effort high), Codex CLI 0.155.1.
Reviewer: ZAI :: glm-5.3 (provider from -Provider, model from -Model; endpoint https://api.z.ai/api/v1, wire_api: responses; provider fingerprint 29edbb79ed7d; harness codex-cli 0.155.1).
Preflight: ok: env ZAI_API_KEY set.
Roster: C:\Users\Dmytro\.codex\codex-consult-roster.json - entry 2 of 10 for -Provider ZAI (nothing applied).
Effort: high sent (requested medium, mapping zai-v1, by caps-v1: api.z.ai, glm-5.3; not confirmed by the provider). Consultation id: b49e1417-e8cb-47bf-ad5f-b5b629d0cda1.
Invocation: `codex-consult.ps1` (mode: resume, sandbox: read-only, purpose: checkpoint). Argv: `codex exec --sandbox read-only --color never --json -m glm-5.3 -c model_reasoning_effort="high" -c model_provider="ZAI" -o C:\Users\Dmytro\AppData\Local\Temp\codex-consult-last-b1ed3ee75781482397c4a69a5bcc6714.md --output-schema C:\Users\Dmytro\.codex\c3\schemas\consult-reply.v1.json resume 01a0e3f7-d55a-7533-a958-6afe35987a9f -` (prompt on stdin).
Parent thread: `01a0e3f7-d55a-7533-a958-6afe35987a9f`. Result thread: `01a0e3f7-d55a-7533-a958-6afe35987a9f` (source: events).
Brief: (none, prompt only). Reviewed: 9d9ccdf, base 9d9ccdf43fe317a6898afdbb387e6aced70ddae9, tree sha256 f3ee73ccc59e, 0 changed files.
Bridge outcome: usable reply. Wall time: 147.9 s. Tokens: in 2687484 (cached 2486528), out 14288, reasoning 7804.
Timeout: 1800 s (-TimeoutSec); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The documented MCP surface mostly matches the implementation, but containment, path wording, status writing, and stale index wording need correction. Findings: 0 blocker, 1 major, 2 minor, 1 note (F03-1..F03-4, tracked in `findings.json`). Structured reply: `handoffs/03-codex-mcp-doc-check.reply.json`.
Raw event stream: `handoffs/03-codex-mcp-doc-check.events.jsonl`.
Verbatim reply follows.

---

Reviewed at HEAD `9d9ccdf43fe317a6898afdbb387e6aced70ddae9`.

**Q1. Document/code mismatches.** The 11 tool names, required/optional argument names, JSON types, CLI mappings, timeout defaults, and `.mcp.json` snippet in `docs/port/mcp.md` match `tool_defs()` and `build_argv()`. Drifts:

1. Containment is lexical, not filesystem-resolved: `reject_escape` accepts a normal component that is a symlink/junction, and pack output is then written directly through it. Thus a path such as `inside-link/pack.md` can escape the working directory, contrary to the claimed invariant (finding #1).
2. Path semantics are described inconsistently: the `c3_consult` schema says an absolute path inside the repo is accepted, and the prose implies only an escaping `..` is rejected; the implementation rejects every absolute path and every `..` component, even `foo/../bar` (finding #2).
3. The note saying `panel`/`index` are “not built” is stale for `index`: the CLI has implemented build/rebuild/query/stats actions, although it is correctly not exposed over MCP (finding #3).
4. `c3_telemetry_status` is described as read-only, but obtaining its instance id creates the telemetry directory and salt file on first use (outside the repository) (finding #4).

**Q2. Is “no tool commits” enforced?** It is more than a tool-list label, but not a centralized capability gate. `tools/call` dispatches through the fixed `build_argv` allow-list; unknown names fail, and no branch passes an arbitrary CLI subcommand. `complain` and `forget-me` have no mappings. Current production Git usage found in code is non-mutating (`rev-parse`, `status`, `diff`, `ls-files`, and `hash-object` without `-w`); `git commit` appeared only in test setup. Future regressions in a mapped subcommand would still need tests or an explicit permission boundary to catch.

---

### Findings

- **F03-1** [major] `crates/c3/src/mcp/mod.rs:188`, `crates/c3-core/src/task_slug.rs:116`, `crates/c3/src/pack/reviewer.rs:317` - The MCP path-containment invariant can be bypassed by a symlink or junction: validation only checks lexical components, while downstream tools follow the link when reading or writing. trigger: call `c3_pack` with `out` set to `<in-repo-symlink>/pack.md`, where the link points outside the MCP working directory. Evidence: read-code: `reject_escape` calls `contained_join` on the untrusted string and otherwise passes the original path to the CLI.; read-code: `contained_join` rejects roots and `..` but accepts all normal components and never canonicalizes or checks symlink targets.; read-code: Pack output creates parents and writes both files directly at the supplied path, so a symlinked parent is followed. Verify: In a disposable repository, create an in-repo link to an external writable directory, invoke MCP `c3_pack` with that link in `out`, and check whether the external directory receives `pack.md` and `pack.pack.json`. Remedy: After lexical validation, canonicalize the resolved target and reject it unless it remains under the canonical working directory; for writes, also verify the final parent/file does not cross a symlink boundary.
- **F03-2** [minor] `docs/port/mcp.md:21`, `crates/c3/src/mcp/mod.rs:596`, `crates/c3-core/src/task_slug.rs:143` - The published path contract does not match runtime validation: absolute paths inside the working directory are rejected, and every `..` component is rejected even when normalization would remain inside. trigger: supply `brief` as an absolute in-repo path or `foo/../bar.md`. Evidence: read-code: The document says paths are rejected when absolute or when a `..` component escapes the working directory.; read-code: The tool schema explicitly says `brief` may be repo-relative or absolute inside the repo.; read-code: Every `ParentDir`, root, or prefix component is rejected regardless of the final normalized location. Verify: Call `build_argv` for `c3_consult` with an absolute in-repo brief and separately with `foo/../bar.md`; both should return a path refusal. Remedy: Change the schema description and prose to state that only `..`-free relative paths are accepted, or implement the more permissive precisely documented policy.
- **F03-3** [note] `docs/port/mcp.md:63`, `crates/c3/src/cli/index.rs:1` - The documentation says `index` is not built, but the current CLI implements the index surface; only its absence from MCP is accurate. trigger: read the current CLI dispatch and index implementation. Evidence: read-code: The text lists `panel`/`index` as not built.; read-code: The file implements milestone-8 build, rebuild, query, and stats actions. Verify: Run `c3 index stats` or inspect the CLI dispatch to confirm the implemented subcommand exists. Remedy: Rewrite as: `panel` is not built; `index` exists but is intentionally not exposed as an MCP tool.
- **F03-4** [minor] `crates/c3/src/mcp/mod.rs:739`, `crates/c3/src/cli/telemetry.rs:120`, `crates/c3/src/telemetry/mod.rs:169` - `c3_telemetry_status` is not read-only on first use: displaying the instance id creates a telemetry directory and persistent salt file outside the repository. trigger: invoke `c3_telemetry_status` before the telemetry salt exists. Evidence: read-code: The tool description says it is read-only and sends nothing.; read-code: Telemetry status prints `telemetry::instance_id()`.; read-code: `instance_id` calls `read_or_create_salt`, which creates the directory and writes the salt when absent. Verify: Point the telemetry home at an empty temporary home, invoke `c3_telemetry_status`, and observe creation of the telemetry directory and salt file. Remedy: For status, read an existing instance id without creating one, or update the tool description and document the one-time local configuration write.

### Prior findings

_(none)_

## Verdict: ADVISE

The documented MCP surface mostly matches the implementation, but containment, path wording, status writing, and stale index wording need correction.

### Blockers

_(none)_

### Unproven scenarios

- The symlink bypass and first telemetry-status write were established by code inspection, not exercised in a live MCP process.

### First-run checklist (observable)

_(none)_
