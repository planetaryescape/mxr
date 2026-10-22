# 28. Web screens under the lamp

Status: proposal, 2026-10-16. Source baseline: `origin/main` at `f209d84ef`.

Each main screen, designed in the terms of [26](26-web-design-concept.md) and built from the parts in [27](27-web-design-system.md). What each screen does is specified in [21](21-web-experience.md) and [22](22-email-modes.md); this chapter says how it should be experienced: where the eye lands first, what recedes, what the lit item shows, and what changes from today. Audit references (`01-now`, `30-screener`) are the screenshots in [the audit](../research/web-design/audit-2026-10-16.md).

Mockups are at 1440px with the rail expanded and the assistant closed, unless noted. `▌` is the lamp's hairline on the lit row. Dim text is described, not drawn.

## The shell

```
┌────────────────────────────────────────────────────────────────────────────────────┐
│ ▣ BK · bk@…  ▾    Messages › Pat Colleague          [ Search  / ]  [◎]  [✎]       │ room
├──────────────┬─────────────────────────────────────────────────────────────────────┤
│ ☼ Now      4 │                                                                     │
│ ▤ Messages 3 │                         the page (desk)                             │
│ ☑ To do    2 │                                                                     │
│ ◔ Updates    │                                                                     │
│ ▥ Reading    │                                                                     │
│ ▢ Archive    │                                                                     │
│              │                                                                     │
│ ▭ Inbox      │                                                                     │
│ Mail      ›  │                                                                     │
│ Views     ›  │                                                                     │
│ Tools     ›  │                                                                     │
│ Labels    ›  │                                                                     │
│              │                                                                     │
│ ◐  «         │                                                                     │
├──────────────┴─────────────────────────────────────────────────────────────────────┤
│ ● 4m ago · Here because: your turn since Wed     r reply  e done here    ? keys │ room
└────────────────────────────────────────────────────────────────────────────────────┘
```

**Intent.** The shell is the room: darkest ink, secondary text, and nothing that moves. The eye never starts here. Counts appear only on modes that are work (Now, Messages, To do; D095). The breadcrumb names the page in the product's nouns, from the route's title, not its URL slug (fixing "LLM" against "Language model"). The top bar's three buttons are the only icon-led controls outside the rail. When the daemon is down, the shell doesn't change shape; one banner appears under the top bar (D131).

## Now

```
  NOW · FRIDAY EVENING
  Four things need you.                                         ● 4m ago
  Since you last looked: 23 arrived · 3 Messages · 12 Updates · 8 Reading
  ───────────────────────────────────────────────────────────────────────
  PEOPLE
▌ (P) Pat Colleague          RustConf 2026 invite · "confirm by Fri"   r reply  e done ⋯
  (W) Wendy Data             API design discussion                    ▢  1d
  DUE
  ☐  Pay invoice 2847 · $15,000                     due Fri  ▤      Mon
  UPDATES
  ◔  2 updates from 2 sources in the 16:30 digest                     g u
  ───────────────────────────────────────────────────────────────────────
  Not now: 1 to read this week
```

**Intent.** The summary sentence is the page title: "Four things need you." in Display, so the first thing read is the answer. The freshness dot and the arrivals line are meta beneath it: present, not loud. The four sections (D110) are short lists with eyebrows, with no section boxes. The eye lands on the summary, then on the first person. The first row is lit when the page opens: a lit row opens nothing, so one lit thing holds and no message opens by itself (BK, 2026-10-09). As built in W1, the page title stays "Now" and the daemon's headline sits under the frame as a Heading, because real headlines run about 60 characters and do not fit Display at laptop widths.

**The lit row** turns its trail into the verbs with keys; the status bar says why it's here.

**From today (`01-now`):** the five underlined links per People row become a mode mark and, on the lit row, the reason. The new-sender question becomes a `new` mark. "Since 01:02: 0 arrived" with a native `▶` becomes the arrivals line, which drops zero counts (D119). "and 9 more in Messages" moves into the People eyebrow ("PEOPLE · 9 more in Messages"). Due rows use the same row as To do, not a bordered chip.

**Empty:** "Nothing needs you right now." and "Next: Reading has 1 this week." The low-tide scene plays when Now is cleared for the first time that day (D134).

## Messages

