# Trust and completeness: sorted mail earns trust by showing where everything went

Research for "Sorting shows its work, so nothing feels hidden" in
[22-email-modes.md](../../blueprint/22-email-modes.md), settled as D119.
Code references are at `1835ddd6` (`origin/main`, v0.6.55, fetched
2026-10-07).

BK, 2026-10-07: "since it reorders emails, I'm always thinking: am I
missing some important email? So from time to time I find myself jumping
to the inbox tab just to see emails in order of arrival... Is there a way
to surface a signal or affordance that reassures users nothing is being
hidden or deprioritised, without forcing them to fall back to the raw
chronological view?" Later the same day he added that in Inbox he reads
nothing: he looks at the newest message's time, and "if I see that the
last message arrived five minutes ago, that gives me confidence".

So the Inbox visit answers two questions: is mail arriving, and did the
sort lose any of it. The first is a freshness question, being answered on
`feat/freshness-indicator`. This note is mostly about the second.

Evidence and inference are kept apart: a line backed by a source cites it,
and design reasoning says "judgement". WebSearch, Exa and Brave were
unavailable (budget spent, invalid key, rate limit). Sources were read
with curl or WebFetch directly, through `r.jina.ai` (marked "proxy"),
through archive.org ("archive"), as abstracts from the OpenAlex or
Semantic Scholar APIs ("abstract only"), and user reports through the
Hacker News Algolia API. Reddit, the ACM Digital Library and Apple
Communities were blocked, so user complaints come almost entirely from
Hacker News, which skews technical.

## 1. Verdict on the six proposed ideas

