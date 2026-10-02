# Messages: email from the people you care about, shown as people and their topics

Research for the Messages mode in [22-email-modes.md](../../blueprint/22-email-modes.md).
Code references are to the `modes` worktree at `bdf997c4`. Screens referenced are
the judge captures from the same build (`01-desk`, `04-reader`, `06-focus`,
`07-reading`, `08-paper-trail`).

Evidence and opinion are marked. "Evidence" means a study, a vendor's own
documentation, or a measured result. "Reported" means reviews and user complaints:
real, but a self-selected sample. "Proposal" and "reasoning" are mine.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## 1. The job is keeping up with a handful of people, and success is never leaving one of them hanging

The Messages job is relational, not informational. You open it to answer
"who is waiting on me, and what do I want to say to the people I'm close to?"
The verb is reply, or start something new with someone.

Success feels like iMessage on a good day:

- You see faces, not subjects. The first thing you read is a name you care about.
- You can tell in one glance whose turn it is. Nothing is lost under a newsletter.
- Answering is cheap. The box to type in is already there, addressed, in context.
- A short answer is fine. Not every message needs a formal reply; "got it" is a
  complete response.
- When you are done with someone, they go quiet without being filed or deleted,
  and they come back the moment they write again.
- You visit when someone writes, or a few times a day, and you leave in minutes.

Today's build does not feel like this. The desk (`01-desk`) is a table of
subjects grouped by mail mechanics (You owe, Waiting on, New from people) and
sorted by age and pace (`sort_lane` in `crates/daemon/src/handler/desk_lanes.rs:605`).
Samir appears three times, once for a security alert. The reader (`04-reader`)
renders his reply as a full HTML letter under a giant `Re: Contract renewal details`
heading, with the four-message thread as a list of "New update in thread" rows.
Focus & reply (`06-focus`) is the closest thing to a conversation, and it is good:
the context line "You and Samir: 13 emails · you usually reply within 47m" is
exactly the right kind of fact. But it is a queue of threads, not a place to be
with a person.

## 2. The best apps make the person the row, the turn visible and the reply box always present

### The unit in chat apps is the person, and topics live inside

