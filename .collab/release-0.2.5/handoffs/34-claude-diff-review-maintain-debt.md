# Handoff 34 - Claude: diff-review of 0.2.5 - `c3 maintain`, telemetry and CI debt

Date: 2026-11-23. Range: `d37a397..70ed824` (base `d37a397`, head `70ed824`; one commit). The commit
also holds another piece of work (the move of C3's project folder to `.c3/`); the files of THIS
brief are `crates/c3/src/maintain/*`, `crates/c3/src/telemetry/classes.rs`,
`crates/c3/src/panel/plan.rs` (one helper), `crates/c3-core/src/quota.rs`,
`.github/workflows/ci.yml` and the tests `crates/c3-cli/tests/maintain.rs` and
`telemetry_parity.rs`. Ignore the rest.

You run in a throwaway checkout of the head of this range; the change is the patch file named in
your prompt (`.collab/`, `.c3/` and `.eck/` left out). You may have no shell: read the patch and the
files it names with your file tools. Everything is in this repository.

## Question

Is this part safe to release in 0.2.5? Find defects that would make C3 damage a machine, a
repository, a roster or a consultation record, lose or split a project's records, send something
private off the machine, or report a result that is not true - not style.

## Delta since the last review

First review of this code; no model has read it. `c3 maintain` updates engine CLIs and the roster's
model ids by itself, reading a public catalog (OpenRouter's `/api/v1/models`). Eleven items:

- **1. In-use probe.** `c3 maintain` skips an engine whose CLI is running. Under
  `CODEX_CONSULT_TEST_MODE` the probe is answered by the hook `CODEX_CONSULT_TEST_MAINTAIN_IN_USE`
  (engine names to report; none by default); the real process table is not read.
- **2. List price.** When the catalog prices both the seated id and a successor of a higher major
  and the candidate's `pricing.prompt` or `pricing.completion` is more than twice the seated
  one's, the swap is announced only ("list price N times higher - seat it by hand"). Only the two
  numbers are kept, for that pass, never written to state (`maintain/models.rs`).
- **3. Lab token.** A roster id with a leading token naming its maker or its plan's vendor
  (`kimi-k3.1-pro`) that no catalog lists as is gets one more try without the token, by the panel's
  lab table (`lab_named_by` in `panel/plan.rs`); a match serves dates and successors, and a
  successor is written in the plan's own spelling (token put back), then smoked on its plan.
- **4. Dotted Claude id.** An id a trusted public source lists under `anthropic/` with dots
  (`claude-opus-4.5`) is also published in dash form when that has the Claude shape, so telemetry
  sends it by name (`telemetry/classes.rs`).
- **5. CI.** The test step of both runners and of the index-surreal job is one step per crate
  (`c3-core`, `c3`, `c3-cli`), each `--no-fail-fast` and `if: success() || failure()`.
- **6. Address table (F27-1, F15-1).** A public source is read only from a global unicast address:
  one table of non-global IPv4 and IPv6 ranges, an IPv4-mapped address and the NAT64 prefix
  `64:ff9b::/96` judged as the IPv4 address they carry, at the lookup and at the peer of every hop
  (`maintain/web.rs`).
- **7. No environment proxy (F29-1).** The maintain client refuses `ALL_PROXY` / `HTTPS_PROXY` /
  `HTTP_PROXY` explicitly, so the peer check sees the origin, not a proxy.
- **8. Deadline (F14-1).** Each hop resolves its name under the fetch's one deadline, then builds
  its client: connect and request get the time left after the lookup and connect to the addresses
  the lookup vetted. A stalled lookup thread may outlive the deadline (recorded).
- **9. Entry fingerprint (F15-3).** A smoke target carries the SHA-256 of the whole roster entry's
  canonical JSON; the smoke is "not run: the roster entry changed" when the entry at that position
  no longer has it (`maintain/smoke.rs`).
- **10. Per-turn proof (F32-2).** The codex smoke binds switch evidence (a `multi_agent_version` in
  a `turn_context`, or a tool list) to its own `task_started` / `task_complete` pair
  (`c3-core/src/quota.rs`); it passes only when every turn has its own, and a rollout whose
  boundaries do not pair up is unproven.
- **11. Decisions, no code.** The catalog's date is a third party's; the effort relaxation
  (`model_declared`) is kept; protected and on-request entries (the ChatGPT ones) are only
  announced, never swapped.

## CURRENT invariants claimed

- A public catalog is never requested from a non-global address (lookup, peer, every redirect hop)
  and never through a proxy from the environment.
- One 15 s deadline bounds lookup, connect and body; the connect gets only the time left.
- An entry changed in any field since it was chosen is not smoked; an update is rolled back.
- A smoke passes only with evidence for each of its turns; unpaired turn boundaries are unproven.
- A swap across a major at more than twice the list price (either field) is announced only;
  without prices for both ids, or within one major, nothing changes.
- A protected or on-request entry is announced only, never swapped. (Known: `openai :: gpt-6.2-sol`
  is neither, so in swap mode a catalog successor of it could be seated.)
- A successor matched through a lab token is written in the plan's own spelling.
- A dotted `anthropic/` id is sent by name only from a trusted source, in Claude shape.
- The maintain tests never read the real process table; none reaches a real network host.

## Evidence

- fmt and clippy clean; c3-core 252 and c3 library 852 tests; c3-cli `maintain` 44 (no real grok
  needed any more), `telemetry_parity` 12, `mcp_integration` 2; every other c3 binary green.
- CI for `70ed824` was still running when this brief was handed over; the commit before it is
  green on Linux and Windows.
- NOT exercised live: no real `c3 maintain` pass has run with this code; the price and lab-token
  rules were seen only against fixtures.

## Questions

- **Q1. Address table.** Name any path where a non-global address passes: a range missed, an
  IPv4-mapped or NAT64 form, a check applied to the lookup but not to every redirect hop, a name
  that resolves to several addresses where one is local, a connect to an address the lookup did
  not vet.
- **Q2. Proxy and deadline.** Is there any other client in the maintain path that still reads the
  environment? Can a hop outlive the deadline (a lookup thread, a body read, a retry)?
- **Q3. Fingerprint and proof.** Name an edit the fingerprint does not see, a rollout shape that
  passes without evidence for a turn, and a legitimate smoke that is now refused or rolled back.
- **Q4. Price and lab token.** Name a wrong match: a token that names a lab but not this model's
  maker, a successor written in the wrong spelling, a price read from the wrong field or source or
  from another model's entry, a missing or zero price taken as cheap.
- **Q5. Telemetry.** Can the dash form let a private or custom model name through as a published
  one?
- **Q6. Test hook.** Is `CODEX_CONSULT_TEST_MAINTAIN_IN_USE` reachable outside test mode, so that
  an environment variable can make a real pass skip or update an engine?
- **Q7. CI.** Does a step now hide a failure (`success() || failure()` after a cancelled step, a
  feature the c3 step names wrongly), or stop testing something the workspace step covered?
- **Q8.** What that matters is untested?

Answer by number. Cite `path:line` for every finding. Keep it under 900 words.
