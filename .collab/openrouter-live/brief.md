# Security review: the key-to-host binding of the http engine

## What you are reviewing

`crates/c3-core/src/roster_ext.rs` — the validator of C3's API reviewers. C3 can send a review
request to an OpenAI-compatible HTTPS endpoint. The API key is never stored: a reviewer entry names
the ENVIRONMENT VARIABLE that holds it (`key_env`) and the endpoint (`base_url`). At run time C3
reads that variable and sends its value as `Authorization: Bearer <value>` to that endpoint.

That makes the pair (`key_env`, `base_url`) dangerous: whoever controls it can make C3 send ANY
secret of the user's environment to ANY host. The entry comes from a roster file in the user's
home directory or from the command-line flags `--key-env` / `--base-url`, and both may be written
by an AI coding agent that was manipulated by a prompt injection.

## The rules the code is meant to enforce

1. A known key goes only to its own provider's host (exact host or a subdomain of it), e.g.
   `OPENAI_API_KEY` only to `api.openai.com`.
2. Any other endpoint needs a variable the user created for C3: its name starts with `C3_KEY_`.
3. Every other variable name is refused.
4. `base_url` is `https` only, with a host, without userinfo, query or fragment.
5. Extra request headers: no reserved names (`Authorization`, `Cookie`, `Host`, ...), no CR or LF.

## What I need from you

Find concrete ways to defeat these rules, in this order of interest:

1. **Exfiltration**: an input (roster entry or flags) that passes validation and makes C3 send a
   secret it should not send, or send a known key to a host that is not the provider's. Think of
   host parsing (trailing dots, case, IDNA and punycode look-alikes, IP literals, ports, percent
   encoding, backslashes), of the subdomain rule, and of the `C3_KEY_` escape hatch.
2. **Header injection or smuggling** through header names or values.
3. **Anything the validator accepts that the HTTP client will interpret differently** from how
   the validator read it.
4. Rules that are correct but whose refusal message would leak a secret VALUE.

For every finding give the exact input, what the code does with it (name the function), and the
fix. Say plainly when a rule holds — a verified "this cannot be bypassed because ..." is as useful
to me as a finding. Do not report style. Do not speculate about code you cannot see: the file is
complete, the HTTP client is `ureq` 2 with redirects disabled.
