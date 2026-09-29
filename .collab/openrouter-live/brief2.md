# Review: the reply normaliser

## What you are reviewing

`crates/c3/src/consult/ingest.rs` — the function `normalise_reply` and its helpers. C3 asks a
reviewer model for ONE JSON object that follows a schema (`schema_version`, `reply_markdown`,
`verdict`, `verdict_reason`, `findings[]`, `prior_findings[]`, `unproven[]`; every finding has
`severity`, `locations[]`, `claim`, `trigger`, `evidence[]`, `verification`, `remedy`,
`supersedes[]`). Reviewers reached over a plain HTTP API cannot be forced to follow the schema, and
in the first live run two good reviews were thrown away for small deviations: one model wrote
`evidence` as a single object instead of an array, the other put raw line breaks inside JSON
strings.

`normalise_reply` runs ONLY after the strict parse has failed. It is meant to be deterministic and
to invent nothing:

1. strip a Markdown code fence around the whole reply;
2. take the outermost JSON object when prose surrounds it;
3. escape raw control characters that occur inside string literals;
4. wrap a single object or string in an array where the schema wants an array; `null` becomes `[]`;
5. a missing `schema_version` becomes `"1"`.

Then the strict validation runs again; if it fails, the reply stays invalid.

## What I need from you

1. **Corruption**: an input for which the normaliser changes the MEANING of a reply — for example
   the outermost-object step picking the wrong braces (a `{` inside a string, inside the prose
   before the object, or inside a code block of `reply_markdown`), or the control-character step
   mis-tracking whether it is inside a string (escaped quotes, a backslash before a quote, a
   `"`).
2. **Acceptance of garbage**: an input that is not a review at all and comes out as a valid
   structured reply.
3. **Non-determinism or non-idempotence**: the same input giving different output, or
   `normalise(normalise(x)) != normalise(x)`.
4. **Cost**: an input of a few megabytes that makes it quadratic or worse.

For every finding give the exact input (short), what the code does with it (name the function and
the line), and the fix. Say plainly when a step is correct. Do not report style.
