# mxr design research: the assistant panel, and the whole app

Accessed 2026-10-16. Two parts. Part 1 covers how the best AI assistant and agent panels are designed, for mxr's right-hand assistant sidebar and its TUI twin. Part 2 covers what makes the best email and calm productivity apps good as whole products, for a design concept covering the whole mxr web app.

Evidence labels used throughout:

- `[doc]` the vendor's own docs, changelog or design writing, read on the access date.
- `[sec]` third-party review, guide or forum report. Useful, but the claim is someone else's reading.
- `[obs]` from my own product knowledge, not re-checked against a source today. Treat as unverified.

Part 2 does not repeat the per-mode research in `../email-modes/`. Those files already cover HEY's Screener and Previously Seen, Readwise Reader's Feed/Library split, Things and Linear teaching, Superhuman's backlog clearing, Shortwave bundling and the Now/navigation proposal. Where a point lives there, this file links to it instead.

---

# Part 1. The assistant side panel

## 1.1 Product by product

### Linear agents render every tool call as a verb, a target and a result, never as a payload

Linear's Agent Interaction SDK is the clearest public statement of how agent work should look inside a calm app. An agent can only emit five semantic activity types: `thought`, `action`, `elicitation`, `response`, `error` `[doc]`. An `action` has three optional fields: `action` (a verb that changes tense with state, "Searching" then "Searched"), `parameter` (the target, "San Francisco Weather") and `result` (Markdown, "12°C, mostly clear") `[doc]`. That is the whole grammar of a tool step. There is no slot for raw JSON, so a client cannot show it.

Hierarchy follows from that. `thought` and `action` can be marked ephemeral, and an ephemeral activity "is replaced when the agent's next activity arrives" `[doc]`. While the agent works you see one live line that keeps rewriting itself. The durable record is the `response`. Session state (pending, active, awaitingInput, error, complete) is derived by Linear from the last activity, so the agent can't misreport it `[doc]`.

Questions are an `elicitation` with a `select` signal that renders options. The user can ignore the options and type, which dismisses the elicitation, and the chosen option comes back as an ordinary user `prompt` `[doc]`. Option `label` and `value` can differ, so the button says something human while the agent gets a precise value `[doc]`.

References: plain Linear URLs in Markdown "render as mentions", so an issue URL becomes an issue chip and a profile URL becomes @user `[doc]`. The agent writes a link; the client decides how it looks. Stop is a first-class signal: the agent "must halt", then emit a final `response` or `error` confirming it stopped `[doc]`.

Linear's design post adds the principles: instant acknowledgment ("an agent should provide instant feedback"), transparent state, clear agent identity, and a human who stays accountable because "an agent cannot be held accountable" `[doc]`. Timing is enforced: first activity within 10 seconds or the agent is shown as unresponsive `[doc]`.

What mxr takes: this is the answer to the raw-JSON complaint. Each mxr tool gets a present and past verb, a human target and a one-line result, and the renderer turns message ids and addresses into chips.

### Cursor shows how much users fight over folding tool steps

Cursor's agent lives in a right sidepane (Cmd+I) `[doc]`. Cursor 3 (April 2, 2026) added a full-screen Agents Window built around running agents in parallel, with a "simpler diffs view" for review `[doc]`. Checkpoints sit in the chat timeline; clicking one previews files at that point, and restoring reverts files without deleting messages `[doc]`. Messages typed while the agent works queue "in order below the active task" and can be reordered; "Send now" delivers at the agent's next tool call `[doc]`.

From use `[obs]`: tool calls render as dim one-line rows ("Read `file.ts`", "Grepped 3 files"), edits as file chips with +/- counts, and a sticky review bar offers Keep/Undo across files. The eye lands on prose and the review bar; steps recede.

The forum shows where this goes wrong. A July 2026 report says tool calls stay collapsed even with the "Detailed" setting on, and the reporter accuses Cursor of hiding useful information `[sec]`. A May 2026 report says 3.5.17 applied changes directly with no per-change Keep/Undo; staff pointed to a setting `[sec]`. A June 2026 report says the Review panel never clears, which staff confirmed: it tracks all edits in the session, not pending ones `[sec]`. Lesson: folding is right by default, but the fold must open on demand and the "pending changes" surface must mean pending.

### VS Code Copilot folds finished steps into "Completed N steps"

Copilot agent mode groups completed tool calls. In Compact mode they become "expandable summaries", in Verbose mode full detail stays visible, and a collapsed group reads "Completed N steps" `[doc]`. Running steps stay visible while they run `[doc]`. Confirmation is a Continue button whose dropdown scopes the approval to the session, the workspace or always; a command resets saved approvals `[doc]`. File edits get Keep/Undo per change or for all `[doc]`. The docs say plainly that "checkpoints don't reverse changes to external services" and stopping "doesn't undo ... actions that already completed" `[doc]`. That honesty about irreversibility is worth copying for anything that hits a mail provider.

