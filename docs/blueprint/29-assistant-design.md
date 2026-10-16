# 29. The assistant as the copy desk

Status: proposal, 2026-10-16. Source baseline: `origin/main` at `f209d84ef`.

How the assistant ([25](25-assistant.md), D122) should look and feel, in the terms of [26](26-web-design-concept.md). The behaviour stays as 25 specifies: the daemon runs the turn, the gate enforces approval, `ask_user` asks, history is append-only. This chapter replaces its presentation. Decision: D135, with D136 for model text.

## What it is to the person

The assistant is for the work that's tedious by hand: analysis ("who am I waiting on?") and bulk changes ("archive every shipping notice older than a month"). BK asked for it as "a way for the user to talk to the email inbox if they don't want to do things manually" (2026-10-15). What it should feel like is a capable colleague at the copy desk across the room: it goes and finds things, tells you plainly what it found with the actual emails in hand, and brings you changes to sign. It never surprises you, and it never shows you its plumbing.

What went wrong in the first version, in BK's words: "this is not a good ui" (an opened step showing `{}` and minified JSON), "you tools return json internally I kno but in the agent interface we can surely make it look nice", and "would be nice if when it replies it gives me links that I can click and it takes me there" (2026-10-15). The readout and links fixed the worst of it (v0.6.105). What remains is that it still looks like a generic chat app grafted onto mxr: right-aligned bubbles, a robot icon, underlined links in a wall of prose, a lone "Thinking" spinner at the bottom, and steps labelled with the model's own odd phrasing ("Done to find the top 3 email threads that require a reply from me").

## The concept here

