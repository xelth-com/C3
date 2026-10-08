# Review: the proxy auth mode of the http engine (Linux port, step 2)

## What you are reviewing

`crates/c3/src/http_engine/mod.rs` — C3's OpenAI-compatible reviewer adapter, as changed by the
Linux port. Two additions:

1. The `ureq` agent now honours the environment's proxy (`HTTPS_PROXY` / `HTTP_PROXY` /
   `ALL_PROXY`, minus `NO_PROXY`) through `try_proxy_from_env`, decided per request by
   `env_proxy_applies` / `proxy_decision`; the request event in `<stem>.events.jsonl` records
   `proxy: true|false`.
2. A second auth mode: when the environment variable `C3_HTTP_AUTH_PROXY` lists the endpoint's
   host (comma-separated; exact or a subdomain), C3 sends NO `Authorization` header and reads NO
   key (`HttpAuth::Proxy`, `HttpConfig::auth_mode`), because a sandbox egress proxy attaches the
   credential itself. The ledger's `provider_config` and the request event carry `auth: proxy`.
   For every other host the key-to-host binding (`c3_core::roster_ext::check_key_host`) is
   unchanged.

## The rules the code is meant to keep

- A key value is never printed, logged, stored or sent anywhere but to its bound host.
- A user-supplied `Proxy-Authorization` header is refused (`roster_ext::header_name_problem`).
- The proxy auth mode must not become a way to send a key to the wrong host, nor a way for an
  agent-writable file (roster, flags) to switch a host into the header-less mode: only the
  environment variable, created by hand, decides.
- `NO_PROXY` and loopback hosts are never proxied.

## What I need from you

1. Any input (URL, host spelling, environment value) that makes `host_uses_proxy_auth` or
   `proxy_decision` answer wrongly: a look-alike host, a trailing dot, a port, an IP literal, an
   empty or odd list entry.
2. Any path where the key mode and the proxy mode disagree with each other (events, ledger,
   dry-run status, precheck), or where the proxy mode still reads or echoes a key.
3. Whether the `ureq` proxy use can leak the pack to an unintended host (the redirect refusal
   `redirects(0)` stays in place).

Give the exact input, the function, and the fix; say plainly when a rule holds. No style remarks.
