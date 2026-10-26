# 34. The composer: writing mail, and drafting in your voice from what you wrote

Status: proposal, 22 October 2026. Source baseline: origin/main at
`b3717dbf2`. Design only; no code has changed. Code references are at
that commit unless marked.

Writing an email in mxr is spread over two crates, eleven daemon handler
files, a web bridge, four clients and four model settings, and each
client does something different when you ask for help. The classifier
had the same problem and became one system with one owner ([32](32-classifier.md)).
This chapter does the same for writing: one subsystem, the composer,
with typed contracts, a pure core fed plain data, rules that come from
one place, a labelled corpus that measures it, and every client drawing
the same thing.

| Part | Its one job | It never |
|---|---|---|
| The draft | Hold what you are writing: who it's to, the subject, your text, the signature, attachments and the message it answers | lives only in one client, or sends without the safety checks |
| Writing help ("Draft for me") | Propose text for your draft from what you typed, the conversation and your own past mail | changes your draft without you accepting, invents a fact, or follows an instruction found in mail |
| Style rules | Say which AI-writing patterns a text has, measured against how you write | calls a model to judge, or blocks a send |
| Send | Run the safety checks, hold the undo window, schedule | sends a draft with an unfilled gap |
| Surfaces | Show the same draft, the same proposal and the same model choice in the web app, the desktop app, the TUI, Raycast and the CLI | build prompts, hardcode instructions, or decide reply versus forward |

The draft format and the `$EDITOR` flow stay as [06](06-compose.md)
describes them (D009). Pre-send safety stays in `crates/safety` as
[`docs/reference/ai-email.md`](../reference/ai-email.md) describes it, with one check added here.
This chapter supersedes how drafting is assembled and shown: the
`DraftCompose` and `DraftRefine` requests, the humanizer rewrite pass,
the three draft model settings, and the compose sheet's assist panel and
tone controls in [28](28-web-screens.md) ("Compose").

## The composer makes four promises, and today's code keeps none of them

**What you typed is kept.** BK, 22 October 2026: "if I select draft for
me while I already have something typed in the box, it should take what
I have already drafted into account and not just give me a draft that
ignores it entirely." Today the model never sees your text:
`Request::DraftCompose` has no field for it (`crates/protocol/src/types.rs:1528`).
What happens to your text then depends on the client:

| Client | What "Draft for me" does with what you typed |
|---|---|
| Web compose | Replaces the whole body with the model's text, signature included (`apps/web/src/features/compose/session/useDraftAssist.ts:96`) |
| Web reader panel | Prefills a new draft above nothing; your reply field is separate (`apps/web/src/features/thread/DraftAssistPanel.tsx`) |
| Raycast reply | Puts the model's text above yours, so the reply holds both (`apps/raycast/src/components/ReplyForm.tsx:192`) |
| TUI | Shows the text in a pane; nothing reaches the draft (`crates/tui/src/runner.rs`, the draft-assist result) |

**It sounds like you, not like a model.** The prompt is good at this
already: it leads with your real emails to this person and marks unknowns as
`[[?: …]]` gaps instead of guessing (`crates/daemon/src/handler/draft_prompt.rs:104-124`).
The parts around it undo some of that:

- The style rules the prompt carries are one sentence, written from an
  older humanizer with 24 categories (`crates/humanizer/src/lib.rs:296`).
  The humanizer skill BK uses is now 3.1.0, with 26 patterns in six groups
  and a rule that the writer's own sample outranks every pattern.
- A second model pass rewrites the whole draft when the score is low,
  capped at 600 tokens, without checking that it finished
  (`crates/daemon/src/handler/humanizer.rs:150`).
- The tone and length the chip says it inferred never reach the prompt
  (`draft_context.rs` infers them, `draft_compose.rs:269-283` builds the
  prompt without them). Rebuilding your voice profile changes the chip,
  not the draft, because the profile and the prompt's examples are two
  different models of your voice.

**You choose which model writes.** BK: "I would like to also have a model
selector for these drafting jobs, there's a default model that i can
select in the settings but during drafting if I want I could choose a
different one." Today the draft model comes from three config keys
(`draft_assist`, `draft_new`, `draft_refine`) plus `humanize_rewrite`, none
of them in the apps, and no request can name a model. The assistant has a
picker (D145) that drafting doesn't use, even though the assistant can draft
through the same request (`mxr_draft_assist`, `crates/mcp/src/lib.rs:1727`).

**Every client is the same composer.** BK: "it just feels like a hodgepodge
of stuff." Some causes are in the contract, not the pixels:

- A reply becomes a forward note when the first To isn't in the thread
  (`draft_compose.rs:67`), and the web app sends the first To, so editing
  To on a reply silently changes what gets drafted (`useDraftAssist.ts:46`).
- Refine works only on a saved draft (`useDraftAssist.ts:80`), has its own
  prompt and retry loop, and sees no conversation (`draft_refine.rs`).
- Raycast hardcodes the instruction and drops the provenance, so its text
  arrives unmarked (D136).
- The TUI's "Draft: New For Sender" puts the email's subject into the
  instruction, which sits outside the untrusted-mail markers
  (`crates/tui/src/app/platform_actions.rs:47`).
- `mxr draft` and `mxr draft-assist` overlap.
- The reader's "Draft in your voice" opens a panel and waits; `d` in focus
  mode and Raycast generate at once.

## One shared model: the draft, the request, the proposal

Three nouns, used by every client and every part of the daemon.