| Idea | Verdict | Change |
|---|---|---|
| 1. Arrivals ledger on Now | Supported, with a stored placement | Sum emails, not threads; Spam and screened out are named buckets; To do and Archive are "also", never added; each count opens those emails in arrival order. Builds on freshness, not beside it |
| 2. Inbox mode chip with reason and one key | Supported | Mode name always, reason on focus or hover; rule-like reasons only; `K` exists, `X` must be built |
| 3. "Not sure" line | Supported only as rule conflicts | No confidence score exists. Ask only about rule conflicts (4 a week on BK's mail today), cap three a day |
| 4. Never-bury guarantee | Supported, strongest evidence | Four rules with stated scope; N1 only for mail addressed to you; N2 needs a detector first; windows still apply |
| 5. Weekly track record | Deferred | Corrections aren't recorded, so it would read "you moved 0" for want of a way to move. Ship after `X` and a stored `now_mode` |
| 6. "You've seen everything" marker | Reworded and merged into 1 | "Seen" would be false; "accounted for" is true and bounded |

What was missing: freshness as the first signal (section 2), Spam as part
of the count (section 6), arrival time from first-seen rather than the
`Date:` header (section 9), and the fact that today's computed membership
drops mail once it leaves the inbox (section 9).

## 2. Freshness answers the question BK actually asks in Inbox

Evidence:

- NN/g's first heuristic, visibility of system status (proxy,
  https://www.nngroup.com/articles/visibility-system-status/): "A lack of
  information often equates to a lack of control." When "the passage of
  time caused a change in the state of the system, explain it." Items that
  "simply disappeared ... with no explanation" mean "users may stop
  relying" on the feature.
- Silent sync failure is a known fear, reported as anecdotes only: Outlook
  on the web "sometimes stopped fetching new emails till you reload"
  (https://news.ycombinator.com/item?id=35601194); a FastMail outage left a
  user unsure "if some other mail was bounced"
  (https://news.ycombinator.com/item?id=36534349).
- No HCI paper was found on users being unable to tell "no new mail" from
  "sync is broken". NN/g on timestamps, data-freshness studies and
  Outlook's status-bar documentation were not read.

Inference: "Latest mail 5m ago · synced 30s ago" separates the two states
BK checks Inbox to tell apart. Synced recently with the latest mail hours
old means a quiet inbox; synced hours ago means a broken sync and should
look different. The popover of the last five arrivals with their modes is
the arrivals ledger in miniature, so both must read the same rows.

## 3. Trust follows performance, process and purpose, and misses on important mail end it

Evidence:

- Lee and See 2004 (archive, full PDF,
  https://web.archive.org/web/2015id_/http://www.engineering.uiowa.edu/~csl/publications/pdf/leesee04.pdf):
  calibration is "the correspondence between a person's trust in the
  automation and the automation's capabilities"; overtrust leads to
  misuse, distrust to disuse. Trust has functional specificity: it
  attaches to "specific subfunctions and modes", not the whole system.
  Their guidance includes "Design for appropriate trust, not greater
  trust", "Show the past performance of the automation", show the process
  by "revealing intermediate results", and show the purpose and "range of
  applications".
- Parasuraman and Riley 1997 (abstract only, DOI
  10.1518/001872097778543886): "Disuse, or the neglect or underutilization
  of automation, is commonly caused by alarms that activate falsely";
  misuse, "over reliance on automation, ... can result in failures of
  monitoring."
- Yin, Wortman Vaughan and Wallach, CHI 2019 (abstract only): trust is
  "affected by both its stated accuracy and its observed accuracy", and
  the effect of stated accuracy depends on observed accuracy.
- Kizilcec, CHI 2016 (abstract only): people whose expectations were
  violated trusted the system less "unless the grading algorithm was made
  more transparent through explanation. However, providing too much
  information eroded this trust."
- Dzindolet et al. 2003 (that explaining why an aid errs raises reliance)
  could not be reached and is unverified.

Inference: the arrivals line shows process (where each email went), the
never-bury rules show purpose and scope (what the sort will never do),
and the track record shows performance. Functional specificity argues for
naming per-mode counts, so a wrong Reading call doesn't cost trust in
Messages. Kizilcec argues for one short clause per row, with the detail a
click away. Yin argues the track record must come from what the user
observed (their own moves), never a claimed accuracy.

## 4. Every sorting inbox drew "am I missing mail", and the ones that lasted gave a way to check

Evidence:

- **Gmail Priority Inbox.** Aberdeen, Pacovsky and Slater 2010 (full PDF,
  direct,
  https://static.googleusercontent.com/media/research.google.com/en//pubs/archive/36955.pdf):
  "Priority Inbox users (approx. 2000 users) spent 6% less time reading
  mail overall, and 13% less time reading unimportant mail. They are also
  more confident to bulk archive or delete email." The users were Google
  employees. Accuracy was "approximately 80 ± 5%", and the false negative
  rate was set to "3 – 4 times the false positive rate" because "users
  read mail they acknowledge is not important". Corrections moved a
  per-user threshold in real time. Users still swept "Everything else":
  one reader reports a miss "once every other month" caught by "a quick
  scan of the 'everything else' box ... which I do anyhow"
  (https://news.ycombinator.com/item?id=8493858).
- **Gmail tabs, 2013** (archive,
  https://gmail.googleblog.com/2013/05/a-new-inbox-that-puts-you-back-in.html):
  drag between tabs, "set certain senders to always appear in a particular
  tab", and "simply switch off all optional tabs to go back to classic
  view". Complaints: after finding professional mail in Promotions, one
  user now checks it "regularly and meticulously ... just to make sure I
  haven't missed something legitimately urgent or important"
  (https://news.ycombinator.com/item?id=28635716); "I turned the tabs off
  within a day" (https://news.ycombinator.com/item?id=6069267); Gmail
  "would consistently bury important messages ... no matter how many times
  I would drag them back" (https://news.ycombinator.com/item?id=21526720).
  A counterpoint: "Am I missing things? Yes, it has happened, but I feel
  like I'm missing less than when my inbox was a superfund site"
  (https://news.ycombinator.com/item?id=47707141).
- **Outlook Focused Inbox** (proxy,
  https://support.microsoft.com/en-us/office/focused-inbox-for-outlook-f445ad7f-02f4-4294-a82e-71d8964e3978):
  "You'll be informed about email flowing to Other", with "Move to
  Focused" and "Always Move to Focused" as the corrections. Admins can
  turn it off org-wide with `Set-OrganizationConfig -FocusedInboxOn`
  (proxy, Microsoft Learn). The exact banner text was not verified.
  Complaints: "I want me mail, and all of it!"
  (https://news.ycombinator.com/item?id=29242842).
- **Apple Mail categories** (proxy, Apple's iPhone guide,
  https://support.apple.com/guide/iphone/use-categories-iphfe4a36baf/ios):
  "When a Transactions, Updates, or Promotions message includes
  time-sensitive information, it will also be included in the Primary
  message list." "Categorize Sender" moves "all current and future
  messages", and "List View" turns categories off. The priority-messages
  summary page was not verified, and complaints came only from HN ("mail
  has stupid categorization",
  https://news.ycombinator.com/item?id=42292807).
- **HEY** (proxy, https://www.hey.com/features/the-feed/): "HEY doesn't
  decide, you do. HEY won't move anything anywhere until you tell it to."
  Screener History shows who was screened out, and screening someone back
  in shows "any email they sent you within the last 90 days". One user
  likes that in the Feed "No number says how many are there"
  (https://news.ycombinator.com/item?id=30353649). No verified HEY
  missed-mail reports were found.
- **Superhuman** (proxy, Split Inbox help; Auto Labels help via the
  Zendesk API, direct): split counts appear beside each split; "You cannot
  hide Inbox, Important, or Other"; auto labels have a preview where each
  result can be confirmed or rejected. No missed-mail complaints found.
- **SaneBox** (direct,
  https://www.sanebox.com/help/170-daily-digest-why-should-i-use-it-and-how-do-i-read-it):
  the Digest is "a single place to review emails that SaneBox has filtered
  out of your Inbox" and "helps ensure you never miss an important
  message"; it is interactive, so moving an email trains the filter
  "almost immediately" (https://www.sanebox.com/help/140). A user: "I just
  check my @SaneLater box once a day and weed it out"
  (https://news.ycombinator.com/item?id=5819193).

Inference:

- One discovered miss turns a whole mode into a place to sweep (28635716),
  which is the case for never-bury rules on the mail where a miss costs
  most.
- Apple's time-sensitive rule is a shipped never-bury: it duplicates into
  Primary rather than risk hiding. Priority Inbox's bias toward false
  negatives is the same choice. mxr's N rules do this.
- A correction that doesn't stick (21526720) is worse than the first
  error, so `K` and `X` must hold across re-runs, as the blueprint
  already requires.
- Outlook and SaneBox both added an account of what went elsewhere. SaneBox
  says outright that the digest exists so people don't miss mail, and that
  it works because corrections happen inside it. The arrivals line should
  be actionable too: each count opens a list with `K` and `X` on every row.
- Even trusting users kept a sweep. The goal is a short sweep from inside
  the sorted view, not no sweep.

## 5. Explanations work when they name a rule, and they don't by themselves let people judge correctness

Evidence:

- Stumpf et al., IUI 2007 (proxy, full text,
  http://web.engr.oregonstate.edu/~tgd/publications/IUI07-stumpf.pdf), a
  think-aloud study with 13 people classifying email: rule-based
  explanations drew "three times as many remarks indicating
  understanding"; negative-keyword lists confused people ("Nobody had
  anything positive to say"); similarity-based explanations had "a serious
  understandability problem".
- Kulesza et al., IUI 2015 (abstract only): explanatory debugging
  "increased participants' understanding of the learning system by 52% and
  allowed participants to correct its mistakes up to twice as
  efficiently".
- Eslami et al., CHI 2015 (abstract only): 62.5% of participants did not
  know their feed was curated; first reactions were surprise and anger,
  most of all when "close friends and family were not shown"; awareness
  later "bolstered overall feelings of control".
- Rader, Cotter and Cho, CHI 2018 (abstract only): explanations made people
  "more aware of how the system works" but were "less effective for
  helping participants evaluate the correctness of the system's output".

Inference: mxr's reasons are already rules ("has List-Unsubscribe",
"no-reply sender"), the style Stumpf found most understandable. Eslami's
anger at hidden close friends is BK's fear in another setting, and N1 is
the answer. Rader 2018 is why the chip alone won't stop the Inbox visits:
the arrivals line and the never-bury rules carry the reassurance.

## 6. People check spam even at a 0.05% false-positive rate, so Spam belongs in the count

Evidence:

- Gmail, 2015 (archive,
  https://gmail.googleblog.com/2015/07/the-mail-you-want-not-spam-you-dont.html):
  "the amount of wanted mail landing in the spam folder is even lower, at
  under 0.05%", while conceding "you might have to wade through your spam
  folder to find that one important email".
- Users: mail moved to spam days later, so "I need to check my spam folder
  every day. Trust forever lost"
  (https://news.ycombinator.com/item?id=4840211); "It's useful that it
  tells you why mail is in the folder"
  (https://news.ycombinator.com/item?id=7303792).
- Validity and Return Path deliverability figures could not be fetched
  (404) and are unverified. No academic survey of spam-checking was found.

Inference: checking is driven by the cost of a miss and by memorable
incidents, not by the average error rate. A ledger that leaves Spam out
leaves the oldest version of this anxiety untouched; "1 in Spam" as a
link costs one word. Mail that moves after arrival (4840211) is the worst
case, which is why the ledger records where mail went at arrival and
shows later moves as moves.

## 7. "Caught up" works when it is bounded and honest

Evidence:

- Instagram, 2018 (proxy,
  https://about.instagram.com/blog/announcements/introducing-youre-all-caught-up-in-feed):
  the marker lets people "know you haven't missed recent photos or
  videos", bounded to "every post from the last two days". A later user
  says it has decayed into "infinite slop" below the marker
  (https://news.ycombinator.com/item?id=49357754).
- Masicampo and Baumeister 2011 (abstract only): unfulfilled goals intrude,
  but "formulating specific plans ... eliminated the various activation
  and interference effects".
- Mark et al., CHI 2016 (abstract only): "we found no evidence that
  batching email leads to lower stress".

Inference: the marker needs an explicit bound (a clock time and a count)
and has to be true. mxr can say mail was accounted for; it cannot say it
was seen, because Reading and Updates mail is sorted, not read. A to-do
with a date is a plan, which supports counting it as handled on Now.
Mark 2016 is a warning that a digest is not proven to lower stress.

## 8. Evidence against the design

1. Too much explanation lowers trust (Kizilcec 2016), and confusing
   explanations drew negative remarks (Stumpf 2007). Keep the chip to one
   clause.
2. Explanations don't help people judge correctness (Rader 2018).
3. Frequent "not sure" flags are false alarms, and false alarms cause
   disuse (Parasuraman and Riley). A 2004 HFES abstract
   (https://doi.org/10.1177/154193120404801807) predicts false alarms hurt
   trust more than misses.
4. Counts can create pressure: Superhuman uses counts to push toward zero,
   and a HEY user valued that the Feed shows none. The arrivals line counts
   what arrived, not what is unread, carries no badge, and is muted.
5. Reassurance doesn't end checking: Priority Inbox users still swept
   "Everything else".
6. Broken trust may not come back ("Trust forever lost"), and observed
   accuracy can override stated accuracy (Yin 2019), so one visible miss
   in a track record could anchor distrust.
7. Batching didn't lower measured stress (Mark 2016).
8. Caught-up markers can be co-opted (Instagram).
9. Showing that sorting happens can raise worry at first (Eslami 2015)
   before it builds control.

Unverified and noted as such: Dzindolet 2003, Validity's figures, the
exact Outlook "Other" banner, Apple's priority-messages page, Superhuman
missed-mail complaints, NN/g on timestamps. Read as abstracts only: Yin,
Kizilcec, Kulesza, Eslami, Rader and Gray, Rader 2018, Masicampo, Mark,
Parasuraman and Riley.

## 9. The code can account for every arrival, but only from a placement stored at arrival

Code at `1835ddd6` (`origin/main`, v0.6.55, fetched 2026-10-07).

**Membership is recomputed from today's inbox, so it forgets mail.**
`place_one` in `crates/daemon/src/handler/modes.rs` builds a thread's
modes on each request. Updates and Reading read only messages still in the
inbox (`!message.in_inbox` skips, line 305), and Messages needs a lane row
or the quiet rule, both of which also need the thread in the inbox
(`desk_lanes.rs` line 413, `quiet` in `modes.rs`). `places::placed_inbox`
starts from the INBOX label. Once the user or Gmail archives a message, it
is in no mode and `mxr modes why` prints "In no mode. Inbox still shows
it." That is right for "what is waiting", and wrong for "where did
everything that arrived go". On BK's mail, 242 of 435 live arrivals in the
last seven days had left the inbox (264 of the 267 out-of-inbox ones were
read, so this is BK's own archiving), and the membership code would put
them in no mode.

So the ledger can't be a sum over `GetModeMembership`. It needs one row
per inbound message written when sync stores it: the mode it went to, the
rule that sent it there, and when mxr first saw it. Later corrections
update a `now_mode` column next to the original, which is also what the
track record counts. This is a deliberate exception to "membership is
computed, never stored" (D097): the row records a past fact, where mail
went, not current membership, which stays computed.

**Exactly one primary mode is available for every arrival.** The base
mode comes from one place, `mail_kind::classify`
(`crates/daemon/src/handler/mail_kind.rs`), plus the thread shape rule
(`conversation_shape.rs`): Person is Messages, Person in a copied or crowd
thread is Updates, List is Reading, Automated is Updates, Denied is
screened out. To do (open `todos` rows) and Archive (`record_messages`)
are aspects added on top, never a replacement. Four cases have no base
mode and need a named bucket so the counts still sum:

| Case | Today | Ledger bucket |
|---|---|---|
| Delivery mail (`delivery_messages`) | Skipped by Updates and Reading (`modes.rs` line 305, `places.rs` line 166) | Updates (tracker), or Archive when a record was filed |
| Calendar invite (`calendar_invites`) | Skipped likewise | To do when an RSVP row exists, else Updates |
| Screened out (`screener_decisions.disposition = 'deny'`) | No mode | "screened out", its own count |
| Spam (provider label or flag) | Hidden everywhere (`NOT_TRASHED` in `crates/store/src/places.rs`) | "in Spam", its own count |

Snoozed mail is skipped by every mode until it wakes. The ledger counts it
in its mode with "snoozed" in the drill-down, not as missing.

**No confidence score exists.** Every placement is a deterministic rule
with a reason string (`mail_kind::reason`). The only scores in the tree
are delivery-detection confidence (`crates/deliveries/src/extract.rs`) and
draft-safety timing, neither about modes. The fast-tier model classifier
(phase 7) is not built. "Uncertain" therefore has to be defined as rule
conflicts, counted in section 10 and defined in the blueprint.

**No correction tracking table exists.** The blueprint specifies
`mode_corrections (account_id, scope, key, mode, verdict, decided_at)`
and it was never migrated (`crates/store/migrations` ends at
`067_records.sql`). What exists:

- `screener_decisions` (`018_screener_decisions.sql`): one row per sender,
  overwritten, with `decided_at` and no previous value, so it can't tell a
  correction of a rule from an agreement with it. `SetSenderKind` writes
  it. BK's store has 0 rows.
- `todos.dismissed_at` and `state = 'dismissed'` (`059_todos.sql`): a
  to-do dismissal is recorded. BK's store has 0 dismissed rows.
- `user_activity`: `sender.kind` and `screener.*` events are recorded
  (`crates/daemon/src/activity/mapper.rs` lines 287 and 484), but To do,
  Now, membership and `SetModeDone` requests are skipped
  (`skip_activity!` at lines 834 and 840). BK's last 14 days hold no
  `sender.kind` or `screener.*` event. The mapper also files every
  disposition other than allow as `screener.snooze`, which is wrong for
  feed, paper trail and deny.
- The per-email correction key (`X`, "not this mode, for this email") is
  in the key map but not in the code.

So a track record today would say "you moved 0", true only because there
is nothing to move with.

**Arrival time.** `messages` has no first-seen column, and `rowid` order
tracks backfill, not arrival. The `Date:` header is set by the sender:
five messages in BK's store are dated more than a day in the future. The
ledger's window uses the stored first-seen time, never `Date:`.

**Freshness comes first and already has an owner.** The status-bar line
"Latest mail 5m ago · synced 30s ago", with sync-failure warnings and a
popover of the last five arrivals and their modes, is being built on
`feat/freshness-indicator` as `GetFreshness` (not on `origin/main` at
`1835ddd6`). The ledger should read the same arrivals rows, so the
popover's last five and the ledger's counts can't disagree.

Files involved: `crates/daemon/src/handler/modes.rs`, `mail_kind.rs`,
`conversation_shape.rs`, `desk_lanes.rs`, `places.rs`, `now.rs`,
`mode_rules.rs`, `screener.rs`, `crates/daemon/src/commands/modes.rs`
(`mxr modes why`), `crates/daemon/src/activity/mapper.rs`,
`crates/store/src/places.rs`, `crates/store/src/mode_views.rs`,
`crates/store/migrations/018_screener_decisions.sql`, `059_todos.sql`,
`061_mode_done.sql`, `067_records.sql`, `crates/todo/src/detect.rs`
(`payment_failed`), and on the web `apps/web/src/features/modes/membership.ts`
(Inbox rows don't fetch membership yet; Now, Messages and the desk do).

## 10. On BK's mail, the counts sum and the never-bury rules are rare

Method: a read-only `.backup` of BK's store (`mxr.db`, 110,506 messages,
three accounts) taken 2026-10-07 into a scratch directory and deleted
afterwards. A Python script mirrored `mail_kind::classify`,
`conversation_shape` and `place_one` at `1835ddd6` and printed counts only:
no subject, sender or body left the script. mxr was not run against the
daemon. Windows are by the `Date:` header, because no first-seen time is
stored (section 9). "Arrival" is an inbound message. "Primary at arrival"
ignores whether the message is still in the inbox; "today's code" is what
membership returns now.

| | Last 24 hours | Last 7 days |
|---|---:|---:|
| Inbound arrivals | 50 | 461 |
| In Spam | 1 | 26 |
| Live | 49 | 435 |
| Messages (primary at arrival) | 8 | 54 |
| Updates | 10 | 215 |
| Updates, delivery mail with no base mode today | 0 | 21 |
| Reading | 31 | 145 |
| Sum of primary modes plus Spam | 50 | 461 |
| Also in To do | 2 | 22 |
| Also in Archive | 1 | 37 |
| In two modes (primary plus an aspect) | 3 | 36 |
| Threads | 48 | 423 |
| Threads whose arrivals split across two primary modes | 0 | 0 |
| No primary mode by today's code | 18 | 263 |
| of which left the inbox since | 18 | 242 |
| of which delivery mail | 0 | 21 |
| In no mode at all, not even To do or Archive | 15 | 224 |

The counts reconcile once every arrival gets a primary mode at arrival
and deliveries count in Updates. They don't reconcile from today's
membership: 263 of 435 live arrivals in seven days (60%) have no primary
mode because they left the inbox or are delivery mail, and 224 (51%) are
in no mode at all. Of the 267 seven-day arrivals outside the inbox, 264
are read, so most of that is BK's own archiving after reading.

Rules that placed the seven days' live arrivals: no-reply sender 187,
has List-Unsubscribe 134, from a person 58, automated sender 23, delivery
21, newsletter domain 11, automated domain 1. In the last 24 hours:
List-Unsubscribe 29, no-reply 8, person 8, newsletter domain 2, automated
sender 2.

Never-bury candidates (subject matches are keyword counts and an upper
bound, not checked by reading):

| Rule | 24 hours | 7 days | Where they went |
|---|---:|---:|---|
| N1. From an address BK has written to | 3 | 21 | 7 days: 3 to Reading by List-Unsubscribe, 4 to Updates as copied, 14 to Messages (8 of those since archived) |
| N2. Security alert by subject | 1 | 12 | 7 days: 11 Updates, 1 Messages; 0 to-dos |
| N3. Failed payment by subject or `payment_failed` to-do | 1 | 2 | both already to-dos |
| N4. Deadline by subject or a dated to-do | 2 | 13 | 6 already to-dos, 7 not |

"Not sure" candidates over seven days: someone written to but sorted as a
list or machine, 3; someone written to who only copied BK, 4; a person
BK has never written to (the new-sender question's ground), 40;
List-Unsubscribe as the only signal, 134; no-reply with no list headers,
187. Only the first two are low enough to ask about, and N1 settles the
first by moving that mail to Messages, which leaves 4 a week (none in the
last 24 hours) for the "Not sure" line.

Corrections available for a track record: 0 rows in `screener_decisions`,
0 dismissed to-dos, 6 `mode_done` marks (all Messages, which are done
actions, not corrections), and no `sender.kind` or `screener.*` activity
in the last 14 days.

## 11. Wording stays plain, counts what happened, and claims nothing it can't check

Judgement, from the evidence above and the copy rules in blueprint 22:

- Say what happened, with a clock time and a number: "Since 08:12, 50
  emails arrived". Never "nothing was missed", which mxr can't know.
- Name scope as rules, in the first person of the user's actions: "People
  you've written to always reach Messages." Not "important mail".
- "Accounted for", never "seen" or "caught up" for mail nobody opened.
- Reasons are rule names in plain words ("has List-Unsubscribe",
  "no-reply sender"), never scores or keywords.
- No "AI", no "smart", no exclamation marks, no em dashes. Model output
  stays italic and names its model, as every view's trust rules require.
- The full copy table is in the blueprint section.

## 12. Open questions

- The window: since the user last opened Now (chosen) or since the last
  Updates cut (08:00, 16:30). The cut is easier to remember; the visit is
  what "anything new?" means.
- Whether Reading's count belongs in the line at all, given HEY's user who
  liked that the Feed shows no number. The chosen line keeps it, because
  the sum is the claim.
- N1's edge: mail from someone you've written to, sent through a list
  (List-Id) with you in To. Chosen: Messages. It may pull in some
  newsletters from people BK once replied to; the 7-day count was 3.
- N2's detector precision is unknown: the 12 security-looking alerts are a
  keyword upper bound. Phase work should measure it with `mxr modes eval`
  before the "?" copy names the rule.
- Whether the stored arrivals row should backfill history on first run, or
  start from the day it ships.
