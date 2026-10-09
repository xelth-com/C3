Write in English.

# Handoff 24 - claude: wave 4 of the compatibility track (the `claude` engine) - diff review

Date: 2026-10-09. Base commit: `b215374` (branch `main`; code at 1fdb8bd). The range under review is
`6bfc080..1fdb8bd` - wave 4 (commits 88ce0b5, 1ca23d6, 115f015, 1fdb8bd; 1185fac is a merge of main and carries
nothing of its own); skip `.collab/`. Plan: `state.md` gap row H, decisions P4 (spawned Claude Code, no native
Messages API in this wave) and P2.

## Question

Wave 4 ports the plugin's 0.6.0 `claude` engine: Claude Code headless as a reviewer with the three auth modes.
Faithful (the harness-claude oracle is 87/0), and safe - the child environment, the strict tree check, the
endpoint route's handling of the key variable? ACCEPT, HOLD (blockers by id) or ADVISE.

## Delta

- `c3_core::claude` (new): the closed model table incl. `claude-haiku-5-5`, the family rule, `[1m]` stripping,
  the endpoint object (`base_url` an absolute https URL without credentials/query/fragment - the value never
  shown; `env_key`; `timeout_ms`), the child-environment allow list (only the variables the plugin passes;
  `CODEX_CONSULT_TEST_CHILD_ENV_PASS` in test mode), the argv.
- Roster and lineage: `engine: "claude"`, `auth: subscription | api-key | endpoint`, `endpoint {...}`, `plan`;
  the validator texts as the plugin's (`auth of engine claude must be "subscription" (the claude.ai login, the
  default) or "api-key" (ANTHROPIC_API_KEY) or "endpoint" (...)`, `the claude model '<m>' is not in the claude
  engine's model table (...)`, `endpoint applies only to engine claude with auth "endpoint"`, `... is an
  Anthropic model id, which the endpoint route cannot carry`); the identity fingerprints; every engine list
  reads `codex, agy, muse, claude`.
- `engines::claude_auth`: the `Get-ClaudeSignIn` preflight (subscription: the claude.ai login; api-key: the
  variable set; endpoint: the `env_key` set), the `c3 providers` rows (the endpoint host as the vendor class).
- `engines::claude` + the orchestrator: the turn `claude -p --output-format stream-json --verbose --restricted
  --strict-mcp-config --disable-slash-commands --tools Read,Grep,Glob --permission-mode dontAsk --model <m>
  [--effort <e>] [--json-schema <text>] [--max-turns <n>] [--add-dir <d>...] (--session-id <minted uuid> |
  --resume <thread> [--fork-session])` - identical to the plugin's `Get-ClaudeArgs`; the prompt on stdin; the
  init proof; the `message.model` check (E13); the killed-turn proof (E12-E15); `rate_limit_event` ->
  `quota_mark` into the machine-wide health; the strict tree check; `switched_off`; the ledger `reviewer`
  (`{provider anthropic, model, engine claude, harness claude-cli <v>, provider_config {engine, launcher,
  credential_mechanism, auth_method, api_provider}}`) and `engine` records (`{auth, model_resolved,
  mcp_servers, permission_mode, api_key_source, quota_mark}`).
- The panel scheduling group and the plan wait (E16) for claude members; `tests/claude_engine.rs` with the
  plugin's fake CLI; the not-spooled warning ends `- dropped` (harness-telemetry FORGET D3; C3 says it for every
  event it could not spool since it does not retry after the commit - the plugin retries 5 s first); README,
  the setup skill section 4c, `docs/port/wave4-claude.md`.

## Requested checks run

| check | command | revision | exit | observation | state |
|---|---|---|---|---|---|
| tests / clippy / fmt | `cargo test --workspace -j 2` | 1fdb8bd | 0 | green except the known-flaky `index::embed` loopback test (passes alone); clean | completed |
| harness-claude (shim v0.6.1, PS 5.1) | `run-all.ps1 -ScriptsDir ... -Only harness-claude` | 1fdb8bd | 0 | 17/5 (crash) -> 87/0 | completed |
| roster / engines / muse / panel / telemetry | the same | 1fdb8bd | - | 125/0, 97/0, 73/1 (UNIT D2 grep of the script source), 62/0, 47/96 (P7 + FORGET D3 shim) | completed |
| LIVE smoke, real Claude Code 2.1.294 on the subscription | scratch repo + scratch roster (`auth: subscription`), `--dry-run` then one `--purpose chore --max-words 50` | 1fdb8bd | 0 | preflight `available (ok: signed in (claude.ai subscription))`; argv `--model haiku --effort low --session-id <uuid>`; usable reply in 6.3 s (8.9 s overall), model `claude-haiku-5-5`, init tools Glob/Grep/Read, 0 permission denials; a seven-day `allowed_warning` rate-limit line in the plugin's wording | completed |

No endpoint-mode live run in this wave (keys); no key read or printed.

## Open findings

None from this wave. Open elsewhere: F22-1 (journal replay after eviction, wave 2h running), F23-1..F23-5
(recovery fail-open paths, wave 3c running); the pending ratings/marks wait for the task lock.

## Questions

- **Q1.** Safety: a variable that reaches the child beyond the allow list, a path by which the endpoint key
  variable's VALUE can reach a file, a log or the ledger, a tree-check gap the plugin closes?
- **Q2.** Fidelity: the init proof, E13's model check and E12-E15's killed-turn proof versus the plugin's; the
  quota_mark write.
- **Q3.** Verdict: ACCEPT, HOLD (blockers by id), or ADVISE.

Answer by number. Keep it under 600 words.