```
┌─ list (420–600, resizable) ─────────────────┬─ Pat Colleague ─────────────────────────────┐
│ MESSAGES                                     │ Pat Colleague                     ⋯          │
│ People you write with. Reply, or done here.  │ You've written 14 times since 2024. Usually  │
│ YOUR TURN                                    │ replies in a day.                            │
│▌(P) Pat Colleague                     6d  ●  │ ─────────────────────────────────────────── │
│     "I asked my manager and they'll…"        │ TOPICS                                        │
│ (D) Diana, Eve                        5d  ●  │ ■ RustConf 2026 invite         your turn 6d  │
│     Q1 report body text section 3…           │    Hotel for the conference    waiting  2w  │
│ (U) Uma Designer                new   5d     │ ─────────────────────────────────────────── │
│ PINNED                                       │  Pat · Mon 20 Oct, 09:12                     │
│ (M) Maya Ortiz                        1w     │  I asked my manager and they'll cover the    │
│ RECENT                                       │  ticket. Can you confirm by Friday so I can  │
│ (I) Iris Chen                         3w     │  book?                     (reading voice)   │
│ QUIET · 12                              ›    │                                              │
│                                              │ ┌──────────────────────────────────────────┐ │
│                                              │ │ Reply to Pat…                            │ │
│                                              │ └──────────────────────────────────────────┘ │
│                                              │  r reply  . got it  e done here  t to-do     │
└──────────────────────────────────────────────┴──────────────────────────────────────────────┘
```

**Intent.** iMessage's shape (D112): people on the left, the conversation as a page on the right. The eye lands on the first Your-turn person, then on the open conversation's newest message in the reading voice. Nothing opens until asked (BK, 2026-10-09). The person's page has one Title (their name), one purpose line (the relationship in one sentence), the topics, then the conversation. The reply field sits where reading ends (D095).

**The lit row** shows the verbs in its trail, and for a new sender the keep-or-move choice with keys (`1` to `4`) in place of the verbs; the status bar gives the reason.

**From today (`02-messages`, `03-messages-person`):** the "New sender. Keep in Messages? Messages Updates Reading Block" line under almost every row becomes the `new` mark plus the lit-row choice, which halves row heights. Letters stop being filled boxes: each message is a ruled section in the reading voice, with the sender and time in meta, showing its new part with the quoted history folded to one line (the collapse BK likes, 2026-10-09: "if I'm not curious about the rest i don't even need to open the rest"). "new" alone at top right becomes the `new` mark. "Pat has written once since 2026." becomes a sentence the daemon writes that can't produce a year as a duration. The teal teaching callout becomes a hint anchored to the element it explains, in `--ink-overlay`, one at a time (D118). The lone `›` empty state becomes "Choose someone to read their conversation." in the content column.

## To do

```
  TO DO
  Things email asked you to do, by when to act.                      ● 4m ago
  NOW · 3
▌ ☐  Pay invoice 2847 · $15,000            Frank Billing · from Mon   Space done  o email ⋯
  ☐  Confirm RustConf ticket                Pat Colleague · from Fri   by Fri
  ☐  Send the sync agenda                   Quinn Dev · from you       today
  COMING UP · 2                                                          ›
  LATER · 3                                                              ›
```

**Intent.** A to-do list first (BK, 2026-10-09: "I'm used to todo checkmark being on the left side ... clearly visible that it's a check box"). The title is the thing to do, not the subject; the due date sits in the trail in `--cue` when it is today or past. The eye lands on the first task's title. The checkbox is Body-sized (16px), not the 32px of today.

**From today (`04-todo`):** the unexplained `⊙` icon and progress bar go (a relevancy window that matters is shown as words: "due Fri", "by Fri"). "Open email" stops being a bordered button on every row; it is `o` on the lit row. "from you" appears consistently in the secondary line. The legend under the list goes to the status strip, and "e tick off" and "Space tick off" become one verb, "done", with `Space` (and `e` as an alias in `?`).

**Moment:** `Space` on the lit row: the box fills, the row folds, the next lights. The last Now task done shows "Nothing due today. Next: Coming up has 2." and plays the scene.

## Updates

```
  UPDATES
  Notifications, gathered twice a day. Read, then let go.            16:30 digest
  2 updates from 2 sources                                       A let go of all
  ───────────────────────────────────────────────────────────────────────
▌ ◔ GitHub         Build #489 failed at integration        Space peek  e let go  ⋯
  ◔ Linear         3 issues assigned to you                     12:40
  ROUTINE · 2 sources                                                    ›
```

**Intent.** A notification reel (BK, 2026-10-14): one line per source that says the thing, so you rarely open the email. The eye lands on the first source's line. Routine stays folded (BK's choice, 2026-10-14). `Space` opens the update's body inline under its line, in the reading voice, without leaving the list.

**From today (`05-updates`):** the page stops saying the same thing three times; the cut and count are the purpose line and one meta line. The folded Routine line becomes an eyebrow with a chevron that looks pressable. Hover key hints go; the lit row carries them. The pinned teaching popover becomes an anchored hint that closes when you move. Deliveries keep their single status row (v0.6.66).