The copy desk is the one place the app spends its novelty budget (Dia's rule: novelty in one place, the rest plain). Three voices make it:

- **The wire** (mono, meta, `--text-2`): what the desk is doing and did. One line per step, verb, target and result, like a wire service log. It is the TUI's natural form too.
- **The copy** (reading voice): the answer, set like the reader, with the emails and people it names drawn as the app's own objects.
- **The proof** (the overlay surface): a change set out for signature, listing exactly what it touches.

Your own messages are quoted, not bubbled: Body in `--text-1`, with the 2px `--text-1` left rule DESIGN.md already uses for agent prompts on the site (§03 "Agent transcripts"), aligned with everything else.

## Anatomy

```
┌─ assistant (400, resizable 320–720) ─────────────────────────────┐
│ ◎ Shipping notices · trusted ▾               ⟲ history  ✎  ✕    │  room, Body
├──────────────────────────────────────────────────────────────────┤
│ ┃ Archive the shipping notices older than a month               │  your words, quoted
│                                                                  │
│ ▸ 3 steps · 14 found · 1 change waiting                          │  folded wire, meta
│                                                                  │
│ I found 14 shipping notices from before 16 September, from       │  the copy, reading voice
│ (◇) Amazon, (◇) DHL and (◇) Royal Mail. None has a delivery     │  ◇ = sender chips
│ problem or a refund in it.                                       │
│                                                                  │
│ ┌─ Archive 14 emails ───────────────────────────── proof ──────┐│  overlay surface
│ │ Older than a month, nothing pending.                          ││  the reason, in words
│ │ (A) Amazon        Your order has shipped           12 Sep    ││  rows, the shared Row
│ │ (D) DHL           Delivery scheduled               9 Sep     ││
│ │ (R) Royal Mail    Your parcel is on its way        2 Sep     ││
│ │ and 11 more                                             ›     ││
│ │                     n send back with a note   [ Archive  y ] ││  one primary
│ └───────────────────────────────────────────────────────────────┘│
├──────────────────────────────────────────────────────────────────┤
│ ◌ Archiving 14 emails…                              4s   ■ stop │  live line (while running)
│ ┌──────────────────────────────────────────────────────────────┐ │
│ │ Ask about your mail, or say what to do                       │ │  composer
│ │ gemma4 · runs on this machine                         ↵      │ │  meta
│ └──────────────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────┘
```

The eye lands on the answer, or on the proof when one is waiting. The wire folds to one line once a turn ends. The header is the room colour with the conversation's title in Body and its trust state written out ("trusted"), not just shown as a shield icon. The robot icon goes; the assistant's glyph is a quiet `◎`-style lucide mark (`CircleDot`), the same in the top bar, the header and toasts.

## Steps are wire lines

Each step is one line of meta in two tenses, with no tool names, ids or JSON (D135):

```
◌ Searching your mail for "shipped" before 16 Sep           running, --text-2, crossfades
✓ Searched your mail: 14 found                               done, --text-3
✕ Couldn't star 1 email: it was already moved                failed, --danger
◇ Waiting for you: archive 14 emails                         waiting, --cue
```

- **The daemon writes both tenses.** `AssistantItemData::ToolCall` gains `title: AssistantStepTitleData { running, done }`, built in the daemon from the tool and its arguments (`crates/daemon/src/assistant/titles.rs`, beside `readout.rs`). For example, `mxr_search` with `query: "shipped"` becomes "Searching your mail for "shipped"" and then "Searched your mail: 14 found", with the count from the search result's own `total`. A title never claims more than the result confirms: a change reads "Ran: archive 3 emails", worded from the request, unless its tool is on a short verified list (mutations, to-do state changes, sync, archive submit, send, search) whose real result fields say what happened, and a result that is blocked, carries an error or reports nothing succeeded reads as failed in every state. The outcome shows in the step's readout. Values from mail or the model sit inside typographic quotes, clipped inside the quotes. The model's own reason stays, as the step's tooltip and its detail, but it is never the title. Small models write odd reasons, and the daemon's words are always right.
- **One source of words.** `describe_change` exists twice today (`crates/tui/src/ui/assistant_describe.rs` and `apps/web/src/features/assistant/describe.ts`). Both move to the daemon's titles, so the web, the TUI and `mxr assistant show` say the same thing.
- **Folding.** While a turn runs, steps show as wire lines under your message. When it ends they fold to one line: "3 steps · 14 found · 1 change waiting", or "· 1 failed" in `--danger`. Clicking the line, or pressing `e` in the TUI, opens them all, and every step opens to its readout (Found N, the rows, "and N more") with the raw call under "Details". This is the research's main lesson from Cursor's forum: folds must always open, fully.
- **No tool icons per step.** The state glyph is the only mark.

## The live line replaces "Thinking"

While a turn runs, one line above the composer says what the desk is doing now, rewritten in place (Linear's ephemeral activity), with the elapsed seconds and Stop:

- `Thinking { step }` reads "Reading your request" on step 1, then "Deciding what to do next".
- `Calling { tool, reason }` reads the running title of the call ("Searching your mail for invoices").
- `Writing` reads "Writing the answer", while the answer itself streams into place above.
- `Summarizing` reads "Making room: summarising earlier messages".

The protocol's `Calling` gains the `title` field so clients don't rebuild it. When a turn has been on one step for over 20 seconds, the line adds the model ("gemma4 is slow on this; 24s"), because a local model that takes 28 seconds otherwise looks stuck (the audit's `21c-assistant-thinking`).

## The copy draws the app's own objects

Replies keep their markdown, but references render as mxr objects (Linear draws an agent's links as its own issue and person chips):

- **A person** (`mxr:person/...`) is an inline chip: a 16px avatar and the name in Body, the 3px chip radius. Click or `Enter` opens Messages on them, beside the assistant.
- **An email** (`mxr:thread/...`) is an inline chip with the mode glyph of where it lives and the subject. It opens in All Mail's reader, beside the assistant (v0.6.105's behaviour, new look).
- **A list** in a reply that names three or more emails or people is drawn from the step readout, not the prose: the shared `Row` in compact rhythm, with avatar, name, subject and time, under the sentence that introduces it. The prose doesn't need to repeat what the rows show. The prompt asks the model to introduce lists in one sentence and let the app show the rows. The daemon attaches the readout rows the reply refers to (`AssistantItemData::Assistant` gains `rows: Vec<AssistantReadoutRowData>`, filled when the reply links two or more objects from one readout).
- **Model text is marked.** The answer's footer has the model mark: `model · local` (D136).

The underlined-link look ("API design discussion" in cyan underline mid-sentence) goes away.

## Proofs to sign

A change waiting for approval is a proof on the overlay surface:

- **The title** is the change in words, from the daemon ("Archive 14 emails", "Send a reply to Pat", "Move 3 senders to Reading").
- **The reason** is the model's one-line reason under it, in `--text-2`.
- **What it touches** is listed as rows: emails with sender, subject and date, a draft as its recipients, subject and body in the reading voice, and "and N more" opening the full list.
- **One Apply** (primary, with `y`). Below it, quiet: `n` "send back with a note", which opens a one-line field whose text goes to the model as the rejection reason (Claude Code's comment-on-No), and `x` "exact call", the raw tool and arguments under "Details".
- **Irreversible changes say so** in `--danger` meta above Apply: "Sends an email. Can't be undone." Trash, spam, rules and anything that reaches another service.
- **When applied,** the proof collapses to its wire line ("✓ Archived 14 emails · Undo"), and the rows in the page beside fold away with the landing mark (D134). That's the moment the copy desk feels connected to the desk.

A proof covers exactly one stored call; the whole call is applied or sent back (D122: approving runs exactly the stored call). Dropping single items from a proof would need the daemon to accept a narrowed call; that is Q5 in [31](31-design-open-questions.md).

## Questions

An `ask_user` question is a card on the overlay surface. The question is in Heading, the options are rows with a radio or checkbox mark, their label in Body and description in `--text-2`, and keys `1`-`6`. "Something else" is a field in the same list, not a separate button, and Skip is quiet. A recommended option, when the model marks one, goes first with "recommended" in meta. Answered questions collapse into your quoted reply ("Newsletters and receipts"), so the transcript reads like a conversation.

## Failure is in the copy, not hidden in the wire

When steps before a reply failed and weren't redone, the reply ends with the failure note (v0.6.106, `failed_call_ids`), restyled: a `--danger` hairline on the leading edge and "These didn't happen, whatever the reply says:" followed by each failed step's done-tense title ("✕ Couldn't star 1 email: it was already moved"). A turn that fails outright shows one line in plain words with what to do ("The model stopped answering. Send your message again, or switch models in Settings › Language model"), never "LLM returned an empty completion".

## Trust is legible

A trusted conversation (D122's amendment) says so in the header ("trusted ▾"), and its steps that ran without asking carry "ran without asking" in their done title with Undo. The menu under "trusted" explains in one line what runs without asking and turns it off. Adobe's research: a mode that changes who is responsible must be visible.

## Empty, history and the composer

- **Empty:** no robot or illustration. A Heading, "Ask about your mail, or have it do the tedious part.", then three suggestions as quiet rows, with the current page's context first ("Summarise who's waiting in Messages" when Messages is open). The `⌘.` hint sits in the composer's meta.
- **History:** a list using the shared `Row`: the title, the last line in `--text-2`, the time, and a mark for conversations waiting on you. `D` deletes after the dry run, as now.
- **Composer:** Body in the input, with the model and where it runs in meta below. `↵` sends; Stop replaces the send button while running.

## Arriving and leaving

The panel slides in from the right in 180ms when opened by click and appears at once when opened by `⌘.` (keyboard-triggered, D134). The page beside it narrows; it never overlays above 1024px. Its open state and conversation survive navigation and reloads (D122). Links open beside it.

## The TUI pane

The same grammar in the terminal, which suits it best:

```
┌ Shipping notices · trusted ─────────────────────────┐
│ ┃ Archive the shipping notices older than a month    │
│ ▸ 3 steps · 14 found · 1 change waiting              │
│ I found 14 shipping notices from before 16 Sep,      │
│ from Amazon [1], DHL [2] and Royal Mail [3].         │
│ ┌ Archive 14 emails ───────────────────────────────┐ │
│ │ Older than a month, nothing pending.              │ │
│ │ Amazon       Your order has shipped      12 Sep   │ │
│ │ DHL          Delivery scheduled           9 Sep   │ │
│ │ … 12 more                                         │ │
│ │ y archive  n send back  x exact call              │ │
│ └───────────────────────────────────────────────────┘ │
│ ◌ Archiving 14 emails…                       4s       │
│ › _                                                   │
│ y archive · n send back · 1-3 open link · e steps    │
└──────────────────────────────────────────────────────┘
```

Steps use the daemon's titles; links keep their numbers (v0.6.106); proofs list the rows; the live line replaces "Working…". The TUI's colours follow the three text tiers ([27](27-web-design-system.md#the-tui-follows-the-same-tiers)).

## Protocol and daemon changes this needs

| Change | Where | Why |
|---|---|---|
| `AssistantStepTitleData { running, done }` on `ToolCall`, and `title` on `AssistantProgressData::Calling` | `crates/protocol/src/types/assistant.rs`, `crates/daemon/src/assistant/titles.rs` (new), `view.rs` | Wire lines and the live line in the daemon's words. |
| `rows` on `AssistantItemData::Assistant` | `view.rs`, from the readouts the reply links | Lists drawn as the app's rows. |
| Change titles from the daemon (replacing both `describe_change` copies) | `titles.rs`; `PendingCards.tsx`, `assistant_describe.rs` read the field | One source of words. |
| A prompt line asking for one-sentence list introductions | `crates/assistant/src/prompt.rs` | The prose stops duplicating the rows. |
| Plain-language turn errors | the daemon's turn error mapping | No raw "LLM returned an empty completion". |

Each is a field with a default, so old clients keep working. The OpenAPI spec, the insta snapshot and the generated web types are updated with it, as the repo requires.