Evidence. In iMessage, WhatsApp and Signal, a one-to-one chat is a person: one
row per counterparty, no subject line, latest message as the preview. Groups are
their own rows. Slack keeps one DM per person and puts topics inside it as
threads; it introduced threads in 2017 to stop side discussions taking over a
conversation ([New Atlas](https://newatlas.com/slack-introduces-threads/47452/),
[Slack Engineering](https://slack.engineering/weaving-threads/)). Telegram added
the same thing for groups: topics are "individual chats within the group" with
their own history, shown either as a topic list or merged into one timeline with a
small topic label per message
([Telegram](https://telegram.org/blog/topics-in-groups-collectible-usernames),
[Metricgram](https://metricgram.com/blog/telegram-forum-topics-guide)).

Email already has topics: the subject line. So the chat pattern that fits email is
not "one bubble stream per person" but "a person, with their topics inside", like a
Slack DM with threads or a Telegram forum.

### Pins, mentions and inline replies are how chat apps rank without an algorithm

Evidence. iMessage lets you pin up to nine conversations above the list, and
added inline replies to a specific message and @mentions in iOS 14
([MacRumors](https://www.macrumors.com/2020/06/22/ios-14-imessage-features/)).
Beeper, which unifies many chat networks, sorts by recency but gives you Low
Priority (hidden, silenced except for mentions and direct replies) and Archive
(hidden until a new message arrives), so "inbox zero" means "only active chats
are left" ([Beeper help](https://help.beeper.com/en_US/android/beeper-android-how-does-inbox-work)).

Reasoning. The common shape: a small user-chosen top set, a recency list below,
and a quiet place for people you've finished with that self-empties when they
write. No chat app ranks people by a hidden score.

### HEY splits by "seen", not by "read", and the thread jumps back when someone writes

Evidence. HEY's Imbox puts "New For You" on top and "Previously Seen" below, and
a thread jumps back to the top when a new email arrives in it, replacing
read/unread state ([HEY](https://www.hey.com/features/different-states/)).
Reply Later moves a thread to a pile at the bottom, and Focus & Reply lays all of
them out with a reply box beside each
([HEY features](https://www.hey.com/features/), [The Sweet Setup](https://thesweetsetup.com/hey-email-disrupted-my-email-workflow/)).
mxr's Focus & reply is the same idea and already ships.

### Relationship history predicts who matters, and the signals are reciprocity, recency and longevity

Evidence, and the strongest support for ranking by person:

- People concentrate most of their communication on a few close contacts, and the
  shape of that concentration (their "social signature") is stable over time even
  as the people in it change
  ([Saramäki et al., PNAS 2014](https://www.pnas.org/doi/10.1073/pnas.1308540110)).
- In Whittaker, Jones and Terveen's contact-management study, reciprocity,
  recency and longevity of email interaction were strong predictors of how
  important a contact was. They also found the reverse problem: stale contacts
  need to drop out ([CSCW 2002](https://dl.acm.org/doi/10.1145/587078.587109),
  [White Rose eprint](https://eprints.whiterose.ac.uk/8387/)).
- In interviews and a survey behind SNARF, "social information was vital for
  determining the importance of an email": close colleagues, managers and partners
  were important, and people you did not usually reply to were not. SNARF sorted
  correspondents by social metrics such as mail you sent them and unread mail from
  them, separating To from CC ([SNARF paper](https://clab.iat.sfu.ca/pubs/SNARF.pdf)).
  A field deployment found users "better able to understand the nature of their
  email relationships and triage mail more effectively"
  ([Fisher et al.](https://www.researchgate.net/publication/221098917_Using_Social_Metadata_in_Email_Triage_Lessons_from_the_Field)).
- Gmail Priority Inbox's ranking uses social features, such as the share of a
  sender's mail the recipient reads, and thread features, such as whether the
  user started the thread ([Aberdeen et al., Google](https://research.google.com/pubs/archive/36955.pdf)).

### Response timing is social information, which is why "usually replies in 47m" works

Evidence. Tyler and Tang found people actively manage a "responsiveness image"
and set a pace per correspondent
([ECSCW 2003](https://link.springer.com/chapter/10.1007/978-94-010-0068-0_13)).
mxr's "usually 47m" pace already uses `reply_pairs` latencies. It is the right
signal for "whose turn, and how late", better than a raw age.

### Spike proved quote-stripped mail reads like chat, with the original one click away

Reported. Spike strips headers, subject lines, quoted history and signatures
and shows the rest as bubbles; reviewers who like it praise exactly that, and the
full original is one click away
([work-management.org](https://work-management.org/productivity-tools/spike-review/),
[TechCrunch](https://techcrunch.com/2020/06/11/spike-raises-8-million-to-make-your-email-look-like-a-chat-app)).
Spike also offers a People mode that groups mail by contact, with frequent
contacts on top ([Wikipedia](https://en.wikipedia.org/wiki/Spike_(application)),
[Spike help](https://www.spikenow.com/help/setting-up-your-spike-account/)).

### Shortwave collapses history by default and quotes by selection

Reported. Shortwave collapses message history so you can get up to speed on a
thread, and replaces inline-reply habits with "quick quote" of a selected part
([Shortwave blog](https://www.shortwave.com/blog/2024-new-shortwave-features-ai-email-calendar-business-teams/),
[email-tools.me review](https://email-tools.me/posts/shortwave-review/)).
Quote-by-selection is the email equivalent of iMessage's inline reply.

### Basecamp and Front keep the conversation with a person separate from team talk about it

Evidence from product docs. Basecamp's Pings are DMs that are not tied to any
project and only visible to the chosen people, distinct from Campfire (group chat)
and Message Boards (records)
([Basecamp updates](https://updates.37signals.com/post/new-in-basecamp-direct-replies-boosts-on-campfires-and-pings)).
Front puts internal comments inside the email conversation, visible only to the
team ([Front help](https://help.front.com/en/articles/2256)). For a single-user
local client the useful lesson is the separation: what you say *to* someone and
what you note *about* the conversation (mxr's private notes, promises, gist) are
different layers and should look different.

### Screening unknown people keeps the list to people you know

Evidence. iOS 26 can move unknown senders to a separate list behind a filter, with
no notifications or badge
([Apple support](https://support.apple.com/en-us/125068)). HEY's Screener does the
same for email. mxr's blueprint already says anyone you've written to is never
screened, which matches the evidence that writing to someone is the strongest
importance signal.

## 3. What fails: bubbles for letters, reactions that become email, nagging, and keying groups on who is CC'd

### Chat bubbles for formal mail feel unprofessional and alter what was sent

Reported, consistently. Spike's chat UI is called "polarizing", "confusing and
unprofessional" for client mail, and harmful to readability for contracts and
proposals; some users say it alters the content without making that clear, and the
format "changes how email looks but not how much of it you process"
([get-alfred](https://get-alfred.ai/blog/best-spike-alternatives),
[unboxd](https://unboxd.ai/blog/unboxd-vs-spike.html),
[G2](https://www.g2.com/products/spike/reviews)). Two failures are mixed here and
need separating: the *visual* of a bubble around a five-paragraph letter, and the
*trust* problem of silently trimming text. mxr should avoid both: no bubbles
around long text, and a visible "trimmed" marker with the original one key away.

### Syntax-based quote stripping misses most quotes in business mail

Evidence, and directly relevant to mxr's code. In Lampert, Dale and Paris's
email-zoning study, a line-initial `>` finds more than 95% of reply lines in
Usenet-style mail but "less than 10% of actual reply or forward lines" in the
Enron business corpus; their SVM zoner reached 87% on nine zones and 91.5% on
three ([EMNLP 2009](https://aclanthology.org/D09-1096.pdf)). Jangada, trained on
newsgroups, did not generalise to Enron. Mailgun's open-source talon combines
heuristics and ML and is described as working in simple cases but not complex
ones ([talon](https://github.com/mailgun/talon)).

mxr's plain-text pipeline (`crates/reader/src/quotes.rs`) detects only
`On ... wrote:` headers and `>` prefixes, and `crates/reader/src/signatures.rs`
detects `-- `, "Sent from my iPhone" and a trailing block of contact-like lines.
The web does better on HTML: `apps/web/src/features/thread/htmlQuote.ts` knows
Gmail's `gmail_quote`, Apple's `blockquote type=cite`, Outlook's `divRplyFwdMsg`
and `appendonsend`, and Yahoo's `yahoo_quoted`. But that logic lives only in the
web, so the CLI, TUI and MCP see a different (worse) "what they said".

Reasoning. mxr has an advantage none of those tools had: the whole thread is
local. A quote is, by definition, text that already exists in an earlier message
in the same thread. Matching a trailing block against the cleaned text of earlier
messages (normalised whitespace, line-level shingles) identifies quotes regardless
of client syntax or language. Syntax rules then only have to find the boundary,
not decide what is quoted.

### Email reactions turn into extra email for everyone not on the same client

Evidence. Gmail's emoji reactions are a MIME part
(`text/vnd.google.email-reaction+json`) with a text and HTML fallback
([Google developers](https://developers.google.com/workspace/gmail/reactions/format)).
Recipients on other clients get a separate email saying someone "reacted via
Gmail"; reactions are disabled above 20 recipients and for groups, and they can't
be taken back after the undo window
([TechCrunch](https://techcrunch.com/2023/10/04/google-gmail-emoji-reactions),
[AlternativeTo](https://alternativeto.net/news/2023/10/gmail-introduces-emoji-reactions-but-you-might-just-receive-an-annoying-email-instead)).
iMessage tapbacks sent to Android became "Liked '...'" text messages that people
described as spam until Google Messages started parsing them
([Macworld](https://www.macworld.com/article/610908/google-messages-android-green-bubbles-tapbacks-reaction-emoji.html)).
Cross-client reactions degrade into clutter.

### Nudges that nag get switched off

Evidence of rejection. Gmail's Nudges ("Received 3 days ago. Reply?") is widely
written about mainly as something to turn off, as "overreach" telling you what you
already know ([Laptop Mag](https://www.laptopmag.com/articles/how-to-turn-off-gmail-nudge),
[How-To Geek](https://www.howtogeek.com/750967/how-to-disable-email-reminder-nudges-in-gmail/)).
"You owe" must be a calm state on the row, not a banner or a scolding colour.
mxr's D101 already forbids red and scolding; keep it.

### Conversation view hides the newest message and the individual message

Reported. When Gmail made conversation view mandatory, enough people objected
that Google made it optional in 2010; complaints were that the preview shows the
oldest unread message rather than the latest, that you can't act on one message,
and that replies get overlooked inside long threads
([PCWorld](https://www.pcworld.com/article/503547/gmailconversationview.html),
[GMass](https://www.gmass.co/blog/gmail-conversation-view/)). Messages must
preview the latest thing said, and must let you act on a single message (quote
it, make it a to-do) inside a conversation.

### Keying a group on its exact member list splits it every time someone is added

Evidence. In Slack, adding someone to a DM creates a new group DM and leaves the
old one where it was
([Slack help](https://slack.com/help/articles/1500002969782-Add-people-to-a-direct-message)).
iMessage and SMS groups have the same rule in many cases, and the support forums
are full of the resulting confusion
([Apple support](https://support.apple.com/en-euro/108303),
[Apple Community](https://discussions.apple.com/thread/256206929)). Email CC
lists change on almost every reply. Keying a group row on its participant set
would fragment one conversation into several rows.

### A classifier that treats any human-looking sender as a person fills Messages with alerts

Observed in today's build (`01-desk`): "Action required: unusual sign-in attempt"
and "Build failed on release branch" sit in You owe and New from people. The
blueprint's phase 2 check addresses this. Messages is only as good as its
membership rule; this is a precondition, not a polish item.

## 4. Is the unit one row per person or one per conversation, and where do group threads go?

### The answer: one row per person, conversations as topics inside it, and each group thread as its own row

This section answers the question with evidence rather than assuming it.

#### What the evidence says about the person as the unit

- Importance attaches to people, not threads. Reciprocity, recency and longevity
  of contact predict importance
  ([Whittaker, Jones, Terveen 2002](https://dl.acm.org/doi/10.1145/587078.587109)).
  Social relationship was "vital for determining the importance of an email" in
  the SNARF studies ([SNARF](https://clab.iat.sfu.ca/pubs/SNARF.pdf)).
  Communication concentrates on a stable few
  ([Saramäki 2014](https://www.pnas.org/doi/10.1073/pnas.1308540110)).
  A view whose job is "the people you care about" should rank and display the
  thing importance attaches to.
- One person spreads across many threads. The SNARF authors built per-correspondent
  sorting because "a great deal of email comes in over many threads", so threading
  alone does not reduce the pile ([SNARF](https://clab.iat.sfu.ca/pubs/SNARF.pdf)).
  Today's desk shows that: Samir is three rows, Ari is three.
- Every chat app the brief names uses the person as the one-to-one unit, and both
  Spike (People mode) and SNARF built person-grouped email views. The field
  deployment of SNARF reported better triage
  ([Fisher et al.](https://www.researchgate.net/publication/221098917_Using_Social_Metadata_in_Email_Triage_Lessons_from_the_Field)).

#### What the evidence says against collapsing everything into one stream per person

- Email threads are topics with subjects, and topics matter. Slack added threads
  inside DMs and channels, and Telegram added topics inside groups, because one
  undifferentiated stream mixes discussions
  ([New Atlas](https://newatlas.com/slack-introduces-threads/47452/),
  [Telegram](https://telegram.org/blog/topics-in-groups-collectible-usernames)).
- Gmail's conversation-view backlash shows that hiding individual messages and the
  latest reply costs people
  ([PCWorld](https://www.pcworld.com/article/503547/gmailconversationview.html)).
- Spike's single bubble stream per person is the format most often called
  confusing for professional mail ([get-alfred](https://get-alfred.ai/blog/best-spike-alternatives)).

So the person is the row; the conversation (thread) is the unit of reading,
replying and done. That is Slack's DM-with-threads and Telegram's forum, which are
the two chat designs that already solved "a person, several topics".

#### What the evidence says about groups

- Chat apps make each group its own row, separate from each member's one-to-one.
- Keying a group on its member set breaks when members change (Slack, iMessage
  above), and email CC lists change constantly. So a group row should be keyed by
  the **thread**, not by the participant set.
- SNARF treated mail where you are CC'd as a separate signal from mail sent to you,
  and Gmail disables reactions above 20 recipients, a vendor's judgement that
  large-recipient mail is not conversational
  ([TechCrunch](https://techcrunch.com/2023/10/04/google-gmail-emoji-reactions)).
  Evidence that large CC threads belong elsewhere is suggestive, not conclusive.

#### The recommendation, with the rule spelled out

Proposal, testable on BK's mail:

1. A thread is one-to-one when, after removing your own addresses
   (`account_addresses`) and automated or list senders (`mail_kind::classify`),
   exactly one other human remains. It appears inside that person's row as a
   topic.
2. A thread is a group when two or more other humans remain *and* at least two
   of them have written in the thread or been written to by you. It gets its own
   row, keyed by `thread_id`, titled with first names and the subject
   ("Samir, Ruth · Contract renewal"). It is also listed as a topic on each
   member's person page under "with others", so going to Samir shows everything
   you have going with Samir.
3. A thread is copied when you are only in CC, have never written in it, and
   nobody named you or asked you anything. It does not enter Messages; it goes to
   Updates (glance and let go). If someone later writes to you directly in it, or
   you reply, it becomes a group thread.
4. A thread with a very large audience (more than about 10 recipients) that you
   have not written in is treated as copied. The number is a starting guess to
   tune with `mxr modes eval`.
5. The same person under two addresses is one person. `contacts` is keyed by
   `(account_id, email)` today, so this needs a small person identity layer
   (section 5).

What this changes in the blueprint: its open question "per person or per
conversation?" becomes "per person, with conversations as topics; groups per
thread; CC-only to Updates". The ranking rule in the blueprint (band, then owed,
then recency) carries over unchanged.

## 5. The data this view extracts from an email

Messages shows the conversational aspect of a message: who said it, what they
said in their own words, what they are asking you, and whose turn it is.

| Field | How it is derived | mxr today | Needed |
|---|---|---|---|
| Person (the row) | Other human in a one-to-one thread, merged across addresses | `contacts` per `(account_id, email)`, `010_contacts.sql`; `account_addresses` for "me" | A `people` identity that merges addresses (same display name + you've used both, or user merge). Start with a user merge action plus exact-name heuristic |
| Group (the row) | Thread with 2+ other humans who took part | `messages.to_addrs`, `cc_addrs`, `direction` | A pure classifier `conversation_shape(thread) -> OneToOne(person) | Group | Copied` in the daemon |
| Closeness band | Reciprocity, recency, longevity: `total_outbound`, `replied_count`, `last_outbound_at`, `first_seen_at`, `cadence_days_p50` | All in `contacts` | The blueprint's `relationship_strength` formula, with the reason string ("you've written to Maya 48 times, last Tuesday") |
| Pinned people | User choice, up to about nine | `relationship_watchlist` (`031_relationship_watchlist.sql`) is a per-person list with an expected cadence | Reuse it as pins; no new table |
| Whose turn | Latest inbound with no later outbound | `crates/store/src/owed_replies.rs` (`overdue_score = waiting_days / expected_days`) | Nothing new. One owed rule shared with the desk |
| Waiting on them | Your message with no reply after their usual pace | Desk Waiting lane in `desk_lanes.rs` | Expose per person |
| Their pace | Median reply latency | `reply_pairs` (`009_reply_pairs.sql`), shown as "usually 47m" | Nothing new |
| What they said, in their words | Body minus quotes, signature, disclaimers, tracking | `mxr_reader::clean` (`crates/reader/src/pipeline.rs`); HTML quote markers only in `apps/web/src/features/thread/htmlQuote.ts` | Move the HTML markers into the daemon (`crates/reader`), add thread-aware quote matching against earlier messages, return `trimmed: {quote, signature}` flags so every client shows the same text and the same "trimmed" marker |
| The ask | The sentence asking you for something, quoted verbatim | `thread_gist.rs` returns a gist and an ask whose quote is checked against the message text; `sender_unanswered_question` in `crates/store/src/sender_profile.rs` (a `?` heuristic) | Use the gist ask when a model is configured; fall back to the last `?` sentence from the cleaned text |
| Due words | Phrase found in the text, resolved by `natural_time` | Planned in the blueprint (`todos.due_words`) | Show as a chip linking to To do |
| Attachments | Names, types, sizes | `attachments` table, `messages.has_attachments` | Nothing new |
| Your history with them | One-line relationship summary and the "You and Samir: 13 emails" facts | `thread_context.rs`, `contact_relationship_summary` (`023`), `sender_view.rs`, web route `sender.$address.tsx` | Reuse on the person page header |
| How you write to them | Greeting, sign-off, length, formality | `contact_style` (`022`), `crates/relationship/src/habits.rs` | Feeds the quick acknowledgement and "Draft for me" |
| Incoming reactions | Gmail reaction MIME part | Not parsed | Detect `text/vnd.google.email-reaction+json` and render it as a small mark on the message it reacts to (via `In-Reply-To`), not as a new message |
| Done in Messages | Per-thread watermark | Planned `mode_done` (blueprint), modelled on `desk_dismissals` | As planned |

## 6. The proposed Messages view

### The layout is a people list on the left and one person's topics on the right

Desktop web, Messages selected in the rail:

```text
+-------------+--------------------------------+----------------------------------------------------+
| Now      3  | Messages                    /  | Samir Patel                       usually 47m · close
| Messages •  |                                | You've written 48 times since 2023. Last: Tuesday.  |
| To do    2  | YOUR TURN                      |----------------------------------------------------|
| Updates     | (SP) Samir Patel        16h •  | TOPICS                                             |
| Reading     |      "Can you take a look and  |  • Contract renewal          your turn · 16h       |
| Archive     |       reply with the next..."  |    Launch checklist          waiting on him · 1d   |
|             | (JB) Jon Bell            8h •  |    with Ruth: Pricing copy   quiet · 3d            |
| Inbox       |      "Does the pricing copy    |----------------------------------------------------|
|             |       read right to you?"      | Contract renewal                         4 messages|
|             | (SR) Samir, Ruth         2d •  |                                                    |
|             |      Contract renewal          |  Samir · Yesterday 18:02                           |
|             |                                |  Message 3. Can you take a look and reply with     |
|             | PINNED                         |  [the next concrete step?]  <- the ask, highlighted|
|             | (MK) Maya   (AS) Ari  (+)      |  Rollout risk: watch sync latency, auth failures   |
|             |                                |  and support tickets for the first hour. ...       |
|             | RECENT                         |  Read all 6 paragraphs                             |
|             | (IC) Iris Chen     Yesterday   |  runbook.pdf  48 KB             trimmed: quote, sig|
|             |      You: "Thanks, on it."     |                                                    |
|             | (LP) Leo Park        Tue       |                         You · Yesterday 18:40      |
|             |      "Sounds good"             |               Thanks, looking now.                 |
|             |                                |----------------------------------------------------|
|             | QUIET (12)              >      | Reply to Samir · Contract renewal       [ ] all    |
|             |                                | |                                                  |
|             |                                | Send ⌘↵   Got it .   Done here e   To do t   Later Z|
+-------------+--------------------------------+----------------------------------------------------+
```

The list has three bands, top to bottom:

- Your turn: people and groups whose latest message is to you and unanswered,
  ordered by closeness band, then by how far past their usual pace you are. The
  preview is the ask when there is one, otherwise the last line they wrote.
- Pinned: up to about nine faces in a row (iMessage's pattern), from
  `relationship_watchlist`, with a dot when it's your turn.
- Recent: everyone else in Messages by recency, with "You: ..." when you
  spoke last.
- Quiet: people marked done here, collapsed, who come back to Recent or Your
  turn the moment they write (Beeper's archive, HEY's jump back).

The right side is a person page, not a thread. Its header is the relationship
(the facts from `thread_context` and the relationship summary), then the topics
list (one line per thread with that person, including group threads under "with
..."), then the selected topic rendered as a conversation, then the composer.

The conversation rendering uses one rule: **length decides the shape, not the
medium**.

- A message of three lines or fewer after cleaning is shown compactly, aligned
  like chat: theirs left, yours right, no bubble chrome beyond a faint tint.
- A longer message is shown as a full-width letter block with the author and time
  on top, the first paragraph or the ask visible, and "Read all N paragraphs" to
  expand. No bubble, no HTML letterhead, no repeated subject heading.
- Quoted history and signatures are removed and marked: a small "trimmed: quote,
  sig" label opens the original exactly as sent (`v`). The marker is the fix for
  Spike's trust problem.
- Selecting text and pressing `>` quotes it into the composer (Shortwave's quick
  quote, iMessage's inline reply). The sent email carries a normal `>` quote, so
  it reads correctly in any client.
- Attachments sit under the message as file chips with name and size, and open
  in place.

### The composer is always present at the bottom, already addressed

Proposal. The box is there whenever a topic is open, addressed to that person on
that topic, with reply-all as a visible toggle for group threads (default on in a
group row, off on a person's one-to-one topic). Typing starts the draft; nothing
needs to be opened. This is the existing `ReplyField`
(`apps/web/src/features/thread/ReplyField.tsx`) promoted from "quiet field after
the last message" to the permanent bottom of the person page. "Draft for me" stays
as it is, since it already knows how you write to this person (`contact_style`,
`habits.rs`).

Starting something new with someone: `c` on a person opens the composer with a
blank subject line on top ("New topic with Samir"), so starting a conversation is
as cheap as replying.

### Acknowledgement replaces reactions, and it is a real email

There is no email reaction that degrades well (section 3), so mxr should not send
emoji reactions. The chat need behind them is "I saw this, no more to say" and
"thanks". Two actions cover it:

- Got it (`.`): sends a short acknowledgement in your voice to this person
  ("Thanks Samir, got it." built from your usual greeting and sign-off for them),
  shown in a preview line with a few seconds to cancel before it goes. It is a
  normal reply, so it reads fine everywhere and counts as your turn taken.
- Done here (`e`): local only. Nothing is sent; the topic leaves Your turn and
  the person drops to Recent or Quiet. This is the existing "done, no reply
  needed".

Incoming Gmail reactions are shown as a small mark on the message they react to,
not as a new message in the stream.

### The primary actions, with keys

Keys are proposals and must be checked against `apps/web/src/lib/actions/keymapParity.ts`
and the TUI keymap; existing meanings are kept where they exist (`r`, `e`, `c`,
`Z`, `u`, `t` from the blueprint).

| Key | Action | Notes |
|---|---|---|
| `j` / `k` | Next / previous person or group | List focus |
| `Enter` | Open person; focus the latest open topic | |
| `[` / `]` | Previous / next topic with this person | Inside a person page |
| `r` | Reply on this topic (focus the composer) | |
| `a` | Reply all | Group threads |
| `⌘↵` | Send, go to the next person whose turn it is | The Focus & reply rhythm, inside Messages |
| `.` | Got it (short acknowledgement, cancellable) | |
| `e` | Done here (no reply needed) | Per-mode done; never archives on its own |
| `t` | Make a to-do from this topic, prefilled from the ask and due words | Handoff to To do |
| `Z` | Reply later, with a time | Existing reply-later flag and `056_reply_later_due.sql` |
| `c` | New topic with this person | |
| `p` | Pin or unpin person | `relationship_watchlist` |
| `>` | Quote selection into the reply | |
| `v` | View original as sent | |
| `?` on a row | Why is this person here and ranked here | "You've written to Samir 48 times" |
| `u` | Undo | Existing undo |

### On mobile the list and the person page are two screens, and swipes do the two common actions

```text
+---------------------------+      +---------------------------+
| Messages              (+) |      | < Samir Patel   usually 47m
| YOUR TURN                 |      | Contract renewal  ▾ 3 topics
| (SP) Samir Patel     16h •|      |---------------------------|
|  "Can you take a look..." |      | Samir · Yesterday         |
| (JB) Jon Bell         8h •|      | Message 3. Can you take a |
|  "Does the pricing copy.."|      | look and reply with [the  |
| (SR) Samir, Ruth      2d •|      | next concrete step?]      |
|  Contract renewal         |      | Read all 6 paragraphs     |
|---------------------------|      | runbook.pdf               |
| (MK) (AS) (IC) (LP)  pins |      |        You · Yesterday    |
|---------------------------|      |    Thanks, looking now.   |
| (IC) Iris Chen  Yesterday |      |---------------------------|
|  You: "Thanks, on it."    |      | [Reply to Samir...]   Send|
| Quiet (12)            >   |      | Got it · Done · To do     |
+---------------------------+      +---------------------------+
 swipe right: Got it                 swipe a message right: quote
 swipe left: Done here               into reply (WhatsApp's gesture)
```

Swipe directions reuse the existing swipe rules (`apps/web/src/features/swipe/swipeRules.ts`);
"Got it" sends, so it must show its preview line and cancel window even on a swipe.

### The TUI is the same two panes, with the topic list as a strip

```text
┌ Messages ──────────────────────┬ Samir Patel · close · usually 47m ─────────────────┐
│ YOUR TURN                      │ Topics: [Contract renewal •] Launch checklist ·    │
│▶Samir Patel          16h •     │         with Ruth: Pricing copy                    │
│   Can you take a look and...   ├────────────────────────────────────────────────────┤
│ Jon Bell              8h •     │ Samir · Yesterday 18:02                            │
│   Does the pricing copy read...│ Message 3. Can you take a look and reply with      │
│ Samir, Ruth           2d •     │ »the next concrete step?«                          │
│   Contract renewal             │ Rollout risk: watch sync latency, auth failures... │
│ PINNED  Maya · Ari             │ [+5 paragraphs]  runbook.pdf 48K  (trimmed q,sig)  │
│ RECENT                         │                         You · 18:40               │
│ Iris Chen       Yesterday      │                         Thanks, looking now.       │
│   You: Thanks, on it.          ├────────────────────────────────────────────────────┤
│ Quiet (12)                     │ > _                                                │
└────────────────────────────────┴────────────────────────────────────────────────────┘
 r reply  . got it  e done  t to do  Z later  [ ] topic  c new topic  ? why
```

The TUI composer line hands off to `$EDITOR` for anything longer than a line, per
`AGENTS.md`.

### The CLI and daemon come first

Per the CLI-first rule, this needs, before any UI: `ListModeItems { mode:
Messages }` returning person and group rows with `band`, `turn`, `pace_seconds`,
`ask`, `preview`, `reason`; `GetPerson { person_id }` returning the relationship
header and topics; `mxr messages` (`--turn mine|theirs`, `--format json`);
`mxr messages show <person>`; and `mxr messages ack <thread> --dry-run` for the
acknowledgement, which must preview the exact text it will send (the preview path
must match the send path).

### You visit when someone writes and leave within minutes

Rhythm, proposed. Messages is visited on arrival from close people and two or
three times a day to clear Your turn. The target session is under five minutes:
open, answer the top three, "Got it" or "Done here" the rest, leave. Notifications
(if mxr has them) fire only for pinned and close people, mirroring Beeper's "only
mentions and direct replies" for low-priority chats.

### The empty state says nobody is waiting, and offers someone to write to

When Your turn is empty: "Nobody is waiting on you." Below it, two or three
people whose usual cadence has lapsed (from `cadence_days_p50` and
`relationship_watchlist.expected_days`): "You usually write to Maya every 9 days.
It's been 15." with `c` to start a topic. This is the only place mxr suggests
writing to someone, and it is phrased as a fact, not a nudge (section 3).

### How an email enters Messages and how it leaves

Enters:

- A message from a person (screener Allow, or `mail_kind` Person, or anyone you've
  written to) in a one-to-one or group thread, per the rule in section 4.
- A message you send to a person: the person moves to Recent, and to Waiting when
  their pace lapses.
- A "this is a person" correction from any other mode.

Leaves, without leaving other modes:

- Reply or Got it: the topic's turn passes to them; it stays visible in
  Recent.
- Done here (`e`): the topic is done in Messages through the current message
  (the `mode_done` watermark); a new message from them brings it back.
- To do (`t`): creates a to-do prefilled from the ask and due words, and marks
  the topic done here, as the blueprint says. The person page shows a small "to-do:
  Sign the form, due Fri" chip on that topic until it's ticked off.
- Reply later (`Z`): leaves Your turn until the chosen time.
- Not a person correction: moves the message (or the sender) to Updates or
  Reading.

The provider archive is not a Messages action. Per the blueprint, it happens only
when the last mode holding the message lets go.

### Micro-interactions worth having are small and about the turn

- When you send, your message slides into the conversation on the right and the
  person's row moves down out of Your turn with a short (150 to 200 ms) motion, so
  you see the turn pass. Reduced-motion users get an instant move.
- The ask highlight fades in after the body renders, so the eye lands on it.
- A soft send sound, using the existing sound feature
  (`apps/web/src/features/sound/`), only for send and "Got it", off by default.
- The "Got it" preview line counts down visibly before sending.
- No typing indicators, read receipts or "seen" marks. Email has no honest
  source for them, and read receipts in email are tracking pixels mxr strips
  (`crates/reader/src/tracking.rs`).

## 7. How this differs from a list of emails

A list of emails has one row per thread, sorted by time, showing sender, subject
and snippet, where every row offers the same mail verbs (archive, label, star,
move). Messages differs in each of those:

- The row is a person, so Samir is one row however many threads he is in, and
  the alert from a no-reply address claiming to be Samir is not in it at all.
- The order is relationship then turn then time, not arrival time. A close
  colleague's reply from yesterday sits above a new acquaintance's message from a
  minute ago.
- The preview is what they asked you, not the first 90 characters of the body,
  which in the demo data is "Subject: Contract renewal details".
- The subject is a topic label inside the person, not the headline of the row.
- The body is what they wrote, with quotes and signatures removed and the
  removal marked, short notes shown compactly and letters shown as letters.
- The verbs are conversational: reply, got it, done here, later, to do, new
  topic. Archive, label and move are not on the surface; they belong to Archive
  mode and the More menu.
- The composer is always there, addressed, so replying has no open step.
- The state is the turn (yours, theirs, quiet), not read or unread.

## 8. Open questions and risks

- Person identity merge. People write from work and personal addresses. A
  wrong automatic merge shows one person's mail under another, which is worse than
  no merge. Start with exact display-name plus "you've written to both" and a
  manual merge, or manual only?
- The group and copied thresholds (two participating humans; more than about
  10 recipients) are guesses. They need `mxr modes eval` counts on BK's mail.
- Quote matching cost. Thread-aware quote detection compares each new message
  with earlier ones in its thread. Cheap for normal threads, but long mailing-list
  style threads need a cap. Should it run at sync time and cache, or at read time?
- "Got it" sends mail on one key. It needs a preview, a cancel window and a
  per-person off switch. Is `.` too easy to hit by accident? Should it be a chord?
- Key conflicts. `a`, `p`, `v`, `>` and `.` may collide with existing web or TUI
  bindings; the parity test will tell.
- Formal mail in a chat-shaped view. The length rule should keep letters
  readable, but the evidence against Spike is about perception as much as layout.
  BK should judge real long threads (contracts, recruiting) in it before it ships.
- Messages with no person. Some important conversations come from shared
  addresses (`support@` at a company you're working with). Mail_kind will call them
  automated. The "this is a person" correction must be one key.
- Desk overlap. The desk's You owe lane and Messages' Your turn must be the
  same rule and the same count, as the blueprint says, or users will stop trusting
  both.
- Mobile swipe that sends. A swipe that sends mail is riskier than one that
  archives. If the cancel window feels slow, swipe right should become "Done here"
  instead.

## 9. Sources

Studies:

- Whittaker and Sidner, Email overload, CHI 1996: https://www.semanticscholar.org/paper/Email-overload:-exploring-personal-information-of-Whittaker-Sidner/d7672ff5d79812e6cc85b18c6c4ed5aae197c676
- Whittaker, Jones and Terveen, Contact management, CSCW 2002: https://dl.acm.org/doi/10.1145/587078.587109 and https://eprints.whiterose.ac.uk/8387/
- Neustaedter, Brush, Smith and Fisher, SNARF: https://clab.iat.sfu.ca/pubs/SNARF.pdf
- Fisher et al., Using Social Metadata in Email Triage: Lessons from the Field: https://www.researchgate.net/publication/221098917_Using_Social_Metadata_in_Email_Triage_Lessons_from_the_Field
- Saramäki et al., Persistence of social signatures in human communication, PNAS 2014: https://www.pnas.org/doi/10.1073/pnas.1308540110
- Aberdeen, Pacovsky and Slater, The Learning Behind Gmail Priority Inbox: https://research.google.com/pubs/archive/36955.pdf
- Dabbish, Kraut, Fussell and Kiesler, Understanding email use, CHI 2005: https://dl.acm.org/doi/10.1145/1054972.1055068
- Tyler and Tang, When can I expect an email response?, ECSCW 2003: https://link.springer.com/chapter/10.1007/978-94-010-0068-0_13
- Lampert, Dale and Paris, Segmenting Email Message Text into Zones, EMNLP 2009: https://aclanthology.org/D09-1096.pdf
- Carvalho and Cohen, Learning to Extract Signature and Reply Lines from Email, CEAS 2004: https://www.researchgate.net/publication/221650827_Learning_to_Extract_Signature_and_Reply_Lines_from_Email
- Multilingual Email Zoning, 2021: https://arxiv.org/abs/2102.00461

Product documentation:

- HEY New For You and Previously Seen: https://www.hey.com/features/different-states/
- HEY features: https://www.hey.com/features/
- Superhuman Split Inbox: https://blog.superhuman.com/how-to-split-your-inbox-in-superhuman/
- Beeper inbox: https://help.beeper.com/en_US/android/beeper-android-how-does-inbox-work
- Slack, add people to a DM: https://slack.com/help/articles/1500002969782-Add-people-to-a-direct-message
- Slack Engineering, Weaving Threads: https://slack.engineering/weaving-threads/
- Telegram topics: https://telegram.org/blog/topics-in-groups-collectible-usernames
- Apple, group messages: https://support.apple.com/en-euro/108303
- Apple, unknown senders in iOS 26: https://support.apple.com/en-us/125068
- Basecamp direct replies and Pings: https://updates.37signals.com/post/new-in-basecamp-direct-replies-boosts-on-campfires-and-pings
- Front comments: https://help.front.com/en/articles/2256
- Gmail reaction format: https://developers.google.com/workspace/gmail/reactions/format
- Spike help: https://www.spikenow.com/help/setting-up-your-spike-account/
- Mailgun talon: https://github.com/mailgun/talon

Reviews, reporting and complaints:

- iMessage in iOS 14 (pins, inline replies, mentions): https://www.macrumors.com/2020/06/22/ios-14-imessage-features/
- Slack threads launch: https://newatlas.com/slack-introduces-threads/47452/
- Telegram topics guide: https://metricgram.com/blog/telegram-forum-topics-guide
- Spike reviews and criticism: https://get-alfred.ai/blog/best-spike-alternatives, https://unboxd.ai/blog/unboxd-vs-spike.html, https://www.g2.com/products/spike/reviews, https://work-management.org/productivity-tools/spike-review/
- Spike funding and design: https://techcrunch.com/2020/06/11/spike-raises-8-million-to-make-your-email-look-like-a-chat-app
- Spike on Wikipedia: https://en.wikipedia.org/wiki/Spike_(application)
- Shortwave features: https://www.shortwave.com/blog/2024-new-shortwave-features-ai-email-calendar-business-teams/ and https://email-tools.me/posts/shortwave-review/
- Gmail reactions: https://techcrunch.com/2023/10/04/google-gmail-emoji-reactions and https://alternativeto.net/news/2023/10/gmail-introduces-emoji-reactions-but-you-might-just-receive-an-annoying-email-instead
- Tapbacks on Android: https://www.macworld.com/article/610908/google-messages-android-green-bubbles-tapbacks-reaction-emoji.html
- Gmail Nudges complaints: https://www.laptopmag.com/articles/how-to-turn-off-gmail-nudge and https://www.howtogeek.com/750967/how-to-disable-email-reminder-nudges-in-gmail/
- Gmail conversation view backlash: https://www.pcworld.com/article/503547/gmailconversationview.html and https://www.gmass.co/blog/gmail-conversation-view/
- iMessage group confusion: https://discussions.apple.com/thread/256206929
- HEY workflow: https://thesweetsetup.com/hey-email-disrupted-my-email-workflow/
