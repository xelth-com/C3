# The Linux port (2026-10-08)

Until this port every build and test of C3 ran on Windows 11; the non-Windows arms of the
runtime were marked "best effort" and had never been compiled. This note records what the first
Linux build found, what was changed, what is gated, how the `http` engine works behind a
sandbox proxy, and what the live check saw. The branch is `cloud/linux-port`; the CI workflow
(`.github/workflows/ci.yml`) now runs on `ubuntu-latest` and `windows-latest`.

Environment of the port: Ubuntu x86_64, 4 vCPU, 16 GB, rustc 1.97, no direct network (all
egress through an HTTP CONNECT proxy that re-terminates TLS with its own CA), no IPv6.

## 1. What failed on Linux and how it was fixed

| where | what the first build or test run said | fix |
|---|---|---|
| `c3-core/src/{health,store}.rs`, `c3/src/cli/index.rs` | `unresolved import fs4::fs_std` — fs4 "1" resolved to 1.1.0, whose API has `fs4::FileExt::try_lock() -> Result<(), TryLockError>`; the code was written for an older `try_lock_exclusive() -> Result<bool>` | the Unix locks take std's `File::try_lock` (stable since Rust 1.89; `flock(LOCK_EX \| LOCK_NB)` on Linux). The fs4 dependency is removed from both crates — Windows never used it. |
| `c3-core/src/health.rs` (machine health lock) | compiled, but the Unix arm used `create_new`: a writer killed mid-update would have left a `.lock` that blocks every later update for the whole budget | the lock is a flock on a file that is created once and kept; a crash releases it with the handle. The `remove_file` after the update stays Windows-only (with flock the file's name is the lock, removing it would let a waiter on the old inode and a newcomer hold it at once). |
| `c3/src/liveness/proc.rs` (`/proc` arm) | `recorded_start_time_reads_back_for_this_process` fails: the arm returned a blank start time for every live pid, so a wrong recorded start time still read as alive | the start time is read from `/proc/<pid>/stat` field 22 (ticks since boot, `USER_HZ` 100) plus `btime` of `/proc/stat` and formatted like the Windows `o` string; a zombie counts as gone, like a Windows process with an exit time. The sub-second tolerance of `same_start_time` on non-Windows is unchanged. |
| `c3/src/engines/subprocess.rs`, `c3/src/panel/run.rs` | compiled; `kill_tree` killed only the direct child (no `taskkill /T`), so a launcher's own children would outlive a timeout as orphans | the descendants are read from the process table (`liveness::proc::descendants_of`, parent-pid walk) and killed before the child; the pid-only path (`kill_tree_by_pid`) does the same. |
| `c3/src/consult/detach.rs` | compiled; the background child shared the foreground's process group, so a group signal (Ctrl-C) aimed at the foreground would have reached it | `process_group(0)` on the spawn (the Unix counterpart of `CREATE_NEW_PROCESS_GROUP`). |
| `c3/src/index/embed.rs` | `loopback_urls_accepted` fails on `http://[::1]:11434/...`: the bracketed host literal the `url` crate hands back went to the resolver, and a sandbox without IPv6 cannot resolve `::1` at all | an IP literal is taken as parsed; only a domain name goes to the resolver (both in the validator and in the pinned resolver of the request). |
| `c3/tests/pending_liveness.rs` | the `.cmd` launcher helpers and their imports are dead code outside Windows (clippy `-D warnings` fails) | the two spawn/kill tests, their helpers and imports are `#[cfg(windows)]` with a reason; the start-time test runs everywhere. |
| `c3/src/liveness/proc.rs` (`codex_rule_matches_the_plugin`) | the recorded launcher paths in the test are Windows paths (`C:\tools\zcode.cmd`); `std::path` splits `\` only on Windows, so the "name of the recorded launcher" rule reads `C:\tools\zcode` as the stem on Linux | gated `#[cfg(windows)]` with the reason; the function is unchanged (the plugin's `GetFileNameWithoutExtension` behaves the same way on Linux PowerShell). |

Everything else compiled and passed as written: the PowerShell 5.1 JSON formatter (`ps_json`)
and its byte-for-byte fixtures, the store lock tests (flock gives the same exclusion as
`FileShare.Read` for a second read-write open), the MCP stdio session, the symlink containment
(a real symlink on Linux, a junction on Windows), the router, telemetry and pack suites.

Every Windows test body is unchanged; the gates add an attribute and a one-line reason.

## 2. What differs on Linux

- **Process liveness**: `/proc/<pid>/stat` instead of `OpenProcess` + `GetProcessTimes`; the
  recorded start time has the same shape (`yyyy-MM-ddTHH:mm:ss.fffffffZ`) and `same_start_time`
  keeps the plugin's one-second tolerance on non-Windows. Without `/proc` (macOS) the start
  time is blank and only existence is checked (`kill -0`) — macOS is untested.
- **Tree kill**: descendants from the process table plus `kill -9`, instead of `taskkill /F /T`.
  A grandchild that re-parents between the table read and the kill can escape; `taskkill /T`
  has the same window.
- **Locks**: advisory `flock` instead of share modes. A process that does not take the lock can
  still read and write the file (the plugin's `FileShare.Read` would refuse a writer); every
  C3 and plugin path takes the lock first, so the discipline holds between cooperating tools.
- **Detached runs**: a direct child of the foreground with stdio redirected to the `.log`, in
  its own process group, instead of a `cmd.exe /c` launch with no inherited handles.
- **Host name**: `COMPUTERNAME` then `HOSTNAME`; a Linux shell that does not export `HOSTNAME`
  records an empty host (open question, see §6).
- **Paths and bytes**: the files under `.collab/` are the same bytes (CRLF where the plugin
  writes CRLF, repo-relative POSIX paths everywhere), so a Linux C3 and a Windows C3 or the
  PowerShell plugin read one history.

## 3. The `http` engine behind a proxy

Three environment variables, all read at run time and none of them a roster field or a flag
(an agent-writable file cannot switch them on):

| variable | meaning | recorded as |
|---|---|---|
| `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` (either case), `NO_PROXY` | the `ureq` agent is built with `try_proxy_from_env` when a proxy applies to the request URL: `ALL_PROXY` or the scheme's variable, unless `NO_PROXY` lists the host (`*`, a host, a domain suffix with or without a leading dot, an optional port; no CIDR). Loopback is never proxied. | `proxy: true\|false` on the `request` event |
| `C3_HTTP_AUTH_PROXY=<host,...>` | for a listed host (exact, or a subdomain of a listed name) C3 sends **no `Authorization` header and reads no key**: the egress proxy attaches the credential. The dry run's `key` line says so. For every other host the key-to-host binding of `roster_ext::check_key_host` is unchanged; for a listed host there is no key to bind, so the binding check is skipped. | `auth: proxy` in the ledger's `reviewer.provider_config` and on the `request` event (`auth: key` on the event otherwise; the ledger field is absent in the key mode, as before) |
| `C3_HTTP_CA_BUNDLE=<pem file>` | extra trust anchors ADDED to the bundled Mozilla roots (ureq's rustls stack with the same `ring` provider), for a proxy that re-terminates TLS. A file that cannot be read, holds no certificate or an unusable one refuses the launch; the refusal names the variable and the path, never the contents. | `ca_bundle: true\|false` on the `request` event |

What stays: `redirects(0)` (a 3xx is never followed, through a proxy or not), the refusal of a
user-supplied `Proxy-Authorization` header, the scrub of every error string, and the rule that a
key is sent only to its bound host. Tests: `http_engine::tests` (host list, scheme/`NO_PROXY`
decision, auth mode, the CA bundle loader on a public root), `consult::http::tests` (the billing
guard in the proxy mode), and `tests/http_engine.rs` (a mock on the second loopback address sees
no `Authorization` header while a mock on the first still needs its key). No test touches the
network.

## 4. The live check (this sandbox)

Recorded under `.collab/cloud-linux/` (ledger, findings, handoffs with the exact packs and the
events files), summarised in `.collab/cloud-linux/state.md`.

LIVE-RESULTS

## 5. Roadmap lines

- Linux port: builds, lints and tests green on Ubuntu x86_64 with both feature sets; CI on
  ubuntu-latest and windows-latest (`.github/workflows/ci.yml`). Done 2026-10-08.
- `http` engine behind a proxy: `HTTPS_PROXY` honoured, `C3_HTTP_AUTH_PROXY` (credential
  attached by the proxy), `C3_HTTP_CA_BUNDLE` (extra trust anchors). Done 2026-10-08.
- Live check through the sandbox proxy on free OpenRouter models: see §4.
- Open: macOS (no `/proc`; blank start times), `SSL_CERT_FILE` as a second source of trust
  anchors, `NO_PROXY` CIDR ranges, the host name on Linux shells without `HOSTNAME`.

The `.eck/ROADMAP.md` the workspace rules name is not tracked in this repository (`.eck/` is
ignored), so these lines live here until the owner moves them.

## 6. Open questions for the owner

1. `.eck/ROADMAP.md` and `TECH_DEBT.md` are not in the clone; where should the roadmap lines go?
2. The fs4 dependency is gone (std's `File::try_lock` covers it). Keep it that way?
3. Should the `http` engine also read `SSL_CERT_FILE` (the convention most tools follow), or
   stay with the explicit `C3_HTTP_CA_BUNDLE` only?
4. On Linux a shell often does not export `HOSTNAME`; should C3 read `/etc/hostname` as a third
   source for the `host` of detached and recovery records?
5. The cloud session's proxy puts `index.crates.io` on its `no_proxy` list while there is no
   direct DNS, so `cargo` hangs on the index until the registry hosts are routed through the
   proxy (`NO_PROXY=localhost,127.0.0.1 cargo fetch`). A setup script for the environment could
   do that once.