## Reading

```
  READING
  Newsletters you chose, as tonight's edition.                          8 issues
  ───────────────────────────────────────────────────────────────────────
▌ ▥ The Changelog                                   Enter read  b later  e let go ⋯
    The state of Rust in 2026, and why async finally feels finished
  ▥ This Week in Rust                                                 Wed
    Issue 571: new compiler release, and a look at the trait solver
```

```
  ← Reading           The Changelog · Mon · 9 min                   Aa  ⋯
  ─────────────────────────────────────────────────────────────────────
              The state of Rust in 2026, and why async             (display)
              finally feels finished

              Body text in the reading voice at 16/26, CASL 0.3, a 68ch
              measure, centred in the reader column. Links in --signal.
              ...
  ─────────────────────────────────────────────────────────────────────
  n next   b later   e let go   D unsubscribe   M Marginalia   R original
```

**Intent.** An edition you read, not a list you process. Issues are rows like every other list (the single bordered card goes; the audit found it was the only card in any mode list), with the dek as the secondary line. The reader is the page under the lamp: Display title, reading voice, generous margins, and one toolbar of quiet buttons, not three button styles.

**From today (`06-reading`, `07-reading-reader`):** the eight-item key legend goes to the status strip. The toolbar's mix of a ghost link, bordered icon buttons and bare key text becomes quiet buttons with their keys in tooltips. The post-article footer keeps the same verbs, as the natural end of reading. Unsubscribe stays visible in the reader (BK, 2026-10-08). The stray `›` on phones goes with the breadcrumb fix.

## Archive

```
  ARCHIVE
  Receipts and records, found by asking.                               [Export ⋯]
  ┌───────────────────────────────────────────────────────────────┐
  │ What are you looking for?   invoices from Frank this year     │   (type-heading input)
  └───────────────────────────────────────────────────────────────┘
  [ All │ Invoices │ Receipts │ Bookings ]   2026 ▾   Issuer ▾          g f filters
  ─────────────────────────────────────────┬────────────────────────────────────
  OCTOBER 2026 · 2 · $15,000.00            │ Frank Billing · Invoice
▌ $15,000.00  Frank Billing   Invoice 2847 │ Amount     $15,000.00   from the email
  $   120.00  Hosting Co      Receipt      │ Due        Fri 17 Oct   from a pattern · unchecked
                                           │ o email   c correct a field
```

**Intent.** The cabinet. The eye lands on the answer box, then on the best match, lit. Amounts are meta tabular figures, right-aligned. Provenance stays on every field, as [22](22-email-modes.md) requires, in meta.

**From today (`08-archive`):** round filter pills become the shared segmented control. The `○` marker on one row becomes a per-field "unchecked" word in the detail. "The email o" becomes the lit row's `o email`. The failed-load mail skeleton becomes the ledger's own skeleton and then an error with what still works ("Records already found are below").

## Inbox and the reader

```
┌─ INBOX · 49 conversations ───────────┬───────────────────────────────────────────────┐
│▌● (M) Mia, Nate, Oscar  5    Wed  ▤  │ ✕  ‹ ›   2 of 49          [Formatted│Reader│Plain] ⋯│
│     PR review: fix auth middleware    │                                               │
│   (H) Hank, Iris, Jack  3    Wed     │ PR review: fix auth middleware        (display) │
│     Team standup notes                │ model · local  Mia approved the change and     │
│ ● (R) Rosa PM                Wed  ▤  │ asks you to merge by Friday.  (long threads)   │
│     Database migration plan           │ You and Mia: 3 emails · your turn since Wed    │
│                                       │ ───────────────────────────────────────────── │
│                                       │ Mia · Wed 22 Oct, 10:02    (earlier, folded)   │
│                                       │ Nate · Wed 22 Oct, 11:40   (earlier, folded)   │
│                                       │ Oscar · Wed 22 Oct, 14:15                      │
│                                       │   Full message in the reading voice …         │
│                                       │ ┌───────────────────────────────────────────┐ │
│                                       │ │ Reply to all…                    r  R  f  │ │
│                                       │ └───────────────────────────────────────────┘ │
└───────────────────────────────────────┴───────────────────────────────────────────────┘
```

**Intent.** Inbox is the way back to everything (D109): arrival order, honest, quiet. In the list beside an open reader, rows use the work rhythm with two lines (sender, subject); the wide list (no reader) uses one line with the snippet after the subject. The reader is the page under the lamp: the subject in Display, the gist only for long threads (D100), as a collapsible block at the top (BK, 2026-05-11), marked as model text, then the relationship line and the messages, earlier ones folded to one line each (BK, 2026-05-11: full height, one scroll).