### Claude.ai and Claude desktop keep tool use to one quiet line and move big output to a side panel

`[obs]`, not re-verified today: tool calls show as a compact line with an icon and a past-tense verb ("Searched the web", with source favicons), expandable to inputs and results. Extended thinking collapses to a "Thought process" disclosure. Artifacts open in a panel beside the chat that pushes the conversation left rather than overlaying it. Research mode shows a progress timeline and returns a cited report.

Connector permissions: Anthropic's help centre documents per-tool access and that Team/Enterprise users authenticate individually `[doc]`. Third-party MCP vendors describe per-tool levels "Always allow / Needs approval / Blocked" `[sec]`. I found no official source for the exact "Allow once" label in the Claude app, so treat that wording as unverified.

Multiple-choice questions: Claude Code's AskUserQuestion tool offers single-select, multi-select and an "Other" free-text option that is always added `[sec]`. Questions carry a short header and per-option descriptions; a recommended option goes first with "(Recommended)" `[sec]`. Whether claude.ai has an official clickable-question widget is unconfirmed; one GitHub issue shows Claude Desktop lacked the tool in March 2026 `[sec]`. One third-party client rendered the tool as raw JSON with only Approve/Deny buttons, which made the options unusable `[sec]`. That is the same failure BK saw.

### ChatGPT agent narrates, confirms before consequential acts, and hands over control for secrets

OpenAI's own pages returned 403 today, so this is secondary `[sec]`: the agent shows a live view with short on-screen explanations, pauses for confirmation before consequential steps such as purchases, uses a takeover mode for logins and payment details, and has a watch mode for sensitive sites where the user must supervise. A governance paper notes the agent stops taking screenshots in watch mode `[sec]`. Sources disagree on whether "agent mode" still exists under that name in 2026 `[sec]`, so treat the current product shape as unverified. The design idea that survives either way: the agent tells you what it is doing in plain words, and the risky moment is the only moment it stops.

### Notion AI separates proposing from applying

Third-party sources describe a suggest-only mode where each proposed edit is approved or rejected in turn, a plan mode that writes a plan page before touching anything, and a track-changes overlay that can be shown or hidden after the agent edits `[sec]`. Notion 3.0 (September 2025) rebuilt AI as agents that can work for 20 minutes across many pages `[sec]`. I did not find Notion's own docs describing how the sidebar shows steps, so the specifics are unverified.

### Superhuman Ask AI puts the assistant in a sidebar on one key, with history

Ask AI moved to a left sidebar on desktop, opened with `?`; chat history is browsable from a top-right icon; when Write with AI needs a clarifying question it opens Ask AI to ask it `[doc]`. That last point is good design: questions have one home. The post says nothing about how sources are cited or whether actions are confirmed, and the help article returned 403, so those details are unverified.

### Shortwave proposes grouped actions and waits for confirmation

Shortwave's assistant opens in the right sidebar or with Cmd-J and "adapts to what's on your screen" `[doc]`. Asked to "organize my inbox", it recommends actions "on groups of threads" (archive, label, delete junk, create todos) while you stay "fully in control at each step" `[doc]`. The changelog records the assistant "now waits for action confirmation while organizing your inbox" `[doc]`. Search results list threads with an `about:` button that opens the full set `[doc]`. Saved prompts appear as suggestions when the input is focused, which is its empty state `[doc]`. Shortwave also recommends Done (archive) over delete because done mail stays searchable `[doc]`. Visual details of the confirm UI are not documented; unverified.

What mxr takes: grouped proposals ("archive these 14 from Substack") with one confirm per group and a way to drop single items, not one dialog per thread.

### Dia spends its novelty budget on the chat panel and keeps everything else plain

The Browser Company's design post is explicit: "In Dia we spend our novelty budget on our Chat features" `[doc]`. The chat sidebar opens with a brand animation that "swells into the view" to signal the page is now context `[doc]`. Context is attached with @-mentions, chosen because it matches tagging people on social media; multiple tabs form an animated pile that the team admits "is mostly superfluous" `[doc]`. Skill output is styled differently so you know a skill is active `[doc]`. Elsewhere: "there should be a single obvious way to accomplish any task", controls appear only when relevant, and there are no update banners `[doc]`. BK uses Dia, so this is the reference for how an assistant can carry the brand without the rest of the app getting louder.

Arc Max (2023) took the opposite route: AI as small features woven into existing gestures, such as Shift-hover link previews, tidied tab titles and an answer in Cmd+F when a word isn't found `[sec]`. Its current status is unverified.

### Raycast keeps quick questions and conversations apart

Quick AI is for one-off questions; Tab then J continues in AI Chat `[doc]`. `@` mentions bring in AI Extensions as tools `[doc]`. Actions that need approval show "a confirmation with a preview of what will be performed" `[doc, older v1 manual]`. A March 2026 beta renamed presets to Agents and added memory, skills, auto-archiving of chats and a "Danger mode" that skips confirmations `[doc]`. The split between a throwaway answer and a kept conversation is relevant to mxr: most assistant uses ("who is this?") don't need history.

