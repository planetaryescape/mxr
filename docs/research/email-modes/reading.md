# Reading: a front page you visit, not a pile you owe

Research for the Reading mode in [22-email-modes.md](../../blueprint/22-email-modes.md).
Code claims are against `bdf997c4` in `/tmp/mxr-feel-clones/modes` (the
email-modes branch, blueprint at v0.6.47 `3da0c119`). Screenshots are the
judge's tour from 2 Oct 2026 (`07-reading-after.png`, `04-reader-after.png`,
`16-mobile-reader-after.png`). Web sources were read on 2 Oct 2026; section 8
lists them. Where a point is my judgement rather than a source's finding, it
says so.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## 1. The job is choosing what to read, then reading it well, and owing nothing

Reading holds things the user asked for: newsletters, digests, posts from
writers. Nobody is waiting on any of it. The user comes here when they choose
to, the way you pick up a magazine, and the job has three moves:

1. Glance at what arrived and pick the one or two worth your time today.
2. Read it properly: the issue itself, or the article it points to, in a
   long-form layout that respects the text.
3. Let the rest go without thinking about it, and drop the sources you never
   pick.

Success feels like this: you open Reading, see at once which three things are
worth it, read one start to finish in a column that looks like a good book
page, put a link from a digest on your later shelf with one key, close the
mode, and feel no debt. A week later, whatever you didn't touch has quietly
gone, and mxr has noticed that you skipped a sender eleven times running and
offers, once, to unsubscribe you.

Failure feels like today's email client: forty bold rows, a count in the
sidebar, each row the sender's subject line, and each one opened into a
600px-wide marketing template with "View in browser" at the top.

## 2. The best reading apps separate the stream from the shelf, and drop the count

### Readwise Reader splits pushed content (Feed) from chosen content (Library)

