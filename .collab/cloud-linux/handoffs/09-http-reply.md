# Handoff 09 - HTTP: reply

Date: 2026-10-08 07:47 local. Author: HTTP (model openai/gpt-6-luna, effort high), http-cli (version unknown).
Reviewer: openrouter :: openai/gpt-6-luna [http] (provider from -Provider, model from -Model; endpoint (default), wire_api: (default); provider fingerprint 7400f3fdc38a; harness http-cli (version unknown)).
Preflight: skipped.
Effort: high sent (requested high, mapping openai, by caps-v1: engine:http, any model; not confirmed by the provider). Consultation id: 953e38e0-fb3e-4c79-90c0-4d47357283fd.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: diff-review). Argv: `` (reviewer pack sent as the request body).
Parent thread: (none - new thread; no thread of lineage openrouter :: openai/gpt-6-luna in this task's ledger). Result thread: (unknown) (source: unknown).
Brief: `.collab/cloud-linux/handoffs/04-claude-diff-review.md` (sha256 8196067232cd). Reviewed: cb02cc8, base cb02cc8ddefcd1cf78535a9b4ef5ff20a2742c3d, tree sha256 a8f22ae6d1d2, 0 changed files.
Bridge outcome: usable reply. Wall time: 174.3 s. Tokens: in 104177 (cached 0), out 18492, reasoning 15362.
Engine turns: 1.
Warnings: reply normalised: evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; the reviewer's own text: handoffs/09-http-reply.original.json.
Timeout: 1500 s (-TimeoutSec); continuation after a timeout kill: up to 900 s. Range: `main..HEAD` - the range changes 53 files, 11868 lines (11715 insertions, 153 deletions).
Verdict: HOLD - The malformed NO_PROXY port can bypass the selected egress proxy, and the Linux PID-reuse and descendant-truncation cases leave concrete liveness and process-cleanup gaps to resolve or explicitly accept. Findings: 0 blocker, 1 major, 3 minor, 1 note (F09-1..F09-5, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/09-http-reply.reply.json`.
Raw event stream: `handoffs/09-http-reply.events.jsonl`.
Verbatim reply follows.

---

## 1. Windows behavior

**Yes.** The new HTTP proxy, proxy-auth, and CA-bundle behavior is shared code, not Windows-gated: when the corresponding environment variables are set, it changes Windows requests too (`crates/c3/src/http_engine/mod.rs:71-75`, `crates/c3/src/http_engine/mod.rs:188-211`, `crates/c3/src/http_engine/mod.rs:371-380`). Those are intentional environment-selected changes, not unchanged Windows behavior.

There is also an unconditional shared-helper change: `host_addrs` now constructs socket addresses directly for IP literals (`crates/c3/src/index/embed.rs:38-50`), with no `cfg` around it. For ordinary literals this may produce the same result as resolution, but a Windows build can take a different path when resolution would fail or differ. The Windows-specific lock and process-kill branches shown here remain under `cfg(windows)`; the Unix process-group and descendant-kill paths are guarded for non-Windows (`crates/c3/src/consult/detach.rs:749-755`, `crates/c3/src/engines/subprocess.rs:583-607`). The Windows-gated test attribute at `crates/c3/src/liveness/proc.rs:796-798` changes where that test runs, not the Windows product path.

## 2. Proxy/auth, host binding, and guards

The key is not read in proxy-auth mode (`crates/c3/src/http_engine/mod.rs:645-650`), and the request omits `Authorization` when there is no key (`crates/c3/src/http_engine/mod.rs:813-817`). The subscription and lab billing checks run before the proxy-auth key exemption (`crates/c3/src/consult/http.rs:150-170`); proxy auth therefore does not skip those checks. The key-mode request uses the URL built from the configured base URL, and redirects are disabled (`crates/c3/src/http_engine/mod.rs:357-369`, `crates/c3/src/http_engine/mod.rs:802-807`). The supplied adapter code shows no different request host from the host used to select auth mode for an ordinary absolute base URL. The orchestrator’s key-binding implementation is not included, so I cannot independently verify its exact comparison.

One host-list risk remains: `C3_HTTP_AUTH_PROXY=com` matches `https://attacker.example.com/v1`, because matching accepts any subdomain of a listed host (`crates/c3/src/http_engine/mod.rs:154-163`, `crates/c3/src/http_engine/mod.rs:172-182`). That switches the endpoint to header-less mode and bypasses the key-presence check. C3 does not read or send the key in this case, but the broad entry can wrongly classify unrelated endpoints as relying on the credential-attaching proxy. Reject public-suffix/single-label entries or use exact-host matching unless broad subdomain trust is explicitly intended.

There is a concrete `NO_PROXY` parsing bug: with `NO_PROXY=api.example:65536` and a request to `https://api.example:8443`, parsing the out-of-range port yields `("api.example", None)` (`crates/c3/src/http_engine/mod.rs:243-246`). The later match treats `None` as an unqualified host entry and disables proxy use on every port (`crates/c3/src/http_engine/mod.rs:295-305`). Ignore malformed port-qualified entries (or reject them) rather than broadening them to host-wide exclusions. This can bypass an environment-selected egress proxy; if proxy auth is selected, the resulting request has no Authorization header.

The proxy-auth/no-environment-proxy case is now made visible by a warning when `proxy` is false (`crates/c3/src/http_engine/mod.rs:736-747`), but the request has already been attempted by then. The behavior is intentionally allowed for transparent egress proxies; the warning is diagnostic, not a fail-closed guarantee. `redirects(0)` remains configured on the request agent and a 3xx is handled as a failure rather than followed (`crates/c3/src/http_engine/mod.rs:802-807`, `crates/c3/src/http_engine/mod.rs:830-838`). The configured-header validator still refuses `Proxy-Authorization`, with an adapter-side check as defense in depth (`crates/c3/src/http_engine/mod.rs:818-824`, `crates/c3/src/http_engine/mod.rs:1727-1750`). CA-bundle errors name the path and use generic certificate errors; the shown code does not print certificate contents (`crates/c3/src/http_engine/mod.rs:86-113`).

## 3. Linux liveness and locks

There is a PID-reuse edge in `same_start_time`: on non-Windows it treats any two parsed timestamps less than one second apart as equal (`crates/c3/src/liveness/proc.rs:116-131`). If a PID is reused within that interval, the `/proc` start time can differ but the old record can still be judged to name the new process. Compare Linux `/proc`-derived values at their available tick resolution, or use exact equality for values generated by this same helper.

The tree-kill traversal also silently truncates at 4096 descendants (`crates/c3/src/liveness/proc.rs:326-341`); `kill_tree` then kills only the returned PIDs and the root (`crates/c3/src/engines/subprocess.rs:598-607`). A process tree with more than 4096 descendants can therefore leave unenumerated children alive. Remove the silent cap or report and handle truncation.

I found no demonstrated Linux health-lock split-inode or waiter-plus-writer overlap: the non-Windows health lock uses `try_lock` on a kept lock file (`crates/c3-core/src/health.rs:982-1021`, `crates/c3-core/src/health.rs:1101-1107`). Its bounded in-lock budget is one attempt, while the full retry is documented to occur outside the task write lock (`crates/c3-core/src/health.rs:920-944`). The task-lock implementation and its call sites are not included, so I cannot certify the complete cross-lock discipline from this pack.

The CA-bundle refusal does not echo file contents; the `Proxy-Authorization` refusal and zero-redirect behavior hold in the shown adapter. The earlier F07-5 concern is marked fixed because the unproxied proxy-auth case now produces an explicit warning; it remains a permitted credentialless request, as the code comments intend for transparent proxies.

---

### Findings

- **F09-1** [major] `crates/c3/src/http_engine/mod.rs:243`, `crates/c3/src/http_engine/mod.rs:295` - An invalid numeric NO_PROXY port is treated as an unqualified host exclusion, which can disable the selected egress proxy for other ports. Trigger: Set NO_PROXY=api.example:65536 and request https://api.example:8443. Parsing fails to produce a u16 port, returns the host with no port, and the later host match excludes the request. Evidence: read-code: The port parse is converted to Option<u16>; None is subsequently treated as no port constraint, so the malformed port-qualified entry matches all ports. Verify: Add a unit test asserting that an out-of-range port entry does not exclude a request on a different port. Remedy: Reject or ignore an entry when it contains a port separator but the port is invalid; do not reinterpret it as a host-only entry.
- **F09-2** [minor] `crates/c3/src/http_engine/mod.rs:154`, `crates/c3/src/http_engine/mod.rs:172`, `crates/c3/src/http_engine/mod.rs:375` - The proxy-auth allowlist accepts broad parent names such as a public suffix, allowing unrelated endpoints to enter header-less proxy-auth mode. Trigger: Set C3_HTTP_AUTH_PROXY=com and configure a base URL such as https://attacker.example.com/v1. The suffix match selects HttpAuth::Proxy and the key-presence check is skipped. Evidence: read-code: Entries are canonicalized as hosts but are not checked for public-suffix or single-label breadth; host matching intentionally includes every subdomain. Verify: Add a test for a public-suffix entry and decide whether it must be refused or explicitly documented as a broad trust grant. Remedy: Reject public-suffix and otherwise dangerously broad entries, or require exact-host matching unless subdomain trust is separately and explicitly enabled.
- **F09-3** [minor] `crates/c3/src/liveness/proc.rs:116`, `crates/c3/src/liveness/proc.rs:723` - A Linux PID reused within one second can compare as the same process even when the /proc-derived start timestamps differ. Trigger: A stored process timestamp and a reused PID's timestamp differ by less than 1000 ms; the non-Windows comparator returns true. Evidence: read-code: The Linux helper supplies a start timestamp, but same_start_time accepts any non-Windows difference strictly below one second. Verify: Add a unit test with two distinct start timestamps separated by less than one second and verify that Linux liveness treats them as different. Remedy: Compare Linux-generated timestamps exactly or within only the /proc clock-tick resolution; retain any broader tolerance only for platforms whose timestamps genuinely require it.
- **F09-4** [minor] `crates/c3/src/liveness/proc.rs:326`, `crates/c3/src/engines/subprocess.rs:598` - The non-Windows tree kill can silently miss descendants because descendants_of stops collecting after 4096 PIDs. Trigger: A launched process tree has more than 4096 descendants; descendants beyond the cap are not returned, while kill_tree reports no survivors. Evidence: read-code: The traversal condition stops adding and enqueuing once out.len() reaches 4096, and the caller kills only the returned list plus the child. Verify: Add a traversal test with more than 4096 synthetic descendants and assert that truncation is either absent or explicitly reported. Remedy: Do not silently cap the traversal; if a bound is required, return a truncation status and surface or handle the surviving PIDs.
- **F09-5** [note] `crates/c3/src/index/embed.rs:38`, `crates/c3/src/index/embed.rs:41` - The IP-literal resolver change is shared and can affect Windows behavior; it is not confined to the Unix/Linux build. Trigger: A Windows caller validates or uses an embedder URL with an IP literal in an environment where resolving that literal would fail or produce different results. Evidence: inferred: host_addrs directly constructs SocketAddr values for IPv4 and IPv6 literals without a platform cfg, while domain names still use ToSocketAddrs. Verify: Run the embedder URL tests on Windows, including IPv4 and IPv6 literals, and compare behavior against the base revision. Remedy: If Windows behavior must remain unchanged, gate the direct-literal path to the intended platforms; otherwise document and test the cross-platform behavior change.

### Prior findings

- F07-5 - fixed - The attempt now adds an explicit warning when proxy auth is selected but no environment proxy applies; the credentialless request remains intentionally allowed for transparent-proxy deployments.

## Verdict: HOLD

The malformed NO_PROXY port can bypass the selected egress proxy, and the Linux PID-reuse and descendant-truncation cases leave concrete liveness and process-cleanup gaps to resolve or explicitly accept.

### Blockers

_(none)_

### Unproven scenarios

- The complete key-to-host binding in consult/orchestrate.rs is not included, so its exact comparison against the final request URL was not independently verified.
- The task-lock implementation and its callers are not included, so complete Linux task/write/health/index lock ordering and mutual exclusion cannot be certified.
- No commands or tests were run; Windows runtime equivalence for the shared IP-literal path and external proxy behavior were not exercised.
- The credential-attaching proxy's own host-matching policy is outside the supplied code, so the effect of an overly broad C3_HTTP_AUTH_PROXY entry on credentials attached by that proxy is not proven.

### First-run checklist (observable)

_(none)_