**The draft** is what 06 already defines: frontmatter and a body, stored
by the daemon, autosaved. The composer adds one idea: the body has
regions. **Your text** is what you wrote. **The signature** is the block
`insert_signature_block` adds (`crates/compose/src/lib.rs:193-196`).
**The forward header** is the `---------- Forwarded message ----------` line.
The quoted conversation is already outside the body, in the context block.
Writing help reads and proposes only your text. The signature and forward
header are never sent to the model as text to edit and never come back
changed, as Mimestream does with Apple's Writing Tools.

**The request** says what you want help with:

```rust
/// Replaces DraftCompose and DraftRefine.
Request::ComposeAssist {
    /// A stored draft, or the start of one.
    draft: DraftTarget,
    /// Your text as it is now, signature and forward header removed.
    /// Empty when you haven't written anything.
    text: String,
    /// A byte range of `text` to work on; the rest is kept as it is.
    selection: Option<Range<usize>>,
    /// None: the daemon picks from the draft's state (below).
    action: Option<AssistAction>,
    /// What you want to say or change, in your words. Optional.
    instruction: Option<String>,
    /// Chips: shorter, warmer, more formal, proofread.
    adjust: AssistAdjust,
    /// None: the drafting default (D156).
    model: Option<String>,
}

enum DraftTarget {
    Stored { draft_id: DraftId },
    Reply { message_id: MessageId, all: bool },
    Forward { message_id: MessageId, to: Vec<Address> },
    New { account_id: AccountId, to: Vec<Address>, subject: String },
}

enum AssistAction { Write, Finish, Polish, EditSelection }
```

The reply kind is explicit (D155). The recipients of a reply are the
draft's, not a hint about what to write.

**The proposal** is what comes back. It never touches the draft:

```rust
struct ComposeProposalData {
    proposal_id: ProposalId,
    action: AssistAction,
    /// Hash of the `text` it was made from, so applying it to text that
    /// has changed since is caught.
    base: TextHash,
    /// The proposed text for your text region, whole.
    text: String,
    /// Word-level changes from your text: kept, added, removed.
    changes: Vec<TextChange>,
    /// `[[?: …]]` gaps, with their ranges in `text`.
    gaps: Vec<GapData>,
    /// Patterns the style check still finds, beyond your own rate.
    style: Vec<StyleMarkData>,
    /// Model, where it ran, the examples and turns it saw (D136).
    provenance: DraftProvenanceData,
    /// Sentences of yours the proposal changed or dropped, by index.
    yours_changed: Vec<usize>,
    composer_version: String,
}
```

