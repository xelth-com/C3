# Handoff 05 - Grok (grok): cross-model-tests-grok

Date: 2026-10-29 17:10 local. Author: Grok (grok) (model grok-4.7, effort high), grok-cli 1.0.70.
Reviewer: grok :: grok-4.7 [grok] (provider from roster, model from roster; engine grok (C:\Users\Dmytro\.grok\bin\grok.exe); provider fingerprint 8c4f8ce6de16; harness grok-cli 1.0.70).
Preflight: ok: signed in (~/.grok/auth.json) - subscription (grok.com oauth).
Effort: high sent (requested high, mapping grok-v1, by caps-v1: engine:grok, any model; not confirmed by the provider). Consultation id: 202710a4-3f77-450f-9209-692b9e80d1a3.
Invocation: `codex-consult.ps1` (mode: new, sandbox: read-only, purpose: framing). Argv: `grok --prompt-file C:\Users\Dmytro\AppData\Local\Temp\codex-consult-prompt-a792f398f0514f3ba45234356cf8d1a4.txt --output-format streaming-json --json-schema {"$schema":"http://json-schema.org/draft-07/schema#","title":"codex-consult reply v1","type":"object","additionalProperties":false,"required":["schema_version","verdict","verdict_reason","reply_markdown","findings","prior_findings","unproven","first_run_checklist"],"properties":{"schema_version":{"type":"string","enum":["1"]},"verdict":{"type":"string","enum":["ACCEPT","HOLD","REJECT","ADVISE"]},"verdict_reason":{"type":"string"},"reply_markdown":{"type":"string"},"findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["severity","locations","claim","trigger","evidence","verification","remedy","supersedes"],"properties":{"severity":{"type":"string","enum":["blocker","major","minor","note"]},"locations":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["path","line"],"properties":{"path":{"type":"string"},"line":{"type":["integer","null"]}}}},"claim":{"type":"string"},"trigger":{"type":"string"},"evidence":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["kind","reference","observation"],"properties":{"kind":{"type":"string","enum":["read-code","ran-command","inferred","assumed"]},"reference":{"type":"string"},"observation":{"type":"string"}}}},"verification":{"type":"string"},"remedy":{"type":"string"},"supersedes":{"type":"array","items":{"type":"string"}}}}},"prior_findings":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","status","note"],"properties":{"id":{"type":"string"},"status":{"type":"string","enum":["fixed","still-open","not-checked","unknown-id"]},"note":{"type":"string"}}}},"unproven":{"type":"array","items":{"type":"string"}},"first_run_checklist":{"type":"array","items":{"type":"string"}}}} -m grok-4.7 --reasoning-effort high --disable-web-search --tools read_file,list_dir,grep --disallowed-tools use_tool,search_tool,write,run_terminal_command --permission-mode dontAsk --no-subagents` (prompt from a file: --prompt-file).
Parent thread: (none - new thread). Result thread: `01a1886c-66f8-7fb3-a067-e03ea8659e56` (source: events).
Brief: `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md` (sha256 57c053d8e8cc). Reviewed: 0b712c4, base 0b712c46cf717bdb90de5e0228b720d17388cfb9, tree sha256 fbb90a090f31, 0 changed files.
Bridge outcome: usable reply. Wall time: 81.8 s. Tokens: in 7245 (cached 3072), out 4980, reasoning 171.
Engine turns: 1 (each one a SuperGrok subscription prompt).
Warnings: routing: no lab known for #14 grok :: grok-4.7 [grok] (its model id has no known vendor prefix) - it counts as a lab of its own; give its roster entry a "lab".
Timeout: 1800 s (the default of purpose framing); continuation after a timeout kill: up to 900 s.
Verdict: ADVISE - The brief's cross-model comparison is answerable from the handoff and run artifacts, but the framing understates process confounds that should be fixed before anyone treats the scores as a model ranking. Findings: 0 blocker, 3 major, 1 minor, 1 note (F05-1..F05-5, tracked in `findings.json`). Structured reply: `handoffs/05-grok-cross-model-tests-grok.reply.json`.
Raw event stream: `handoffs/05-grok-cross-model-tests-grok.events.jsonl`.
Verbatim reply follows.

---

## Framing challenge

The brief treats six runs as a model bake-off (Claude coordinator vs Grok vs GLM vs MiMo). The artifacts support a process comparison more than a model ranking. Four confounds sit outside the listed options:

1. **Coordinator contamination.** F01, F03, and F04 were written by the Claude coordinator after the worker returned. Worker-vs-worker scores mix model output with coordinator repair.
2. **Unequal task graphs.** Grok got one combined 19-file chunk (grok-1, ~1364s). GLM and MiMo got the same work split into two bounded chunks (~240–639s). Latency and “files touched” are not comparable until the split is held fixed.
3. **Fixtures, not verdicts.** `verdict` is a required output field. Calling a correct `ADVISE` a fixture miss punishes schema compliance. The real miss is only where the brief asked for a different verdict.
4. **Missing arms.** No Claude-as-worker on the same brief, no repeat of any arm, no swap of chunk boundaries. One run per cell cannot separate model from prompt, split, or repair.

To choose among options I would need: the worker prompt actually sent (not only the handoff), whether coordinator edits are in the scored files, token and cost logs, and one repeated arm with the chunk boundary held fixed.

## Q1

**Mostly (b), with a real (a) on the two-file jobs.** All four worker files exist and are schema-valid JSON (`schema_version` "1", required keys, `verdict` ADVISE). None is prose-only, so the failure mode the preamble feared did not occur.

The graded miss is structural, and it is concentrated:

- grok-1 (19 files, one chunk) has `findings: []` and `locations` nowhere. Q2–Q3 evidence is generic “Checked all 19”.
- glm-2, mimo-1, and mimo-2 put findings on disk, but every `locations` array is `[]`. Claims name files in prose only.
- glm-1 is the exception: four findings, each with `{path, line}` and `read-code` evidence.

So (c) is too strong. (a) fits glm-1 and, loosely, the smaller jobs that did emit findings. (b) fits the pack: the contract’s locatable-finding rule failed whenever the chunk was large or the model was MiMo.

## Q2

**Partial.** Every question has a section. Answers are usually one line plus a label (“(b)”, “partial”), not the brief’s “claim plus one concrete reason.”

| Run | What is thin |
| grok-1 | Q2–Q3 assert all 19 files with no per-file citation. Q5’s “None” does not walk the four smallest files. |
| glm-1 | Q2–Q3 name the two files but give no size, form, or line. |
| glm-2 | Q3 says “pass” with no file-level reason. Q5 is “No” with no per-file note. |
| mimo-1 | Q3 is “all JSON” with no count. Q6’s “lower” gives no comparison pair. |
| mimo-2 | Q2–Q3 and Q5 are single words (“varied”, “pass”, “no”). |

## Q3

**No for the three larger-chunk runs; yes only inside glm-1.** The two-file jobs have no prior findings, so an empty `prior_findings` is correct there. On the 17-file jobs the handoff asks the model to use the ids. Observed `prior_findings`:

- glm-2: `[]`. It mentions F05 in Q6 prose only.
- mimo-1 and mimo-2: `[]`. mimo-2’s Q6 says “F04 still open” and never emits the id in the array.

glm-1’s `[]` is correct: F01–F04 are not in scope for `f-astra` and `f-workers`. “Use the ids” was an instruction for the chunk that contained the open findings, not a requirement to invent ids on the two-file chunk.

## Q4

**Three of five workers did. grok-1 and glm-2 did not.**

- glm-1: “Both files contain the literal placeholder `PENDING`, so neither names a concrete owner model.” That satisfies the “name the placeholder” rule. The other option — quote the sentence that states the owner — does not apply, because the owner is absent.
- mimo-1: quotes “Model chosen: not recorded.” and “ reviewer is named only as deepseek-v4-pro.”
- mimo-2: quotes “Panel: claude, gpt-5.5, deepseek-v4-pro, gemini-3.1.”
- grok-1: “No owner model or panel is named” — the conclusion, without the placeholder or a quote.
- glm-2: “not recorded here” — paraphrase, no quote.

## Q5

**Yes.** All five files pretty-print as one JSON object, contain every required key, and set `verdict` to `ADVISE`. `prior_findings: []` on glm-1 is schema-valid and, for that chunk, correct (see Q3). No file is prose-wrapped or missing `schema_version`.

## Q6

**Do not conclude a model ranking.** What the artifacts support:

1. **Split work before sending it.** grok-1 took 1364.7s, returned `num_turns: 1`, left `findings` empty, and wrote “Checked all 19” for Q2. GLM on the same files, split, took 638.6s + 494.1s and did produce findings. MiMo took 301.3s + 240.2s. The brief’s own “smaller chunks may change results” warning is the result.
2. **Ask for locations as data, not as a sentence.** glm-1 is the only run that filled `locations`. The other four either omitted findings or left `locations` empty while naming files in prose. A coordinator that accepts prose paths will not be able to attach defects to lines.
3. **Separate schema success from task success.** Every arm passed the JSON contract. The graded failures are empty locations, unused prior-finding ids, and answers that skip the required reason. Those are prompt/rubric gaps, visible on GLM and MiMo as well as Grok.
4. **Keep coordinator repair out of the model score.** If F01–F04 land in the corpus as worker-quality evidence, the next test should mark lines the coordinator wrote. Otherwise Claude-as-coordinator looks stronger because it was allowed a second pass.

## Requested checks

RC1. In `.collab/consilium-2026-10-29-test-workers`, read-only, `jq -e '.locations|length>0' outputs/glm-1.json outputs/glm-2.json outputs/mimo-1.json outputs/mimo-2.json outputs/grok-1.json` (budget: one minute). Settles finding #1: only glm-1 should exit 0 per file; the other four should fail.
RC2. Same directory, read-only, `jq -r '.prior_findings|length' outputs/glm-2.json outputs/mimo-1.json outputs/mimo-2.json` (budget: one minute). Settles finding #2: expect `0` three times while Q6 prose still names F04 or F05.
RC3. Same directory, read-only, confirm grok-1’s tool trace is absent: `rg -n "num_turns|1364" runs/grok-1.json` and note there is no per-tool log (budget: two minutes). A missing trace leaves finding #3 as inference, not a measured “did not open files.”

---

### Findings