Reader's launch post states the split directly: "Library is where high signal
documents that you manually curate for yourself go... Feed is where low signal
documents that are automatically pushed to you go such as RSS feeds, email
newsletters, and other digests." The founders call unifying a feed reader
with read-it-later "quite the product challenge" because the feed "pushes low
signal-to-noise content to you all day" ([Readwise, public beta](https://blog.readwise.io/the-next-chapter-of-reader-public-beta/)).
The Feed has only two states, Unseen and Seen; the Library has Inbox, Later,
Shortlist and Archive ([Readwise docs, adding content](https://docs.readwise.io/reader/docs/faqs/adding-new-content),
[default views](https://docs.readwise.io/reader/guides/filtering/default-views)).

Why it works: the two piles have different contracts. The feed promises
nothing and can be skimmed and abandoned; the library is a short list of
things you chose, so it can ask more of you. mxr's Reading mode is a feed,
and the "later" shelf the blueprint plans (phase 6) is a library. Keeping
them visibly separate is the main lesson.

Two more Reader details carry over. Newsletters arrive by a dedicated
`@feed.readwise.io` address and are shown as cleaned text by default, with a
switch to the original layout, and "Reader will remember whichever view you
used last for a particular email sender" ([Readwise docs, email newsletters](https://docs.readwise.io/reader/docs/faqs/email-newsletters)).
And the mobile Feed has a card mode where you "swipe up to advance, swipe down
to reverse, save for later, and tap to start reading. Advancing marks a
document as seen" ([Readwise, public beta](https://blog.readwise.io/the-next-chapter-of-reader-public-beta/)).

### Reeder removed unread counts and syncs a position instead

The 2024 Reeder rebuild is a single timeline across feeds, video, podcasts and
social posts. Its App Store description: "Say goodbye to unread counts! With
Reeder, your timeline position is synced across all your devices," and it
saves items with tags: Links, Favorites, Bookmarks and Later
([App Store](https://apps.apple.com/app/reeder/id6475002485)). Rizzi told
TechCrunch the new sync fetches only "subscriptions, timeline position, and
tagged items", and that dropping unread counts across devices "has also had a
positive impact on speed overall"; content is no longer "retrofitted into a
viewer that was originally designed just for RSS feed articles", each kind
gets its own viewer ([TechCrunch](https://techcrunch.com/2024/09/23/the-new-reeder-app-is-built-for-rss-youtube-reddit-mastodon-and-more)).

Why it works: a position answers the only useful question ("where was I?")
without turning every item into an obligation. A "you left off here" line is
enough.

### Current goes further: items have a half-life and fade away

Current (Terry Godier, 2026) traces unread counts to NetNewsWire's 2002
three-pane design, "a pragmatic design decision that calcified into
convention", and names the result "phantom obligation". Items have a velocity:
breaking news lasts about 3 hours, standard articles 18 hours, essays 7 days.
"You don't mark them as read. You don't file them. They simply pass." Every
design decision serves one argument: "you are not behind"
([Godier](https://www.terrygodier.com/current), [Kottke](https://kottke.org/26/02/0048381-current-is-an-interesting-new)).

Why it works, and the evidence under it: auto-expiry is not new, only its
framing is. Feedly has marked everything older than 31 days as read for years,
whether you read it or not, originally so it could compute counts quickly
([Feedly docs](https://docs.feedly.com/article/372-do-you-mark-articles-older-than-30-days-as-read)).
Users mostly don't notice, which suggests old feed items have little value to
them. Current turns the same mechanic into a visible, per-kind lifetime.

### Matter and Substack give newsletters their own address and a queue

Matter gives each user a newsletter address; issues land in an Inbox and a
swipe right moves one to the Queue, which is sortable by word count, date and
author with "quick reads" filters. Typography is adjustable (eight sizes, six
fonts, three spacings), and highlights export to Obsidian, Notion and
Readwise. MacStories' main complaint is that it prefers archiving to deleting
when most users want read items gone ([MacStories](https://www.macstories.net/reviews/matter-a-fresh-take-on-read-later-apps/),
[The Sweet Setup](https://thesweetsetup.com/apps-were-trying-matter/)).
Substack's 2023 app moved the inbox to the centre tab and added a reading
queue you swipe through post to post ([TechCrunch](https://techcrunch.com/2023/09/20/substack-redesigns-its-mobile-app-to-boost-discovery-and-engagement)).

Feedbin and Inoreader do the same with dedicated addresses, showing issues
"as articles in a newsletter feed, right next to your followed websites"
([Inoreader, Aug 2026](https://www.inoreader.com/blog/2026/08/a-better-way-to-keep-up-with-newsletters.html),
[Feedbin](https://feedbin.com/blog/2025/10/01/newsletter-extension/)). Chris
Coyier's reason is the whole case for a Reading mode: "I don't really need
email newsletters in my actual email, I'd rather read them with all the other
stuff I read" ([Coyier](https://chriscoyier.net/2024/04/26/feedbin-email-newsletter-emails/)).

mxr can do this without a second address: the classifier already knows which
mail is a list (section 4). That is a real advantage over every app above,
which all require the user to resubscribe or set up forwarding.

### Gmail's subscription page ranks by volume and unsubscribes in one click

Gmail's Manage subscriptions page (July 2025) lists senders by how many emails
each sent in recent weeks and sends the unsubscribe on the user's behalf
([TechCrunch](https://techcrunch.com/2025/07/08/gmails-new-manage-subscriptions-tool-will-help-declutter-your-inbox/),
[Gmail Help](https://support.google.com/mail/answer/15621070)). It shows
volume but not engagement. RFC 8058's `List-Unsubscribe-Post:
List-Unsubscribe=One-Click` lets a client POST the unsubscribe with no
browser, and it is only valid when DKIM covers both headers
([Mailgun on RFC 8058](https://www.mailgun.com/blog/deliverability/what-is-rfc-8058/)).

### Kindle and Medium make length visible

Medium's "N min read" is word count over about 265 words per minute plus a
few seconds per image ([explainer](https://blog.markdowntools.com/posts/markdown-word-count-reading-time-how-it-actually-works)).
Kindle starts from a typical speed and learns the reader's own pace, showing
time left in chapter ([MakeUseOf](https://www.makeuseof.com/show-reading-progress-kindle/)).
Length is what decides "now or later", so it belongs on every item.

### Typography research sets the reader's column

Butterick: 45 to 90 characters per line including spaces, or two to three
alphabets; longer lines make it harder to find the start of the next line
([Practical Typography](https://practicaltypography.com/line-length.html)).
Firefox's Reader View uses Readability.js, a rule-based extractor that scores
DOM nodes by tag, text length and link density and returns title, byline,
site name and cleaned HTML ([overview](https://webcrawlerapi.com/blog/how-to-extract-article-or-blogpost-content-in-js-using-readabilityjs)).

## 3. What fails: counts, piles that never shrink, and apps that disappear

### Unread counts turn a choice into a debt

This is the shared finding of Reeder, Current and Readwise's two-state feed,
and it is a design consensus more than a measured effect; I found no
controlled study. Durnell's essay puts the user side well: a read-later app
also offers "freedom from reading that article", and "Your free time is not a
job" ([Durnell](https://tracydurnell.com/2025/09/06/its-ok-to-not-read-your-read-later-backlog/)).
mxr already agrees: `PlaceMessageData.unread` is documented as "Shown for
reference only: nothing in a place counts as unread"
(`crates/protocol/src/types/places.rs:84`).

### People skim newsletters; few read one through

NN/g's newsletter studies (eyetracking and diaries over many rounds; the
figures are old but consistent) found users fully read only 19% of
newsletters, spent an average of 51 seconds on one after opening, and 67% had
zero fixations on the introduction; their eyes went to the first two words of
headlines ([NN/g, inbox congestion](https://www.nngroup.com/articles/email-newsletters-inbox-congestion/)).
A later report puts thorough reading at 11% and unopened at 27%
([NN/g report](https://www.nngroup.com/reports/email-newsletter-design/)).
On the web generally, users read at most 28% of the words on a page, 20% more
likely ([NN/g, how little do users read](https://www.nngroup.com/articles/how-little-do-users-read/)).

This is the strongest evidence in this document, and it cuts two ways. Most
issues need a scan surface (headline, standfirst, links), not a full render.
And the rare issue you do read deserves a proper reader, because that is the
11 to 19% where the value is.

### Read-later piles grow until people stop opening the app

The pattern is widely described: save feels productive, the pile grows, the
guilt grows ([Creativerly](https://www.creativerly.com/the-save-for-later-paradox-why-we-hoard-digital-content-we-never-read/)).
That article quotes "73% of newsletters saved to read later apps are never
read" without a source I could find, so treat the number as unverified. The
design lesson stands without it: Later must be small and must age, or it
becomes the pile Reading was meant to replace.

### Read-later services die and take your highlights with them

Omnivore was acquired by ElevenLabs on 1 Nov 2024 and shut down on 15 Nov,
with two weeks to export ([Creativerly](https://www.creativerly.com/the-exit-us-of-omnivore-from-open-source-to-ai-vc-money/)).
Mozilla shut Pocket on 8 Jul 2025 and deleted data after 8 Oct 2025
([Android Police](https://www.androidpolice.com/pocket-shuts-down/),
[Wikipedia](https://en.wikipedia.org/wiki/Pocket_(service))). Artifact
closed in January 2024 because, Systrom said, the market opportunity was not large enough
([TechCrunch](https://techcrunch.com/2024/01/18/why-artifact-from-instagrams-founders-failed-shut-down/)).
This is mxr's opening: the issues are already in the user's mailbox, and a
local SQLite store of extracted articles and highlights can't be shut down by
someone else.

### Uniform viewers and over-taxonomy hurt

Reeder's own rationale for the rebuild is that content "retrofitted" into one
viewer reads badly. Its critics note the opposite failure: heavy users missed
read state and keyboard shortcuts for opening articles
([Michael Tsai roundup](https://mjtsai.com/blog/2025/02/05/reeder-rebuilt/)).
Matter's iPad layout drew complaints for huge margins
([MacStories](https://www.macstories.net/reviews/matter-a-fresh-take-on-read-later-apps/)).
Readwise's own docs admit "there's no way to find a list of all email
newsletters you're subscribed to inside Reader", and its unsubscribe only
blocks the sender rather than leaving the list ([Readwise docs](https://docs.readwise.io/reader/docs/faqs/email-newsletters)).
mxr can do both properly: it has the sender list and the real
`List-Unsubscribe` method.

### What mxr ships today is a full-render feed

`ReadingRoute.tsx` renders every issue fully open, newest first, 12 at a time
(`apps/web/src/features/places/ReadingRoute.tsx:38-46`). In the screenshot
each item is the subject line, then the sender's own HTML body in a white
card with its own H1 repeating the subject. There is no length, no standfirst,
no article, no later, and the only exit is "Sweep all" (`A`), which archives
everything unpinned. On mobile (`16-mobile-reader-after.png`) the icon rail
takes about a quarter of a 390px screen and the body sits in a padded card, so
lines run to roughly 20 characters, well under Butterick's floor. Because
nothing in the feed marks a message read, the existing open-rate evidence for
unsubscribing (section 4) is starved of signal from mxr itself.

## 4. The data this view extracts from an email

Reading treats an email as a container for one or more readable items. The
item, not the message, is the unit on screen.

| Field | How it's derived | In mxr today | Needed |
|---|---|---|---|
| Source (publication) | Sender display name, falling back to `List-Id` phrase, then domain | `PlaceBundleData.sender_name`, `sender_email` (`types/places.rs`) | A per-sender display override |
| Why it's here | `mail_kind::classify` reason, screener `Feed` disposition | Yes, `WhyHereLine.tsx`, `mail_kind.rs` | Nothing |
| Issue headline | Subject with boilerplate stripped ("Issue #42 \|", "[Name]", emoji, "This week in") | Raw subject only | Subject cleaner, rule-based |
| Standfirst | First paragraph of real text after preheader, "View in browser" and masthead are removed | `mxr_reader` strips tracking footers and "view in browser" (`crates/reader/src/tracking.rs`) | Preheader and masthead skipping, first-paragraph pick |
| Shape | `single` (one essay), `digest` (many links with blurbs), `teaser` (short body pointing to one link), `notice` | None | Rule-based classifier on link count, link density, words per link |
| Main link | For `teaser`: the link whose anchor text matches the headline, or "Read more"/"Continue reading" | None | Link extractor over the HTML |
| Digest items | For `digest`: each external link with anchor text over three words and its sibling blurb, deduplicated by unwrapped URL | None | Same extractor, block grouping by parent element |
| Word count, minutes | Cleaned text words / 238 wpm (configurable), images add seconds as Medium does | `ReaderOutput.cleaned_lines` only (`crates/reader/src/pipeline.rs`) | Word count in `ReaderOutput` |
| Hero image | First large image after masthead, only when remote content is allowed | Remote images blocked by default (`sanitizeHtml.ts`, `RenderConfig.html_remote_content`) | Respect the existing setting; no fetch otherwise |
| Cadence | Median days between issues from this sender, from history | Message dates in store | One query; drives expiry |
| Engagement | Opened in mxr (dwell over 10 s or scroll past 30%), finished (scroll past 90%), plus the provider READ flag set elsewhere | `SubscriptionSummary.opened_count`, `archived_unread_count` from the READ flag (`crates/store/src/message.rs:1084`) | Local engagement rows, since the feed never sets READ |
| Unsubscribe method | `List-Unsubscribe`, `List-Unsubscribe-Post`, body link | Yes: `UnsubscribeMethod::{OneClick, HttpLink, Mailto, BodyLink, None}` (`crates/core/src/types.rs:888`), `UnsubscribePurge { dry_run }` (`crates/protocol/src/types.rs:966`) | Nothing |
| Article (on request) | Fetch main or digest link, follow redirects, run a Readability-style extractor | No HTTP in `mxr-reader` (`crates/reader/Cargo.toml`), no article code | Fetcher plus extractor, user-initiated only |
| Position | Scroll fraction per item, last-seen item in the front page | None | Small table |
| Highlights | Text range plus note, on issue or article | None | Phase 3 of this proposal |

The new storage, all local, all derived or user-made:

- `reading_items (message_id, idx, kind, title, standfirst, url, url_unwrapped,
  domain, word_count, content_hash, extractor_version)`: the extraction cache,
  rebuilt when the hash or extractor version changes. Rule-based, no model,
  so it runs for every list message during `post_sync_fanout` like
  deliveries do (`crates/daemon/src/loops.rs`).
- `reading_state (account_id, item_key, opened_at, dwell_ms, progress,
  finished_at, later_at, let_go_at)`: one row per item the user touched.
- `reading_articles (url_unwrapped, fetched_at, title, byline, site,
  html_clean, word_count, status)`: fetched only on "open article" or "later".
- `reading_highlights (id, item_key, quote, note, created_at)`.

Activity records for these writes carry ids and counts only, per the
invariant in `AGENTS.md`. Article HTML is user content and stays in the
store, not in `context_json`.

The extractor is the one buildable risk. `mxr-reader` today is an
email-cleaning pipeline (`html2text` at 80 columns, quote, signature,
boilerplate and tracking stripping), not a Readability port. For fetched web
articles a Rust Readability port is the right adoption (Rust ports exist on
crates.io; which one to adopt needs a spike against 30 real articles). For
newsletter HTML, Readability is tuned for web pages and table-based email
layouts are its weak case, so the issue extractor should be mxr's own rules
over `scraper` (already a dependency), tested on fixtures from real
newsletters. This is a judgement, to be confirmed by that spike.

## 5. The proposed view: an edition, a reader and a shelf

Reading becomes three surfaces over one daemon list:

1. **The edition** (default): what arrived, laid out like a front page,
   ranked by how much you read each source, with no counts.
2. **The reader**: one item in a book-like column, the issue or the article,
   with length, progress and position.
3. **Later**: a short shelf of items you chose, kept offline.

A fourth, **Sources**, is the manage view (today's Subscriptions), reached
from the edition footer, not the rail.

### The edition is ranked by your reading, banded by time, and fades

Order: three time bands (Since you were last here, Earlier this week, Fading),
and inside a band by source affinity (finished issues in the last 90 days,
then opened, then never) and then recency. The top one to three items from
sources you read most get a lead treatment. This is Reeder's position marker
plus a front page's hierarchy, which I prefer to a pure timeline because the
NN/g finding says most issues get a glance, so the glance surface must put the
likely reads first. Pure affinity ranking risks never surfacing a new
source; new sources get one lead slot for their first three issues.

Each item shows what a scan needs, in the order eyes hit it per NN/g:
headline first, source and minutes second, standfirst third. Digest issues
show their first four link items inline with "+ 12 more". No sender address,
no time of day, no bold, no checkbox.

Desktop web (rail collapsed to the five modes; the existing app shell stays):

```text
+------+---------------------------------------------------------------------+
| Now  |  Reading                                   Later 4   Sources   /    |
| Msgs |                                                                     |
| ToDo |  SINCE YOU WERE LAST HERE                                           |
| Upd  |  +---------------------------------------------------------------+  |
|>Read |  | The quiet death of the three-pane layout                      |  |
| Arch |  | Long Reads Weekly  .  14 min  .  you read 9 of 10        |  |
|      |  | Why every RSS reader since 2002 shipped the same window, and   |  |
| Inbx |  | what replaces it when the inbox stops being the model.        |  |
|      |  |                          [enter] read   [l] later   [e] let go |  |
|      |  +---------------------------------------------------------------+  |
|      |                                                                     |
|      |  SQLite Notes . weekly digest . 6 links                    4 min    |
|      |    > Local-first mail is having a moment        demo.mxr.local      |
|      |    > SQLite 3.51 release notes                  sqlite.org          |
|      |    > Terminal workflows that stuck              example.com         |
|      |    > Why sync engines need tombstones           example.org         |
|      |    + 2 more                                                         |
|      |                                                                     |
|      |  Conference Passes  .  Early-bird ends Friday             2 min     |
|      |  Prices go up after Friday; workshop seats are limited.             |
|      |                                                                     |
|      |  ------------------- you left off here --------------------------   |
|      |  EARLIER THIS WEEK                                                  |
|      |  Platform Weekly . Shipping a sync engine in 2026         9 min     |
|      |  ...                                                                |
|      |  FADING  (goes on Sunday, unless you keep it)                       |
|      |  Growth Digest . 5 ways to...  . you opened 0 of the last 11  [U]   |
+------+---------------------------------------------------------------------+
```

The "Conference Passes" item shows a handoff: an issue with a deadline
("ends Friday") is also a To do candidate, and the To do detector from phase
1 of the blueprint owns that aspect, not Reading.

Mobile web: no rail on the reading surfaces; a bottom bar holds the five
modes. Cards fill the width with a 16px gutter. Swipe right is Later, swipe
left is Let go, tap reads, as in Matter and Readwise's card feed.

```text
+-------------------------------+
| Reading            Later 4  ⋯ |
|-------------------------------|
| SINCE YOU WERE LAST HERE      |
| +---------------------------+ |
| | The quiet death of the    | |
| | three-pane layout         | |
| | Essay . 14 min . read 9/10| |
| | Why every RSS reader since| |
| | 2002 shipped the same...  | |
| +---------------------------+ |
|  <- let go          later ->  |
| SQLite Notes . 6 links  4 min |
|  > Local-first mail is hav... |
|  > SQLite 3.51 release notes  |
|  + 4 more                     |
|-------------------------------|
| Now  Msgs  ToDo  Upd  Read  ▤ |
+-------------------------------+
```

The reader on mobile is full screen, text at 17 to 19px, 16px gutters, no
card inside a card, which gives about 35 to 45 characters per line at 390px.
That is the bottom of Butterick's range and normal for phone reading.

### The reader is a book page with the article one key away

```text
+---------------------------------------------------------------------+
|  <- Reading     Issue | Article       14 min . 9 left     [l] [e] [U]|
|  ====================--------------------------------------  38%    |
|                                                                     |
|            The quiet death of the three-pane layout                 |
|            Long Reads Weekly . 2 Oct                          |
|                                                                     |
|            Body text in a 66-character column, 1.5 line             |
|            height, the user's chosen serif or sans, links           |
|            underlined, images only if remote content is on.         |
|            Masthead, "view in browser", share bars and the          |
|            footer are gone; "Original" shows the sender's           |
|            layout and is remembered per source, as Readwise         |
|            does.                                                    |
|                                                                     |
|            ---- end of issue ----                                   |
|            From this source: you finished 9 of the last 10.         |
|            [n] next   [l] later   [e] let go                        |
+---------------------------------------------------------------------+
```

`Issue | Article` appears only when the item has a main link. Article view
fetches and extracts the linked page on first press, shows "Fetching from
example.com" with the domain named (the fetch tells that site you clicked),
and caches the result. If extraction fails or hits a paywall, it says so and
offers the browser. For paid newsletters where the email is the article,
Issue is already the full text.

Time left is minutes remaining at the user's pace, Kindle-style: start at
238 wpm and adjust from finished items' dwell, clamped so one tab left open
overnight can't skew it.

### Primary actions are read, later, let go, unsubscribe

Keys follow mxr's existing conventions (`e` done, `l` for later is new; check
against `keymapParity.test.ts` before adopting):

| Key | Edition | Reader |
|---|---|---|
| `j` / `k` | next / previous item | scroll |
| `enter` / `o` | read the item | in a digest link: open article |
| `a` | read the linked article (Article view) | toggle Issue / Article |
| `l` | put on Later (fetches the article if it's a link) | same |
| `e` | let go now | let go, go to next |
| `U` | unsubscribe, with evidence and preview | same |
| `n` | | next item without letting go |
| `v` | | Original layout, remembered per source |
| `h` | | highlight selection (phase 3) |
| `space` | | page down; at the end, next item |

Three things are deliberately absent: reply, forward and labels. They exist in
Inbox for the rare newsletter you want to answer, and `g i` takes you there.

### Items enter by classification and leave by fading, letting go, or the shelf

Entering: every message whose sender's base mode is Reading, per
`mail_kind::classify` and screener `Feed` (`crates/daemon/src/handler/mail_kind.rs`,
`crates/store/src/screener.rs`), produces items once extraction runs. A
message that is also a To do or an Archive record shows in those modes too
(blueprint, "One email can live in several modes").

Expiry: each source gets a lifetime from its cadence: twice the median
interval, clamped to 2 to 14 days (a daily lasts 2 days, a weekly 14). This
is Current's half-life idea with a per-source rule instead of a per-genre
guess, and Feedly's 31-day mark-read shows users tolerate expiry. In its last
day an item moves to the Fading band, which says when it goes. Expired items
get `let_go_at` with reason `expired`; nothing is deleted, and search and
Inbox still find them. Expiry is a mode-done mark (`mode_done` in the
blueprint), so the provider archive follows the blueprint's last-mode rule.
Later items never expire silently.

Let go: `e` marks the item done in Reading. A digest is let go when its issue
is; its links saved to Later live on independently.

Later: a shelf, not a queue. It shows a count because it is small and chosen,
and it's the only count in the mode. Items over 30 days old get one quiet
line, "Saved 5 weeks ago. Still want it?", with Keep and Let go; no
re-asking. Saving a link fetches and stores the article at once so it reads
offline; saving an issue needs nothing, the body is local.

Unsubscribe: `U` opens a preview built on the existing
`UnsubscribePurge { dry_run: true }`:

```text
Unsubscribe from Growth Digest?
  You opened 0 of the last 11 issues (since 12 Jul). Last finished: never.
  Method: one-click (the sender is told directly, no browser)
  Also lets go of 11 issues still in Reading.
  This can't be undone from mxr; you'd resubscribe on their site.
  [enter] Unsubscribe    [esc] Keep
```

The evidence line is the part Gmail lacks. Once per source, when it has sent
at least 8 issues and you opened none in the last 60 days, the item carries a
small inline "Unsubscribe?" offer; dismissing it means it never comes back for
that source. A third option worth testing is "Read less": hold that source's
issues and show them as one weekly bundle. Thresholds are my starting
guesses and belong in config.

### The rhythm is pull, and the desk mentions Reading only as a fact

Reading never notifies and has no rail badge, matching the blueprint's
"badges count only work". The desk's footer line, today "Reading 6 this week"
(`01-desk-after.png`), stays a fact and could name the lead item instead
("New from Long Reads Weekly, 14 min"). The edition remembers your
position across web, TUI and devices via `reading_state`, as Reeder syncs
timeline position.

### The empty state says you're current, not that you're done

When nothing is in the first band: "Nothing new since Tuesday. Later has 4
things saved." With a link to Later. When the whole mode is empty: "Nothing
to read. New issues from 14 sources will appear here." No low-tide animation
for Reading, because an empty Reading is not an achievement and D101 keeps
delight rare.

### Motion and sound stay small

- Let go: the card folds to a hairline and the next one rises, 150 to 200 ms,
  none under reduced motion, none when `e` is held (D101).
- Later: the card slides toward the Later tab, which ticks up by one. This
  teaches where things went.
- Reader progress: a thin line under the header; at 100% it fills and the
  end-of-issue block appears. No sound.
- Fading: items in the Fading band render at reduced contrast, still above
  WCAG AA, so age is visible without a label on every row.

### The TUI is a two-pane edition with a 72-column reader

```text
┌ Reading ─────────────────────────┬─ The quiet death of the three-pane… ──┐
│ SINCE YOU WERE LAST HERE         │ Essay . 14 min . 9 left     38% ▓▓▓░░ │
│▶The quiet death of the three-p…  │                                       │
│   Essay . 14m . read 9/10        │ Body reflowed to 72 columns by        │
│ SQLite Notes . 6 links . 4m      │ mxr-reader, links numbered [1] [2],   │
│   › Local-first mail is having…  │ footer and masthead stripped.         │
│   › SQLite 3.51 release notes    │                                       │
│   + 4 more                       │ [1] demo.mxr.local/articles/local-…   │
│ ── you left off here ──          │                                       │
│ EARLIER THIS WEEK                │                                       │
│ Platform Weekly . Shipping a…9m  │                                       │
│ FADING                           │                                       │
│ Growth Digest . 0/11 opened  U?  │                                       │
├──────────────────────────────────┴───────────────────────────────────────┤
│ enter read  a article  l later  e let go  U unsubscribe  L later shelf   │
└──────────────────────────────────────────────────────────────────────────┘
```

The lens builds on `crates/tui/src/ui/place_lens.rs`. Article view in the TUI
renders the extracted article through the same `mxr-reader` text path.

### CLI and daemon contract first

Per `AGENTS.md`, the mode is daemon IPC plus CLI JSON before any client:

- `ListReadingItems { account, band, cursor }` returning items with
  `kind`, `title`, `standfirst`, `minutes`, `source`, `affinity`, `expires_at`,
  `why`. Builds on `ListModeItems { mode: Reading }` from the blueprint.
- `FetchArticle { item_key }`, `SetReadingState { item_key, progress | later |
  let_go, dry_run }`, `ListLater`, `ListReadingSources` (extends
  `ListSubscriptions` with engagement).
- `mxr reading` (exists) gains `--format json` items, `mxr reading later
  [add|list|done]`, `mxr reading article <item>` (prints extracted text),
  `mxr reading sources --rank` (today's `mxr subscriptions --rank` plus
  engagement), `mxr reading let-go --expired --dry-run`.
- MCP: `mxr_reading_items` and `mxr_reading_article` let an agent build a
  morning brief from the user's own subscriptions, which fits the
  positioning note that local history is signal an agent can use.

### Phasing inside the blueprint's phase 6

1. Edition and reader over the issue only: extraction cache, headline and
   standfirst, minutes, shapes, digest link items, bands, affinity, local
   engagement, let go, expiry with dry run, unsubscribe with evidence. No
   network. Check: on BK's real mail, `mxr reading --format json` shows
   minutes and shape for every issue and BK agrees with the shape on 30
   sampled issues (counts only recorded).
2. Article fetch and Later: fetcher, Readability port, offline storage,
   Issue/Article toggle. Check: "later" on a digest link, then airplane mode,
   then the article reads in full.
3. Highlights and notes: stored locally, searchable from Archive, exported as
   Markdown with `mxr reading highlights --format md`. Check: a highlight made
   in the reader is found by `mxr records --query` (or Archive search).

## 6. How this differs from a list of emails

| A list of emails | Reading in mxr |
|---|---|
| One row per message | One item per readable thing; a digest is many items, a teaser is its article |
| Title is the subject line as sent | Title is the cleaned headline, with a standfirst |
| From, date, unread bold | Source, minutes, how much you read this source |
| Sorted by arrival | Banded by time, ranked by your reading of the source |
| Unread count in the sidebar | No count; a "left off here" line; only Later counts |
| Opening shows the sender's HTML at 600px | Opening shows a 66-character reader; original is one key away |
| The linked article is a browser tab | The article is extracted, cached, offline, and its own item |
| Leaves when you archive it | Leaves when you let it go, or fades on a per-source clock |
| Unsubscribe is a link in a footer | Unsubscribe is a key, with evidence and a dry-run preview |
| Reply, forward, label up front | Read, later, let go, unsubscribe up front; reply lives in Inbox |

## 7. Open questions and risks

- Extraction quality decides whether this feels magic or broken. Table-based
  newsletter HTML is the hard case; a spike on 30 of BK's real issues comes
  before committing to shapes. A bad headline is worse than the subject, so
  fall back to the subject when confidence is low.
- Fetching an article tells the publisher you clicked, and many newsletter
  links are tracked redirects. Unwrapping known redirect parameters before
  the fetch hides some tracking but can break attribution the writer depends
  on. Default: follow the link as given, only on an explicit key, never
  prefetch outside Later. Confirm.
- Should expiry trigger the provider archive (via the last-mode rule) or only
  hide items in mxr? The blueprint's unresolved question applies here most,
  because Reading is where most volume is.
- Affinity ranking can trap the user in what they already read. One lead slot
  for new sources is a guess.
- Engagement tracking (dwell, scroll) is local, but it is still tracking.
  Should it be visible and switchable in settings, and does `MXR_ACTIVITY=off`
  cover it? My view: it should be separate from activity, local only, and
  on by default, since it powers features the user sees.
- Where highlights land: Archive search, a Markdown export, or Obsidian. BK
  uses Obsidian heavily; an export path may matter more than an in-app view.
- Paywalled and members-only links fail extraction; the fallback copy and the
  "open in browser" path must be good, not an error.
- The rail and mobile shell: the bottom bar on mobile is a shell change
  bigger than Reading, and should be decided across all five modes.
- Is "Read less" (weekly bundling of a noisy source) worth its maintenance
  cost, or is unsubscribe enough? Per BK's product rules, test the evidence
  line first and add bundling only if BK keeps sources he never reads.

## 8. Sources

Product write-ups and docs:

- Readwise, The Next Chapter of Reader: Public Beta. https://blog.readwise.io/the-next-chapter-of-reader-public-beta/
- Readwise docs, Adding Content to Reader. https://docs.readwise.io/reader/docs/faqs/adding-new-content
- Readwise docs, Default Filtered Views. https://docs.readwise.io/reader/guides/filtering/default-views
- Readwise docs, Email Newsletters. https://docs.readwise.io/reader/docs/faqs/email-newsletters
- Reeder on the App Store. https://apps.apple.com/app/reeder/id6475002485
- TechCrunch, The new Reeder app (23 Sep 2024). https://techcrunch.com/2024/09/23/the-new-reeder-app-is-built-for-rss-youtube-reddit-mastodon-and-more
- Michael Tsai, Reeder Rebuilt (reaction roundup). https://mjtsai.com/blog/2025/02/05/reeder-rebuilt/
- Terry Godier, Current. https://www.terrygodier.com/current
- Kottke on Current. https://kottke.org/26/02/0048381-current-is-an-interesting-new
- Feedly docs, articles older than 30 days marked read. https://docs.feedly.com/article/372-do-you-mark-articles-older-than-30-days-as-read
- MacStories, Matter review. https://www.macstories.net/reviews/matter-a-fresh-take-on-read-later-apps/
- The Sweet Setup, Matter. https://thesweetsetup.com/apps-were-trying-matter/
- TechCrunch, Substack app redesign (Sep 2023). https://techcrunch.com/2023/09/20/substack-redesigns-its-mobile-app-to-boost-discovery-and-engagement
- Inoreader, A better way to keep up with newsletters (Aug 2026). https://www.inoreader.com/blog/2026/08/a-better-way-to-keep-up-with-newsletters.html
- Feedbin, newsletter addresses in the extension. https://feedbin.com/blog/2025/10/01/newsletter-extension/
- Chris Coyier, Feedbin Email Newsletter Emails. https://chriscoyier.net/2024/04/26/feedbin-email-newsletter-emails/
- TechCrunch, Gmail Manage subscriptions (Jul 2025). https://techcrunch.com/2025/07/08/gmails-new-manage-subscriptions-tool-will-help-declutter-your-inbox/
- Gmail Help, Manage your subscriptions. https://support.google.com/mail/answer/15621070
- Mailgun, What is RFC 8058. https://www.mailgun.com/blog/deliverability/what-is-rfc-8058/

Research and typography:

- NN/g, Email Newsletters: Surviving Inbox Congestion. https://www.nngroup.com/articles/email-newsletters-inbox-congestion/
- NN/g, Marketing Email and Newsletter Usability report. https://www.nngroup.com/reports/email-newsletter-design/
- NN/g, How Little Do Users Read? https://www.nngroup.com/articles/how-little-do-users-read/
- Butterick, Practical Typography, Line length. https://practicaltypography.com/line-length.html
- Readability.js overview. https://webcrawlerapi.com/blog/how-to-extract-article-or-blogpost-content-in-js-using-readabilityjs
- Reading-time formula explainer. https://blog.markdowntools.com/posts/markdown-word-count-reading-time-how-it-actually-works
- MakeUseOf, Kindle reading progress. https://www.makeuseof.com/show-reading-progress-kindle/

Shutdowns and opinion:

- Creativerly, The exit(us) of Omnivore. https://www.creativerly.com/the-exit-us-of-omnivore-from-open-source-to-ai-vc-money/
- Android Police, Pocket shuts down. https://www.androidpolice.com/pocket-shuts-down/
- Wikipedia, Pocket (service). https://en.wikipedia.org/wiki/Pocket_(service)
- TechCrunch, What happened to Artifact? https://techcrunch.com/2024/01/18/why-artifact-from-instagrams-founders-failed-shut-down/
- Tracy Durnell, It's ok to not read your read later backlog. https://tracydurnell.com/2025/09/06/its-ok-to-not-read-your-read-later-backlog/
- Creativerly, The save for later paradox (its 73% figure is unsourced). https://www.creativerly.com/the-save-for-later-paradox-why-we-hoard-digital-content-we-never-read/

Not covered from primary sources because the search budget ran out: Apple
News, Flipboard, Kindle typography defaults, Instapaper, NetNewsWire's own
writing, Hacker News readers and Stratechery's delivery. Their points above
come from the sources listed, not from those products' own pages.
