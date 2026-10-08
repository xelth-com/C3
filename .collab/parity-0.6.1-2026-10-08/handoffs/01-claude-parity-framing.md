Write in English.

# Handoff 01 - claude: framing - C3 to 0.6.1 parity, and where to go beyond

Date: 2026-10-08. Base commit: `4cd8d18` (branch `main`, the cloud session's Linux port merged). Context:
`.collab/parity-0.6.1-2026-10-08/state.md` (the gap table A-H and the proposed waves), `docs/DESIGN.md`,
`docs/port/*.md`. The specification is the PowerShell plugin at tag v0.6.1
(github.com/xelth-com/claude-codex-consult: CHANGELOG `[0.6.0]`, `[0.6.1]`; you accepted both).

## Question

The operator's goal: C3 reaches the plugin's 0.6.1 behaviour, and where C3 has more (the `http` engine, packs,
the SurrealDB index, the router, MCP) it should do better, not merely equal. C3 was verified live only through
OpenRouter (the cloud sandbox had no subscriptions); this machine has the Codex, Antigravity and Muse CLIs
signed in and the coding-plan keys in the environment.

## Options considered

- Wave order as in state.md (small parse/scan/roster items -> 0.6.1 telemetry -> recovery hardening -> the
  `claude` engine), versus the `claude` engine first (the biggest user-visible 0.6.0 feature).
- The endpoint route (`auth: endpoint` - z.ai, MiMo, Kimi Code through an Anthropic-compatible endpoint): port
  it as in the plugin (spawn Claude Code headless), or make it a native Messages-API path of C3's `http` engine
  (no Claude Code dependency, the pack instead of tools, the same 3-6x credit saving the plugin's A/B measured)
  and keep the spawned Claude Code only for `subscription` (the claude.ai login cannot be used natively).
- The SurrealDB index: today "never required for a consultation". Could it carry what the plugin keeps in files
  (the machine-wide endpoint health, the telemetry spool, the pending/recovery records) to remove the
  lock-and-fold machinery of E2/E20/E24/E26 instead of porting it?
- Oracle: the plugin's harnesses through the shim (`docs/port/harness-shim.md`) where they exist; C3's own
  `cargo test` for the rest; live runs on this machine's subscriptions for the engines.

## Constraints

- Byte-compatible `.collab` files with the plugin (either bridge reads the same task history) - the ledger entry
  gains `consult_ref` exactly as 0.6.1 writes it; marks gain `rating_rev` and `judge`.
- The telemetry contract is the as-built intake at xelth.com/T/ (it already reads `consult_ref`, `judge`,
  `rating_rev`; the replacement rule is the highest `rating_rev`, then `created`).
- Keys never created, printed, stored or committed; reviewers read-only; one worker per wave with tests.

## Questions

- **Q1.** The wave order: keep A-D, E, F-G, H - or H first? What would you merge or split?
- **Q2.** The endpoint route: spawned Claude Code (parity) or native Messages API in the `http` engine (better)?
  What does the native path lose (tools, the repository walk, the `[1m]` context, the quota_mark from
  `rate_limit_event`) and does it matter for a read-only reviewer that gets a pack?
- **Q3.** Should the SurrealDB index (or a small embedded store) replace the file-and-lock machinery for
  endpoint health, the spool and the recovery records - or is "files the plugin can read" worth more?
- **Q4.** What must be verified live on this machine before any of it is called 0.6.1-equivalent (which engines,
  which harnesses through the shim, what a panel must show)?
- **Q5.** Anything in the gap table that is wrong or missing?

Answer by number. Keep it under 700 words.