### Granola marks provenance in the type colour

"Everything from you is in black, everything else is in gray", and "Zoom In" shows the transcript quotes a note is based on `[doc]`. A critic called the result "gray on gray" with weak hierarchy `[sec]`. The lesson is the principle (authorship visible at a glance, evidence one click away), with care that the AI layer is still readable.

### Apple's HIG gives the strongest written rules for approvals and progress text

Apple's generative AI page (updated June 8, 2026) says: "ask for confirmation before performing a significant action"; avoid automating destructive actions; surface "Edit, Undo, Retry, or Adjust near generated content"; give "a clear signal that their action had an effect"; replace "Processing..." with specific text like "Summarizing key themes from your notes"; when something fails, "describe what happened in plain language and offer a clear next step"; offer curated suggestions for open-ended inputs `[doc]`.

## 1.2 How the terminal agents do it

### Claude Code collapses by default and opens everything with one key

`Ctrl+O` toggles a transcript viewer with "detailed tool usage and execution", and expands lines that collapse by default, such as MCP calls shown as a single `Called slack 3 times` line `[doc]`. `Ctrl+T` toggles a task list showing up to five items with pending/in-progress/done markers `[doc]`. `Esc` interrupts and "Claude keeps the work done so far"; queued messages go next `[doc]`. `Shift+Tab` cycles permission modes, shown in a mode indicator `[doc]`.

The permission prompt shows a title (the tool), a plain description ("Run the test suite"), the exact command, and then "Do you want to proceed?" with Yes / Yes, and don't ask again for ... / No `[doc]`. Two details stand out. Claude Code offers the broader options "only when the prompt can show you everything they would allow, so a rule you save from a prompt covers only what its option named" `[doc]`. And `Tab` on Yes or No opens a comment field, so a denial can carry the reason and the agent continues instead of stopping `[doc]`. `/diff` opens a diff panel beside the conversation (auto-opens at 144+ columns) listing files with +/- counts, refreshed on every edit, with an `ask` button that attaches one file's diff to the next prompt `[doc]`.

### Codex CLI puts single-key shortcuts on approval options

Codex has three sandbox modes and a `/permissions` command to set when it may act without asking `[doc/sec]`. The TUI approval overlay labels "Yes, proceed" with `y` and "Yes, and don't ask again" with `a`; a session-scoped "don't ask again" was later extended from shell commands to file patches for parity `[sec, mirrored commits]`. Its docs say you can "inspect commands and diffs as they appear" `[doc]`. Exact diff rendering is unverified.

### aider makes every change a git commit, so undo is a real command

aider commits each edit with a descriptive message, commits your dirty work first so its changes stay separate, and offers `/undo` and `/diff` `[doc]`. The undo story is the version control the user already trusts. mxr's equivalent is that every assistant mutation goes through the same daemon path as a manual action and lands in the activity log, so "undo last assistant change" is one verb.

### gemini-cli previews sensitive changes and asks "Allow once"

Users are prompted to approve file writes and shell commands "with a preview of changes"; the getting-started example uses "Allow once" `[doc]`. Note: the same page says Gemini CLI was replaced by Antigravity CLI for unpaid and Google One users on June 18, 2026 `[doc]`.

## 1.3 Design writing worth carrying

- Adobe Design's research: people want conversation for exploring and "structured, inspectable interfaces" before committing; "When autonomy outpaces visibility, trust breaks, even with correct outputs"; trust is "built in sequence"; mode changes must be legible because they change who is responsible `[doc]`.
- An arXiv paper on agent UI governance lists visible plans and actions as a pattern worth protecting from removal "for convenience or A/B testing" `[sec, academic]`.
- Streaming text libraries converge on a short per-word fade (not per-character), excluded from code, citations and structured blocks, switched off under reduced motion, and never replayed when an old conversation reloads (LibreChat, Streamdown) `[doc]`. Pacing is separate from the animation `[doc]`.

## 1.4 Patterns the best ones share

