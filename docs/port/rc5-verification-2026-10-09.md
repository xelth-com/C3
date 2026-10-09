# RC5 verification, 2026-10-09

Repository `C:\Users\Dmytro\C3`, branch main at 97f85d0 (code at fd10f81, every wave of the parity track merged).
Main checkout, `CARGO_TARGET_DIR=C:\Users\Dmytro\C3\target`, `-j 2`, stale sources touched first.
Binary: `c3 --version` = `c3 0.1.0`; the binary strings contain `kill_unconfirmed` and `forget-pending`; copy at
`%TEMP%\c3-main-fd10f81\c3.exe` (used as `C3_EXE`). Plugin tree staged from tag v0.6.1 (a688de1) under
`%TEMP%\c3-main-fd10f81\scripts`, the six shims from `tests/shim/` over the plugin scripts. Windows PowerShell 5.1
(5.1.26100.9444), `PSModulePath` reset to the 5.1 default, one harness at a time under `HARNESS.lock`
(waited on it, never deleted), run through `tests/run-all.ps1 -ScriptsDir ... -Only <h>` (test mode, telemetry off).
Harness checkout at HEAD bd7bf84 (`tests/` identical to v0.6.1).

## Cargo

| step | result |
|---|---|
| `cargo build -j 2` | ok (1 linker warning in c3-cli) |
| `cargo test --workspace -j 2 --no-fail-fast` | 716 passed, 0 failed (first run, no rerun needed) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |

Test suites (passed/failed): c3 lib 391/0, exit_codes 1/0, http_engine 21/0, index_embeddings 5/0, index_federation 6/0,
index_integration 4/0, pending_liveness 3/0, recovery_wave3a 1/0, router 12/0, scrub_markers 1/0, telemetry 28/0,
telemetry_peers 1/0, c3 bin 0/0, claude_engine 12/0, compat_wave2b 17/0, kill_record_durable 2/0, mcp_integration 1/0,
notspooled_parity 18/0, panel_light_required 1/0, telemetry_parity 7/0, c3-core lib 143/0, contracts 8/0, formats 26/0,
store 7/0, doc-tests 0/0 (both crates). The `index::embed` tests passed first time.

## Harnesses (22, run-all order of the brief)

"differs" = differs from the expectation of the waves' reports.

| harness | pass/fail | exit | wall | failing checks | differs |
|---|---|---|---|---|---|
| lock2 | 11/0 | 0 | 27 s | | no |
| roster | 125/0 | 0 | 369 s | | no |
| panel | 62/0 | 0 | 348 s | | no |
| fixes | 54/2 first run; 56/0 on the rerun | 1; 0 | 185 s; 162 s | first run: F04-10 "survivor gone -> run proceeds", F04-10 "timeout: tree killed..." (the live Codex app processes of another session were on the machine; the rerun was clean) | no (after the rerun) |
| fixes28e | 63/2 | 1 | 113 s | RECORD E1; RECORD E18, E23 | no |
| fixes28d | 25/2, no summary line (stops after the second failure) | 1 | 29 s | MARKER D2; LOCK D3 | no |
| pending | 26/0 | 0 | 105 s | | no |
| detach | 50/1 | 1 | 294 s | CARRY F11-2 | no |
| telemetry | no result | timeout | >1200 s | run-all buffers the output and it was lost when the 20-minute limit killed it (see the direct rerun below) | UNKNOWN (expected 47/96) |
| claude | 87/0 | 0 | 825 s | | no |
| 0.3 | 229/0 | 0 | 765 s | | no |
| engines | 97/0 | 0 | 566 s | | no |
| muse | 73/1 | 1 | 774 s | UNIT D2 (every engine) | no |
| companions | 42/0 | 0 | 216 s | | no |
| visibility | 34 PASS, 0 FAIL, crashes | 1 | 15 s | "Invoke-EngineTurn" not found at harness-visibility.ps1:510 (extraction crash; no FAIL row) | no |
| host | 56/9 | 1 | 199 s | REFUSE D3; WARN D3; WARN D3/F04-10; WARN wave 27b; ENV D4/wave 27b; ENV D4 (detached); ENV D3/D4 (panel); PREFIX D6 (refused prefixes); PREFIX D6 (default stays claude) | YES: one fewer failure than the expected 55/10 (which one passes now is not known from the brief) |
| format | 37/0 | 0 | 236 s | | no |
| 3b | 12/0 | 0 | 16 s | | no |
| fixes26b | 50/1 first run; 51/0 on the rerun | 1; 0 | 599 s; 240 s | first run: STALLTOOL 26c D3 (F25-1), the 5 s silent tool call was cut at 20.3 s (load-sensitive timing; the rerun was clean) | no (after the rerun) |
| fixes27c | 34/2 | 1 | 278 s | POINTER D15 (F32-11); ZCODE D20/D21 | no |
| fixes28b | 20/0 | 0 | 117 s | | no |
| fixes28c | 14/1 | 1 | 30 s | IDENTITY D8 | no |

## Conclusions

1. Cargo is green: 716 passed / 0 failed, clippy and fmt clean; the binary is main's (kill_unconfirmed, forget-pending present).
2. By design (P7/P8) or shim artifacts: fixes28e RECORD E1/E18,E23, fixes28d MARKER D2/LOCK D3, fixes28c IDENTITY D8, muse UNIT D2, fixes27c POINTER D15, detach CARRY F11-2 and the telemetry rows read the plugin's own source or files and match the expectation exactly.
3. Not applicable (single Claude Code host): fixes27c ZCODE D20/D21 and the host rows that expect another host (WARN D3 x2, WARN wave 27b, ENV D4/27b, ENV D4 detached, ENV D3/D4 panel).
4. Still open / worth a look: harness-host is 56/9 instead of 55/10 (identify the check that now passes), and PREFIX D6 (host) reported "Es wurde kein Parameter ..." from the shim (the `codex-consult.ps1` shim seems not to take `-BriefPrefix`; REFUSE D3 also fails) - confirm that these are in the expected ten.
5. Timing-sensitive: fixes F04-10 and fixes26b STALLTOOL failed once under load / with live Codex processes of another session and passed on the rerun; harness-telemetry exceeded 20 minutes under run-all (output lost), so its 47/96 expectation is unconfirmed here.
