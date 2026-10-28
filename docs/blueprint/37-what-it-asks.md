# 37. What an email asks of you

Status: R2c shipped in v0.6.163; later rungs remain proposals. The
[implementation contract](../implementation/r2c-asks/README.md) records the
shipped boundary; the [To do guide](https://mxr.sh/guides/todo/) describes the
user workflow. The design and measurements below retain their original
2026-10-22 context. Original source baseline: `origin/main` at `a4bf49248` (v0.6.142). This chapter specifies the third of blueprint 32's five questions, "What does it ask of you?": the judge, its facts, its models, its measurement and its rungs. It refines [32](32-classifier.md) and the To do enters and leaves rules of [22](22-email-modes.md#to-do-shows-instructions-on-a-runway-not-emails-with-flags). It rests on D174 to D185. The research is in [what-it-asks/literature.md](../research/what-it-asks/literature.md), [what-it-asks/industry.md](../research/what-it-asks/industry.md) and [what-it-asks/methods.md](../research/what-it-asks/methods.md).

BK asked for this to be designed properly, not patched: "I do't want quick fixes, I need you to do some comprehensive research on how to do this right and how to improve the classifier so taht it's done right not just monkey patched onto it" (2026-10-22). He also said a model need not be local: "if this is a task that needs more intelligence then we can suggest that maybe this needs a cloud model or something" (2026-10-22).

## On BK's mailbox, 40% of to-dos are wrong and most asks are missed

Measured on BK's store on 2026-10-22, read-only. Counts only; the shapes are described, never the mail (32, "Real mail shapes scenarios without entering them").

| Measure | Count |
|---|---|
| To-dos made by today's code (since v0.6.59) or shown to BK | 40 |
| Right / wrong / unclear | 21 / 16 / 3 |
| Wrong: not a to-do / wrong action / wrong person / wrong date | 8 / 4 / 4 / 0 |
| Asks in the last 60 days that made no to-do (sample, not exhaustive) | 20, of which 12 are bills and 8 are requests |
| Rows BK ticked off on one evening that were wrong | 10 of 15 |
| Dismissals made by BK | 0 (all 48 were made by an automatic repair in one second) |

The 16 wrong rows and 20 misses have six causes:

1. **The promise model doesn't know who the owner is.** The commitments prompt says "the account owner" but never names one, and code keeps the model's `direction` while ignoring its `who_owes` field (`crates/relationship/src/commitments.rs:22, 88-112, 156-163`). So the owner's requests to others ("could you please book an engineer?") become the owner's promises, and forwarded letters become the owner's words. Across the store, 30% of outbound "yours" commitments name someone other than the owner as the one who owes.
2. **The first regex wins.** `detect()` tries kinds in a fixed order and returns the first hit (`crates/todo/src/detect.rs:226-292`). A perks email listing "Pay Now" rates is a bill, because BILL runs before the promo gate. A booking that asks you to sign a waiver and mentions paying at the car-park meter is a bill, because BILL runs before SIGN.
3. **Negatives veto the whole email.** "Why not set up a Direct Debit?" matches AUTO and suppresses a real minimum payment (`detect.rs:94-97, 261`). A support agent explaining that an old payment "was declined" and nothing is outstanding is an urgent failed payment, because FAILED wins over everything (`detect.rs:245-252`).
4. **There is no document type or payment state.** A credit note and a service record with "Amount Due £0.00" are bills. "Final reminder" is a payment word even when the deadline is to register apps.
5. **Requests have no path.** The rule detector knows seven admin kinds. The only model path mirrors promises from sent mail (`crates/store/src/todo_first_run.rs:220-236`). "Please send your meter readings", "upload your CV", "sign the attached consent form" and "enter your availability" make nothing. Messages has a separate `directed_request` for Your turn that excludes action items on purpose (`crates/daemon/src/handler/mail_kind.rs:751-785`).
6. **Facts are thin.** Rand amounts ("R953.63") aren't amounts (`crates/todo/src/money.rs:66-86`). Two accounts billed by one sender in one month share a dedup key, so one is silently merged (`crates/todo/src/pass.rs:522-548`). Reminders and late-payment notices never escalate the bill they belong to.

The eval reported 99.8% to-do accuracy over the same period. The causes are in [§ The eval grades what the person relies on](#the-eval-grades-what-the-person-relies-on).

## Every published system that works splits the problem the same way

The literature and the shipping products agree on five things. Each is the opposite of something mxr does today.

1. **Decide whose words a passage is before deciding what it asks.** Lampert, Dale and Paris kept only the sender's own text (dropping quoted, forwarded, signature and disclaimer zones) and request detection went from 72.28% to 83.76%, an error reduction of more than 41% (NAACL 2010, read in the paper). Microsoft's commitment patent attributes every extraction to its author in a thread (US10361981B2).
2. **Who must act is its own question, and the owner is an input to it.** Microsoft's annotation rule for commitments is "the action must be on the sender and must not already be complete" (Azarbonyad, Sim and White, WSDM 2019, read in the paper). Viva separated what you promised, what you were asked and what you asked others. Microsoft's people-assignment dataset allows "no-one" as the responsible person (Rameshkumar et al. 2018).
3. **Machine mail is a document with a type, a status and required fields.** Google's Juicer classifies "is it a bill", then a subtype (payment confirmation, autopay notification, reminder, overdue), and surfaces only reminder and overdue. "If any of the required fields for a vertical fail to extract, we discard the other field values and generate nothing" (KDD 2018, read in the paper). schema.org's `PaymentStatusType` has five states: automatically applied, complete, declined, due and past due. Mindee and Veryfi make `credit_note` and money-in a field.
4. **Label sentences, decide messages, keep several labels.** Sentence-level labels train better message detectors than message labels (Bennett and Carbonell 2005). A message carries 1.3 acts on average (Cohen, Carvalho and Mitchell 2004), and 44.8% carry two or more intents (Wang et al. 2019). An ordered cascade can't represent that.
5. **Measure per class, on the positives, on senders you haven't seen.** Pooled metrics hide rare-class failure: micro-F1 78.9 against macro-F1 37.4 (Jeong et al. 2009), and Juicer's bill classifier has ROC-AUC 0.97 but PR-AUC 0.80. Every paper that tested on a new population reported a drop. Juicer's bill precision fell from 84.6% to 66.8% on unseen templates, with "a statement of an account balance" as the named false positive. Commitment AUC fell from 0.86 to 0.67 across corpora.

Two findings set limits rather than methods:

- **People disagree on commitments.** Annotators agree on requests at κ 0.78 to 0.80 but on commitments at κ 0.54, rising to 0.74 only after guidelines ruled on conditional and indirect cases (Lampert et al. 2008). A classifier won't beat its labels, so abstaining is part of the design, not a stopgap.
- **No published result says how well an LLM detects owner-directed obligations in personal mail.** The one email-specific LLM evaluation found (MAILEX, EMNLP 2023) had few-shot GPT-3.5 far below fine-tuned models, with 42% of sampled errors fabricated or wrong. mxr has to measure its own.

## The judge reads units of authored text, with a speaker, an addressee, an act and a state

The design replaces "match a phrase anywhere in the email" with four steps. Facts and units are code. Tagging is rules first, then models. The answer and the items are code.

```text
 email ─▶ FACTS ──────────────▶ UNITS ──────────▶ TAGS ─────────────────▶ ANSWER ─────────▶ ITEMS
          zones with authors     sentences of     per unit: act, actor,    asks you / you     to-do rows, Your
          owner identity         each zone,       state, action, due and   promised / you     turn, Waiting on,
          recipient role         plus the         amount candidate,        asked / nothing /  each with one
          candidates (amounts,   subject          evidence span            not sure           quoted reason
          dates, refs, links)                     (rules vote, then a
          document reading                        unit model, then a
                                                  language model for
                                                  the band)
```

### Facts: zones, owner identity, recipient role and candidates

These extend R2's facts record. They are observations, never verdicts (32).

- **Zones.** Each message body is split into authored text, greeting and sign-off, quoted reply, forwarded block, signature, disclaimer or footer, and advertising. Every zone has an author where one is known: the `From` of this message for authored text, and the quoted or forwarded header's sender otherwise. The reader already separates new text from earlier thread content (`crates/reader/src/new_text.rs`, `new_text`, `authored_text`) and recognises forwarded headers (`:47`). It gains forward-author parsing and zone labels. Lampert found simple quote heuristics miss most reply and forward content in real mail, so zoning is measured (spike S6).
- **Owner identity.** The account's addresses, send-as aliases and display names (ch. 33), plus a household list the person names (Q4). Every unit's speaker is compared with it.
- **Recipient role.** The owner is alone in To, in To with others, in Cc, in Bcc or undisclosed, or the mail came through a list (`List-Id`).
- **Sender context.** The person-or-machine evidence `mail_kind` already uses (list headers, Auto-Submitted, no-reply, written-to history), plus a template key: sender plus subject skeleton, as Juicer induces it, counted over this mailbox.
- **Candidates.** One parser per kind (32):
  - Amounts carry sign, currency (including the rand's "R" prefix and every ISO code the account's mail uses), zero and the words around them ("amount due", "credit", "balance", "paid", "price").
  - Dates resolve through `natural_time` and keep their tense words.
  - References: account, invoice, policy and order numbers.
  - Links carry their registrable domain and the sender's DMARC result.
  - schema.org objects count only when the sender authenticates (DKIM or SPF aligned with `From:`), following Gmail's markup rules.
  - ICS objects carry their method and the owner's participation status.
- **Thread inputs.** The one message this replies to, and whether the owner wrote in the thread afterwards. Wang et al. found context helps when read in relation to the sentence, and that concatenated history does not. Mukherjee et al. found the replied-to message useful in 70% of replies. So the judge gets the preceding message, never twelve messages per contact.

### Units and tags

A unit is a sentence or clause of the authored zone, plus the subject. Units from a forwarded zone are kept and marked forwarded: they are context, and an ask in them counts only when the owner's own text delegates it ("can you deal with this?"), which Lampert found is rare.

Each unit gets one tag:

```text
UnitTag {
  act:    request | commitment | obligation | offer | question | information | pleasantry | none
  actor:  owner | correspondent | third_party(name) | everyone | nobody     // who must act
  state:  pending | done | failed | cancelled | conditional | hypothetical
  action: verb + object (verbatim span) → kind: pay | fix_payment | sign | verify | reply | send
          | provide | upload | book | attend | renew_or_cancel | register | contact | other
  due:    a date candidate id, or none
  amount: an amount candidate id, or none
  evidence: unit id and the span within it
  source: rule (id, version) | unit model (name, version) | language model (name, prompt version)
}
```

- **Act.** "Request" is a directive to the reader. "Commitment" is the speaker promising to act. "Obligation" is an institution stating what is owed ("your payment of £… is due"). It is grammatically an assertive, and Searle's directive cues miss it, which is why a directive regex both over-fires on marketing and misses statements. "Offer" covers suggestions and promotions ("Why not set up a Direct Debit?", "rejoin for half price").
- **Actor** follows from the speaker (the zone's author), the addressee (the recipient role) and the pronoun and modal cues: "can you…" from them is the owner, "I'll…" from the owner is the owner, "could you…" from the owner is the correspondent. Cohen et al. found these bigrams carry the signal, and they mean something only once the speaker is known.
- **State** is what the regex vetoes tried to be, scoped to one unit: "was declined last month" is failed in the past, "nothing outstanding" is done, "if you'd like to join, come…" is conditional.
- **Action, due and amount** are selections. Code finds every candidate, and a tag picks one candidate id or none. No model writes a date, an amount or a quote that the daemon can't find verbatim (D111, D149, 32).

### The document reading decides what machine mail is

Machine mail gets one more reading, shared by this judge and by Worth-keeping (32). Both call the same code over the same facts, and neither reads the other's claim, so 32's rule that judges never read each other's answers holds.

| Document kind | Payment status | Makes a pay or fix to-do? |
|---|---|---|
| Invoice or bill | due, past due | Yes, when the amount due is above zero (or stated by schema.org) and a due date or "now" is found |
| Statement | due, with a minimum payment and a date | Only when both are present; otherwise it's an update (Juicer's named false positive) |
| Payment confirmation, receipt | complete | No; it offers "looks done" on the open bill (D180) |
| Autopay or direct-debit notice | automatically applied | No; it's an update. The judge needs an unpaid-on-file state, not a mention of direct debit |
| Failed payment | declined, pending | Yes, now. A past decline narrated as resolved is `done` and makes nothing |
| Credit note, refund | money to you | No |
| Quote, offer, marketing | none | No. A deadline in it ("final reminder before the price ends") is an offer's end, not an obligation |
| Booking, account notice, other | none | Only through units: "please sign the waiver" is a sign request |

The rule is Juicer's: required fields or nothing. A bill with no amount due and no date doesn't become a to-do on its words alone (D176).

### Rules vote or abstain, per unit

Today's regexes, `mail_kind`'s `directed_request` and `billing_document`, and the schema.org reader all become labelling functions (D177). Each returns a vote for one tag field with its evidence span, or abstains. It never returns the email's answer.

- A negative clue votes against one unit: "set up a Direct Debit?" votes `offer`. It no longer vetoes every other unit in the email.
- Genre gates come first, because Lampert found 10% of request errors were marketing and spam imperatives. A list or promotional sender votes `offer` on pay words, and a platform announcement votes `information` on "RSVP".
- When rules agree, the resolver keeps their answer. When they conflict or all abstain, the unit goes to the unit model.
- On the labelled corpus each rule's precision and coverage are measured directly. On real mail, where only corrections are ground truth, a label model (Snorkel's data programming, NeurIPS 2016; FlyingSquid's closed form, 2020) estimates them. Majority vote weighted by measured precision is the baseline it must beat. Google's DryBell matched tens of thousands of hand labels with this approach, and Juicer's migration replaced 45,000 lines of rules with it ("Oftentimes, fixing error cases results in breaking correct cases", CIDR 2020).

### The answer aggregates units into what the owner must do

| Units | The email's answer | Where it shows |
|---|---|---|
| A pending request or obligation with actor owner, in authored text, with an action other than reply | **asks you**: one item per distinct action | To do, labelled with the person or organisation ("[name] asked", "[organisation]") |
| A pending request or question whose action is reply, actor owner | **asks you to reply** | Messages, Your turn (replaces `directed_request`) |
| Outbound: a pending commitment with actor owner, in the owner's authored text | **you promised** | To do ("you promised [name]") |
| Outbound: a pending request with actor correspondent | **you asked them** | Messages, Waiting on; never To do |
| Inbound: a commitment with actor correspondent | **they promised you** | Messages, Waiting on |
| Only offers, information, pleasantries, done or cancelled states | **asks nothing** | — |
| A high-consequence unit left in the band, or tags that disagree | **not sure** | Not-sure line (Q2) |

One judge now feeds To do, Your turn, Waiting on and promises. `todo::detect`, `directed_request`, `billing_document` and the relationship worker's commitment extraction retire into it (D182).

### Items: one row per obligation, kept through its lifecycle

- **Identity** is counterparty, kind, reference and due period. The reference is the account, invoice or policy number when one is present, and the object otherwise. Two property levies from one managing agent have different account references, so they are two rows. Today's key (sender domain, counterparty and due day) merges them (D180).
- **Lifecycle.** A reminder or overdue notice that resolves to the same identity updates the row, marks it overdue and brings it back to Now. A payment-confirmation document, or a later outbound unit by the owner with state done, offers "looks done". Closing stays a suggestion, as BK decided for detected to-dos. RSVP auto-completion from the invite's participation status stays.
- **Title** is verb plus object from the tag's span, rewritten from the owner's view: "Send the signed form to [name]", "Pay [organisation] £…". "You" and relative dates are resolved against the owner and the send date before storing (Corston-Oliver et al. 2004; Mukherjee et al. 2020).

## Models do what rules can't, each at the tier that fits

Every model step is a judge behind the same claim interface, answering only what rules left open (32). The tier follows the task's difficulty, as `docs/research/model-fit.md` and BK's rule of fit over cost require (D178).

| Step | Tool | Why this tier |
|---|---|---|
| Zones, owner identity, recipient role, candidates, template key | Code | Structural. A 2009 SVM zoner reached 93.6% on author-versus-other; mxr's quoted-text code plus forward parsing is the baseline (spike S6) |
| Document kind and payment status for templated machine mail | Rules, then the unit model | Juicer-style subtypes are learnable from templates. The hard part is state, which twins teach |
| Unit tags in bulk (every email, the history backfill, offline) | A local unit classifier in-process through `ort` | A 50,000-email backfill through a 7B local LLM takes about 78 hours on an M2 Pro (methods §2); an encoder over the same units costs the same order as today's embeddings. ONNX Runtime already ships in the binary (`fastembed 7.1.1`, `ort 2.0.0-rc.13`) |
| The band the unit classifier is unsure of: indirect, polite, multi-party and in-body-attributed asks in person mail | A language model selecting from candidates under a JSON schema. Recommended: a cloud fast tier on the user's own key (Claude Haiku 5.5 or gpt-6-luna). Supported: a local model through Ollama. Neither: not sure | These are the cases small models miss and cross-domain drops hit hardest (Azarbonyad). A fast cloud tier costs about $0.23 per 1,000 emails live (methods §7). At 200 arrivals a day with rules abstaining on a quarter, that is about $0.35 a month |
| Commitments in the owner's sent mail | Unit classifier plus abstention; the language model in the band | People agree at κ 0.54 to 0.74. A stronger model doesn't fix ambiguous labels, so "not sure" beats a guess |
| Labelling synthetic training data, writing twins, audit reviews | A strong cloud tier (Sonnet 5.5, gpt-6.1-sol or Opus 5.5), on synthetic mail only | Offline, where quality matters more than cost. This is DryBell's "non-servable resource": used to label, never on the run path |

### The unit classifier is trained on the corpus, never on real mail outside the device

- **First measured model:** a linear head on mxr's existing BGE-small embeddings, trained SetFit-style on the scenario corpus. SetFit beat standard fine-tuning by 19.3 points at 8 examples per class. It needs no new model file, and the head can retrain on the laptop.
- **Steady state:** a fine-tuned small encoder (ModernBERT-base or DeBERTa-v3-small class), trained at build time on the corpus plus labels the strong cloud tier wrote on synthetic mail. It ships as a model file next to the embedding models, and spike S2 picks the model by held-out-template accuracy and latency.
- **Personalisation:** per-user thresholds, not a per-user fine-tune (R3e). Corrections stay on the device as labels (32).
- Synthetic data helps less as a task gets more subjective (Li et al., EMNLP 2023), and commitment detectors lose accuracy across corpora (Azarbonyad). So `classify eval --set mine` is the only measure of transfer, and the gap between it and the corpus is reported.

### The language-model judge can only select, and the daemon checks everything it returns

- The prompt names the owner and their addresses, labels the unit's zone and speaker, and gives the subject, the unit with its neighbours, the recipient role, the sender kind and the candidate list. It never gives the thread, other contacts' mail or attachments.
- The output schema is `{act, actor, state, action_kind, action_span, due_candidate_id | null, amount_candidate_id | null}`. Free text is allowed only in `action_span`, which the daemon checks verbatim against the zone and drops if absent, as the resolution worker already does (`crates/daemon/src/handler/todo_resolution/worker.rs:262-290`). A judge that can return only enums and ids can't be talked into an action; the worst an injected email can do is mislabel itself.
- Structured output (JSON schema on the provider) comes first, as 32 says. OpenAI honours `response_format` with `json_schema`. Whether Anthropic's and Google's OpenAI-compatible endpoints do is spike S4; if not, those get native clients.
- The email is spotlighted: delimited and datamarked as untrusted data (Hines et al. 2024 cut attack success from above 50% to below 2%), on top of `wrap_untrusted_mail`.
- Disagreement between the unit classifier and the language model is uncertainty of the "a model disagrees with a rule" kind (32), not a tie broken silently.
- Answers are cached by content hash, prompt version and model, and recorded for the eval (below).

### Scores stay inside the judge

D119 rejected "a confidence score on every placement with a threshold". That still holds on screen. Inside the judge, the unit classifier's score is compared with a per-class threshold. The judge emits a claim with its evidence, or abstains, and only the outcome is stored, with the judge id, version and threshold version (D179).

- Thresholds are set per class on held-out corpus phrasings and templates. The rule is the lowest threshold whose precision meets the class's target, with the coverage it leaves reported (selective classification, Geifman and El-Yaniv 2017; Learn then Test, 2021).
- Targets follow the cost of a mistake. BK chose automatic creation, and SmartMail's prototype users reacted badly to tasks added automatically ("fail hard"), so "asks you" needs a high bar. Proposed: 95% for pay, fix payment, sign and verify, 90% for other requests. The exact numbers are recorded in the rubric and raised only by measurement.
- Two-sided bands apply to the high-consequence classes: above the upper bar is a claim, below the lower bar is "asks nothing", and between them the next judge decides, then "not sure".
- A model's self-reported confidence is not a score. For the language model, agreement with the unit classifier is the usable signal.

## The eval grades what the person relies on

The 99.8% was accuracy on a set that is 99.7% "no to-do". R1b's own baseline says recall on the 98 live asks is 7 (`4902f18c2`, blueprint 32's full-set baseline). The eval also grades a single yes or no per email (`crates/daemon/src/handler/classify_eval.rs:40, 49, 128`), runs no model (the default config has the LLM off, `crates/config/src/types.rs:384-387`), and its generated bills use exactly the words the rules match. R1c replaces that with:

- **Labels per email** (label schema v2): every expected item with its kind, verb, object, actor, direction, state, counterparty, amount and due words; whether a reply is owed; promises in sent mail.
- **Metrics:**
  - precision and recall per kind on the positives, plus field exact-match for amount, due, counterparty and title;
  - duplicate and merge counts;
  - abstention rate and precision among answered;
  - pair accuracy on twins, where both members must be right.
  Accuracy is not reported.
- **Slices:** sender kind (person, organisation, platform, list), locale (en-GB, en-ZA, en-US to start), template family, zone, thread position, and whether rules abstained (the band gets its own scorecard). R1c reports sender kind, locale, template family and visible against held-out phrasing; zone, thread position and rules-abstained arrive in R2c, where those facts first exist.
- **Held out, reported separately:** phrasings, whole template families the generator never used for training, and `--set mine`.
- **Twins for every shape in this chapter:**
  - marketing with pay words against a real invoice from the same kind of sender;
  - a direct-debit offer against a direct-debit notice;
  - a past decline narrated as resolved against a current decline;
  - a credit note and £0.00 due against a positive balance;
  - "final reminder" with and without money owed;
  - a platform announcement against a personal invitation;
  - the owner's request against the owner's promise;
  - a forwarded letter against a direct request;
  - two accounts from one sender;
  - rand, euro and pound amounts;
  - a group ask against a cc'd ask against an ask to you alone;
  - a conditional ask against an unconditional one;
  - "let me know if you have any questions" against a real question;
  - an ask answered an hour later against an open one.
- **Behavioural tests** in CheckList's three types:
  - invariance: changing names, amounts, dates and domains doesn't change the act;
  - direction: adding "nothing outstanding" moves pay to no;
  - minimum function: one-line canonical asks.
- **Leak guard:** a surface-only baseline (subject pattern, sender, length) must fail the twins. If it passes, the corpus leaks the label (Gururangan et al. 2018).
- **Recorded model responses:** every language-model call in the eval is recorded with model, prompt version, schema version and input hash. CI replays the recordings and never calls a live model, because temperature 0 isn't reproducible on batched servers. Recordings are re-made only when a prompt, schema or model changes, and the claim diff is reviewed.

## Privacy follows the background-classification rules

- Unit tagging on the device sends nothing anywhere.
- The language-model band reads incoming mail in the background, so it runs on loopback by default. It reaches a cloud model only when the user sets `allow_cloud_background_classification` with their own API key (22, D114). The opt-in screen says what goes: "the subject and the sentences mxr couldn't decide, from about N emails a day, to [provider]".
- Third parties' mail goes only to a processor whose contract covers it. Anthropic's, OpenAI's and paid Gemini's APIs have DPAs and don't train on API data. Gemini's unpaid quota uses content and allows human review, so it is never suggested, and the screen says a Gemini key must have billing enabled (methods §7).
- Anthropic's privacy page and its API docs disagree on default retention, 30 days against "not retained by default". The screen states the longer figure until spike S5 settles it.
- Corrections, labels, thresholds and per-user heads stay on the device. Real mail never trains a shipped model. Measurements on real mail are counts only (32).

## How it should feel

No new visual concept: the judge is seen through To do's runway (22) and the night desk's reason and model marks (26, 27). The person should feel that every row is obviously theirs to do, and that its reason is the sentence that asked.

- **The row.** Verb and object first, then who: "[name] asked", "you promised [name]" or the organisation, with the amount and the act-by. The reason under it is the quoted span, never a rule name: `"please could you send your meter readings" · asked you directly`. A model-made tag names the model in words, with no sparkle (D141).
- **Not sure.** A row the judge couldn't decide sits on a quiet "Might need you" line at the foot of To do, collapsed like Whenever, and never in Now's can't-wait list. At most three a day reach Now as one-key questions (32). Gmail's AI Inbox does the same: it generates more to-dos than it shows and offers "View all".
- **Correcting.** `X` on a row opens four choices, each a different teaching event and label (D183): **Not a to-do** (an offer or an FYI), **Not mine** (someone else's job), **Already done**, **Wrong details** (opens the edit). The row leaves at once with a one-line undo. BK ticked ten wrong rows off as done because "done" was the nearest key, and that taught mxr nothing.
- **Restraint.** No confidence bars, no percentages, no "AI found this" banners. Every row shows exactly one reason.

## What changes in the code

| Today | After |
|---|---|
| `crates/todo/src/detect.rs` ordered detection | Rules as voting functions over units; the document reading; item builder |
| `mail_kind::directed_request`, `billing_document` | Labelling functions; Your turn reads the asks answer |
| Relationship worker's commitment extraction (12 messages per contact, no owner identity) | Unit tags on each email's authored text; the relationship profile reads commitments from assessments |
| `todo::money`, `todo::dates` | The facts record's single amount and date parsers, with sign, currency and reference |
| Bill dedup key (domain, counterparty, due day) | Item identity with reference (D180) |
| `todo_resolution` worker | Lifecycle: later units and documents offer looks-done |
| `classify eval` to-do boolean | Label schema v2, per-kind and per-field metrics, twins, recorded responses |
| — | Zones with authors, owner identity, recipient role, template key, unit classifier, language-model band, structured output in `crates/llm` |

Provider-specific logic stays in provider crates. The judge reads only the facts record and talks to models through `crates/llm`.

## Delivery is five rungs, and the first only measures

They extend 32's ladder with letters (D184). R1c runs before R2, and the rest after R2 because they write into the stored assessment R2 freezes.

### R1c: Measure asks by kind and field

**Status:** built. `mxr classify eval --set quick|full` reports precision and recall per kind of ask, exact match for the amount, due and counterparty, duplicates and merges, twin-pair accuracy, reply owed and slices. The per-kind floor (`crates/daemon/src/handler/tests/classify_eval_floor.json`) fails the quick test and the full-set job on a regression. The surface-only leak guard is `scenarios/leak_guard.rs`. The recorded-response harness is `mxr_llm::recorded::RecordedProvider`; no eval question calls a model yet, so R2d is its first user. Zone, thread-position and rules-abstained slices move to R2c. Today's baseline is in [32](32-classifier.md#baseline-the-asks-by-kind).

**Previously:** the eval says 99.8%, and to-do recall on live asks is 7%. **Now:** the eval shows recall, precision and field accuracy per kind of ask, on every shape in this chapter.

**Promise:** "I can see how often mxr finds what an email asks me, and what it gets wrong, kind by kind."

**Build:** the label schema v2; per-kind and per-field metrics, slices, pair scores and the leak guard; generator families and twins for every shape above, including the en-ZA locale and multi-account senders; held-out template families; the recorded-response harness (built here, first used in R2d). No behaviour change.

**Done when:** `mxr classify eval --set quick|full` reports per-kind precision and recall and field accuracy, with today's baseline recorded in 32. CI fails on a regression in any kind. The surface-only baseline fails the twins.

**Left out:** any change to detection.

R2 stores home assessments while keeping the existing
asks detectors and To do/Archive aspects. The unit-level asks changes below
remain the next separate rung; R2 does not raise their measured baseline.

### R2c: Every to-do is something an email asks you, with whose move it is

**Previously:** marketing and credit notes make bills, your requests make your promises, and requests to you make nothing. **Now:** to-dos come from units the owner must act on, with the asking sentence as the reason.

**Promise:** "Every to-do is mine to do, says who asked, and quotes the sentence that asked."

**Build:**
- zones with authors, owner identity, recipient role, the template key;
- candidates with sign, currency and reference;
- units, and rules as voting functions;
- the document reading with required fields;
- the answer table, feeding To do, Your turn and promises from sent mail;
- item identity and lifecycle;
- the four-way correction on `X`;
- the "Might need you" line.

Retire `detect()`'s cascade, `directed_request` and `billing_document`. No model.

**Design:** To do rows, the reason line, the not-sure line and the correction picker, in 26/27's parts. Comps approved before building.

**New unknowns:** zoning quality on real mail (S6); the rules-abstain share (S1).

**Demo:** on the demo, every twin pair in R1c lands right where rules can decide it, and the rest land on the not-sure line, not as wrong rows.

**Done when:**
- R1c shows precision at or above the class targets on every kind the rules answer, and no regression elsewhere;
- on BK's mail (counts only), the six causes above produce no new wrong rows over a week of dogfooding;
- the design gate in 26 passes.

**Left out:** any model, and free-form person requests beyond the rules' reach (they go to not sure).

### R2d: A model answers the asks rules can't tell

**Promise:** "When a person asks me something in their own words, mxr catches it, and I chose which model reads it."

**Build:**
- JSON-schema structured output in `crates/llm`, with native clients where S4 requires them;
- the band judge, with its prompt, spotlighting, selection schema and verbatim check;
- the cloud opt-in screen and the processor check;
- recorded responses in the eval;
- disagreement as uncertainty.

**New unknowns:** local against cloud quality and cost on the band (S3); the providers' structured output (S4).

**Done when:** R1c shows recall on person requests up from R2c's baseline at the precision target, on held-out templates and `--set mine`. Each model the user can pick is measured and its scorecard recorded.

**Left out:** the unit classifier; history backfill with a model.

### R2e: mxr understands asks offline and on old mail

**Promise:** "mxr finds asks without a model server, and on my history too."

**Build:**
- the unit classifier (the SetFit head first, then the encoder chosen by S2), shipped in-process through `ort`;
- the training pipeline over the corpus and the strong tier's labels on synthetic mail;
- per-class thresholds;
- the band shrinks to what the classifier leaves;
- a bounded backfill through the first-run rules (22).

**Done when:** with no language model configured, R1c's recall on the band matches or beats R2d's local-model arm, and latency on an M1 is within the bench budget.

### R3e: Your corrections tune what counts as asking you

**Promise:** "When I say a kind of mail isn't a to-do for me, mxr asks me less often about mail like it."

**Build:** after R3's labels, per-user thresholds by class and sender kind, moved only after a minimum number of corrections (Q6), reported by `classify eval --set mine`.

**Left out:** fine-tuning the shipped model on real mail.

## Decisions

D174 to D185 are recorded in [15-decision-log.md](15-decision-log.md). In summary:

| ID | Decision |
|---|---|
| D174 | What an email asks is judged per unit of authored text, with speaker, addressee, act and state; the email's answer aggregates its units |
| D175 | Zones with their authors, owner identity and recipient role are facts; quoted and forwarded text is context, never the speaker's act |
| D176 | Organisational obligations come from a document reading (kind, payment status, required fields); no required field, no to-do |
| D177 | Rules become labelling functions that vote or abstain per unit; first-match-wins and whole-email vetoes are removed |
| D178 | Models by fit: a local unit classifier for the bulk; a language model only for the band, with a cloud fast tier on the user's key as the recommendation; a strong tier only for synthetic labels and audit |
| D179 | Scores stay inside the judge; per-class thresholds from the corpus; the screen shows a claim with its evidence, or not sure |
| D180 | A to-do's identity is counterparty, kind, reference and due period; reminders update and escalate; confirmations offer looks done |
| D181 | The eval grades asks per kind and field on the positives, with slices, twin pairs, held-out templates and recorded model responses; accuracy is not reported |
| D182 | One asks judge serves To do, Your turn, Waiting on and promises; `todo::detect`, `directed_request`, `billing_document` and the relationship worker's commitment extraction retire into it |
| D183 | Correcting a to-do says what was wrong (not a to-do, not mine, already done, wrong details), and each is its own label |
| D184 | Five rungs extend the ladder: R1c before R2; R2c, R2d and R2e after R2; R3e after R3 |
| D185 | A request that names a task becomes a to-do labelled with who asked; a request answered by replying stays Your turn (BK) |

## Open questions

### Spikes

| # | Question | Why it matters | Blocks |
|---|---|---|---|
| S1 | Once rules vote per unit, what share of units do they leave undecided, on the corpus and on BK's mail (counts)? | Sets the model load and cost | R2d sizing |
| S2 | A SetFit head on BGE-small, a fine-tuned ModernBERT-base or DeBERTa-v3-small, or zero-shot GLiClass/GLiNER2: which wins on held-out templates and twins, and at what latency through `ort` on an M1 and an M-Pro (with and without CoreML)? | Picks the shipped classifier | R2e |
| S3 | On the band slice: local Gemma 4 E4B or Qwen3-4B, against Claude Haiku 5.5, gpt-6-luna and one mid tier. Per-kind precision and recall, cost and latency | Decides the recommended model honestly | R2d |
| S4 | Do Anthropic's and Google's OpenAI-compatible endpoints honour strict `json_schema`, or does each need a native client? | Provider work in `crates/llm` | R2d |
| S5 | Which retention applies to Anthropic API calls without a ZDR agreement: 30 days, or not retained? | The opt-in screen's wording | R2d |
| S6 | How accurate is mxr's zoning (author versus other, and forward author) on the corpus and on a licensed public sample, against Zebra's 93.6% two-zone figure? | Every tag depends on it | R2c |

### Product questions for BK

| # | Question | Status |
|---|---|---|
| Q1 | Is an action asked of you by a person or organisation ("please send your meter readings") a to-do in To do, or only Your turn in Messages? | Decided by BK, 2026-10-22 (D185): a request that names a task (action and object) is a to-do labelled with who asked; one where replying is the whole job stays Your turn |
| Q2 | Where do asks the judge isn't sure of appear? | Open; blueprint assumes a collapsed "Might need you" line in To do plus Now's three daily questions |
| Q3 | Which mail may the band read, and when may it go to a cloud model? | Decided by BK, 2026-10-22 (D178): the model may judge requests on all non-list inbound mail, on loopback, and reaches a cloud model only under the existing `allow_cloud_background_classification` rule with the user's key; where the task needs more intelligence, mxr recommends the cloud fast tier |
| Q4 | Are group asks ("please upload your CV", you among several in To) and asks to someone you share an account with yours? | Open; blueprint assumes group asks in To are yours, Cc'd asks are not, and asks to a named household member are yours |
| Q5 | May a model claim that lands after arrival (slow local model, batch) change an email the person has seen? | Open; blueprint assumes it may add a to-do marked "found later" but never remove one the person has seen |
| Q6 | How many corrections of one kind before your threshold moves? | Open; blueprint assumes 10 per class |
| Q7 | Is a renewal charged to a card on file a to-do? | Open; blueprint assumes it's an update unless the email asks you to act, and yearly renewal to-dos from records stay automatic (32) |