1. **One grammar for a step: verb, target, result.** Linear's `action/parameter/result`, Claude's "Searched the web", Copilot's step groups. Present tense while running, past tense when done. Payloads never appear by default.
2. **Live steps collapse into one summary line when the turn ends.** Linear's ephemeral activities, Copilot's "Completed N steps", Claude Code's `Called slack 3 times`. The answer is the hero, the steps are a footnote you can open.
3. **The fold opens on demand, fully.** Claude Code's `Ctrl+O`, Copilot's Verbose. Cursor's forum shows the anger when it doesn't.
4. **Approval is a structured card, not a chat bubble.** Title, plain description, the exact effect, then a short set of options with keys (Claude Code, Codex `y`/`a`). Broader approvals are offered only when the card can show what they cover.
5. **Batch proposals for batch work.** Shortwave groups threads; Copilot and Cursor offer Keep/Undo per change and for all. One decision per group, with single items removable.
6. **A denial can carry a reason and the agent keeps going.** Claude Code's comment on No. Linear lets you ignore options and type.
7. **Questions are a first-class event with options, a recommended default and always a free-text escape.** Linear `select`, AskUserQuestion "Other". The option label is human; the value is precise.
8. **References render as the app's own objects.** Linear turns URLs into issue and user chips. The model writes a link; the client draws the chip.
9. **State is derived, not narrated.** Linear computes session state from the last activity; Claude Code shows the mode in an indicator. The panel always answers "working / waiting on you / done / failed".
10. **Interruption is cheap and keeps finished work.** Esc in Claude Code, Stop in Linear, "Send now" in Cursor. Copilot is honest that stopping doesn't undo what already ran.
11. **Specific progress text and plain-language failures with a next step.** Apple HIG, Linear `error`.
12. **Novelty spent in one place.** Dia spends it on the chat panel's arrival; Granola on provenance colour. The rest of the app stays plain.

## 1.5 Anti-patterns

