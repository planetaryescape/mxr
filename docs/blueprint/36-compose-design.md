# 36. Compose design: your page under the lamp, help as a proof

Status: proposal, 22 October 2026. Source baseline: origin/main at
`d96454660`. Design only; no code has changed. The drawn mockups are in
the [compose design doc](https://claude.ai/artifact/DJPdrPMcVCx2e7cXXxMgXU);
this chapter is the record and wins where the two differ.

[34](34-composer.md) designs the composer as a system. This chapter designs
what you see and touch: the reply, Draft for me, the new-message sheet, the
model picker, the Messages filter bar, the TUI review and Raycast. It rests
on D161 (the page and the proof) and D162 (Messages filters), and follows
chapter 35's key rules. Colours are the shipped generated theme
(`apps/web/src/styles/themes.generated.css`), not chapter 27's printed hex
values, which have drifted.

## Today every way into a reply looks different, and every control has the same weight

Audited on mxr 0.6.140 against the demo mailbox, 22 October 2026, from 38
screenshots of the web app:

- A new message shows 14 controls before the body: four layout buttons,
  From, `⋯`, To, Cc, Bcc, Subject, a Draft for me row, Generate, Adjust,
  Refine context and four refine chips.
- Messages, `r` in the reader and the reader's "Draft a reply" open three
  different layouts. The buttons say "Inline" for a floating window and
  "Popout" for a docked panel.
- Two AI surfaces do one job with different words, and Generate wipes what
  you typed with no undo.
- Voice and humanizer scores show twice; two provenance lines say the same
  thing; refine chips are greyed out unless the draft came from Drafts.
- Each reply attempt makes a new draft; Compose reopens the last unsent one.
- The Rich text/Markdown toggle changes the setting for every draft.
- No reply composer shows the message it answers.
- Typing a known contact in To crashes the app
  (`docs/issues/compose-recipient-autocomplete-crash.md`).

The screen is noisy because too many things have equal weight, and
confusing because one job has several shapes. The design removes and merges
before it restyles.

## Eight rules

1. Your text is the one lit thing: reading type (16/26, `--text-1`), the
   lamp surface `--ink-lamp`, a 2px `--signal` edge. Everything else is
   12px meta until touched.
2. A reply has one shape everywhere: under the message it answers. A new
   message is always the sheet.
3. Show only what you can act on now. No scores, no repeated provenance, no
   greyed-out controls.
4. Help is one line, not a panel.
5. Help returns a proof, never an overwrite. Additions in `--signal`
   underlined, cuts struck in `--text-3`, gaps in `--cue`.
6. Model text stays marked until edited (D136); no sparkle (D141).
7. Words say what happens: "In the conversation" and "Sheet", "Keep mine",
   a blocked send names what's missing.
8. Every client tells the same story, with the daemon owning the strings.

Hidden until asked: Cc and Bcc, a reply's subject, From with one address,
Rich text/Markdown, snippets and signatures (`⋯` and their keys), scores
(under Details), every control for a feature that's off.

## The reply opens under the message it answers

```
 Pat Lee                                   Tue 21 Oct, 16:40 · 2h ago
 Can you make the Thursday review? What time suits you? And could you
 send the Q3 numbers before then?
▌To Pat Lee · reply all                                     [W] work
▌
▌Thursday works, any time after 2.
▌Numbers by Wednesday.|
▌
▌▸ signature · Bhekani
▌──────────────────────────────────────────────────────────────────
▌Draft for me   Attach                                  ⋯   [ Send ]
 ⌘↵ send · ⌘J draft for me · ⇧⌘F sheet · Esc back          (status strip)
```

| State | What you see |
|---|---|
| Not started | "Reply to Pat…" under the message in `--text-2` on the desk; `r` or a click opens it |
| Reply all | "To Pat Lee, Ruth Ng · reply all"; switching recalculates To and Cc |
| Recipients open | To, Cc and Bcc as three plain rows; `Esc` or clicking the page folds them |
| Long reply | It grows and the conversation scrolls, so Send stays in view |
| Sheet (`⇧⌘F`) | The same reply on `--ink-overlay` over the scrim, with a two-line quote of the message at the top |
| Resumed | One unsent reply per conversation; replying again opens it |
| Save | Silent; only a failure shows, as one meta line above Send |

Every entry point opens this: Messages, `r` in the reader, Focus and the
`⋯` menu's "Draft a reply", which opens it with the help line running. The
right-rail "Draft assist" panel and the floating window go.

## Draft for me is one line that comes back as a proof

```
1 Ask (⌘J)                                2 Wait
▌Thursday works. numbers by wed           ▌Thursday works. numbers by wed
▌┌──────────────────────────────────────┐ ▌✓ Read the conversation · 3 messages
▌│|Finish my draft, or say what to change│ ▌✓ Matched your voice · 3 replies to Pat
▌│ gemma4 · this Mac ▾                  │ ▌▸ Writing with gemma4 on this Mac · 4s
▌│[Shorter][Warmer][More formal][Proofread] Polish
▌└──────────────────────────────────────┘

3 The proof                               4 Accepted
▌+Hi Pat,+                                ▌Hi Pat,
▌Thursday works. +Any time after+         ▌Thursday works. Any time after
▌  [[?: a time]] +suits me.+              ▌  [[?: a time]] suits me.
▌-numbers- +Numbers+ by -wed- +Wednesday+.▌Numbers by Wednesday.
▌+Thanks, B+                              ▌Thanks, B
▌[ Accept ]  Revise  Try again with…  Keep mine
▌gemma4 · this Mac · from 3 of your replies to Pat · 2 of your words fixed
                                          ▌1 gap left: a time · Send takes you there  [ Send ]
```

Here `+…+` is `--signal` underlined, `-…-` is `--text-3` struck through, and
`[[?: …]]` is `--cue`. After Accept, model words keep a dotted `--text-3`
hairline until edited.

| Your draft | Help line placeholder | The proof |
|---|---|---|
| Empty | "Draft a reply, or say what it should say" | everything added |
| Has text | "Finish my draft, or say what to change" | your sentences kept, additions and fixes marked |
| Text selected | "Edit the selection: say how" | only the selection changes; the rest in `--text-3` |

- Keys in the proof: `↵` accept, `⌘J` revise with a note, `Esc` keep mine,
  `←` `→` step between this draft's proofs, `⌘Z` undoes the whole accept,
  `Tab` moves gap to gap.
- Send is never disabled. With a gap left it moves the cursor to the gap
  and says why in the line above it (D159's block, drawn without greying
  out).
- A failure says what happened in the wire line, with the next step:
  "gemma4 didn't answer in 60s. Try again, or pick another model." Your
  text is untouched.
- With AI off, "Draft for me" isn't drawn. The palette entry says where to
  turn a model on.
- Polish is a quiet word beside the chips, not a chip, because it is the
  one action that may change your words.

## A new message has three things before your words

```
 New message                                          ⤢   Close
 ───────────────────────────────────────────────────────────────
 [W] work · bk@work.example ▾
 To   Pat Lee, |                                          Cc  Bcc
 Subject
 ───────────────────────────────────────────────────────────────
 Write your message.
 Or say what it's for and press ⌘J.

 ▸ signature · work default
 ───────────────────────────────────────────────────────────────
 Draft for me   Attach                              ⋯  [ Send ▾ ]
```

| Today | In the sheet |
|---|---|
| Inline, Popout, Fullscreen, Close | Full screen, Close (Close keeps the draft) |
| From select on every message | Account chip and address, only with more than one to pick |
| "Maybe cc" row | Suggestions inside To while typing |
| Recipient chips with Cc and Bcc buttons | Names as text; "Cc" and "Bcc" as words that open a row |
| The Draft for me panel | The help line |
| Send, ▾, clock, Attach, Rich text/Markdown, badges, save status | Send with its menu (later, send and archive, remind), Attach, `⋯` |

Compose always starts empty; unsent messages wait in Drafts.

## The model is named in words and picked per draft

The model mark in the help line ("gemma4 · this Mac ▾") turns `--signal`
only while its menu is open. The menu groups models by where they run and
what they may see:

```
 ON THIS MAC · SEES YOUR PAST MAIL
 ▌gemma4                                  default ✓
  qwen2.5:3b-instruct
 CLOUD · SEES THIS CONVERSATION ONLY
  Claude Sonnet 5.5
  Claude Haiku 5.5
  GPT-6 Luna
 ─────────────────────────────────────────────
  GPT-5.6 Terra
  needs a key: mxr llm key set openai
 ─────────────────────────────────────────────
 For this draft only · default in Settings
```

The cloud heading reads "sees your past mail" while
`allow_cloud_relationship_data` is on. No speed or price scores.

Settings › Compose gets a Drafting section: "Draft with" (the default
model), "Fix AI-sounding sentences" (the repair pass, on), "Cloud models may
read your past mail" (with "Off: cloud drafts see only the conversation"),
and "Your voice" (when and from how many sent emails it was built, and
Rebuild). Settings › Language model loses the four drafting overrides.

## Messages filters are one segmented control and two toggles

```
 Messages                                     12 people · 4 your turn
 People you write with. Reply, or done here.
 [ All | Your turn | Waiting on them ]  [Unread]  [Past your pace]
 Showing 2 of 12 · Clear filters
```

- The header count never changes with a filter (D132).
- The turn is the square segmented control; Unread and Past your pace are
  toggles drawn with `--signal-soft` when on.
- Rows are unchanged: the turn is the state, unread is the existing 2px bar,
  and only past-your-pace uses `--cue`.
- An empty filtered view names the filters, says what's still there ("Your
  turn still has 4 people") and offers Clear filters.
- Reached with `g f` on the web, `Ctrl-f` in the TUI, `⌘F` in Raycast; the
  URL keeps it (`?turn=mine&unread=1`).

## The TUI review and proof

```
┌ Send this reply? ──────────────────────────────────────┐
│ To       Pat Lee <pat@northwind.co>                     │
│ From     work · bk@work.example                         │
│ Subject  Re: Thursday review                            │
│                                                         │
│ Thursday works. numbers by wed                          │
│                                                         │
│ Attachments  (none)                                     │
│ Checks       ok                                         │
│                                                         │
│ s send · a send at · r draft for me · e edit again      │
│ d save draft · # discard draft · Esc back to draft      │
└─────────────────────────────────────────────────────────┘
```

The proof pane uses the accent for additions, struck `text_muted` for cuts
and `warning` for gaps. Under `NO_COLOR` it prints git's word-diff marks
(`[-numbers-]{+Numbers+}`) and underlines additions. Its keys are `y`
accept, `e` accept and edit, `n` send back with a note, `m` model, `←` `→`
versions, `Esc` keep mine. Key lines are generated from the keymap
(chapter 35), and borders are square to match the web.

## Raycast

The reply form keeps Raycast's layout: To as a description, the Reply text
area, "Send Reply" as the one primary action (`⌘↵`), and the rest behind
`⌘K`: Draft for Me, Draft With Model…, Open in mxr. The proof is a pushed
Detail view: added words bold, cuts `~~struck~~`, gaps as inline code
(Raycast's markdown has no colour), with Model, Written from, Your words and
Gaps in the metadata panel. `↵` accepts into the form, `⌘R` tries again,
`⇧⌘R` tries again with another model, `Esc` keeps yours.

## Keys, motion and checks

| Key (web and desktop, while writing) | Does | Status |
|---|---|---|
| `⌘↵`, `⇧⌘↵`, `⇧⌘L` | Send; send and archive; send later | today |
| `⌘J` | Draft for me; in a proof, revise | new; check Ctrl+J (Downloads in Chrome on Windows and Linux) can be claimed |
| `↵`, `Esc`, `⌘Z`, `Tab`, `←` `→` | Accept, keep mine, undo accept, next gap, versions (in a proof) | new |
| `⇧⌘F` | Move a reply between the conversation and the sheet | today's chord, repurposed |
| `⇧⌘C`, `⇧⌘B`, `⇧⌘A`, `⌘;`, `⇧⌘G` | Cc, Bcc, attach, snippets, signature | today |
| `⌘S`, `⇧⌘R` | Save, refresh from the daemon | removed: browser-reserved; autosave covers save, refresh moves to `⋯` |

Motion uses the shipped tokens and moves only transform and opacity:

| Moment | Motion |
|---|---|
| Reply opens | Fade and rise 8px, `--motion-duration-base` (180ms), `--ease-out` |
| Help line, wire lines | 120ms fades; the live marker pulses at 1500ms |
| Proof arrives | Your words don't move; additions fade in over 180ms |
| Accept | Marks settle into hairlines with the 600ms landing underline (D134) |
| Reduced motion | 120ms fades only |

A build that touches these screens passes these checks before it ships:

1. One filled button per surface.
2. No control drawn that can't act; nothing greyed out.
3. No key chips inside buttons (chapter 27).
4. `--signal` only for focus, the primary action and change; `--cue` only
   for gaps and past your pace.
5. Every model-written span is marked until edited.
6. The proof's words come from the daemon and match in every client.
7. Every way into a reply opens the same placement (one journey test).
8. `Esc` never destroys.
9. Body and meta text at least 4.5:1 in Midnight and Light.
10. At 390px the reply is full width and the chips wrap under the field.
11. The TUI proof reads under `NO_COLOR`.
12. Midnight and Light screenshots of each screen go through chapter 26's
    design gate.

Drift to fix in the same rung, found while extracting the tokens: the
inline composer uses `--ink-desk` instead of `--ink-lamp`; ghost buttons
hover to `--input` instead of `--ink-lamp`; `Chip` uses 11px sans instead of
12px meta; the compose sheet, menus and dialogs carry shadows chapter 27
rules out; dialogs blur instead of using `--scrim`; inputs and text areas
use different border greys; Send carries a key chip; chapter 27's printed
hex values have drifted from the generated theme.

## Where this lands in the ladder

The help line and proof ship in C1 (34's ladder) on the existing compose
layout. The reply placement, sheet, recipients line, Send menu, settings
section and the drift fixes ship in C4. The model menu ships in C2.
Messages filters ship with D162, independent of the composer ladder.

## Unresolved questions

Q7 to Q9 in [31](31-design-open-questions.md), plus whether "the page and
the proof" is the right idea (assumed yes, as Q1 assumed the night desk).
Two checks before building: Ctrl+J on Chrome for Windows and Linux, and
whether Raycast's Detail markdown renders strikethrough.
