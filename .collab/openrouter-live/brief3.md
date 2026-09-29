# Review: the second request of the http engine

## What you are reviewing

`crates/c3/src/consult/http.rs` — the function `drive_seat_turns` and what it calls. C3 sends a
review request to an OpenAI-compatible HTTPS endpoint: a system message (the reply contract) and a
user message (a sanitised pack of the repository). Every token is billed to the user's API key.

Since today a second request may follow the first:

- **Format repair**: the first reply is not a valid structured reply, even after a local
  normaliser, but it is substantive prose. One more request is sent: system, the pack, the first
  reply as the assistant's message, and a convert-only prompt. The first reply is kept on disk.
- **Retry**: the first request failed with the class `unavailable` (a timeout, a 5xx, an
  overloaded provider). After a pause (`retry_after` from the provider, else 20 s, never more than
  120 s) the same request is sent once more. Never after `auth`, `quota`, or a `burst` whose
  `retry_after` is above 120 s. The flag `--no-continue` suppresses it.

## What I need from you

1. **Money**: a sequence of provider answers that makes C3 send MORE than two requests for one
   consultation, or send the second request when the rules above say it must not (after `auth`,
   after `quota`, with `--no-continue`, after a long `burst`).
2. **The wrong record**: a sequence after which the ledger or the files say something that did not
   happen — a usable reply recorded as failed or the reverse, `engine_turns` wrong, the first reply
   lost, the second reply recorded under the first one's name.
3. **The pause**: an answer of the provider that makes the pause negative, zero when it must not
   be, or longer than 120 s (a huge, negative, fractional or non-numeric `retry_after`; an HTTP
   date instead of seconds).
4. **Both at once**: a retry followed by a format repair, or a format repair whose own request
   fails with `unavailable`. Say what the code does and whether that is within two requests.

For every finding give the exact sequence of provider answers, what the code does with it (name
the function and the line), and the fix. Say plainly when a rule holds. Do not report style.
