# Model fit for email modes: System One, GPT-6 Luna and the Decisions API

Researched 2026-10-02 against branch `docs/email-modes` at `37f6ff61`
(`docs/blueprint/22-email-modes.md`, `docs/research/email-modes/*.md`,
`crates/llm`, `crates/config`, `crates/daemon/src/state.rs`). Product facts
come only from pages fetched that day. Pages read through the `r.jina.ai`
reader proxy are marked "(via jina)". Web search was unavailable, so
discovery went through the HN Algolia API and the vendors' `llms.txt`
indexes.

BK's brief: use a System One model only where it is the best tool, never as
a cheaper stand-in for a System Two model, and say so when it doesn't fit.

## Verdicts

- **Jev (TypeSafe):** not as a dependency now. The shape fits about five
  mode tasks well, but Jev is cloud-only. Its data terms leave open
  questions for third parties' mail, and its speed advantage barely matters
  for mxr's background work. The `/v1/systemone` wire format is the part
  worth keeping, because open local models already serve it.
- **GPT-6 Luna (`gpt-6-luna`, the "gpt-luna" in the brief):** it exists and
  is OpenAI's cheapest workhorse model. It is a good choice for the smart
  tier's cloud endpoint when the user opts in with their own key, and it
  works with today's `OpenAiCompatibleProvider` as is. It is not a
  calibrated judge: published tests show its log-probabilities are
  overconfident.
- **OpenAI Decisions API:** not now. It is a limited preview with no
  published schema, pricing, data terms or confirmation that it returns
  probabilities. Re-check at broad release.
- **The plan as a whole:** keep D114's two LLM tiers. Add System One as an
  eval arm in phase 7's `mxr modes eval --extract`, run against a loopback
  decision runtime. Adopt a `judge` route only for the tasks where that eval
  beats the fast LLM on BK's own mail.

## TypeSafe and Jev: evidence

### Jev returns typed answers with probabilities, never text