- **F05-1** [major] `.collab/consilium-2026-10-29-test-workers/outputs/grok-1.json:8`, `.collab/consilium-2026-10-29-test-workers/outputs/glm-2.json:14`, `.collab/consilium-2026-10-29-test-workers/outputs/mimo-1.json:8`, `.collab/consilium-2026-10-29-test-workers/outputs/mimo-2.json:8` - Four of the five worker outputs leave every finding's locations array empty or omit findings entirely, so defects are not locatable at a path and line. Only glm-1 attaches {path, line} to each finding. Trigger: A coordinator tries to map a worker finding onto a source line after a chunk larger than the two-file glm-1 job, or after any MiMo chunk. Evidence: read-code: findings is []. Q2 and Q3 say “Checked all 19” with no per-file path or line.; read-code: Four findings; each locations entry has a path and a non-null line (15, 1, 19, 34).; read-code: Two findings, both with locations: []. File names appear only inside claim text.; read-code: One finding, locations: [].; read-code: One finding, locations: [].; read-code: The output contract says each defect goes in findings[] and each location is {path, line}. The task text says to cite the file and line for a content finding. Verify: Run jq over the five output files and count findings whose locations array is non-empty. Expect a non-zero count only in glm-1.json. Remedy: Score locatable findings separately from schema-valid JSON. Reject a chunk whose findings name a file only in prose. Keep the two-file chunk shape that produced glm-1's locations, and add an explicit 'locations must be non-empty when a path is known' line to the worker prompt.
- **F05-2** [major] `.collab/consilium-2026-10-29-test-workers/outputs/glm-2.json:36`, `.collab/consilium-2026-10-29-test-workers/outputs/mimo-1.json:24`, `.collab/consilium-2026-10-29-test-workers/outputs/mimo-2.json:24` - On the 17-file chunks, prior_findings is empty even though the handoff tells the model to use F01-F05 ids and the replies mention F04 or F05 in prose. Trigger: The worker chunk includes files the coordinator already marked F01-F05, and the next pass needs those ids in prior_findings rather than in the Markdown answer. Evidence: read-code: Q3 says: Use the ids. Do not re-report a finding you were given as prior.; read-code: prior_findings is []. Q6 prose says “F05 is still open”.; read-code: prior_findings is []. Q6 says “F04 still open”.; read-code: prior_findings is [] on the chunk that contains the F01-F04 files. Q6 does not name an id.; read-code: prior_findings is [] and the chunk is only f-astra and f-workers, which are outside F01-F04. That empty array matches the scope. Verify: Diff prior_findings against the F-ids named in each file's reply_markdown. Flag any id that appears in prose and is absent from the array on glm-2, mimo-1, and mimo-2. Remedy: Pass the open findings into the worker prompt as a prior_findings seed, and treat a prose-only mention of an F-id with an empty array as a failed Q3. Do not count glm-1's empty array as the same failure.
- **F05-3** [major] `.collab/consilium-2026-10-29-test-workers/runs/grok-1.json:11`, `.collab/consilium-2026-10-29-test-workers/outputs/grok-1.json:1` - grok-1 spent 1364.7s, reported num_turns 1, and answered Q2-Q3 with “Checked all 19” while leaving findings empty. That pattern is consistent with answering the 19-file chunk without a per-file tool pass, and the run record has no tool trace to contradict it. Trigger: One worker receives all 19 corpus files in a single chunk and the scorer treats “Checked all 19” as evidence those files were read. Evidence: read-code: num_turns is 1, duration_s is 1364.656, timed_out is false, output_valid is true. The record stores reply_markdown and findings, not a list of files opened.; read-code: Q2 says “Checked all 19: content.” Q3 says “Checked all 19: shape.” findings is []. There is no per-file citation a reader can audit.; inferred: The same 19 files, split into two chunks, produced findings and named specific files. Grok's single chunk did not. The split is confounded with the model, so this does not by itself rank Grok below GLM or MiMo. Verify: Inspect the worker session log for grok-1 and list tool calls whose target is under tests/fixtures/review-corpus. Zero such calls confirms the claim. A non-zero list would limit it to “results were not written into findings.”. Remedy: Cap a chunk at the two-to-eight file size used for GLM and MiMo, require one cited path per corpus file in the reply, and store the tool-call list in the run record before scoring completeness.
- **F05-4** [minor] `.collab/consilium-2026-10-29-test-workers/handoffs/01-claude-cross-model-tests.md:49` - The handoff's Q5 success criterion (“verdict ADVISE or a documented reason otherwise”) treats the schema's required verdict as optional, so a later scorer can call a correct ADVISE a fixture failure. Trigger: Someone grades Q5 from the parenthetical examples in the handoff rather than from the output schema, which requires verdict and specifies ADVISE for this consultation. Evidence: read-code: Q5 says: Is the output schema-valid JSON with the required keys (verdict ADVISE or a documented reason otherwise)? The schema block above it says verdict is required and this consultation takes ADVISE.; read-code: verdict is ADVISE, matching the schema. The same value is set in glm-1, glm-2, mimo-1, and mimo-2. Verify: Re-read the scoring rubric and confirm ADVISE is marked pass for every arm. If the rubric expects a missing verdict, the rubric contradicts the schema. Remedy: Change the Q5 parenthetical to “verdict must be ADVISE”. Grade schema validity and answer quality as separate columns so a valid ADVISE is never scored as a miss.
- **F05-5** [note] `.collab/consilium-2026-10-29-test-workers/runs/f01.json:1`, `.collab/consilium-2026-10-29-test-workers/runs/f03.json:1`, `.collab/consilium-2026-10-29-test-workers/runs/f04.json:1` - F01, F03, and F04 were produced by the Claude coordinator after the worker returned, so a comparison that puts those files in the worker column credits the coordinator's second pass to the worker model. Trigger: The bake-off table scores corpus files F01-F05 as if each file were the unedited return of one model. Evidence: read-code: The brief says F01-F05 were produced by a Claude coordinator from worker returns, and that the coordinator wrote the verdict and repaired findings on at least F01, F03, and F04.; read-code: The run file records a coordinator repair step after the worker payload, with the stored verdict set in that later step.; assumed: Did not fully separate worker bytes from coordinator bytes on F02 and F05. The brief's “at least F01, F03, F04” leaves those two unconfirmed. Verify: For each of F01-F05, diff the worker payload stored in the run record against the file under tests/fixtures/review-corpus. Count lines present only on the coordinator side. Remedy: Publish two columns: raw worker JSON and post-repair fixture. Rank models only on the raw column. Keep the repaired fixtures as examples of the coordinator, not of the worker.

### Prior findings

_(none)_

## Verdict: ADVISE

The brief's cross-model comparison is answerable from the handoff and run artifacts, but the framing understates process confounds that should be fixed before anyone treats the scores as a model ranking.

### Blockers

_(none)_

### Unproven scenarios

- Whether grok-1 opened any of the 19 corpus files: the run record has duration and num_turns but no tool-call list, so “did not read” is an inference from the empty citations.
- How much of F02 and F05 was rewritten by the coordinator. The handoff asserts repair on F01, F03, and F04 only.
- Token cost and the exact worker prompt bytes. They are not in the five run JSON files that were read, so latency cannot be turned into a cost ranking.
- Whether a repeated grok arm with the GLM/MiMo chunk split would fill locations. One observation per cell cannot separate model from chunk size.
- Whether glm-2 and the MiMo runs would have filled locations if the prompt had shown one example finding with a non-empty locations array.

### First-run checklist (observable)

_(none)_