The daemon computes `changes` with the `similar` crate's word diff
(already in `Cargo.lock` through insta), so the
web app, the TUI, Raycast and the CLI draw the same marked-up text without
each writing a diff. Applying is the client's: the web editor and Raycast's
form hold the live text, so they apply `text` themselves. The CLI, the TUI
and agents use `ApplyComposeProposal { proposal_id, draft_id }`, which
writes exactly the proposed text when the draft's text still hashes to
`base`, and refuses otherwise. The daemon keeps proposals in memory for 30
minutes. The proposal is the preview, and applying it can only write what
was previewed (AGENTS.md's preview-equals-apply rule).

## What you get depends on what you've written

The draft's state picks the action, as Gmail, Superhuman, Front and Apple's
Writing Tools all do. One action runs whatever fits: `⌘J` on the web, `r` on the TUI's send
review (the palette's "Draft for me" in the reader), and "Draft for Me"
in Raycast's action panel. Keys follow chapter 35 (D164):

| Your draft | Action | What the model is told |
|---|---|---|
| Empty | **Write** | Write the reply (or the new email) from the conversation and your instruction. Unknowns become gaps. |
| Has text, nothing selected | **Finish** | These are your points, in your words. Keep every point and keep your sentences. Add what the email still needs (a greeting and sign-off the way you write them, the answer to each question in the TARGET you haven't covered, a closing line if you'd write one). Don't add points you didn't make; mark what only you can decide as a gap. |
| Has text, some selected | **Edit selection** | Rewrite only the selected sentences, following the instruction or chip. Return the whole text with everything outside the selection unchanged. |

**Polish** is the one secondary action: keep your structure and points,
fix grammar, tighten, remove AI-writing patterns, add nothing. It sits
beside Finish, not under it.

The chips (shorter, warmer, more formal, proofread) adjust whichever
action runs. They replace the register and length dropdowns and the four
refine buttons. Each chip is one line in the task, not a separate prompt,
so refining is the same request run again on the proposal's text.

**Finish keeps your sentences word for word** (blueprint assumes; Q8).
It changes one only to fix a typo or join it to an added sentence, and
`yours_changed` lists each one, so the proof shows exactly what moved. The
Coman and Cardon study of 1,100 professionals
([UF News, 2025](https://news.ufl.edu/2025/08/writing-ai-work/)) found messages seen as
sincere 83% of the time with low AI help and 40–52% with high AI help,
and the drop was largest for thanks, praise and personal feedback. Those
are the emails where your own words matter most, so a draft that starts
from yours keeps them.

Your text goes into the prompt inside the untrusted-mail markers, like
the conversation, because a reply you started can quote the email you're
answering. The instruction you type in the help line stays outside them.
An instruction a client builds from mail (the TUI's "New For Sender") moves
into the conversation block, never the instruction.

## Writing help runs in five stages, and only two call a model

```
gather (daemon) → plan (pure) → write (model) → check (pure) → repair (model, at most once)
                                                       ↓
                                                   proposal
```

**Gather** is the daemon handler, `handler/compose_assist.rs`. It loads
what the composer needs and passes it as plain data:

- the draft and its regions;
- the conversation with the TARGET marked, using `draft_voice::conversation`;
- your voice (below);
- the relationship summary;
- the model and the privacy policy (`DraftPolicy::pin`).

The policy decides what may go to that model. A cloud model sees your other
mail only with `llm.allow_cloud_relationship_data` on, as now.

**Plan** is pure, in a new crate, `mxr-composer`. It picks the action from
state, sets the target length, decides what must be kept, applies the
budget and builds the prompt. `draft_prompt.rs` moves here nearly as it is;
its block order and its untrusted markers are sound. The new blocks are
`[MY DRAFT SO FAR]` for Finish and Polish, `[EDIT ONLY THIS]` for a
selection, and the style rules built from the catalogue (below), replacing
`writing_constraints()`.

**Write** is one call to the chosen model through `complete_with_retry`,
which keeps its retry for an empty completion (v0.6.31's thinking-model
fix).

**Check** is pure, also in `mxr-composer`, and runs on every proposal:

- `clean_draft` (moved from `draft_output.rs`): strip a subject line,
  preamble, markdown fences and a repeated signature.
- **Regions.** `mxr-compose` owns the shared body partition used by clients and
  the checker. The output may not contain the signature or forward header; if
  it does, they are removed.
- **Kept.** For Finish, Polish and Edit selection, each of your sentences
  is matched in the output; a missing or changed one goes into
  `yours_changed`. For Edit selection, any change outside the selection is
  reverted, not just reported.
- **Gaps.** Find each `[[?: …]]` and its range.
- **Invented facts.** Numbers, dates, amounts and links in the output that
  appear in neither the conversation, your text nor your instruction.
  `draft_eval.rs` already has `invented_numbers`. Each one becomes a gap
  with the model's value shown beside it ("[[?: price — the draft said
  £40]]"), because a figure the model guessed is the error people least
  forgive.
- **Style.** Run the catalogue's detectors and compare with your own rate
  (below).

**Repair** runs at most once when a supported style mark exceeds your own
rate and `humanizer.auto_fix` is on (default on). It sends only marked,
generated sentences and their catalogue instructions to the same pinned
model. Any sentence span that overlaps kept user text is ineligible; when the
primary proposal changes a typed sentence, the proposal keeps that warning
for you to review. A repair is accepted only when the style score improves,
the exact gap placeholders and typed sentences stay the same, and each
eligible sentence keeps its numbers, dates, amounts and links. A failed,
malformed or rejected repair leaves the primary proposal and its marks intact.

At most two model calls per proposal, and one is the norm. The live line
says which stage is running ("Reading the conversation", "Writing with
gemma4", "Checking"), in the copy desk's wire voice ([29](29-assistant-design.md)).

## One voice, built once per draft, and what's shown is what was used

Today there are two models of how you write. The prompt uses habits and
up to five real emails (`draft_voice::voice_material`). The chip and the
Tone Mismatch safety check use stylometry from `contact_style`. They can
disagree, and only the first one writes.

The composer builds one `Voice` per draft:

```rust
struct Voice {
    /// Up to five of your sent emails, this person first, then similar
    /// threads, each with what it answered.
    examples: Vec<VoiceExample>,
    /// Plain sentences: greeting, sign-off, length, punctuation.
    habits: Vec<String>,
    /// From stylometry with this person, else overall: words, register.
    usual_length: Option<u32>,
    usual_register: Option<VoiceRegisterData>,
    /// Your own rate of each style pattern, from your sent mail (below).
    pattern_rates: PatternRates,
}
```

The usual length and register go into the task line as habits ("With Pat
you usually write about 40 words, casually"), unless a chip overrides
them. The provenance line names what shaped the draft ("Written with
gemma4 on this Mac, from 3 of your replies to Pat"), and each example opens
the email it came from. Research on few-shot style transfer, and
Shortwave's account of building Ghostwriter, both found examples of your
own mail beat a generated style description, with gains flattening beyond
a handful. That is why examples lead the prompt and stay capped at five.

With `allow_cloud_relationship_data` off and a cloud model chosen, the
voice is empty and the proposal says so, as `CLOUD_NOTE` does today.

## The style rules are humanizer 3.1, in one catalogue, used three ways

BK, 22 October 2026: "drafting should also understand all the things in
/humanizer", and the humanizer updates "will need to be baked into the mxr
composer". The skill was updated the same day to upstream 3.1.0 with BK's
additions (`~/.dotfiles/.skills/humanizer/SKILL.md`, version `3.1.0+bk.1`).

`crates/humanizer` becomes the catalogue. Each of the 26 patterns is one
entry with:

- its number and group as the skill numbers them (A staging, B rhythm,
  C inflation, D formatting, E leftovers, F wrong reader);
- a detector where the pattern can be found deterministically, or
  `prompt_only` where it can't (§5 arguing with no one, §26 re-explaining
  what the reader knows);
- one positive instruction for the prompt, saying what to do rather than
  what to avoid ("Say the point directly. Don't set it up as 'not X but
  Y'"), because Superhuman's guidance and the skill both find positive
  rules work better than bans;
- its strength: §1–§5 count on one sighting, and patterns the skill marks
  *weak alone* count only beside another tell in the same paragraph;
- whether it applies to email. §20 (decorative headings) and §24 (a
  heading repeated in the first sentence) rarely do; they stay in the
  catalogue for other text and are left out of the email prompt.

The catalogue carries `HUMANIZER_VERSION = "3.1.0+mxr.1"`, and a test
fails if the skill's pattern list and the catalogue's disagree in number
or name. The skill is vendored as a fixture, so the check runs in CI
without BK's dotfiles.

The catalogue is used three ways:

1. **In the prompt.** The style block lists the email patterns as positive
   instructions, strongest first, in about 20 lines.
2. **In the check.** Every proposal is scored, and a supported pattern
   counts only above your own normalized rate. The skill's rule is that the
   writer's sample outranks every pattern, dashes included. `PatternRates`
   is computed locally for each request from up to 200 recent sent messages;
   rates are normalized per thousand words. With no sent mail, every rate is
   zero. The derived rates stay out of cloud prompts and provenance; any
   source emails follow the existing cloud-history opt-in.
3. **In the eval.** Recorded outputs include style scores and catalogue
   marks beside the pass/fail fact and kept-text checks. Style is reported,
   not a pass/fail threshold.

The legacy API retains 24 categories. Categories with direct catalogue
equivalents contribute to pattern rates through this map:

| Today | 3.1 pattern |
|---|---|
| `NegativeParallelism` | §1 Not X but Y |
| `GenericPositiveConclusion` | §2 One-line closers |
| `FigurativeLandscape`, `FigurativeKey` | §3 Sayings that sound deep |
| `RuleOfThree` | §6 Forced triads |
| `EmDashOveruse` | §8 Dashes as the universal connector |
| `HedgingStack` | §9 Stacked qualifiers |
| `AiVocabulary` | §12 Overused AI words |
| `HighlightVerb`, `ChallengesProspects` | §13 Inflated significance |
| `IngTailClause` | §15 Shallow -ing riders |
| `PromotionalLanguage` | §16 Sales language |
| `CopulaAvoidance` | §18 Avoiding is, are and has |
| `ExcessiveBoldface`, `InlineHeaderColonList` | §19 Bold as decoration |
| `EmojiHeading`, `TitleCaseHeading` | §20 Decorative headings |
| `CurlyQuotes` | §21 Curly quotation marks |
| `SycophanticOpener`, `CollaborativeArtifact` | §22 Chatbot residue |
| `KnowledgeCutoffDisclaimer` | §23 Knowledge-limit disclaimers |
| `FillerPhrase` | remains detectable by the legacy scorer, but has no catalogue rate mapping |
| `FalseRange`, `Profanity` | kept as mxr's own, outside the 26 |
| — | new: §4 staged run-up, §5 arguing with no one, §7 repeated openings, §10 hyphenated pairs, §11 missing subjects, §14 vague association, §17 borrowed authority, §24, §25 writing about the document, §26 re-explaining |

§26 matters most in email. A reply that recaps the thread back to the
person who wrote it is the commonest robotic reply, and the prompt already
says "Don't repeat the conversation back" (`draft_prompt.rs:119`). The
catalogue makes it a rule the eval measures, instead of one line in a list.

The score (D092) stays deterministic and local. The quality badges on the
draft (`DraftQualityBadges.tsx`) become the proposal's style marks: quiet
underlines on the sentences that carry a pattern, with the pattern's name
on hover. The number moves under "Details".

## One model setting for drafting, with a pick per draft

Drafting gets one setting, `[llm.compose] model`, shown in Settings >
Compose as a list. With nothing set it inherits `[llm]`. It replaces
`llm.overrides.draft_assist`, `draft_new`, `draft_refine` and
`humanize_rewrite`. Those keys keep loading and warn once, naming the new
key, as D144 retired settings. Repair uses the model that wrote.

The list is one catalogue shared with the assistant. `llm.assistant_models`
becomes `llm.models`, and the old key still loads. The assistant keeps its
own pick in `[llm.overrides.assistant]`. Each entry is
`ModelChoiceData { id, label, cloud, available, unavailable_reason, sees }`,
generalised from `AssistantModelChoiceData`. `sees` is new: "this
conversation and your past mail" or "this conversation only". It is
derived from the cloud flag and `allow_cloud_relationship_data`, so a
cloud pick is clear about what it can't use.

Per draft, the model mark in the help line ("gemma4 · on this Mac") opens
the list. The pick lasts for that draft only, and the next draft starts on
the setting, because a default that silently changes is how the wrong model
ends up reading mail. "Try again with…" in the proof runs the same request
on another model. The CLI takes `--model`, the TUI `m` in the help line,
and Raycast a "Draft with Model" submenu.

Drafting never turns `allow_cloud_relationship_data` on. Whether a cloud
model can be picked for drafting while it is off is Q9; the blueprint
assumes it can, labelled "this conversation only", because the draft then
sends only the conversation the cloud model is answering, and the policy
already enforces that (`draft_provenance.rs`).

D114's tiers don't apply to drafting: it's always one model, the one you
chose.

## A draft with an unfilled gap doesn't send

A seventh pre-send check, **Unfilled gap**, finds `[[?:` in the body and
blocks the send, naming each gap ("2 gaps left: the date, the price").
Superhuman warns at send time for the same reason. It is a blocker rather
than a warning because nobody means to send `[[?: the date]]`, and filling
it is one edit. A scheduled send with a gap keeps the draft, clears the
schedule and emits the event, as every blocker does. In the editor, `Tab`
moves to the next gap.

## Quality is measured on a labelled drafting corpus before anything changes

The classifier's lesson ([32](32-classifier.md), "A change is a redesign
checked against the corpus") applies here: change the composer only
against cases that say what a good draft must do.

**The corpus** lives in `crates/provider-fake/src/fixtures/compose/`. Each
case is a thread, a sender voice (a few sent emails), the draft's state
(empty, typed text, a selection), the action and chips, and what the
proposal must satisfy:

- keep these phrases;
- answer these questions from the TARGET;
- leave a gap for this missing fact;
- name no number, date or link not in the inputs;
- stay within this length band;
- no style marks beyond the voice's rate;
- keep the signature region untouched.

Cases come in near-miss pairs, as the classifier's twins do: the same
thread with and without typed text, a voice that uses dashes and one that
doesn't, a question answered in your text and one left open. The first set
is 40 cases. Typed text is the largest group.

**Two layers.** The plan and check stages are pure, so a daemon test
snapshots, with insta, the prompt for every case and the check result for
recorded model outputs. The recordings include the bad ones: a subject
line, a recap of the thread, a dropped sentence of yours, an invented
price, a duplicated signature. Every prompt or rule change then shows up
as a reviewed diff, with no model in CI.

`mxr draft eval --set quick` runs the live layer on this machine with the
drafting model you have configured. It drafts every case with a real model
and grades it with the same checks, and its numbers are the live baseline.
Choosing the model per run (`--model`) waits for rung C2. `--set mine` is
today's replay of your own recent replies. It stays on this machine, and no
longer calls the rewrite model unless asked.

`composer_version` combines the prompt's version and the catalogue's, and
goes into every proposal's provenance, so a bad draft can be traced to
the composer that wrote it.

### Baseline: today's drafting on the quick set

The snapshot layer is measured by `scripts/cargo-test -p mxr --lib -- handler::tests::compose_eval`.
Its record is in `crates/daemon/src/handler/tests/snapshots/`: the prompt
today's code builds for each of the 40 cases, and the grade table for 32
recorded model outputs. The corpus is `crates/provider-fake/src/fixtures/compose/`:
40 cases in 20 near-miss pairs, 22 with typed text (two of them with a
selection) and 18 without.

The 32 outputs are good ones and recorded defects, so the counts show what
the checks catch. They do not say how often a model writes the defect.

| Check | Passed | Of |
|---|---|---|
| Kept: typed sentences survive, as whole sentences | 29 | 32 |
| Answers: each question from the target is addressed, as whole words | 30 | 32 |
| Gaps: facts nobody gave are left as gaps | 31 | 32 |
| Invented: no number outside the inputs, compared as whole tokens | 29 | 32 |
| Length: inside the case's word band | 31 | 32 |
| Leftovers: no subject, preamble, repeated sign-off or copied run | 28 | 32 |

Every good output passes all six checks. Every bad output fails the check it
is recorded for: a dropped typed sentence and a typed sentence reversed inside
a changed one (kept), an invented price (invented, and gaps or answers where
the price was the answer), "now" standing in for "no" (answers), a subject
line, a preamble, a recap of the thread and a repeated sign-off (leftovers).

Today's request has no field for typed text or a selection, so neither
reaches the prompt. Each typed case's prompt in the snapshot says so. That
is the gap the next rung closes, and `kept` is the check it should move.

Three rules are judgement calls. A typed sentence is kept only when the draft
has it as a whole sentence, so a model that rewords it with a comma or a
prefix fails `kept` as "changed", which is the strictest reading of "keep
what I typed". A leftover is a run of eight words copied from the thread,
which catches a recap that quotes the thread but not one written in the
model's own words; length is the only check that catches that. A preamble is
commentary about generating the reply: a line that opens with a lead word
such as "Sure" or "Here's" and names the reply or its draft ("draft of your",
"my reply", "drafted"). A line that only mentions a draft document is part of
the email. A repeated sign-off is the same short line appearing twice in the
last four lines of the output. Numbers are compared as whole tokens: a
thousands comma, a `.00` cent part and a space before am or pm do not change
the number, so "£1,200.00" matches "£1,200" and "3 pm" matches "3pm", while
"2pm" and "12pm" stay apart.

The live baseline runs `mxr draft eval --set quick` against a real model, on
2026-10-22, with `gemma4:latest` through a local Ollama endpoint on this Mac.
The current table is graded with the rules in this section:

| Check | Passed | Of |
|---|---|---|
| Kept: typed sentences survive | 20 | 40 |
| Answers: each question from the target is addressed | 29 | 40 |
| Gaps: facts nobody gave are left as gaps | 38 | 40 |
| Invented: no number outside the inputs | 40 | 40 |
| Length: inside the case's word band | 40 | 40 |
| Leftovers: no subject, preamble, repeated sign-off or copied run | 40 | 40 |

An earlier run, graded before the whole-sentence, whole-word, exact-number and
preamble rules, gave kept 20, answers 31, gaps 38, invented 40, length 40 and
leftovers 39. Its outputs were not kept, so it cannot be graded again under the
current rules. The difference in answers and leftovers mixes the rule change
with run-to-run variation.

Runs vary by up to four cases per check. `kept` fails for most typed cases
because typed text is not sent yet; the 18 cases with nothing typed pass it by
construction.

### After rung C1: the typed text is sent

`ComposeAssist` sends the typed text (Finish) and the selection (Edit
selection), and `mxr draft eval --set quick` now drafts every case through the
same plan. The prompt snapshot shows the change: each typed case gains a
`[MY DRAFT SO FAR]` block inside the untrusted-mail markers (a selection also
gains `[EDIT ONLY THIS]`), and its task now says to keep every sentence word
for word instead of asking for a fresh reply.

Two live runs on 2026-10-23 with `gemma4:latest` through the same local
Ollama endpoint, graded with the rules above, before the check stage touches
the answer (the grades measure the model and the prompt; the check stage is
the safety net on top):

| Check | Baseline | Run 1 | Run 2 | Of |
|---|---|---|---|---|
| Kept: typed sentences survive | 20 | 40 | 40 | 40 |
| Answers: each question from the target is addressed | 29 | 39 | 40 | 40 |
| Gaps: facts nobody gave are left as gaps | 38 | 37 | 37 | 40 |
| Invented: no number outside the inputs | 40 | 40 | 40 | 40 |
| Length: inside the case's word band | 40 | 40 | 40 | 40 |
| Leftovers: no subject, preamble, repeated sign-off or copied run | 40 | 40 | 40 | 40 |

`kept` moved from 20 to 40: all 22 typed cases now keep their sentences, the
18 empty ones pass it by construction. `answers` moved from 29 to 39 and 40,
which is mostly the same cause: a typed answer now reaches the model instead of
being dropped. Gaps did not move. The three cases that miss it in both runs
(`date-open`, `seats-open`, `rate-open`) are ones where the model left a gap
that does not name the fact ("by [[?: time]]" for a date, "[[?: number]]
seats") or wrote around it with no gap at all ("the rate per person"). Rung
C3's repair pass is the place to move it. Style mean was 98.7 and 99.0.

## The compose screen: your page under the lamp, help as a proof

The screens are drawn, with every state, key and check, in
[36](36-compose-design.md). This section is the reasoning behind them.

**What it is to the person.** BK writes replies at night, at a keyboard,
to the few people who matter (26, "What this is to the person"). The reply
is the reason he opened the conversation. He wants to type his point,
maybe get help finishing it, and send it sounding like himself. The
screen today shows him every control at once, and each way into a reply
behaves differently. Observed on mxr 0.6.140 against the demo mailbox,
22 October 2026, from 38 screenshots (not kept in the repo):

| Where | What happens today |
|---|---|
| A new message | The docked panel, top to bottom: four layout buttons, From and `⋯`, a "Maybe cc:" row, To with Cc and Bcc buttons, Subject, a "Draft for me" row that expands to a textarea, Generate, "Tone auto-matched" with Adjust (Register, Length), a Refine context field and four refine chips, the editor, attachments, then Send, ▾, a Send later clock, Attach, Rich text/Markdown and save status |
| Reply from Messages | Opens inside the reader pane, in the layout labelled "Inline". It's taller than its scroll area, so Send starts off-screen |
| Reply with `r` in the reader | Opens the docked panel over the thread, from a different account than Messages chose |
| "Draft a reply" in the reader | A second AI surface with its own wording ("Draft assist", "Tone will match how you write to this thread"), Reply with this and Copy, then a composer with its own "Draft for me" row. The voice came from one person and the To from another |
| After Generate | Your typed text is gone with no undo. "Voice 90% · high" and "Humanizer 94/100" show twice, and two provenance lines say the same thing |
| Refine chips | Greyed out unless the draft was opened from the Drafts list |
| Layouts | "Inline" is a floating window, "Popout" is the docked panel, and Messages' "Inline" is embedded in the reader |
| Drafts | Compose reopens the last unsent draft instead of a new one; three replies to one thread made three drafts |
| Editor toggle | Switching to Rich text in one draft silently changes the setting for all |
| Signatures | Exist in Settings, and nothing in the composer shows or inserts one; the pickers are keyboard-only |
| Context | No reply composer shows the message it answers |

Each of these carries the same weight on screen, which is the hodgepodge
BK describes.

**The concept: the page and the proof.** It extends the night desk ([26](26-web-design-concept.md)):

- **Your text is the page under the lamp.** It is the one lit thing on the
  screen, and everything else folds into a single line or a menu until
  needed.
- **Draft for me is the copy desk.** The desk is across the room, as in
  [29](29-assistant-design.md). It sends back a **proof** of your page: your words
  in ink, its additions underlined in the lamp's cyan, its cuts struck
  through in the third text tier, and its gaps as yellow fill-ins.
- **You sign the proof or send it back with a note.** It never lands on
  your page on its own.

The same proof appears in the TUI pane, in Raycast and as CLI JSON.

**The reply sits where reading ends, the same way from every entry.** On
the web a reply opens inline under the message it answers, whether it
started from Messages, `r` in the reader, Focus or the `⋯` menu. Fastmail
(since March 2026) and HEY's Focus & Reply work this way, and Messages
half does today.

- There are two placements, named for what they are: **in the
  conversation** (every reply) and **the sheet** over the scrim (a new
  message, or a reply moved there with `⇧⌘F`, today's full-screen chord).
  Full screen becomes a button on the sheet. The floating window goes.
- The inline reply grows with your text and keeps Send in view: the
  reader scrolls, not the composer.
- The message it answers is right above it, so quoted text stays hidden.
- The signature is a faint fold under your text that opens on click, and
  its picker lives there.
- One unsent reply per conversation: starting a reply on a conversation
  that has one resumes it. Compose starts an empty message, and unsent
  ones wait in Drafts.
- Draft for me lives only in the help line. The reader's separate "Draft a
  reply" panel goes; its menu item opens the reply with the help line
  already running.
- The editor toggle changes this draft only. Settings > Compose changes
  the default.

**Recipients are one line until you touch them.** "To Pat Lee · Reply all"
is the whole header of a reply, in meta.

- Clicking it, or today's `⇧⌘C`/`⇧⌘B`, opens To, Cc and Bcc.
- Switching between reply and reply all recalculates them, as Fastmail does.
- From shows as the account chip (D147). It opens only when the account has
  more than one address to send as.
- The subject of a reply is hidden, since it is "Re:" plus the thread's.

**Help is one line under the page, not a panel.** `⌘J` opens it with the
cursor in a single instruction field. The field's placeholder names the
action the draft's state picked: "Finish my draft", "Draft a reply" or
"Edit the selection". After the field come the chips (Shorter, Warmer,
More formal, Proofread), then Polish, then the model mark at the right.

- `↵` runs it. While it runs, the live line replaces the field, in the
  wire's mono, and `Esc` cancels.
- The proof then replaces the page in place. The text doesn't jump: kept
  words stay where they were, and additions grow in with the landing
  motion (D134).
- Under the proof are its keys: `↵` Accept, `⌘J` Revise with a note,
  "Try again with…" for another model, `Esc` Keep mine.
- The provenance line is under the keys, in meta.
- Back and forward arrows step between this draft's proofs, as Gmail's
  versions do.

**After Accept, model text stays marked until you edit it** (D136). Each
added span carries a hairline mark that clears when you edit it. `⌘Z` undoes
the whole accept in one step. Style marks are underlines on the sentences
that carry a pattern. They aren't badges or a score.

**Send is the one filled button.**

- Its menu holds Send later, Send and archive, and Send and remind.
- The undo countdown stays (v0.6.130).
- Attach is an icon. Files show as chips under the text.
- Rich/Markdown, snippets and signatures move to the `⋯` menu and the
  palette, with their keys kept.
- A blocked send says why in one line above the button, naming the gap or
  the check.

**Each moment, and how it should feel:**

| Moment | What happens | It should feel |
|---|---|---|
| Opening a reply | The cursor is in the page, under the message, with recipients as one line | like you were already writing |
| Asking for help | One line opens with the right action already named | like asking someone at the next desk, not opening a tool |
| Waiting | The wire line names the stage; nothing else moves | short and accountable |
| The proof | Your words in ink, its changes marked, gaps yellow | like reading an editor's marked-up copy of your own letter |
| Accept | The marks settle into the page; model spans keep a hairline | yours, with help you can still see |
| A failed draft | "gemma4 didn't answer in 60s. Try again, or pick another model." in the line, with the keys | plain, with the next step |
| Send blocked by a gap | "1 gap left: the date" above Send; `Tab` jumps to it | caught, not scolded |

**Left out or hidden until asked:**

- the register and length dropdowns and the refine context field, which
  become chips and the one instruction field;
- the score number, under Details;
- the Cc/Bcc buttons, which wait in the recipients line;
- the subject on a reply;
- Rich/Markdown, snippets and signatures, which move to `⋯`;
- the sparkle on "Draft for me" in 28's mockup, which goes (D141).

**The TUI keeps `$EDITOR`** (D009), and help works on the file you're
writing:

- The send review after the editor closes becomes "Send this reply?" in
  aerc's order (headers, body, attachments, checks, keys). `r`, refine's
  letter today, runs the action the draft's state picks. In the reader,
  "Draft for me" stays in the palette until it earns a key (D164).
- `Esc` on the review goes back to the draft; discarding is `#` "Discard
  draft" (chapter 35).
- The proof shows in the pane: additions in the accent colour, cuts struck
  (or prefixed `-` under `NO_COLOR`), gaps in the warning colour.
- `y` applies it through `ApplyComposeProposal`, `e` applies it and reopens
  the editor, `n` sends it back with a note, `m` picks the model, and `Esc`
  keeps yours.

**Raycast keeps its form.**

- "Draft for Me", renamed from "Draft in My Voice", sits in the action
  panel with no shortcut until it earns one (chapter 35 moves `⌘D` off
  it). It sends the field's text with the reply kind.
- It pushes a Detail view with the proof as markdown (added in bold, cut
  struck through, gaps as inline code) and the provenance in metadata.
- Accept (`↵`) writes the text into the field; `⌘R` tries again and
  `⇧⌘R` tries again with another model, Raycast's own conventions.

**The desktop app** is the web app in the Tauri shell, so it gets
everything above.

**The CLI is the contract** the others draw:

- `mxr compose assist (--draft <id> | --reply-to <message> [--all] | --forward <message> --to <addr> | --new --to <addr>)`
  takes `[--text <file>|-] [--select <start>..<end>] [--finish|--polish|--write] [--instruction <text>] [--shorter --warmer --formal --proofread] [--model <id>] --json`.
  It prints the proposal.
- `mxr compose apply <proposal-id> --draft <id>` applies it.
- `mxr draft-assist` and `mxr draft refine` stay as aliases for one
  release, then go, and `mxr draft` keeps managing drafts.

## The invariants hold throughout

- Email is data, never instructions. Mail-derived text, including your
  draft so far, sits inside the untrusted markers. Only the composer's
  own wording and the instruction you typed stay outside.
- Nothing is sent by writing help. A proposal changes nothing until a
  person accepts it, and accepting changes only your text region.
- Model text is marked with where it ran and what it saw, until edited
  (D136).
- The style score and every check are deterministic and local (D092).
- Your other mail reaches a cloud model only with
  `allow_cloud_relationship_data` on (D111). Drafting never turns it on.
- Activity records the action, model and counts, never your text or the
  proposal (AGENTS.md's activity invariant).
- Every capability is daemon IPC plus CLI JSON first. The web app, the
  TUI, Raycast and MCP draw it, and none builds a prompt.

## What stays, what merges, what is new

| Today | In this design |
|---|---|
| `DraftCompose`, `DraftRefine`, `mxr draft-assist`, `mxr draft refine`, `mxr_draft_assist` | `ComposeAssist`, `ApplyComposeProposal`, `mxr compose assist` and `apply`; the MCP tool wraps `ComposeAssist` |
| `DraftSuggestion` | `ComposeProposalData` (adds `changes`, `gaps`, `style`, `yours_changed`, `base`) |
| `draft_prompt.rs`, `draft_output.rs` | moved into `mxr-composer` (plan and check) |
| `draft_refine.rs` and its retry loop | removed; refining is `ComposeAssist` on the proposal's text |
| `draft_compose.rs`, `draft_context.rs`, `draft_voice.rs` | one gather handler, `compose_assist.rs`, and the `Voice` builder |
| `handler/humanizer.rs` whole-draft rewrite | repair of flagged sentences, in the composer |
| `writing_constraints()`, 24 categories | the 26-pattern catalogue, versioned with the skill |
| `draft_assist`, `draft_new`, `draft_refine`, `humanize_rewrite`, dead `voice_match` | `[llm.compose] model`; old keys warn and load |
| `llm.assistant_models`, `AssistantModelChoiceData` | `llm.models`, `ModelChoiceData` with `sees` |
| `ToneControls.tsx`, `DraftAssist.tsx`, `DraftAssistPanel.tsx`, `DraftQualityBadges.tsx` | the help line, the proof and style marks |
| `mxr draft eval` (replay) | `--set mine`; adds `--set quick` over the corpus and the snapshot gate |
| `draft_attachments.rs`, `draft_preview.rs`, `draft_provenance.rs`, `mxr-compose`, safety | kept; safety gains Unfilled gap |
| — | regions, the action rule, the proof, `TextChange`, the kept and invented-fact checks, `PatternRates`, the compose corpus |

## Build order: measure first, then one usable rung at a time

Every rung ships as a release with its docs, CLI JSON and tests (AGENTS.md,
and "Docs ship with features"). Parts such as `mxr-composer` or the
catalogue are built inside the first rung that needs them.

| Rung | After it ships, you can… | Builds | Status |
|---|---|---|---|
| C0 | see how good drafts are, case by case, and have every prompt change show as a diff | the compose corpus (40 cases), `mxr draft eval --set quick`, the snapshot gate over today's pipeline | planned |
| C1 | type a few words, press `⌘J`, and get your draft finished, as a proof you accept, in every client | `ComposeAssist` with `text`, `selection` and `DraftTarget`; `mxr-compose` (shared body regions); `mxr-composer` (plan, check: kept, gaps); `ComposeProposalData` with `changes`; the help line and proof on web; the TUI and Raycast proofs; `mxr compose assist`/`apply`; Unfilled gap; the MCP tool on the new request | planned |
| C2 | pick which model drafts, with a default in Settings | `[llm.compose] model`, `llm.models`, `ModelChoiceData.sees`, `model` on the request, the picker in all clients, old keys warn | planned |
| C3 | get drafts that follow humanizer 3.1 and your own habits | the catalogue, `PatternRates`, the style block, the voice-relative check, sentence repair, style marks, the invented-fact check | partial: catalogue, local `PatternRates`, style marks, bounded sentence repair, and deterministic style/fact regression coverage are wired; the full recorded corpus still reports rather than gates on style |
| C4 | reply inline under the message, with one recipients line and one Send | the compose screen above on web and desktop; sheet for new messages; `⋯` for the rest | planned |
| C5 | see which of your emails shaped a draft, and have a voice rebuild change your drafts | the one `Voice`, usual length and register in the prompt, provenance links, `--set mine` without the rewrite model | planned |

C0 comes first because C1 rewrites the request and the prompt, and only a
corpus can show that the rewrite didn't lose what today's prompt does
well. C1 is the rung BK asked for most directly. C2 and C3 can run in
parallel after C1, since they touch the settings and the catalogue, and
share only the request. C4 depends on C1's proof. C5 can follow any of
them.

## Decisions this rests on

From BK, 22 October 2026: the composer is architected as its own system,
the way the classifier is; draft for me builds on what you typed; the
compose UI is designed, not patched; a default draft model in Settings
with a per-draft pick; the humanizer's current rules baked into drafting;
Messages filters in every client ([22](22-email-modes.md), "Messages filters"). D152–D162
record them with what was considered.

Carried from earlier decisions: D009 (`$EDITOR`), D061 (core owns drafts
and send), D091 (relationship as weak background), D092 (deterministic
local scoring), D111 (AI off by default, mail local unless configured),
D134 (landing motion), D136 (model text marked), D141 (no sparkle), D145
(the assistant's picker, generalised by D156), D147 (account identity).

## Unresolved questions

Q7 to Q9 are in [31](31-design-open-questions.md). The blueprint
proceeds on the assumption written in each.
