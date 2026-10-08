# Handoff 10 - HTTP: reply

Date: 2026-10-08 07:49 local. Author: HTTP (model nvidia/nemotron-3-super-120b-a12b:free, effort low), http-cli (version unknown).
Reviewer: openrouter :: nvidia/nemotron-3-super-120b-a12b:free [http] (provider from -Provider, model from -Model; endpoint (default), wire_api: (default); provider fingerprint 7400f3fdc38a; harness http-cli (version unknown)).
Preflight: skipped.
Effort: low sent (requested low, mapping openai, by caps-v1: engine:http, any model; not confirmed by the provider). Consultation id: fe6b8396-3679-4bd1-a822-19cdcaa069ba.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `` (reviewer pack sent as the request body).
Parent thread: (none - new thread; no thread of lineage openrouter :: nvidia/nemotron-3-super-120b-a12b:free in this task's ledger). Result thread: (unknown) (source: unknown).
Brief: `.collab/cloud-linux/handoffs/04-claude-diff-review.md` (sha256 8196067232cd). Reviewed: cb02cc8, base cb02cc8ddefcd1cf78535a9b4ef5ff20a2742c3d, tree sha256 a8f22ae6d1d2, 0 changed files.
Bridge outcome: usable reply. Wall time: 146.6 s. Tokens: in 127167 (cached 0), out 16582, reasoning 14717.
Engine turns: 1.
Warnings: reply normalised: evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; the reviewer's own text: handoffs/10-http-reply.original.json.
Timeout: 1500 s (-TimeoutSec); continuation after a timeout kill: up to 900 s. Range: `main..HEAD` - the range changes 53 files, 11868 lines (11715 insertions, 153 deletions).
Verdict: HOLD - Open findings F09-1, F09-2, F09-3, F09-4 remain unaddressed, posing risks to proxy safety and Linux liveness. Findings: 0 blocker, 1 major, 3 minor, 0 note (F10-1..F10-4, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/10-http-reply.reply.json`.
Raw event stream: `handoffs/10-http-reply.events.jsonl`.
Verbatim reply follows.

---

# Answer to the Brief

## 1. Windows behaviour changes

The diff changes Windows behaviour in the embedder validation (`crates/c3/src/index/embed.rs`). The function `host_addrs` now treats IP literals as parsed without resolver lookup, which may cause IP literals that previously resolved to loopback (via the hosts file) to be rejected as non-loopback. See F09-5.

## 2. Proxy auth mode or proxy decision unsafe inputs

- **F09-1**: Setting `NO_PROXY` to a host with an invalid port (e.g., `example.com:99999`) causes the host to be excluded for all ports, potentially disabling the proxy for other ports on that host. See `crates/c3/src/http_engine/mod.rs` lines 243-248 and 298-301.
- **F09-2**: Setting `C3_HTTP_AUTH_PROXY` to a public suffix (e.g., `co.uk`) causes any host ending with that suffix to use the proxy auth mode (no Authorization header), allowing unrelated endpoints to enter header-less proxy-auth mode. See `crates/c3/src/http_engine/mod.rs` lines 174-182.

## 3. Linux-specific holes in liveness or locks

- **F09-3**: A Linux PID reused within one second may be incorrectly considered the same process due to the one-second tolerance in `same_start_time`. See `crates/c3/src/liveness/proc.rs` lines 118-132.
- **F09-4**: The non-Windows tree kill may miss descendants beyond the 4096 PID limit in `descendants_of`. See `crates/c3/src/liveness/proc.rs` lines 329-341 and `crates/c3/src/engines/subprocess.rs` line 598.

Each of these is detailed in the findings below.

---

### Findings

- **F10-1** [major] `crates/c3/src/http_engine/mod.rs:243`, `crates/c3/src/http_engine/mod.rs:295` - An invalid numeric NO_PROXY port is treated as an unqualified host exclusion, which can disable the selected egress proxy for other ports. Trigger: Setting NO_PROXY to a value containing a host with an invalid port number (e.g., 'example.com:99999'). Evidence: read-code: The function no_proxy_entry returns (host, None) when the port string is not a valid u16, causing the host to be matched without port consideration in proxy_decision. Verify: Set NO_PROXY=example.com:99999 and verify that a request to example.com:80 (or any other port) bypasses the proxy even if a proxy is configured for that port. Remedy: Treat an invalid port as making the entire NO_PROXY entry invalid (ignore it) or require a valid port for the entry to be considered.
- **F10-2** [minor] `crates/c3/src/http_engine/mod.rs:154`, `crates/c3/src/http_engine/mod.rs:172`, `crates/c3/src/http_engine/mod.rs:375` - The proxy-auth allowlist accepts broad parent names such as a public suffix, allowing unrelated endpoints to enter header-less proxy-auth mode. Trigger: Setting C3_HTTP_AUTH_PROXY to a public suffix (e.g., 'co.uk'). Evidence: read-code: The function host_uses_proxy_auth returns true for any host that ends with the suffix (e.g., 'example.co.uk' matches suffix 'co.uk'). Verify: Set C3_HTTP_AUTH_PROXY=co.uk and verify that a request to http://example.co.uk/ uses the proxy auth mode (no Authorization header sent) even though the proxy was not intended for this host. Remedy: Use a proper domain matching library that requires a dot boundary and does not match public suffixes (e.g., match only if the host is exactly the suffix or has a dot boundary before the suffix).
- **F10-3** [minor] `crates/c3/src/liveness/proc.rs:116`, `crates/c3/src/liveness/proc.rs:723` - A Linux PID reused within one second can compare as the same process even when the /proc-derived start timestamps differ. Trigger: A PID is reused within one second (e.g., process A exits at time T, process B starts at time T+0.5s with the same PID). Evidence: read-code: The function same_start_time returns true if the absolute difference in start times is less than 1000 milliseconds, allowing a reused PID within one second to be considered the same process. Verify: Create two processes with the same PID within one second and verify that pid_alive considers the second process as still alive when using the first process's start time. Remedy: Reduce the tolerance to zero (exact match) or use a higher-resolution start time source if available, noting that the /proc start time has 10ms resolution.
- **F10-4** [minor] `crates/c3/src/liveness/proc.rs:326`, `crates/c3/src/engines/subprocess.rs:598` - The non-Windows tree kill can silently miss descendants because descendants_of stops collecting after 4096 PIDs. Trigger: A process tree with more than 4096 descendants. Evidence: read-code: The function descendants_of limits the output list to 4096 PIDs, causing excess descendants to be omitted from the kill list. Verify: Create a process tree with over 4096 descendants and verify that some descendants are not killed by the non-Windows tree kill. Remedy: Remove the limit or increase it to a safe number (e.g., the maximum number of processes on the system), or change the algorithm to kill as it walks the tree without storing all PIDs.

### Prior findings

- F07-5 - fixed - The warning for proxy auth without a proxy was added.
- F09-1 - still-open - Invalid numeric NO_PROXY port still treated as host exclusion.
- F09-2 - still-open - Proxy-auth allowlist still accepts broad parent names.
- F09-3 - still-open - Linux PID reuse within one second can still cause false same-start-time match.
- F09-4 - still-open - Non-Windows tree kill still misses descendants beyond 4096 PIDs.
- F09-5 - still-open - IP-literal resolver change is shared and affects Windows behavior.

## Verdict: HOLD

Open findings F09-1, F09-2, F09-3, F09-4 remain unaddressed, posing risks to proxy safety and Linux liveness.

### Blockers

_(none)_

### Unproven scenarios

_(none)_

### First-run checklist (observable)

_(none)_