- Raw JSON, ids or tool names in the default view (the third-party AskUserQuestion client; BK's own reaction).
- Approve/Deny with no description of the effect, or a broad "always allow" that covers more than the card shows.
- Folding that can't be unfolded, or a "Review" list that doesn't mean "pending" (Cursor forum).
- One modal per thread for a 40-thread cleanup.
- Every step as a full-width bubble, so a ten-step turn pushes the answer off screen.
- Chat bubbles with avatars for the assistant's own working notes; the steps aren't messages.
- Vague spinners ("Thinking...") when the agent knows what it is doing.
- Replaying streaming animation when history reloads; per-character fades.
- Autonomy changes without a visible mode (Adobe: role changes must be legible).
- Confirm-everything, which trains blind clicking; and its opposite, Raycast-style "Danger mode" as a default.

## 1.6 Three assistant concepts for a calm, keyboard-first, local-first mail client

**Marginalia.** The assistant is an editor writing in the margin of your mail. The panel is a narrow ruled column, no bubbles: your prompt as an italic quote with the 2px paper rule already in DESIGN.md, the answer as body prose, and steps as small mono marginal notes (`searched 14 threads from Priya · 0.4s`) that fold into one line when the turn ends. Emails and people the answer mentions are underlined like footnote anchors; hovering or `Enter` on one highlights the row in the list beside it. Approvals look like proof marks: a short list of proposed changes with a checkbox per line and one "apply 3 changes" action. Mood: quiet, literate, it never shouts; it reads like the mxr site's agent transcripts (§03).

**The log.** The assistant as a ship's log or shell session, leaning into the "publishing-meets-shell" half of the brand. Every turn is a timestamped entry; each step is one mono line with a status glyph (`·` running, `✓` done, `✗` failed) and the verb/target/result grammar, exactly what the TUI can render too. Approvals are "orders" that wait at the bottom of the log with key hints (`y` apply, `n` skip, `tab` note). Mood: precise and auditable; strongest for TUI/web parity and for trust in an app whose pitch is "your whole history as signals an agent uses".

**Proof tray.** The conversation is secondary; what matters is the tray of proposed changes the assistant prepares. The panel has a fixed lower region, the tray, that fills with grouped proposals ("Archive 14 from Substack", "Star 2 from Sam") each with a count, an expand to see the threads, and keep/drop. The chat above is short and mostly answers questions. Mood: calm desk work; the assistant is a clerk who lays things out for your signature. Fits mxr's preview-before-mutation rule most directly, since the tray is the dry run.

---

# Part 2. The whole app

## 2.1 Product by product

### Superhuman makes speed and the next action the product

Rahul Vohra's "game design, not gamification" essay is the design brief behind it `[doc]`. The points that are design, not features:

- One concrete, rewarding goal (Inbox Zero), rewarded with calm, awe-inspiring imagery when reached `[doc]`.
- "Rapid, robust controls": keystrokes are pipelined so a fast typist never creates the duplicate-draft errors Gmail does `[doc]`.
- "Make the next action obvious": archive shows the next email, never the list `[doc]`.
- "Clear, immediate feedback with no distractions": one conversation at a time; "you cannot see the inbox at the same time as the conversation" `[doc]`.
- Cmd+K shows each command's shortcut, so the palette teaches its own replacement `[doc]`.
- Toys inside the work: the snooze parser accepts "3D", "1MO" or "never" `[doc]`.

Secondary design breakdowns add a 50-100 ms response target, a split inbox navigated with H/L between splits and J/K within, colour used "for status and focus, never for decoration", and a serif for message bodies with a sans UI `[sec, uncited numbers]`. Ask AI now sits in a sidebar on `?` `[doc]`. See `teaching-in-place.md` §9 and `relevance-and-backfill.md` §4 for onboarding and backlog.

### HEY names its places like rooms and its verbs like errands

Nouns are places with a job: Imbox (approved people), The Feed ("like a Facebook ... feed, but for email"), Paper Trail ("a digital shoebox; stuff you need to keep, but out of your face"), Previously Seen `[doc]`. Verbs are errands: Reply Later, Set Aside ("At hand, but out of your face"), Focus & Reply ("Line 'em up, knock 'em down"), Bubble Up `[doc]`. The philosophy is "let it flow ... then just move on" and "let time do the busy work", with notifications off by default `[doc]`. Jorge Arango's information-architecture reading: the novel part is treating the message as the core object (renamable subjects, merged threads, a Files view of every attachment), not the filtering, and the specialised views "could be simulated" elsewhere `[sec]`. The voice is opinionated and casual; copy openly criticises normal email `[doc]`. Hand-drawn illustration and the yellow-led palette are `[obs]`, unverified today. Screener, Previously Seen and Paper Trail behaviour is already covered in `messages.md`, `now-and-handoff.md` §G and `archive.md`.

### Mimestream wins by being native and faithful, not novel

MacStories calls it "the core Gmail experience wrapped in a thoughtfully designed native Mac app": three panes, profiles with their own icons and counts tied to macOS Focus, compact and expanded list densities, its own shortcuts plus a switch to Gmail or Apple Mail keymaps, customisable swipes `[sec]`. Another review calls it fast and clean but "not very visually appealing" `[sec]`. The lesson for mxr: fidelity to the provider's model (labels as labels) plus platform-correct behaviour earns trust without visual flair. Its gaps (no deep links into the app) are the kind mxr's daemon/CLI already avoids.

### Apple Mail's categories treat a sender as a digest

macOS 15.4 sorts into Primary, Transactions, Updates and Promotions; time-sensitive mail from the other three also shows in Primary; you can recategorise a sender; the whole thing turns off from the View menu `[doc]`. Opening a message in a non-Primary category shows a digest of that sender's mail, like a profile page `[sec]`. Two design moves matter: the sender, not the message, is the row outside Primary; and the escape hatch (All Mail, turn it off) is one click away. mxr's Updates research already makes the per-source row argument (`updates.md` §2).

### Notion Mail is attractive and empty of opinion

Reviews agree the draw is a stripped, text-first layout that resembles Notion `[sec]`. Notion's stated goal was not to "attach AI to an existing inbox" but a modular inbox you configure into views, with AI auto-labels from natural-language prompts `[doc via TechCrunch]`. Reviewers note you can't test or refine a label prompt after creating it `[sec]` and call it a pretty Gmail wrapper `[sec]`. Lesson: configurability is not a mental model. mxr's five modes are an opinion; that is the advantage.

### Spark front-loads a Home screen and a sender gate

Spark 3 (2022) added a Home screen meant to reduce how often you open the inbox, a Smart Inbox grouping newsletters and notifications below personal mail, and Gatekeeper cards for new senders at the top of the inbox `[sec]`. I found no 2025-2026 design review; current behaviour is unverified. The Gatekeeper carousel is a known pattern; the risk is that a carousel at the top of the list becomes a permanent chore.

### Shortwave bundles and collapses, and puts AI on a key

Bundles group mail by kind, history collapses by default, Done keeps mail searchable, the assistant is on Cmd-J `[doc]`. Covered in `messages.md` and `updates.md`; not repeated.

### Fastmail is utilitarian and keyboard-complete, not a design reference

Fastmail documents `?` for a per-screen shortcut sheet, `g` jumps and toggles for shortcuts, light/dark themes and adjustable text size `[doc]`. I found no credible design review; a third-party extension complains the shortcuts aren't intuitive `[sec]`. It belongs here as the honest, fast, plain baseline: per-screen shortcut help is the useful idea.

### Linear is the reference for calm density

The March 12, 2026 refresh ("A calmer interface for a product in motion") states the rule mxr should adopt: "the parts central to the user's task should stay in focus, ones that support orientation and navigation should recede" and "Don't compete for attention you haven't earned" `[doc]`. Concretely: the sidebar dimmed a few levels, tabs compacted, fewer and smaller icons, decorative team-icon backgrounds removed, borders softened because separators had multiplied, "Structure should be felt not seen", and default greys moved from cool blue toward a less saturated warm grey `[doc]`. The 2024 redesign generated themes in LCH from three inputs (base, accent, contrast) instead of 98 variables, cut "how much chrome (blue in our case)" fed into the palette, used Inter Display only for headings, and aligned sidebar labels and icons on both axes, which users "feel after a few minutes" `[doc]`. Keyboard: `g` then a letter, mnemonic single keys, Cmd+K that keeps you on the keyboard after it opens `[doc/sec]`; mxr already uses the `g` grammar (`now-and-handoff.md` §D). Motion numbers in third-party breakdowns are illustrative, not Linear's `[sec]`.

Note for mxr: Linear went warmer and less blue to feel calmer. mxr's identity is deliberately blue. The transferable part is the relative rule (navigation dimmer than content, accent used sparingly), not the hue.

### Things 3 makes the core act feel physical

MacStories: a to-do "pop[s] out into a card-like form while the rest of the list fades into the background"; the Magic Plus button can be dragged onto a project or a day to create there; "This Evening" splits Today without a second list; Type Travel lets you start typing anywhere to jump; the design is "dominated by white space" with "bold fonts ... and thoughtful splashes of color"; actions "give a sense of depth, of meaningful tactile engagement" `[sec]`. Lessons: open one item in place with the rest fading back (not a page change); split a day by time-of-day inside one list; let typing be navigation.

### Dia keeps the core familiar and puts novelty in one place

Covered in 1.1. The whole-app point: "a single obvious way to accomplish any task", minimal core UI, controls shown only when relevant, the page colour extended into the chrome so the content leads `[doc]`.

### Readwise Reader is the reading reference

Covered in `reading.md` (Feed vs Library, keyboard reading, 65ch measure). One addition: Reader moved all its AI reading tools into a single web chat sidebar `[sec]`, which supports one assistant home rather than AI buttons per surface.

### Basecamp and HEY's philosophy is "less is less", and calm as a stance

37signals' calm-company writing argues against urgency as a default and for protecting attention `[sec]`; "Getting Real" argues for designing the real screen over describing it `[sec]`. I did not find a 37signals essay specific to HEY's visual design; unverified.

## 2.2 Patterns the best whole products share

1. **A small set of nouns that are places with a job, and verbs that are errands.** HEY's Imbox/Feed/Paper Trail and Reply Later/Set Aside; Things' Today/This Evening. mxr's five modes plus Now already are this; the copy should name verbs as errands ("let go", "later", "done here").
2. **Navigation recedes, content leads.** Linear's dimmed sidebar, Dia's minimal chrome, Superhuman hiding the list while you read.
3. **The next action is obvious after every action.** Superhuman's archive-to-next; HEY's Focus & Reply.
4. **Every action has a key, and the palette teaches it.** Superhuman's Cmd+K shortcut hints, Linear's mnemonics, Fastmail's per-screen `?`.
5. **One accent, used for state and focus only.** Superhuman's rule; Linear cutting chrome colour. mxr already restricts cyan and keeps yellow under 3%.
6. **Structure felt, not seen.** Linear's softened separators; Things' whitespace. For mxr: ruled lines at `--rule-soft`, few of them, no boxes.
7. **One typeface family with roles.** Linear (Inter / Inter Display), Superhuman (sans UI, serif bodies, mono time) `[sec]`. mxr has this in Recursive's axes: CASL body, MONO meta, display headings.
8. **A rewarding, bounded end state.** Superhuman's Inbox Zero image; HEY's flow; mxr's research already specifies honest end states per mode (`now-and-handoff.md` §E, `trust-and-completeness.md` §7).
9. **Open one thing in place.** Things' card pop; Superhuman's one-conversation view; mxr's raised reader plane.
10. **Speed is felt: optimistic UI with undo instead of confirmation for reversible acts.** Superhuman, Linear `[sec]`; Apple's HIG reserves confirmation for significant or irreversible acts `[doc]`.
11. **Novelty is budgeted.** Dia spends it on chat; Superhuman on the zero state; Things on Magic Plus. Everything else is quiet.
12. **The provider's model is respected.** Mimestream's labels, Apple's "turn it off". Opinionated views sit on top of a mailbox the user can still see whole (mxr's Inbox entry).

