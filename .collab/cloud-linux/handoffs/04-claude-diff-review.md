# Handoff 04 - Claude: the Linux port diff (`main..cloud/linux-port`)

Date: 2026-10-08. Base commit: `44defbf` (main); head: the branch `cloud/linux-port`
(`b114241` the Linux port, `de24304` the http engine behind a proxy, `6d721e7` CI, plus the
docs commit). The diff is in the pack: focus files below, periphery from the repository.

## Question

Does this diff change Windows behaviour anywhere, and does the new header-less auth mode of the
http engine open a way to send a key to the wrong host or to skip a guard? Review it as an
adversarial diff review before it lands on `main`.

## Delta since the last review

Follows: `handoffs/02-http-reply.md` / `03-http-reply.md` (the proxy auth mode only, from the
brief `brief.md`). This review covers the whole branch:

- `crates/c3-core/src/{health,store}.rs`, `crates/c3/src/cli/index.rs`: the Unix locks take
  std's `File::try_lock` (flock); the fs4 dependency is gone; the health lock keeps its file on
  non-Windows (flock on a kept file instead of `create_new`).
- `crates/c3/src/liveness/proc.rs`: the `/proc` arm reads a real start time
  (`/proc/<pid>/stat` field 22 + `btime`, `USER_HZ` 100) in the Windows `o`-string shape; a
  zombie counts as gone; `descendants_of(pid)` walks the process table.
- `crates/c3/src/engines/subprocess.rs`, `crates/c3/src/panel/run.rs`: the non-Windows tree
  kill kills the descendants before the child.
- `crates/c3/src/consult/detach.rs`: the background child gets its own process group (Unix).
- `crates/c3/src/index/embed.rs`: an IP-literal host is taken as parsed, not resolved.
- `crates/c3/src/http_engine/mod.rs`, `crates/c3/src/consult/http.rs`,
  `crates/c3/src/consult/orchestrate.rs`: `try_proxy_from_env` gated by `env_proxy_applies`;
  `C3_HTTP_AUTH_PROXY` (no `Authorization` header, no key, `auth: proxy`); `C3_HTTP_CA_BUNDLE`
  (extra trust anchors added to the bundled roots); the key-to-host binding is skipped only for
  a host the listing variable names.
- Tests gated `#[cfg(windows)]`: the cmd.exe launcher tests of `tests/pending_liveness.rs`
  and `codex_rule_matches_the_plugin`; `tests/http_engine.rs` gained a mock on 127.0.0.2.
- `.github/workflows/ci.yml`: fmt, clippy (both feature sets), tests on ubuntu and windows.

## CURRENT invariants claimed

- Every Windows code path is byte-for-byte what it was: each change sits under
  `#[cfg(not(windows))]` / `#[cfg(unix)]`, or is reached only when a new environment variable
  (`C3_HTTP_AUTH_PROXY`, `C3_HTTP_CA_BUNDLE`, the proxy variables) is set.
- A key value is never printed, logged, stored or sent anywhere but to its bound host; in the
  proxy auth mode no key is read at all; the refusal of a `Proxy-Authorization` header stays.
- `redirects(0)` holds through a proxy; loopback hosts are never proxied.
- The `.collab` files a Linux C3 writes are the same bytes a Windows C3 writes.
- The lock discipline (task lock, write lock, health lock, index lock) gives the same
  exclusion between cooperating processes on Linux as the share modes give on Windows.

## Changed files

| File | Change |
|---|---|
| `crates/c3/src/http_engine/mod.rs` | proxy decision, auth mode, CA bundle, events fields |
| `crates/c3/src/consult/http.rs` | billing guard passes a proxy-auth host without a key |
| `crates/c3/src/consult/orchestrate.rs` | key-to-host binding skipped for a proxy-auth host |
| `crates/c3/src/liveness/proc.rs` | `/proc` start time, `descendants_of` |
| `crates/c3/src/engines/subprocess.rs` | descendant kill on non-Windows |
| `crates/c3/src/consult/detach.rs` | `process_group(0)` |
| `crates/c3-core/src/health.rs` | flock on a kept file, Windows-only `remove_file` |
| `crates/c3-core/src/store.rs` | std `try_lock` |
| `crates/c3/src/index/embed.rs` | IP literal taken as parsed |

## Open findings

From `c3 findings --task cloud-linux --list`: see the ledger of runs 2-3 (the proxy auth
mode); each is answered in `state.md`.

## What I need from you

1. Any place where the diff changes what a Windows build does (an attribute that is not a
   `cfg`, a shared helper whose behaviour moved, a test whose body changed).
2. Any input that makes the proxy auth mode or the proxy decision unsafe: a host list that
   matches too much, a URL whose host differs from what the key binding saw, a `NO_PROXY`
   entry that is mis-parsed, a bundle refusal that echoes file contents.
3. A Linux-specific hole in liveness or the locks: a pid reuse the `/proc` start time does
   not catch, a descendant the tree kill misses, a waiter that can hold the health lock
   together with another process.

Give the exact input, the function and the fix; say plainly when a rule holds.