- "Jev evaluates typed *questions* against a *state* and returns structured
  results directly. No text generation, no parsing."
  (https://docs.typesafe.ai/introduction.md)
- The three primitives are Choice ("choice, probabilities, confidence"),
  Score ("score, probabilities, confidence") and Noul ("noul (0-1)"). The
  page states: "All three *question* types can be mixed in a single API
  call. Every *question* is evaluated in parallel and in isolation against
  the same *state*." (same page)
- Limits: a Choice allows up to 255 options, and a Score 2 to 10 levels
  (https://docs.typesafe.ai/api.md). Input is "Text only. String, JSON
  object, or array of text values." (https://docs.typesafe.ai/models.md)
- Choice confidence is derived from the probabilities, not judged
  separately. The formula in the confidence page's widget is
  `(count * peak - 1) / (count - 1)` (https://docs.typesafe.ai/confidence.md).
  Synthpop confirms this from the outside: "It's the 0.77 rescaled so that
  spreading evenly over the six faces [...] would read 0"
  (https://www.synthpop.ai/resources/does-a-decision-model-know-when-it-is-guessing-post-1,
  via jina).

### Jev is fast and cheap, but runs only as a US-hosted API

- Price: "\$42 / \$0.042" per Btok / Mtok, and "Charged per input token.
  Output tokens are free." Rate limits are "100K tokens per second / 40
  requests per second" and "can change without notice". Context is "64k
  tokens per request; 32k tokens for `state` plus the longest question".
  (https://docs.typesafe.ai/models.md)
- Latency: "Most queries complete in about 100 ms."
  (https://docs.typesafe.ai/concepts/how-to-build-with-system-one.md) The
  use-case map says "150ms". Ollaya cites third-party benchmarks at "236-276
  ms" with the network included (https://ollaya.dev/, via jina).
- Deployment: one endpoint, `POST https://api.typesafe.ai/v1/systemone`
  (api.md). "the same weights serve every account" (models.md).
  Stackness says: "Can I run Jev itself locally? No. TypeSafe has not
  released Jev's weights." So Jev has no offline mode.
- Access: two third-party posts describe Jev as "early-access" (astgl.com,
  via jina) and "behind a waitlist with no published SLA" (stackness.dev,
  via jina). TypeSafe's quickstart only says to get a key from the console.
  I could not confirm the waitlist from TypeSafe itself.
- Language: "English is the primary training language and where accuracy is
  currently best." (models.md)

### TypeSafe's data terms cover API customers, with gaps for third parties' mail

- No training: "Jev is not trained on customer requests or responses."
  (models.md) The privacy policy says: "We will not train or fine tune any
  artificial intelligence or machine learning models on your prompts or
  other Input." (https://typesafe.ai/legal/privacy-policy, via jina)
- There is a DPA, and it makes TypeSafe the processor: "Typesafe is the
  'processor' and 'service provider'". It includes EU SCCs and the UK
  Addendum, and promises 72-hour breach notice.
  (https://typesafe.ai/legal/data-processing, via jina)
- The DPA's Schedule I lists the categories of data subjects as only
  "Customer and Customer's users". It does not name third parties whose
  data appears in the input. Sensitive data is "N/A".
- Retention: "Customer Personal Data will be retained for as long as
  necessary taking into account the purpose of the Processing" (DPA). "We
  also offer zero data retention (ZDR) for enterprise customers."
  (https://docs.typesafe.ai/legal.md)
- Telemetry license: the MCA grants TypeSafe rights "in perpetuity, any
  Customer Data (i) to derive and generate Telemetry". Telemetry includes
  "summary statistics and classifications", which "TypeSafe may Process
  [...] without restriction, including to improve the Services".
  (https://typesafe.ai/legal/mca, via jina)
- Sub-processors are all in the USA: AWS ("Customer information for live
  requests is stored and processed"), plus Modal, Nebius and CoreWeave
  ("Customer AI prompts are processed, but not stored").
  (https://trust.typesafe.ai/subprocessors, via jina)

### TypeSafe documents Jev's weak spots, and several of them are email's

From https://docs.typesafe.ai/model-jaggedness/jev-1.13.md (last reviewed
2026-09-17):

- "reads dates as text, not as ordered quantities [...] Instead: split the
  work. Extraction is a judgment, so give it to the model. Arithmetic is
  not, so keep it in code."
- "Accuracy falls as the state grows with content unrelated to the
  decision."
- "State is data, and `jev-1.13` does not treat it as hostile by default.
  Content written to adversarially steer the model [...] can move the
  answer."
- "`jev-1.13` is not trained to generate text. [...] it is better to extract
  possible options using regex or a generative model and let `jev-1.13` pick
  the correct extraction."

### The calibration claims are contested, and the strong results come from the closed-choice tasks mxr has

- TypeSafe's claim: "Calibration is measured across groups of predictions;
  it does not guarantee that an individual answer is correct."
  (https://docs.typesafe.ai/concepts/system-one.md)
- Anthus, on 3,600 ProofWriter problems: "Jev stated 99% or more on 1,667
  answers and got 98.9% of them right." Expected calibration error was 0.04
  and 0.03. (https://anth.us/blog/openai-decisions-api-preview/)
- Synthpop, over about 575,000 calls: "On familiar closed-choice tasks,
  confidence tracked accuracy well", but "When the prompt contained no basis
  for an answer [...] confidence often stayed high." (synthpop, via jina)
- Red Hat, on guardrails: "we did not find that decision models produced
  faster, cheaper, or higher-quality answers compared with LLM-as-a-judge."
  (https://developers.redhat.com/articles/2026/10/02/benchmarking-ai-decision-models-against-traditional-guardrails,
  via jina)

## Open local decision models exist and speak Jev's wire format

All of these are one to three weeks old, and almost every number below is
self-reported.

- **Kev** (Apache-2.0, built on Qwen3.5/3.8) comes in 0.8B, 4B, 9B and 27B
  sizes. "The API matches TypeSafe's System One, so you can point their
  Python SDK at your local server." On a Mac: "Kev-4B takes 721 ms for five
  questions, or 136 ms when the text repeats and comes from the cache." At a
  5% error budget, "Kev-4B, 9B and 27B can automate 0.52-0.69 of new-source
  decisions, Kev-0.8B 0.14 and Jev 0.70". Kev-4B needs a "32 GB Mac".
  (https://github.com/jaredpalmer/kev README, via jina)
- **Ollaya** is "Inspired by Ollama". It serves `/v1/systemone` on
  `127.0.0.1:11435` and loads 16 model families. On its typed-decisions
  test, winnow:e4b scores 0.722 in 89 ms (RTX 4090) against hosted Jev's
  0.738. (https://ollaya.dev/, via jina) Stackness adds: "Ollaya logs no
  states and listens only on 127.0.0.1" and notes the project went from
  first commit to 0.7.3 in four days.
- **gutsy-0.8b** is a "775 MB GGUF by llama.cpp on an ordinary CPU". Its
  answers include a `reject` probability for "none of the options fits".
  (https://github.com/kouhxp/gutsy README, via jina)
- **Runtime caveat:** "The same weights can give two confidence numbers
  through two runtimes, so if you threshold on confidence, pin the runtime
  version as well as the model." (stackness.dev, via jina)

## GPT-6 Luna: evidence

- It exists as `gpt-6-luna`, released 2026-09-22. OpenAI calls it "Our most
  efficient model for focused, high-volume tasks". It is a reasoning model,
  and `reasoning.effort` accepts `none` through `max`.
  (https://developers.openai.com/api/docs/models/gpt-6-luna.md;
  https://developers.openai.com/api/docs/changelog.md)
- Price per 1M tokens: "$0.1" input, "$0.01" cached input and "$0.5" output.
  Batch and Flex cost 50%.
- Context window is 1,050,000 tokens, with 922,000 input and 128,000 output.
  Knowledge cutoff is May 18, 2026.
- Structured outputs and function calling are supported. EU data residency
  is available on Standard, Flex and Batch.
- Latency: OpenAI's own published figure, as reported second-hand, is "150
  milliseconds [for the Decisions API], compared to GPT-6 Luna, which would
  take 1.6 seconds" (https://thenewstack.io/openai-decision-api-luna/). I
  did not find a first-party Luna latency figure.
- Log-probabilities are available only with reasoning off. The guide says:
  "When reasoning effort is not `none`, remove `temperature`, `top_p`, and
  `top_logprobs`."
  (https://developers.openai.com/api/docs/guides/latest-model/gpt-6-astra.md)
- Calibration: "Luna stated 99% or more on 2,672 of the 3,600 answers, and
  68% of those were right." Expected calibration error was 0.32. Even at
  depth 0, Luna said "true, 100% sure" to "Charlie is not nice" when the
  text says "Charlie is nice". (anth.us)
- API data terms: "data sent to the OpenAI API is not used to train or
  improve OpenAI models (unless you explicitly opt in)". `/v1/chat/completions`
  and `/v1/responses` keep "30 days" of abuse-monitoring logs and are "Zero
  Data Retention eligible", "subject to prior approval".
  (https://developers.openai.com/api/docs/guides/your-data.md)

## OpenAI Decisions API: evidence

- OpenAI's post: "Give your app real-time decision-making with Decisions
  API, powered by GPT-6 Luna. Define questions and possible answers to
  classify content, route requests, or choose an agent's next action.
  Available in limited preview." A follow-up post says: "Send text or images
  as context. [...] The API returns a selection your app can use. Preview
  access is limited to selected API customers for testing. Broad release
  planned in the coming days." (https://twitter.com/OpenAIDevs/status/2105003318917697873,
  2026-09-29, via jina)
- "What's still unclear is what the Decision API costs per call, how many
  candidate answers a single request can handle, and whether developers can
  tune it on their own data." (The New Stack, 2026-09-29)
- "Some coverage says the Decisions API returns a confidence score. OpenAI
  hasn't published documentation, so we can't say." (anth.us, 2026-10-01)
- `developers.openai.com` has no Decisions page. The `.md` paths I tried
  return 404, and neither the API changelog (latest entry Sep 29) nor the
  docs `llms.txt` mentions it. Its data terms, retention and ZDR
  eligibility are unpublished.

## What mxr has today

All citations are from `37f6ff61`.

- `crates/llm` has one real provider, `OpenAiCompatibleProvider`. It speaks
  chat completions only, and the request body carries no `response_format`
  or schema. Features prompt for "STRICT JSON" and parse it themselves
  (`thread_gist.rs:569`, `deliveries/src/extract.rs:248`).
- The privacy gate is locality only (`crates/llm/src/endpoint.rs:16-36`;
  `crates/daemon/src/state.rs:333-370`). There is no per-provider
  data-terms attribute. `docs/research/chatgpt-sign-in.md:485-492` proposes
  one.
- Tier config (`llm.tiers.*`) and `allow_cloud_background_classification`
  don't exist in code yet. They are planned in D114 and BP:146-175.
- Existing confidence heuristics: deliveries auto-create at a heuristic
  score of 0.8 or more and shortlist for the LLM at 0.5 or more
  (`crates/deliveries/src/lib.rs:269,273`). The LLM's own `confidence` field
  is self-reported.

## Fit by task

This section is inference. It ranks tools in this order: code, then a
System One judgment over code-found candidates, then a fast LLM or a
decisions-style API, then a System Two LLM. Locality follows the plan's
rule: background work is loopback-only by default, and cloud is the user's
own key, opted in.

| # | Task | Best tool | Local or cloud | Why |
|---|---|---|---|---|
| 1 | Base mode from sender | Code | Local | `mail_kind` plus screener already decides it, so no judgment is left. |
| 2 | Message aspects: "is there a task for me", "is this a notification" (shortlist only) | System One Nouls, with the fast LLM as fallback | Local runtime | Two narrow yes/no questions per message is exactly Noul's shape. The calibrated probability gives the plan its missing knob: a threshold tuned to the "under one false to-do a week" bar (todo.md:606). The volume is every shortlisted message, so it must be local. The speed is irrelevant here. |
| 3 | Updates `needs_you` breakthrough | Rules first, then a Noul on rule-flagged updates | Local | This answers updates.md:504's open question: breakthrough without rules only above a measured probability. Failure words find candidates. The Noul asks "does this ask the recipient to act or confirm". |
| 4 | To-do verb | Lexicon, then a Choice over the 8 verbs plus `other` | Local | A closed set of eight. A Choice gives a distribution, so a near-tie can stay unchecked. |
| 5 | To-do object (verb plus object title) | Fast LLM, copying verbatim | Local by default | This is span generation. System One can't generate, and choosing among code-made noun phrases is fragile. |
| 6 | Counterparty / payee | Code | Local | It is a join over `mail_kind` and `contacts` (todo.md:223). |
| 7 | Amount | Regex and schema.org candidates, then a Choice among them plus "not stated" | Local runtime; cloud judge if opted in | This is the pre-parsed value extraction cookbook almost word for word. The value is verbatim by construction, so the verbatim check can't fail. The model only picks which number is the amount due. |
| 8 | Deadline among candidate dates | `natural_time` and regex candidates, then a Choice "which is the due date" plus "none" | Local runtime | TypeSafe's own guidance: let the model extract, keep comparison in code. It replaces "model copies due words" with "model picks a phrase code already found". The ambiguous numeric dates (BP:765) remain a locale question for code. |
| 9 | Act-by and surface-at | Code | Local | The fixed lead-time table; the plan already forbids model output here. |
| 10 | Action link among candidate URLs | Code extracts links, a Choice ranks them by anchor and nearby words, and the DMARC/domain gate in code decides | Local | A judgment over candidates fits. But Jev's docs admit adversarial content "can move the answer", and phishing mail is adversarial. The model may only rank; the gate stays the authority. |
| 11 | Receipt or confirmation completes a to-do | Code computes the pair features (same domain, amount or reference equal); a Noul judges "this message confirms the to-do was done, not a reminder that it's due" | Local; small volume | Telling "payment received" from "payment due" is the hard part of the plan's "strong pay match", and it's a judgment. A calibrated probability turns BP:359's two bands (auto with undo, offer) into measured thresholds. Numbers stay in code. |
| 12 | Messages "what they asked" | Code splits the cleaned new text into sentences; a Choice picks the request sentence, or "none" | Local runtime, or a cloud judge on the smart tier | This replaces generation plus `verify_quote` with selection. The quote is verbatim by construction, and "none" is a first-class answer. It matches the line-by-line search cookbook. The last `?` sentence stays the no-model fallback. |
| 13 | Thread gists | System Two LLM (summarise) | Smart tier; local by default | Free text. System One does not fit. The plan should also assign it a tier, since D114 doesn't. |
| 14 | Got it acknowledgement | Code template from `contact_style` and `habits.rs` (greeting, a short fixed line, sign-off) | Local, no model | A model writing mail that is sent after a countdown buys risk for no gain. The `--dry-run` preview is trivial when the text is a template. |
| 15 | Updates fact sentence | Cleaned subject; fast LLM only for generic subjects | Local | Generation. |
| 16 | Updates number labels | Regex for values; fast LLM labels | Local | Labels are open-ended, so a Choice needs a label list nobody has yet. |
| 17 | Archive record kind | Rules and schema.org, then a Choice over the kind enum | Local | A closed set. The calibrated probability guards against archive.md:233's "label everything automated as a receipt" failure. |
| 18 | Archive amount, issued_on, good_until | Same as #7 and #8 | Local | Same reasoning. `checked` stays schema-or-user only. |
| 19 | Archive answer box, `mxr ask` | Field match in code, then a System Two LLM with citations | User's endpoint | Generation and multi-step. A Noul citation check could add a second guard (the citation-check cookbook), but the existing validator already covers it. Not now. |
| 20 | Person merge across addresses | Rules and manual (as planned) | Local | Low volume, and wrong merges are costly. The entity-alignment pattern would work as a suggestion source later, but the evidence is a display name and addresses, where code signals are as good. |
| 21 | Conversation shape, closeness | Code | Local | Pure functions are already specified. |
| 22 | Quote and signature stripping | Code | Local | Deterministic shingles. |
| 23 | Reading item segmentation | Rules (as planned); a Choice over shape only where rules are low-confidence, if the 30-issue check fails | Local | Rules fall back to the subject. A model is a later fix, not a default. |
| 24 | Reading rank, expiry, minutes | Code | Local | Statistics. |
| 25 | Unsubscribe evidence | Code | Local | Counts with a threshold; a model adds nothing. |
| 26 | Embeddings and index recipes | Local embedding model (fastembed, as now) | Local | System One is not an embedder. |
| 27 | Search reranking (not in the plan) | Optional later: a Noul per (query, hit) over the top 20-30 hybrid hits | Local runtime | The rerank cookbook took top-1 from 5% to 18% on legal text. Only worth building if the mode retrieval eval (BP:588) shows ranking, not recall, is the gap. Product discipline says don't add it speculatively. |
| 28 | Now screener "classifier is unsure" (now-and-handoff.md:582) | Probability from #2 | Local | The plan needs a confidence signal it never defines. A calibrated Noul is that signal. |

### System One wins on shape, not speed, for mxr

Jev's headline advantages are latency and price. Neither moves mxr much.
Nearly all of the plan's model work is background work after sync, and the
cloud cost is negligible either way: Jev at $0.042 per million input tokens
or Luna at $0.10. What matters for mxr is twofold:

- **Selection by construction.** For amounts, dates, the ask sentence and
  link choice, the model can only return something code already found.
  That removes the plan's "verbatim check" failure path (BP:169-172).
- **A probability you can set a threshold on.** The plan's auto-actions
  (auto-complete on receipt, needs-you breakthrough, the model-placed
  "check" style) currently rest on rule strength or tier, with no number.

Both hold only if calibration survives on email. The evidence says
closed-choice tasks with the answer in the text calibrate well. Email
judgments are mostly like that, but no one has measured it on email.

### Where System One doesn't fit

These tasks need generation or multi-step reasoning: gists, the to-do title,
Updates facts, `mxr ask`, and drafting. They stay on the LLM tiers. Others
need no model at all: Got it, unsubscribe evidence, ranking, shape,
stripping and counterparty.

### A local LLM is good enough for most of it

The fast tier's Ollama model with a schema-constrained enum can answer every
Choice and Noul above. What it lacks is a calibrated probability. Without
one, the honest rule is: an LLM answer may place and suggest, and only a
calibrated judge, or a strong code rule, may auto-act.

## How a judge would plug in without breaking local-first

This section is inference.

- **Protocol, not vendor.** Add a `DecisionProvider` to `crates/llm` beside
  `LlmProvider`, with `decide(state, questions) -> answers`. The shapes
  differ: no messages, typed answers back. Speak the `/v1/systemone` wire
  format, so one client reaches Ollaya, Kev or gutsy on loopback, or Jev
  with the user's key. No new crate is needed.
- **Config.** Add `llm.tiers.judge` with explicit `base_url`, `model` and
  `api_key_env`, plus a `kind = "systemone"`. Unlike D114's tiers, it must
  not inherit `base_url` from `[llm]`, because a chat endpoint can't answer
  `/v1/systemone`.
- **Routing.** Extend D114's fixed feature-to-tier table with
  judge-eligible features (#2, #3, #4, #7, #8, #10, #11, #12, #17, #18).
  With no judge configured, those features fall back to the fast or smart
  LLM with an enum schema, and the result is marked uncalibrated.
- **Gate.** Use the same loopback rule. A non-loopback judge needs
  `api_key_env`. Background judge work (#2, #3) also needs
  `allow_cloud_background_classification`. Add the data-terms attribute from
  `chatgpt-sign-in.md:485` so a vendor can be marked as not covering
  third-party mail.
- **Pinning.** A threshold is valid only for the model version and runtime
  it was measured on. Store `model` (Jev returns the versioned ID) and the
  runtime version with each cached answer. Invalidate thresholds when
  either changes.
- **Offline.** Nothing breaks. With no judge and no LLM, layers 1 and 2
  still place everything (BP:110).

## Recommendation: not now as a dependency, yes as an eval arm in phase 7

1. Ship phases 1-6 as planned. Nothing there needs a model choice.
2. In phase 7, build `mxr modes eval --extract` with three arms on BK's
   mail: the fast LLM (local), the smart LLM (Luna or another cloud model
   on his key), and a `/v1/systemone` judge on loopback (Ollaya with
   winnow:e4b or Kev-4B). Score #2, #7, #8, #11 and #12 on precision,
   precision at a fixed false-to-do budget, and calibration. Record counts
   only.
3. Adopt the judge route only for tasks where it wins. Pin the model and
   runtime. Allow auto-complete and breakthrough only from a calibrated
   judge, or from a strong code rule.
4. Use Luna as the suggested cloud model for the smart tier's generation
   tasks, and add `response_format` JSON schema support to
   `OpenAiCompatibleProvider` for it. That is a fix to the existing
   prompt-and-parse path, worth doing whatever happens with System One.
5. Leave Jev as a user-selectable cloud judge only after BK settles the
   DPA question below. Revisit the Decisions API when OpenAI publishes docs
   and terms.

## Top three tasks where a non-default tool clearly wins on fit

1. **Receipt matching (#11).** A Noul over code-matched pairs, gated by a
   measured threshold, replaces an unquantified "strong match".
2. **Messages ask (#12).** A Choice over sentences code split out replaces
   generation plus a quote check.
3. **Amount and deadline selection (#7, #8, #18).** A Choice over
   regex, schema.org and `natural_time` candidates, with "none stated",
   replaces "model copies, code verifies".

"Clearly" means the task's shape matches System One's documented strengths.
No one has yet measured that it beats the local LLM on email.

## Blockers

- **Jev's data terms for third-party mail.** The DPA names only "Customer
  and Customer's users" as data subjects, retention is open-ended, and the
  MCA grants a perpetual Telemetry license over "classifications". Whether
  that meets mxr's processor rule is a judgment call for BK.
- **Jev access.** Third parties describe it as early-access with no SLA,
  and rate limits "can change without notice". Unverified from TypeSafe.
- **Decisions API.** No docs, pricing, schema, probabilities or data terms
  yet.
- **Local runtime maturity.** Ollaya, Kev and gutsy are one to three weeks
  old with self-reported numbers. Calibration depends on the runtime. Kev-4B
  needs about 32 GB of memory on a Mac. Kev's small models trained on states
  of up to 384 tokens, which is enough for a subject and first lines but not
  whole emails.
- **Adversarial mail.** Every judge (Jev, Kev, Luna) can be steered by
  content. Code gates (DMARC, domain match, verbatim) must stay the
  authority for any action.
- **No email benchmark.** Every accuracy and calibration figure above comes
  from support tickets, logic puzzles, guardrails or legal text.

## Unresolved questions

- Does TypeSafe's DPA, which names only "Customer and Customer's users" as
  data subjects, plus its perpetual Telemetry license, count as "terms that
  cover" third parties' mail under mxr's processor rule?
- Should auto-complete (BP:359) and needs-you breakthrough require a
  calibrated judge, or is a strong code rule enough on its own?
- Is asking users to run a second local daemon (a decision runtime beside
  Ollama) acceptable, or must the judge share the Ollama process? Ollama
  0.35 serves two decision models with "Raw softmax; documented as
  uncalibrated", according to ollaya.dev.
- Which tier owns thread gists, given that they produce the same ask that
  D114 assigns to the smart tier?