**From today (`09-inbox`, `10-thread-reader`):** "49 messages" and "2 of 35" become one unit, conversations (D132). The right column's stack of to-do count, star, clip, mode word and date becomes mode marks plus the time; the mode word moves into the lit row's reason. Snippets that are gists get the model mark; machine phrasing ("Preview of: ...") is fixed in the gist prompt, not hidden. The orphaned rule left when the inline reply opens goes away with the gist block. The reader's toolbar is quiet buttons; reply, reply all and forward live in the reply field's corner (D095) and in the ⋯ menu on narrow widths (BK, 2026-05-11).

## Compose

```
                                  ┌─ New message ──────────────────────────── ✕ ┐
                                  │ To   (A) Ann Lee ×  type a name…             │
                                  │ Subject                                      │
                                  │ ──────────────────────────────────────────── │
                                  │ The editor, in the reading voice, taking     │
                                  │ the sheet's height.                          │
                                  │                                              │
                                  │ ✦ Draft for me                          ⌘J   │
                                  │ [Rich│Markdown]          ⌘↵ [ Send ]        │
                                  └──────────────────────────────────────────────┘
```

**Intent.** "compose page is about writing the email" (BK, 2026-05-11). The sheet sits on the overlay surface over a scrim, so writing and reading are clearly different states. The editor takes the height, and Send is the page's one filled button. The vim mode line appears only when vim mode is on. "Draft for me" is one quiet row with its key, and its output is marked as model text until edited.

**Replaced by [34](34-composer.md) (D161, 2026-10-22)** for replies, Draft for me and the controls: a reply opens inline under the message, help is one line that returns a proof, and the sparkle above goes (D141). A new message keeps this sheet.

**From today (`11-compose-new`):** the scrim is new; "NEW MESSAGE" in mono caps becomes the sheet's Title; Rich/Markdown uses the shared segmented control.

## Search, the palette and `?`

**Intent.** Search is one sheet over the scrim with one input. Results open as a page with the query as its Title, the mode segmented control (Exact, Hybrid, Meaning), and filters behind `g f`; a single result does not sit under eleven controls. The unit is named and counts agree ("2 conversations", and facets that sum).

The palette groups commands with eyebrows in the one eyebrow style, every item with an icon, and the key in meta on the right; it fades at the bottom when it scrolls. The `?` sheet starts with the current page's purpose once, then its keys in a two-column list where each key sits next to its verb, not in a 120px gutter. TUI parity remarks move to the docs.

## Settings and tool pages

**Intent.** Tool pages (Analytics, Rules, Accounts, Activity, Jobs, Diagnostics, Screener, Subscriptions, Deliveries, Invites, Drafts, Snoozed, Reply later, Owed replies, Waiting on) use the same `PageFrame` and `Row`. They are calmer than modes: no lit-row reasons, few marks. Settings uses its sections as the page's tabs on wide screens and as a list that drills in on phones; it has no second sidebar. The selected section uses the lamp, not a saturated teal block.

**From today:** Screener's 48 identical outlined buttons become the lit row's choice with keys (`1` Allow, `2` Deny, `3` Feed, `4` Paper trail) and quiet buttons on hover. Its badge and page count come from the same query (D132). Activity shows Pause or Resume, never both, and its table shows the action in words with the context under "Details". Waiting on gets its own purpose line ("People you're waiting to hear from.") and its own empty state, with the inverse sentence fixed. "Reply later" is the page's name too, not "Reply queue".

## Focus & reply

**Intent.** One person at a time, the composer ready, the rest of the app dimmed to the room colour. The person's name is the Title, the reply field the hero. Its count of people comes from the same "waiting on you" definition as Now (D132). The finishing moment when the last person is answered is the strongest in the app: "Everyone's answered." in Display, the scene, and "Next: 2 due in To do."

## Phone

```
┌──────────────────────────────┐
│ ● 4m     [ Search ]   ◎   ✎  │
├──────────────────────────────┤
│ Four things need you.        │
│ PEOPLE                       │
│▌(P) Pat Colleague        2h  │
│     RustConf invite · "co…   │
│ (W) Wendy Data           1d  │
│ DUE                          │
│ ☐ Pay invoice 2847   Fri     │
├──────────────────────────────┤
│ Now  Messages  To do  Read  ⋯│
└──────────────────────────────┘
```

**Intent.** The same desk in a smaller room. Tabs: Now, Messages, To do, Reading, More; More opens a sheet with Updates, Archive, Inbox and the Mail, Views and Tools groups, so every destination is reachable (D133). The active tab always matches the page. Rows truncate with ellipses. No key chips. The reader, a person's conversation and the assistant replace the page and come back with the system back gesture.
