Write in English.

# Handoff 02 - claude: framing panel B - the event-driven watch, and a browser engine inside C3

Date: 2026-10-14. Base commit: C3 `v0.2.1` (8a133f8). C3 is a Rust bridge that lets a coordinating agent (Claude
Code first; Codex CLI and other hosts through the same skills) consult reviewer models through their CLIs
(engines: codex, agy for Gemini, muse for Meta's Muse Code, http for OpenRouter/API models) with a roster, a
findings store, detached runs and panels. Read `README.md`, `plugin/skills/coordinate/SKILL.md` (the waiting rule)
and `docs/port/wave8-quota.md`.

## Question

Two designs before implementation. (A) The WATCH: a coordinator session waits for workers, detached panels, builds
and peer sessions by a wake every 20-30 minutes that keeps its prompt cache warm; most wakes find nothing and still
cost a full cached read of a large context. We want an event-driven watch inside the bridge that answers "did
anything change since <instant>?" so a wake can be skipped or answered in one line without reading the context.
(B) A BROWSER ENGINE: reviewers that exist only as web chats (through the user's own Chrome tab, user-land
adapters) as a C3 engine beside codex/agy/muse/http - the product rules are already decided by the operator (below);
the panel is asked about the architecture inside C3.

## Task state

- Done: detached runs and panels with `--status` / `--wait`; the waiting rule in the coordinate skill (a wake about
  every 20 minutes via the host's scheduler, small reads at each wake, an idle rule after two quiet wakes); the
  SessionStart hook line.
- Open (A): nothing in the bridge knows whether a wake is useful; the host's scheduler (Claude Code: a cron-like
  prompt, `ScheduleWakeup`, a `Monitor` on a file) can only fire or not fire.
- Open (B): no engine for web-only reviewers; the operator's rulings (from a panel of 2026-10-14 in the plugin
  repository, ROADMAP.md "Browser engine"): nothing site-specific in the product; adapters compiled to WASM from
  Rust as the primary form, JavaScript allowed behind ONE deny-by-default host API; draft-only by default (the
  adapter prepares, the user sends); pacing is rate limiting, never stealth jitter; a signed hash/pointer catalogue
  of adapters; a private plugin first; the public usefulness table ranks per adapter and vendor.
- Not in scope: the host's scheduler itself; any site-specific adapter; the measurement designs (panel A).

## Evidence

- `crates/c3/src/consult/detach.rs`, `crates/c3-cli/src/main.rs` (`--status`, `--wait`) - the status files a
  watch could fold.
- `plugin/skills/coordinate/SKILL.md` - the waiting rule as written for Claude Code (ScheduleWakeup ~1200 s).
- `crates/c3/src/engines/{codex.rs,subprocess.rs}` - the engine contract today: a CLI spawned with a prompt on
  stdin, a structured reply by output schema, a transcript, liveness and kill records (the recovery chain that the
  acceptance rounds hardened: "no kill without a record").
- `~/gemini-mcp` (the operator's prior art, outside this repository): a Chrome extension plus a WebSocket relay that
  drove a web chat from an MCP server.

## The coordinator's sketch (to be challenged)

(A) `c3 watch --task <t> --since <instant> [--worker-state <path>...]`: reads the status files of detached runs,
the tails of the named worker STATE files, the ledger's newest entry and the health journal, and prints ONE line:
`nothing since <instant> (next known event: <detached run budget end / quota reset>)` or `changed: <what>` - exit
code 0 nothing, 3 changed. The host's wake then runs this command first (a few hundred tokens) and reads the context
only on exit 3; a Monitor on the status directory can replace the periodic wake entirely where the host has one. A
`--next` hint lets the coordinator schedule the next wake at the earliest known event instead of a fixed period.

(B) An engine `browser` with the same contract as the others (brief in, structured reply out, transcript, kill
record): C3 starts a local relay (WebSocket, loopback only, a per-run token), the Chrome extension connects, an
ADAPTER (WASM, loaded from the signed catalogue by hash) runs inside the extension with a host API of a few calls
(open the chat URL, put text in the composer, read the reply stream, done) that is deny-by-default per capability;
default mode draft-only: C3 writes the brief into the composer and the user presses send; `--send` lets the adapter
send when the user allowed it once for that adapter; pacing by a token bucket per vendor; the reply is parsed by
the output schema like the others; the usefulness table keys on adapter + vendor.

## Alternatives weighed

1. **(A) the watch command + `--next` hint (the sketch)** - small; the host still has to fire the wake.
2. **(A) a long-lived `c3 serve` that pushes events** (a local socket the host's Monitor can read) - no wasted
   wakes at all; a daemon to keep alive and secure.
3. **(B) WASM adapters in the extension (the sketch)** vs **(B') the adapter runs in C3 and the extension is a dumb
   DOM bridge** (the host API is then C3's, the extension is tiny and auditable) vs **(B'') a Chrome DevTools
   Protocol driver without an extension** (no user-land install; fragile, against the "user's own tab" ruling).

Current preference: (A) 1 now, 2 later; (B) B' - the extension as a dumb, signed DOM bridge; the adapters as WASM
in C3's process with the deny-by-default host API there.

## Questions

- **Q1.** (A) What should the watch fold, and what is the cheapest reliable "next known event" the coordinator can
  schedule against? Where does a skipped wake lose something (a cache that expires, a worker that died silently)?
- **Q2.** (B) Where should the adapter run - the extension (WASM), C3's process (B'), or elsewhere - given the
  rulings (deny-by-default host API, draft-only, signed catalogue)? What is the minimal host API?
- **Q3.** (B) What must the engine record for the recovery chain (liveness, kill, a reply that never comes because
  the user did not press send)?
- **Q4.** A risk or a design we did not list, for either.

Answer by number. Keep it under 600 words.