## 2.3 Anti-patterns

- Configurable views as a substitute for an opinion (Notion Mail).
- A top-of-list carousel or card strip that becomes a standing chore (Spark Gatekeeper risk).
- Unread counts and badges on modes that aren't work (`reading.md` §3, `now-and-handoff.md` §D).
- Separators, icons and coloured backgrounds multiplying until structure is seen, not felt (Linear's own diagnosis).
- AI buttons scattered on every surface instead of one assistant home.
- Cards for every row; this also breaks mxr's "ruled lines, not cards" rule.
- Per-mode full-page tints that make modes feel like different apps (`now-and-handoff.md` §E).
- Confirmation dialogs for reversible actions; slow modal flows where an undo toast would do.
- A tour on first run (`teaching-in-place.md` §1).

## 2.4 Three whole-app concepts for mxr

All three keep the brand in DESIGN.md: navy ink surface, cyan signal for focus and state, a small yellow cue, Recursive's axes as the type system, ruled lines and folios, no cards.

**The Edition.** mxr as a daily paper about your own mail. Now is the front page: a masthead with the date and a folio, four fixed sections in ruled columns, each a short list with an honest end line. Each mode is a section of the paper with its own folio (`§ 01 Messages`, `§ 02 To do`), its own row shape and density, and the same masthead and rules. The thread reader is a raised page set in CASL body type at a 65 to 72ch measure; compose is a clean galley. The assistant is the editor's desk on the right, in the Marginalia style: italic prompt, prose answer, mono marginal notes for steps, footnote-anchored references, proof marks for approvals. Mood: unhurried, literate, finished; you read it and you are done. Risk: an editorial frame can feel slow for rapid triage, so keys and density in Messages and To do must stay tight.

**The Logbook.** mxr as a well-kept log, the shell half of the brand made primary. Every surface is time-ordered and dense; dates and counts in MONO tabular numbers; the status bar is always present and tells you sync freshness, mode and pending changes; Cmd+K is a prompt (`$`) that runs the same verbs as the CLI and shows each key. Modes are filtered views of one log with clear end lines. The assistant is The Log concept: timestamped entries, one-line steps with glyphs, orders at the bottom awaiting `y`/`n`. The TUI and web are nearly the same picture, which makes parity cheap. Mood: precise, trustworthy, a tool for people who like their terminal. Risk: Reading and Messages lose warmth; they need the CASL body voice to soften the log.

**The Desk.** mxr as a calm desk with trays, extending the desk model already in the app. Now is the desk surface: what's on it today, with each mode a tray you pull open (Messages as letters from people, To do as a clipboard, Updates as a folded briefing, Reading as a stack of periodicals, Archive as the cabinet). Moving mail between modes is handing it to a tray, and the toast names the tray. Opening an item lifts it onto the desk while the rest fades (the Things move), set in the raised reader plane. The assistant is the Proof tray concept: a clerk who lays proposed changes out for your signature, with grouped keep/drop and one apply. Mood: physical, tidy, a sense of putting things away. Risk: skeuomorphic drift; keep it in nouns and motion, never in textures, which DESIGN.md forbids.

## 2.5 Open questions for BK

- Is the assistant one panel per app (Superhuman, Readwise) or does each mode get its own suggestions? The evidence favours one home.
- Which accent marks the assistant's work: cyan (state) or the yellow cue (attention)? Yellow is under 3% of a screen today.
- Should approvals be per change, per group, or a tray applied at once? Shortwave and Copilot suggest per group with single-item drop.
- Does the TUI pane follow the Log concept regardless of which web concept wins?

---

# Sources (all accessed 2026-10-16)

Assistant and agent panels

- Linear, Agent Interaction docs: https://linear.app/developers/agent-interaction
- Linear, Agent Signals: https://linear.app/developers/agent-signals
- Linear, Our approach to building the Agent Interaction SDK: https://linear.app/now/our-approach-to-building-the-agent-interaction-sdk
- Cursor, Agent overview docs: https://cursor.com/docs/agent/overview
- Cursor, 3.0 changelog: https://cursor.com/changelog/3-0
- Cursor, Agents Window docs: https://cursor.com/docs/agent/agents-window
- Cursor forum, tool call details hidden: https://forum.cursor.com/t/new-version-hides-agent-tool-call-details-in-defiance-of-setting/165292
- Cursor forum, 3.5.17 removed Keep/Undo: https://forum.cursor.com/t/cursor-3-5-17-removed-keep-undo-and-applies-agent-changes-directly/161325
- Cursor forum, Review panel doesn't dismiss: https://forum.cursor.com/t/review-panel-doesnt-dismiss-rendering-it-useless-reproduced-by-another-user-too/163722
- VS Code, Copilot agent mode: https://code.visualstudio.com/docs/copilot/chat/chat-agent-mode
- VS Code, Copilot edits: https://code.visualstudio.com/docs/copilot/chat/copilot-edits
- Claude help centre, Manage Claude's tool access: https://support.claude.com/en/articles/13730515-manage-claude-s-tool-access
- Kiteworks, MCP connector permission levels: https://developer.kiteworks.com/configure-mcp-connector-claude.html
- Claude Agent SDK, approvals and user input: https://code.claude.com/docs/en/agent-sdk/approvals
- ClaudeLog, AskUserQuestion: https://claudelog.com/faqs/what-is-ask-user-question-tool-in-claude-code/
- slopus/happy issue 635 (raw JSON for AskUserQuestion): https://github.com/slopus/happy/issues/635
- Claude Desktop AskUserQuestion issue: https://claudeissues.com/issue/39094-support-for-askuserquestion
- ChatGPT agent help (403 today; content via secondary sources): https://help.openai.com/en/articles/11752874-chatgpt-agent
- Pluralsight, What is ChatGPT agent: https://www.pluralsight.com/resources/blog/ai-and-data/what-is-chatgpt-agent
- arXiv 2512.00742, UI for AI agent governance: https://arxiv.org/pdf/2512.00742
- Notion 3.0 release: https://www.notion.com/nl/releases/2025-09-18
- Matthias Frank, Notion AI show/hide changes: https://matthiasfrank.de/en/notion-updates/notion-ai-show-hide-changes/
- Superhuman blog, Agentic writing + Ask AI: https://blog.superhuman.com/new-in-superhuman-ai-agentic-writing-ask-ai-android/
- Shortwave, AI Assistant guide: https://www.shortwave.com/docs/guides/ai-assistant/
- Shortwave, changelog: https://www.shortwave.com/changelog/
- Shortwave, The Shortwave Method: https://www.shortwave.com/docs/guides/method/
- The Browser Company, The strategy behind Dia's design: https://browsercompany.substack.com/p/the-strategy-behind-dias-design
- TidBITS, Arc Max: https://tidbits.com/2023/10/06/arc-web-browser-introduces-focused-ai-features/
- Raycast manual, AI: https://manual.raycast.com/v1/ai
- Raycast manual, AI Extensions: https://manual.raycast.com/ai/ai-extensions
- Raycast beta 0.51 changelog: https://www.raycast.com/changelog/macos-beta/0-51
- Granola, announcement: https://granola.ai/blog/announcement
- TechCrunch, Granola launch: https://techcrunch.com/2024/05/22/granola-debuts-an-ai-notepad-for-meetings/
- Apple HIG, Generative AI (via JSON endpoint): https://developer.apple.com/design/human-interface-guidelines/generative-ai
- Adobe Design, Designing trust for agentic AI: https://adobe.design/ideas/designing-trust-for-agentic-ai-in-enterprise-software
- LibreChat, smooth streaming: https://www.librechat.ai/docs/features/smooth_streaming
- Streamdown: https://streamdown.ai/llms.txt

Terminal agents

- Claude Code, Interactive mode: https://code.claude.com/docs/en/interactive-mode
- Claude Code, Permissions: https://code.claude.com/docs/en/permissions
- Codex CLI docs: https://learn.chatgpt.com/docs/codex/cli
- Codex TUI mirrored commits (approval key labels, session approvals): https://forge.lthn.ai/core/core-agent-ide/src/commit/66450f04457faffbe97f6ae725bd95b589d26f39
- aider, Git integration: https://aider.chat/docs/git.html
- Gemini CLI, Get started: https://geminicli.com/docs/get-started

Whole-app references

- Superhuman, Game design not gamification: https://blog.superhuman.com/game-design-not-gamification/
- Blake Crosley, Superhuman design guide (secondary, uncited numbers): https://blakecrosley.com/guides/design/superhuman
- HEY, features: https://www.hey.com/features/
- HEY, Let it flow: https://hey.com/flow/
- Jorge Arango, TAOI: Rethinking email: https://jarango.com/2020/06/17/taoi-rethinking-email/
- MacStories, Mimestream review: https://www.macstories.net/reviews/mimestream-the-perfect-email-app-for-gmail-users-on-the-mac/
- Kowatek, Mimestream review: https://blog.kowatek.com/?p=29531
- Apple Mail User Guide, categories: https://support.apple.com/guide/mail/mlhl64d76621/mac
- Digital Trends, Apple Mail categories on Mac: https://www.digitaltrends.com/computing/apple-mail-mac-ipad-update-categories-feature/
- TechCrunch, Notion Mail launch: https://techcrunch.com/2025/04/15/notion-releases-its-ai-driven-email-inbox
- eesel, Notion Mail reviews: https://www.eesel.ai/blog/notion-mail-reviews
- Spark, Introducing the new Spark: https://sparkmailapp.com/blog/all-new-spark
- 9to5Mac, Spark 3: https://9to5mac.com/2022/10/04/spark-mail-app-major-update/
- Fastmail help, keyboard shortcuts and accessibility: https://fastmail.help/hc/en-us/articles/7686985052559-General-accessibility
- Linear, A calmer interface for a product in motion (2026-03-12): https://linear.app/now/behind-the-latest-design-refresh
- Linear, How we redesigned the Linear UI: https://linear.app/now/how-we-redesigned-the-linear-ui
- Blake Crosley, Linear design guide (secondary, illustrative numbers): https://blakecrosley.com/guides/design/linear
- MacStories, Things 3 review: https://www.macstories.net/reviews/things-3-beauty-and-delight-in-a-task-manager/
- Readwise Reader, Ghostreader docs: https://docs.readwise.io/reader/guides/ghostreader/overview
- Antoine Buteau, Lessons from Jason Fried: https://www.antoinebuteau.com/lessons-from-jason-fried/

Internal references (not repeated here)

- ../../../DESIGN.md
- ../email-modes/now-and-handoff.md
- ../email-modes/messages.md
- ../email-modes/todo.md
- ../email-modes/updates.md
- ../email-modes/reading.md
- ../email-modes/archive.md
- ../email-modes/teaching-in-place.md
- ../email-modes/trust-and-completeness.md
- ../email-modes/relevance-and-backfill.md
