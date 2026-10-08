# Handoff 07 - HTTP: reply

Date: 2026-10-08 07:37 local. Author: HTTP (model openai/gpt-6-luna, effort high), http-cli (version unknown).
Reviewer: openrouter :: openai/gpt-6-luna [http] (provider from -Provider, model from -Model; endpoint (default), wire_api: (default); provider fingerprint 7400f3fdc38a; harness http-cli (version unknown)).
Preflight: ok: checked at launch (http engine).
Effort: high sent (requested high, mapping openai, by caps-v1: engine:http, any model; not confirmed by the provider). Consultation id: c83e5ba9-8d2c-485f-be39-a5c620c05e18.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: decision). Argv: `` (reviewer pack sent as the request body).
Parent thread: (none - new thread; no thread of lineage openrouter :: openai/gpt-6-luna in this task's ledger). Result thread: (unknown) (source: unknown).
Brief: `.collab/cloud-linux/brief.md` (sha256 d3795bc970a8). Reviewed: e108fe6, base e108fe6c510ecae5a43bdee4d9467abd60ea399c, tree sha256 862244b8a7ec, 0 changed files.
Note: HEAD moved during the review (e108fe6 -> 2cff380) - no file content changed: not a tree change.
Bridge outcome: usable reply. Wall time: 183.3 s. Tokens: in 21860 (cached 0), out 16961, reasoning 13866.
Engine turns: 1.
Warnings: reply normalised: evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; evidence: object wrapped in an array; the reviewer's own text: handoffs/07-http-reply.original.json.
Timeout: 1500 s (-TimeoutSec); continuation after a timeout kill: up to 900 s.
Verdict: HOLD - The proxy decision has concrete NO_PROXY and loopback gaps, and proxy-auth mode can be selected without an applicable proxy route. Findings: 0 blocker, 4 major, 1 minor, 0 note (F07-1..F07-5, tracked in `findings.json`). Structured reply (prompt-only transport): `handoffs/07-http-reply.reply.json`.
Raw event stream: `handoffs/07-http-reply.events.jsonl`.
Verbatim reply follows.

---

## 1. Host spelling and proxy decisions

- **Look-alike DNS names:** The ordinary domain boundary check works: `evilopenrouter.ai` does not match `openrouter.ai`; a true subdomain such as `eu.openrouter.ai` does. Trailing dots and case are normalized for proxy-auth matching (`crates/c3/src/http_engine/mod.rs:150-175`). Empty auth-list entries are dropped (`:150-163`).
- **Auth-list ports:** `C3_HTTP_AUTH_PROXY=api.example.com:8443` does **not** match a base URL whose host is `api.example.com`: `HttpConfig::host()` excludes the port, while the auth-list parser keeps it (`:150-155`, `:319-335`). The list is documented as hosts, not authorities, so entries should omit ports; this is a silent key-mode fallback, not a port-aware match. For IPv6, use the URL-style bracketed host spelling if relying on exact matching; the host returned for a URL IPv6 literal is not normalized against an unbracketed list entry.
- **Unicode/IDN auth-list entry (finding below):** A URL containing `bücher.example` is canonicalized by URL parsing, while the auth list is only lowercased. Listing `bücher.example` can therefore fail to match the parsed punycode host and select key mode (`:150-155`, `:319-335`).
- **`NO_PROXY` IPv6 (finding below):** For a non-loopback IPv6 target, a bracketed entry such as `[2001:db8::1]` is mangled by the bracket parsing and fails to exclude the host (`:240-259`).
- **`NO_PROXY` ports (finding below):** The code removes a numeric port from an entry and never compares it with the target port. A port-qualified entry therefore excludes the host on every port (`:240-259`).
- **Loopback (finding below):** `localhost`, `127.*`, and `::1` are explicitly excluded from proxying, but an IPv4-mapped IPv6 loopback literal such as `[::ffff:127.0.0.1]` is not recognized (`:224-228`).
- **Empty or odd `NO_PROXY` entries:** Empty entries are ignored; `*` excludes all hosts. A malformed, nonnumeric port is not stripped and will not match as a host entry (`:232-260`).

## 2. Auth mode, key handling, and recorded state

With a stable environment, the key/proxy auth paths agree: proxy auth sets `key` to `None` without calling `resolve_key`; key auth resolves the configured environment variable (`:600-631`). `post` adds `Authorization` only when it receives a key (`:755-759`). `key_status` and precheck also avoid resolving a key in proxy mode (`:408-420`, `:1160-1185`). The request event records the selected auth mode, header names, and proxy decision; the ledger adds `auth: proxy` only in proxy mode (`:513-537`, `:658-679`).

The mode is recomputed at several points rather than snapshotted. If in-process code changes `C3_HTTP_AUTH_PROXY` during an attempt, the provider config, actual key selection, and event header names can disagree (`:533-537`, `:602-605`, `:674-675`). No such mutation is shown in this excerpt; snapshotting the mode once per attempt would make the consistency guarantee explicit.

A separate configuration mismatch is possible: `auth_mode()` does not require `env_proxy_applies()` to be true. For example, with `C3_HTTP_AUTH_PROXY=api.example.com` and no applicable proxy environment variable (or with that host in `NO_PROXY`), the attempt records `auth: proxy`, `proxy: false`, reads no key, and sends no Authorization header (`:326-335`, `:600-605`, `:661-679`). If credential attachment depends on the environment-selected ureq proxy, this is an unauthenticated direct request; require an applicable proxy or explicitly guarantee transparent egress credential injection.

The adapter refuses configured `Proxy-Authorization` headers: it only sets a configured header when `header_name_problem` accepts its name, and the tests cover `Proxy-Authorization` (`:760-765`, `:1608-1632`). The excerpt does not show where `check_key_host` is applied when an `HttpConfig` is constructed, so I cannot verify the unchanged key-to-host binding from this file alone.

## 3. Pack routing and redirects

The redirect refusal holds: the agent uses `redirects(0)`, and a 3xx response is recorded as a failure rather than followed (`crates/c3/src/http_engine/mod.rs:741-746`, `:772-798`). I found no redirect path that resends the pack to another host. The `NO_PROXY` defects above can nevertheless route a request through the environment proxy when an IPv6 exclusion or port-qualified exclusion was intended. The actual proxy endpoint is environment-selected; this excerpt does not establish whether ureq's environment-variable interpretation exactly matches the helper's decision.

---

### Findings

- **F07-1** [major] `crates/c3/src/http_engine/mod.rs:240`, `crates/c3/src/http_engine/mod.rs:257` - Bracketed IPv6 entries in NO_PROXY fail to exclude non-loopback IPv6 hosts, so the request can be sent through the proxy despite an explicit exclusion. Trigger: For example, with proxy settings selected, request `https://[2001:db8::1]` and `NO_PROXY=[2001:db8::1]` (or `[2001:db8::1]:443`). The parser removes the closing bracket even when no `]:` port delimiter was present; the resulting entry does not match the host. Evidence: read-code: The bracketed-entry branch splits on `]:` and unconditionally trims a trailing `]`; comparison then uses the still-bracketed host and its bracket-stripped form. Verify: Add a unit test calling `proxy_decision("https", "[2001:db8::1]", None, Some(proxy), None, Some("[2001:db8::1]"))` and assert it returns false. Remedy: Parse bracketed IPv6 entries without discarding the literal's closing bracket, and handle an optional port separately before comparing the normalized address.
- **F07-2** [major] `crates/c3/src/http_engine/mod.rs:240`, `crates/c3/src/http_engine/mod.rs:249`, `crates/c3/src/http_engine/mod.rs:257` - NO_PROXY port qualifiers are discarded rather than matched against the request port, so exclusions apply to the host on the wrong ports. Trigger: For `https://api.example:8443` with `NO_PROXY=api.example:443`, the helper strips `:443` and returns false (do not proxy), even though the port-qualified entry does not match the target port. The inverse mismatch also incorrectly bypasses the proxy. Evidence: read-code: The helper removes a numeric port and has no target-port input. The existing test also expects `example.test:8080` to exclude a target without that port. Verify: Add tests for matching and mismatching explicit target/NO_PROXY ports, including a target on 8443 with an entry on 443. Remedy: Pass the parsed URL port into `proxy_decision` and require a port-qualified NO_PROXY entry to match that port; retain host-only matching for entries without a port.
- **F07-3** [major] `crates/c3/src/http_engine/mod.rs:224`, `crates/c3/src/http_engine/mod.rs:226` - The loopback guard misses IPv4-mapped IPv6 loopback literals, allowing them to be proxied. Trigger: With a proxy selected, `proxy_decision("https", "[::ffff:127.0.0.1]", Some(proxy), None, None, None)` returns true; the same applies to the equivalent hexadecimal mapped address. Evidence: read-code: The guard recognizes `127.*`, `localhost`, and exactly `[::1]`/`::1`, but does not parse IP literals or recognize IPv4-mapped IPv6 loopback. Verify: Add a test asserting that `proxy_decision` returns false for `[::ffff:127.0.0.1]` and its canonical hexadecimal equivalent. Remedy: Parse the host as an IP address and classify IPv4-mapped IPv6 addresses by their embedded IPv4 address before selecting a proxy.
- **F07-4** [minor] `crates/c3/src/http_engine/mod.rs:150`, `crates/c3/src/http_engine/mod.rs:319`, `crates/c3/src/http_engine/mod.rs:330` - A Unicode IDN spelling in C3_HTTP_AUTH_PROXY can fail to select proxy auth for the same endpoint, causing key mode instead. Trigger: Set `C3_HTTP_AUTH_PROXY=bücher.example` and configure `base_url=https://bücher.example/v1`. URL parsing canonicalizes the endpoint hostname, but the list parser only lowercases its entry, so the host comparison can miss. Evidence: inferred: The URL host and environment list are normalized by different code paths; the list path contains no IDNA canonicalization. Verify: Add an auth-mode test using a Unicode IDN in both the base URL and C3_HTTP_AUTH_PROXY, and check that it selects the same mode as the equivalent punycode spelling. Remedy: Canonicalize each auth-list hostname using the same IDNA/URL hostname rules as `HttpConfig::host()`, or reject noncanonical entries explicitly instead of silently falling back to key mode.
- **F07-5** [major] `crates/c3/src/http_engine/mod.rs:326`, `crates/c3/src/http_engine/mod.rs:600`, `crates/c3/src/http_engine/mod.rs:661`, `crates/c3/src/http_engine/mod.rs:749` - Proxy auth can suppress the key even when this adapter is not using an environment-selected proxy, so the advertised credential-attaching proxy may never see the request. Trigger: Set `C3_HTTP_AUTH_PROXY=api.example.com` but leave `HTTPS_PROXY`, `ALL_PROXY`, and their lowercase forms unset, or put the endpoint in NO_PROXY. The attempt selects proxy auth and sends no Authorization header while `proxy` is false. Evidence: read-code: Auth selection and proxy selection are independent; no check requires an applicable proxy when proxy auth is selected. Verify: Add an attempt-level test with the host listed for proxy auth and no applicable proxy, and verify whether the request is rejected or intentionally supported through documented transparent egress. Remedy: If credential injection requires the ureq proxy, reject precheck when proxy auth is selected but `env_proxy_applies` is false; otherwise document and test the transparent-egress guarantee.

### Prior findings

_(none)_

## Verdict: HOLD

The proxy decision has concrete NO_PROXY and loopback gaps, and proxy-auth mode can be selected without an applicable proxy route.

### Blockers

_(none)_

### Unproven scenarios

- No commands or tests were run; the conclusions are from the supplied source excerpt.
- The excerpt does not establish whether ureq's `try_proxy_from_env` behavior exactly matches `env_proxy_applies` for every environment spelling and proxy setting.
- The excerpt does not show the caller-side enforcement of `c3_core::roster_ext::check_key_host`, so the full key-to-host binding cannot be verified here.
- Whether proxy auth is supported through transparent egress when `proxy` is false is not established by the supplied evidence.

### First-run checklist (observable)

_(none)_
