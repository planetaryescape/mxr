# Archive mode: email as a filing cabinet you ask questions of

Research and proposal for the Archive mode in
[22-email-modes.md](../../blueprint/22-email-modes.md). Code references are
at `bdf997c4` on `docs/email-modes` in `/tmp/mxr-feel-clones/modes`.
Screenshots of today's UI are the judge set (`01-desk`, `04-reader`,
`08-paper-trail`).

The short version: Archive should not be a folder of emails. Its unit is
the record (a receipt, an order, a booking, an invoice, a contract), built
from one or more emails, filed without the user lifting a finger, and
retrieved by asking for the thing you need ("Lisbon booking ref", "laptop
receipt") and getting the field back, not a list of messages to open.

Keys proposed here were reconciled across all six notes against both
keymaps; the binding map is "One key map across Now and the modes" in
[22-email-modes.md](../../blueprint/22-email-modes.md), which wins where
they differ.

## 1. The job is to hand back a fact or a file in seconds, months later

People come to Archive with a specific need and a deadline that someone
else set: the airline desk wants a booking reference, the shop wants proof
of purchase for a warranty claim, the accountant wants last year's invoices.
The email that holds the answer arrived long ago and was never opened
twice.

Success feels like this:

- You type four words you half remember ("dell receipt", "airbnb porto")
  and the answer is on screen before you finish: the reference, amount and
  date on a card, a copy key, and the PDF one key away.
- You never filed anything. It was already there, correctly labelled as a
  receipt from Dell for £1,249 on 3 March 2025.
- When you need a set ("everything for 2025 taxes"), you get a table you
  can check and export, with the PDFs, in one step.
- You trust the numbers, because every extracted field can show the line it
  came from, and the ones nobody has checked say so.
- Nothing about your purchases left your machine.

Failure feels like Gmail today: search returns forty "Your order" emails
from the same merchant in arrival order, and you open them one by one.

## 2. The best tools file for you, answer with fields, and let you browse by time and source

### Search beats folders for refinding, and filing effort is wasted

Whittaker et al. logged 345 long-term users and over 85,000 refinding
actions in an instrumented client. Preparatory behaviour (folders, tags)
was only 13% of accesses; tags were 1%. A folder access took 58.8 seconds
on average against 17.2 for a search and 14.0 for a sort. High filers did
use their folders, "but these preparatory behaviors are inefficient and do
not improve retrieval success." Overall 88% of finding sequences
succeeded. Scrolling was 62% of all accesses, and the authors note that
"current search-based clients ignore scrolling, the most prevalent
refinding behavior" ([Whittaker et al., CHI 2011](https://focusplandoit.com/docs/email_retrieval_study.pdf)).

This is evidence, and it points two ways at once. Asking the user to file
is a loss: filing must be automatic. But a search box alone is not enough:
people scroll a lot, so Archive needs a browsable surface too.

### People remember when and who, so date and source are the browse axes

Stuff I've Seen (Microsoft's desktop search, deployed to 234 people) found
date was "by far the most common sort order, even for people who had
best-match Rank as the default"; 25% of queries contained a person's name;
50% of query refinements were filters, mostly type and date range. The
authors conclude that "date and people names, in particular, provide rich
contextual cues for retrieval, while standard ranking functions seem less
important in the context of personal information"
([Dumais et al., SIGIR 2003](http://susandumais.com/siscore-sigir2003-final.pdf)).

For receipts the "person" is the merchant or institution. Archive's two
primary axes follow directly: who issued it, and when.

### People orienteer toward a known item instead of teleporting

Teevan et al.'s diary study found people took "small, local steps using
their contextual knowledge as a guide, even when they knew exactly what
they were looking for in advance", because it let them "specify less of
their information need" and gave "a context in which to understand their
results" ([Teevan et al., CHI 2004](https://www.microsoft.com/en-us/research/publication/perfect-search-engine-not-enough-study-orienteering-behavior-directed-search/)).

In Archive that means the path "Dell, then 2025, then the PDF" must be as
cheap as typing the perfect query. A merchant page that lists every record
from Dell in time order is that local step.

### Paperless-ngx is the closest model: correspondent, type, date issued, custom fields

Paperless-ngx describes each document with a correspondent ("the person,
institution or company that a document either originates from, or is sent
to"), a document type ("letter, bank statement, invoice, contract"), a
"date created" ("the date you bought a product, the date you signed a
contract") distinct from the date added, tags, and custom fields such as
"Invoice Number" or "Date Paid" with typed values. It has no folders on
purpose: "tags are much more versatile than folders"
([Paperless-ngx usage docs](https://github.com/paperless-ngx/paperless-ngx/blob/dev/docs/usage.md)).

Its auto-matching is instructive in both directions. The *Auto* matcher
learns from how you already labelled documents, but it "only takes
documents into account which are NOT placed in your inbox" (so it learns
only from confirmed labels), needs "a reasonable number of documents", and
needs negative examples: "If all your documents are either from 'Webshop'
or 'Bank', paperless will assign one of these correspondents to ANY new
document" ([Paperless-ngx advanced usage](https://github.com/paperless-ngx/paperless-ngx/blob/dev/docs/advanced_usage.md)).
Its recommended workflow still includes a human check: "Check that the
date of the document is correct. Paperless tries to read the date from the
content of the document, but this fails sometimes."

Paperless also warns, for its LLM features, that it "will send document
content to the AI provider you have configured, so consider the privacy
implications", which is the same line mxr already draws.

### Most of this mail is machine-generated from templates, so extraction can be reliable

Google's Juicer paper reports that over 60% of email traffic is
business-to-consumer (flight reservations, payment reminders, order
confirmations), most of it "generated by filling a template with user or
transaction-specific values", and Juicer serves "over a billion Gmail
users daily" with tasks such as bill reminders and hotel reservations
([Sheng et al., KDD 2018](https://research.google/pubs/pub46991/)).
Templates mean a per-sender extractor, once right, stays right.

Senders can also declare the data. Gmail's markup supports Order, flight,
hotel, event, restaurant, train, bus and rental car reservations, parcel
delivery and Invoice. Order requires `merchant`, `orderNumber`, `price`,
`priceCurrency` and `acceptedOffer`
([Gmail Order reference](https://developers.google.com/workspace/gmail/markup/reference/order));
Invoice carries `paymentDue`, `totalPaymentDue`, `paymentStatus`,
`accountId` and `provider`
([Gmail Invoice reference](https://developers.google.com/workspace/gmail/markup/reference/invoice)).
Senders must register with Google and pass SPF or DKIM aligned with the
From domain ([Register with Google](https://developers.google.com/workspace/gmail/markup/registering-with-google)),
so markup is present mainly from large merchants and airlines. Absence is
the common case; mxr's own extractor already says so
(`crates/deliveries/src/schema_org.rs`: "Adoption is uneven across
merchants, so this is purely opportunistic").

### TripIt and Gmail Purchases show the composite object: one trip, one order

TripIt reads confirmation emails (forwarded, or by Inbox Sync) and builds
one itinerary per trip; its parser pulls "flight numbers, times,
confirmation codes, addresses, and contact info" and either creates a trip
or updates an existing one
([TripIt Inbox Sync](https://help.tripit.com/en/support/solutions/articles/103000063336-authorizing-inbox-sync),
[TripIt via Concur](https://community.concur.com/t5/What-s-New-in-Product/Automatically-Import-Travel-Plans-with-TripIt-Inbox-Sync/ba-p/82105)).
The user never sees the eight emails (flight, hotel, car, seat change);
they see the trip.

Gmail's Purchases view, rolled out to all personal accounts in 2025,
groups "order confirmations, shipping updates, and delivery estimates"
for one purchase
([TechCrunch](https://techcrunch.com/2025/09/11/gmail-makes-it-easier-to-track-upcoming-package-deliveries),
[Thurrott](https://www.thurrott.com/cloud/326411/gmail-adds-new-purchase-tracking-view)).

### Apple Wallet surfaces a record when its date or place arrives

A Wallet pass appears on the lock screen at its relevant date or location,
and "if a pass has no location or date information, it can't trigger
automatically" ([Neatpass](https://neatpass.app/learn/wallet-pass-not-showing-lock-screen)).
In iOS 26 boarding passes add Live Activities plus "Apple Maps directions
to the appropriate airport terminal, a shortcut to the Find My app's newer
baggage tracking feature"
([MacRumors](https://www.macrumors.com/2025/06/09/ios-26-enhances-boarding-passes/)).
The lesson for an archive: some records have a moment when they stop being
filed and become needed (a flight, a concert ticket, a return-by date), and
the record should come to you then.

### Receipt capture tools extract four fields and make you confirm them

Expensify, Dext and Hubdoc all reduce a receipt to merchant, date, total
and tax (plus invoice number), then put it in a review inbox before it is
published to accounting
([Dext email-in](https://www.expensent.com/guides/email-receipts-to-dext),
[Dext vs Hubdoc](https://eightx.co/blog/dext-vs-hubdoc-receipt-capture)).
Dext's per-supplier rules get stable suppliers to "near-zero touch coding
within a month" (vendor comparison, opinion). Expensify's own docs describe
a Concierge audit that checks the merchant, date and amount against the
receipt ([Expensify](https://docs.expensify.com/using-expensify-day-to-day/concierge-receipt-audit)).

### Smart folders are saved queries, not places

Apple Mail's Smart Mailboxes "automatically organize email messages into a
single mailbox, based on criteria you specify", and deleting one deletes
none of its mail ([Apple Mail guide](https://support.apple.com/guide/mail/use-smart-mailboxes-mlhlp1190/mac)).
HEY's Paper Trail routes a whole sender there once, at screening, and
moves past mail too ([HEY Paper Trail](https://www.hey.com/features/paper-trail/)).
Both confirm that the user decides at the level of a sender or a rule, never
per message.

### Filters work when they are concrete, ordered and scoped to the content type

NN/g: filter labels must be "as concrete and precise as possible", jargon
makes filters "useless", values should be ordered logically (numbers low to
high, not alphabetical), and filter sets should be customised "for each
content or product type"
([NN/g filter categories](https://www.nngroup.com/articles/filter-categories-values/)).
Exploratory users want each click to update results; users with fixed
criteria want to set several filters and load once
([NN/g user intent](https://www.nngroup.com/articles/applying-filters/)).
A tax pull is the second kind; "what did I buy from Dell" is the first.

## 3. What fails: manual filing, lists of messages, silent wrong numbers, and opaque tracking

**Manual filing fails because the effort comes before the need.** The
Whittaker data above: folder access is three times slower than search and
does not raise success. The current Paper trail page ("Pin what matters,
sweep the rest", `08-paper-trail`) is built around a filing decision per
message, and pins (`052_message_pins.sql`) exist to protect a message from
a sweep, not to help find it later.

**A list of messages answers the wrong question.** Today's Paper trail rows
show sender, count, subject and a reason ("Here because: automated sending
domain"). None of that is what you came for: the amount, the reference,
the date of purchase, the attachment. Gmail search has the same shape, and
for an order you get the confirmation, the dispatch notice and the
delivery notice as three equal rows.

**Wrong extracted numbers do more damage than missing ones.** Expensify
users report that OCR "grabs the larger number from whichever line happens
to be cleanest" and, with two dates, "you get whichever date OCR read
first" ([Expensent troubleshooting guide](https://www.expensent.com/guides/expensify-smartscan-troubleshooting),
a third-party guide, so treat as reported experience). Paperless says the
same about dates. A wrong total in a tax export is worse than no total.

**Tracking the user cannot see or remove breeds distrust.** Gmail's
earlier Purchases page drew complaints that it was "difficult to delete
them and impossible to stop", and that removing an entry meant deleting
each source email ([gHacks, 2019](https://www.ghacks.net/2019/05/18/gmail-tracks-all-your-purchases-and-it-is-difficult-to-delete-them-and-impossible-to-stop/)).
mxr's answer is that the data is local and every record has a one-key
"not a record" that removes it without touching the mail.

**Auto-classifiers trained on too few examples over-assign.** Paperless's
own warning about Webshop and Bank. A model-only record detector on a new
mailbox would label everything automated as a receipt.

**Records that only live in a cabinet are forgotten at the moment they
matter.** A boarding pass filed under Travel is useless at the gate unless
it surfaces. This is opinion supported by Wallet's design rather than a
study.

## 4. The data: a record is a typed row built from emails, with provenance per field

### The record and its fields

| Field | Example | How derived, in order of trust |
|---|---|---|
| kind | receipt, order, booking, invoice, statement, ticket, contract, warranty, account | schema.org `@type`; sender template; subject/body keywords; model on shortlist; user |
| issuer | Dell, TAP Air, HMRC | schema.org `merchant`/`provider`/`reservationFor.provider`; contact display name for the sending domain; user rename (merges issuers) |
| issued_on | 2025-03-03 | schema.org `orderDate`; first date near "Order date", "Invoice date"; message date as fallback, flagged as fallback |
| amount, currency | 1249.00 GBP | schema.org `price`/`totalPaymentDue`; the number next to "Total"/"Amount paid"/"Grand total" (rule); model only to pick between candidates that appear verbatim |
| reference | ABC123, order 402-118 | schema.org `orderNumber`/`reservationNumber`/`confirmationNumber`; regex near "booking reference", "confirmation", "order #", "invoice no" |
| title | "XPS 14 laptop", "Lisbon, 3 nights" | schema.org `acceptedOffer.itemOffered.name`, `reservationFor.name`; subject cleaned of "Your order" boilerplate |
| span | 2025-06-12 to 2025-06-15 | reservation start/end; event date; subscription billing period |
| good_until | return by 2025-04-02, warranty to 2027-03-03, ticket valid to | explicit "return by", "warranty", "valid until" phrases via `mxr_core::natural_time`; never inferred from policy guesses |
| documents | invoice.pdf (182 KB) | `attachments` rows of the source messages, PDFs first |
| sources | 3 emails: ordered, shipped, delivered | `record_messages` provenance rows, like `delivery_messages` |
| group | Trip "Lisbon, June 2025", Order 402-118 | dedup key and grouping rules (below) |
| field provenance | amount: schema / rule / model / you | stored per field, shown on hover and in JSON |
| checked | yes/no | true when every money and date field came from schema.org or the user confirmed it |

### Composite records

- **Order:** keyed by issuer plus order number. Confirmation, dispatch,
  delivery, refund and return emails attach to one row. While the parcel
  moves, it is the existing `deliveries` row in Updates; once delivered,
  the order record carries `delivered_on` and its return window.
- **Trip:** bookings (flight, hotel, car, train, event) whose spans overlap
  or touch within 24 hours and share a destination city become one trip.
  The user can split or merge. A trip is a record of records.
- **Series:** statements and recurring invoices from one issuer and account
  (`accountId`) form a series (a year of electricity bills). The series is
  how "all my BT bills for 2025" is one click.

### What mxr already has (at `bdf997c4`)

- `crates/deliveries`: a deterministic detector (`detect`), a schema.org
  JSON-LD reader for `Order` and `ParcelDelivery`
  (`schema_org.rs`, fields: status, carrier, tracking, order number,
  merchant, ETA, items), checksum-validated tracking numbers, LLM confirm
  for shortlisted candidates (`extract.rs`), and lifecycle collapse of many
  emails into one row (`lifecycle.rs`). The `deliveries` and
  `delivery_messages` tables (`042_deliveries.sql`) are the exact pattern a
  `records` table needs: dedup key, `ON CONFLICT` upsert, provenance rows,
  non-destructive `resolved_at`/`dismissed_at`.
- `crates/daemon/src/handler/mail_kind.rs`: the sender classifier already
  knows receipt-ish local parts (`receipts`, `receipt`, `invoice`) and maps
  no-reply senders to `PaperTrail`. `places.rs` serves the Paper trail
  view, `message_pins` and sweep.
- `crates/search`: Tantivy index with `attachment_filenames`,
  `has_attachments`, `content_hints` (`has:document`, `has:spreadsheet`),
  date fields, and a Gmail-like query grammar. `SearchMode` has lexical,
  hybrid and semantic (`crates/core/src/types.rs`).
- `crates/semantic/src/lib.rs`: reads PDF attachment text with `unpdf` for
  semantic indexing, but only when the attachment has a `local_path` (no
  OCR, by design).
- `attachments` table (`001_initial.sql`): filename, mime type, size,
  `local_path`. Downloads happen on demand
  (`crates/daemon/src/handler/mod.rs`, around line 3403).
- `mxr ask` (`crates/daemon/src/handler/archive_ask.rs`): a
  citation-validated answer over retrieved messages, with filters
  (`--from`, `--after`, `--before`) and rejection of citations outside the
  retrieved set. Its header notes "semantic fallback deferred".
- `crates/core/src/natural_time` for due and validity phrases;
  `triage_cache` (`044_triage_cache.sql`) for per-message model caching by
  content hash; `calendar_invites` (`038`) for event dates.

### What mxr needs

- Extend `schema_org.rs` (or a sibling in a new `records` crate that
  depends on `mxr_deliveries` for the shared JSON-LD walk) to read `price`,
  `priceCurrency`, `orderDate`, `acceptedOffer`, `Invoice`,
  and the `*Reservation` family (`reservationNumber`, `reservationFor`,
  start and end times).
- A record detector in the `mxr_deliveries::detect` mould: schema first,
  then per-issuer templates learned from confirmed records (Juicer's
  insight), then labelled-line heuristics for total and reference, then a
  model shortlist. The model may only choose among candidate strings that
  appear verbatim in the message, the way `promises.rs` checks quotes.
- `records`, `record_messages` and `record_fields` tables (one row per
  field with value, source and quote span), plus `record_groups` for trips
  and series. Upsert by dedup key, never `INSERT OR REPLACE`.
- PDF prefetch for record emails only: download attachments of messages
  that become records so `unpdf` text is indexed and the PDF opens offline.
  Bounded by size and count; respects whatever storage limit config exists.
- Search fields for records (`kind:`, `issuer:`, `amount:>100`, `ref:`)
  either as Tantivy fields on the message doc or a small SQLite FTS over
  `records`. The second keeps the message index unchanged.
- Record export: `crates/export` today exports threads as Markdown, JSON,
  mbox or LLM context. It needs a records CSV writer and a "copy PDFs to a
  folder" step.
- IPC: `ListRecords { filter, group_by, cursor }`, `GetRecord`,
  `AnswerRecordQuery { text }`, `SetRecordField { record_id, field, value }`
  (user correction, wins forever), `DismissRecord`, `FileAsRecord {
  message_id, dry_run }`, `ExportRecords { filter, format, attachments_dir,
  dry_run }`. CLI: `mxr records` as the blueprint names it, with `list`,
  `show`, `find`, `file`, `fix`, `export`, all with `--format json`.

## 5. The proposed view: an answer box over a ledger of records, grouped by time and issuer

### Layout, desktop web

```text
+-- rail --+------------------------------------------------------------------+
| Now      |  Archive                                       1,284 records   |
| Messages |  +------------------------------------------------------------+ |
| To do    |  | / What are you looking for?   "lisbon booking"             | |
| Updates  |  +------------------------------------------------------------+ |
| Reading  |                                                                  |
|>Archive  |  ANSWER                                                          |
| Inbox    |  +------------------------------------------------------------+ |
|          |  | Booking ref  K7QX2M                         [y] copy       | |
|          |  | TAP Air Portugal . LHR -> LIS . Thu 12 Jun 2025 07:40     | |
|          |  | Part of trip "Lisbon, June 2025" (flight, hotel, 2 tickets)| |
|          |  | from schema.org markup . checked     [o] e-ticket.pdf [e] email|
|          |  +------------------------------------------------------------+ |
|          |  Also matching: Hotel Lisboa Plaza (ref 88213), Lisbon Oceanario |
|          |                                                                  |
|          |  Kind: All  Receipts  Orders  Trips  Bills  Documents   [f] more |
|          |  Year: 2026  [2025]  2024  older         Issuer: any v           |
|          |                                                                  |
|          |  2025 . MARCH                                      3 . £1,412.40 |
|          |  03 Mar  Dell           XPS 14 laptop     £1,249.00  402-118  PDF|
|          |          Order: ordered . shipped . delivered 7 Mar              |
|          |          Return by 2 Apr (passed) . Warranty to 3 Mar 2027       |
|          |  11 Mar  Octopus Energy Bill, Feb         £  128.40  A-99312  PDF|
|          |  28 Mar  Apple          iCloud+ 200GB     £    2.99  MSXK21   -  |
|          |                                       amount unchecked (model) o |
|          |  2025 . FEBRUARY                                   5 . £  611.05 |
|          |  ...                                                             |
+----------+------------------------------------------------------------------+
            footer: / ask  y copy ref  o open PDF  e email  f filter  x export
```

The answer box is the default focus. Below it is a ledger: one line per
record (not per email), grouped by month with a count and a total, newest
first, because date is how people browse personal records. Columns are
fixed so the eye can scan amounts and references down the page. A composite
record (an order) shows its stages in one line under the row. The record's
best document shows as `PDF` and opens with `o`.

Selecting a row opens the record card in the right pane, not the email:

```text
+-- Record ------------------------------------------------------+
| Dell . Receipt + order                          [e] 3 emails     |
| XPS 14 laptop, 1 item                                           |
|                                                                 |
| Paid        £1,249.00   (schema.org)                            |
| Date        3 Mar 2025  (schema.org)                            |
| Order       402-118     [y]                                     |
| Delivered   7 Mar 2025  (from the delivery email)               |
| Return by   2 Apr 2025  passed                                  |
| Warranty    to 3 Mar 2027   (from "2-year limited warranty",   |
|             you confirmed)                                      |
|                                                                 |
| [o] Invoice-402118.pdf  182 KB     [O] all files               |
|                                                                 |
| Also from Dell: 4 records since 2019                [g i]       |
| Here because: Dell sends structured order data                  |
| [c] fix a field   [d] not a record   [t] to do (e.g. claim)     |
+-----------------------------------------------------------------+
```

Hovering or focusing any field shows the sentence it came from, with the
value highlighted. An unchecked money or date field has a small open dot;
`v` marks the whole card checked.

### Layout, mobile

```text
+---------------------------------+
| Archive                    ... |
| [ What are you looking for? ]  |
|                                 |
| Coming up                       |
| Lisbon trip . in 3 days    >   |
| Return Dell laptop by Wed  >   |
|                                 |
| Receipts Orders Trips Bills >  |
|                                 |
| MARCH 2025         £1,412.40   |
| Dell                 £1,249.00 |
| XPS 14 laptop . PDF . 3 Mar    |
| ------------------------------ |
| Octopus Energy         £128.40 |
| Bill, Feb . PDF . 11 Mar       |
+---------------------------------+
```

Tapping a row opens the record card full screen with copy buttons beside
the reference and amount and the PDF as the largest target. Facets open in
a bottom tray with "Show 14 records" as the apply button, the NN/g batch
pattern, because on a phone each reload costs more.

### TUI sketch

```text
 Archive  1284 records          / lisbon booking_
 ────────────────────────────────────────────────────────────────────
 ▶ K7QX2M  TAP Air  LHR→LIS  12 Jun 2025 07:40  ✓schema   y copy  o pdf
   Trip: Lisbon, June 2025 (4)
 ────────────────────────────────────────────────────────────────────
 [all] receipts orders trips bills docs       year: 2025   issuer: *
 ── 2025-03 ───────────────────────────────── 3 records   £1,412.40
   03 Mar  Dell            XPS 14 laptop       1,249.00 GBP 402-118  pdf
   11 Mar  Octopus Energy  Bill, Feb             128.40 GBP A-99312  pdf
   28 Mar  Apple           iCloud+ 200GB           2.99 GBP MSXK21   ·?
 ── 2025-02 ───────────────────────────────── 5 records     £611.05
 ────────────────────────────────────────────────────────────────────
 / ask  y copy  o open  e email  f facets  [ ] year  x export  ? keys
```

`·?` marks an unchecked amount. The TUI would live beside `desk_lens.rs`
and `place_lens.rs` as a records lens.

### Primary actions, with proposed keys

| Key | Action | Why it is up front |
|---|---|---|
| `/` | Ask: type a query; answer card updates as you type | The job starts with a question |
| `y` | Copy the reference (or the amount with `Y`) | The most common use of a booking or order |
| `o` | Open the record's main PDF; `O` lists all files | Attachments are the proof |
| `e` | Open the source emails (the thread or all contributing messages) | The email is evidence, one step away |
| `f` | Facets: kind, issuer, year, month, amount range, has PDF, checked | Orienteering steps |
| `[` `]` | Previous and next year | Time is the main browse axis |
| `g i` | All records from this issuer | The merchant page, Teevan's local step |
| `x` | Export the current filter as CSV plus PDFs, with a dry-run preview | Taxes and expense claims |
| `c` | Fix a field (inline, natural-time for dates) | Trust needs a cheap correction |
| `v` | Mark checked | Separates verified from extracted |
| `d` | Not a record (this email or this sender) | One-key removal, no mail touched |
| `t` | Hand to To do ("claim warranty", "return by Wed") | Archive hands off when a record needs action |

These must be checked against `keymapParity.test.ts` and the existing
global keys before they are fixed.

### Rhythm: on demand, plus one push when a record's date comes up

Archive is opened when needed. It has no badge and no unread state. The
one exception follows Wallet: a record with a moment (a trip starting in
the next 72 hours, an event ticket today, a return window closing in 3
days, a warranty ending within 30 days) appears in a small "Coming up"
strip at the top of Archive and as one line on the Now desk. Return-by and
warranty-ending become To do items if the user says so (`t`), not
automatically, because most return windows pass on purpose.

### Empty and first-run states

On first run the daemon backfills records from history, newest first. The
empty state reports progress and invites a question in the same breath:

```text
Filing your records. 418 found in the last 2 years so far (of ~12,400 emails checked).
Try asking: "laptop receipt", "booking reference", "council tax 2025".
```

When a search finds nothing in records, the answer box falls back to
message search and says so ("No record matches. 3 emails mention
'warranty' from Dell"), with a key to file one of them as a record. Zero
results never leave the user stuck.

### How items enter

1. **Automatically, from detectors.** After sync, in `post_sync_fanout`
   (`crates/daemon/src/loops.rs`) where deliveries run, the record detector
   files records. Schema.org and confirmed issuer templates file silently.
   Heuristic and model extractions file too, but their money and date
   fields stay unchecked.
2. **From Updates, when a delivery completes.** A `deliveries` row that
   reaches `delivered` hands its order to Archive, carrying the delivered
   date. Updates lets go of it; Archive keeps it.
3. **From To do, when a to-do is ticked off.** "Paid the council tax"
   offers to file the receipt or the confirmation email as a record, with
   `paid_on` set to the tick-off time. This is the blueprint's handoff.
4. **From Messages, by hand.** A contract or a quote from a person is filed
   with `F` (file as record), prefilled by the detector, shown as a
   dry-run card before it saves.
5. **Per sender.** "Always file this sender" is the existing per-sender
   aspect override in the blueprint, HEY-style.

### How items are retrieved

- **Answer first.** A query that matches a record field returns the field
  itself on the answer card. "Lisbon booking ref" resolves from the
  record's `reference` without any model. Only when no record field
  matches does it fall back to `mxr ask`, which already validates
  citations, and the card says the answer came from a model and links the
  quoted email.
- **Browse second.** The ledger by month, facets, the issuer page, and the
  trip and series pages. Sorted by issued date by default, following SIS.
- **Search third.** Full message search stays one key away (`2` today in
  the footer) for things that never became records.

### Micro-interactions worth having

- Copy feedback that shows what was copied ("K7QX2M copied") so the user
  can read it aloud to a call centre without switching windows.
- The month header total updates as facets change, so "how much did I
  spend at Amazon in 2025" is answered by the header, not a calculator.
- Hover on any value shows the source sentence with the value highlighted;
  a mismatch is visible at a glance.
- Correcting an issuer name ("AMZN Mktp UK" to "Amazon") offers to apply to
  all records from that sender, and future ones.
- The export preview shows row count, total, how many rows are unchecked,
  and missing PDFs before writing anything (mxr's dry-run rule from
  `AGENTS.md`).
- A trip card lays its bookings out as a day-by-day strip, the TripIt
  shape, with each booking's reference copyable.

### Privacy

All extraction is local. Rules and schema.org parsing need no model. Model
extraction in the background follows the blueprint's stricter rule: a
loopback endpoint only, unless the feature-specific cloud opt-in is set.
Records carry money, addresses and account numbers, so activity records
for record writes carry ids and counts only, never fields, and the CSV
export writes only where the user points it.

## 6. How this differs from a list of emails

- The row is a record, not a message. Three emails about one Dell order are
  one row; a trip's eight confirmations are one trip.
- The row shows the fields you came for (issuer, what, amount, reference,
  date issued, PDF), not sender, subject and snippet. The subject line is
  not on the row at all.
- The date on the row is the date of the transaction, not the date the
  email arrived. A booking made in January for June files under January
  and shows under "Coming up" in June.
- The primary action is copy or open the document, not reply or archive.
  The email is behind `e`, as evidence.
- A query returns an answer card with the field, then the records, then
  emails as a fallback. A mail client returns emails.
- Grouping is by month with totals and by issuer, so the list is also a
  ledger you can read for spending.
- Nothing is ever filed by the user one message at a time. The user
  corrects at the level of a field, a record or a sender.
- Archive has no "done". Records do not leave; they can only be dismissed
  as not a record.

## 7. Open questions and risks

- Extraction accuracy on BK's real mail is unknown. Phase 5 of the
  blueprint has `mxr modes eval`; Archive needs the same for fields:
  sample 200 detected records, label issuer, amount, date and reference,
  report per-field precision by source (schema, template, rule, model).
  Without this number the "checked" marker is the only safety net.
- Is the record table worth it over computed views? The blueprint computes
  mode membership at read time. Records hold user corrections and grouping
  that outlive the email, as to-dos do, which argues for a table, but it is
  a second source of truth beside the message.
- Should `deliveries` become a kind of record, or stay separate and link by
  order number? Merging saves code; keeping them apart keeps Updates'
  in-transit logic simple.
- PDF prefetch costs disk and sync time. What cap, and does BK want PDFs
  offline at all?
- Many receipts carry the total only inside a PDF or an image. Without OCR
  (deliberately absent in `crates/semantic`) those amounts stay empty. Is
  `unpdf` text enough, and is OCR ever acceptable locally?
- Trip grouping by overlapping dates and city will mis-group business
  travel that chains cities. How often does BK travel that way?
- Currency: totals across currencies in a month header need either
  separate totals per currency or a conversion source; a conversion source
  is a network call.
- Is "Coming up" one feature too many? It overlaps To do and the Now desk.
  It earns its place only if trips and tickets are common in BK's mail.
- Tax packs: "export 2025" is clear for receipts and invoices. Which kinds
  count for BK's taxes (UK self-assessment?) is his call, not a default.
- Retention: should Archive ever suggest deleting old records (e.g.
  statements older than seven years), or never touch them?
- Key choices (`y`, `o`, `f`, `x`, `d`, `t`) may clash with global keys;
  check `keymapParity.test.ts`.

## 8. Sources

Research on refinding:

- Whittaker, Matthews, Cerruti, Badenes, Tang. "Am I wasting my time
  organizing email? A study of email refinding." CHI 2011.
  https://focusplandoit.com/docs/email_retrieval_study.pdf (abstract and
  record: https://www.semanticscholar.org/paper/b946e48d2d4872631e7245f67d5cd5f954b7c6a7)
- Dumais, Cutrell, Cadiz, Jancke, Sarin, Robbins. "Stuff I've Seen: A
  System for Personal Information Retrieval and Re-Use." SIGIR 2003.
  http://susandumais.com/siscore-sigir2003-final.pdf
- Teevan, Alvarado, Ackerman, Karger. "The Perfect Search Engine Is Not
  Enough: A Study of Orienteering Behavior in Directed Search." CHI 2004.
  https://www.microsoft.com/en-us/research/publication/perfect-search-engine-not-enough-study-orienteering-behavior-directed-search/
- Sheng, Tata, Wendt, Xie, Zhao, Najork. "Anatomy of a Privacy-Safe
  Large-Scale Information Extraction System Over Email." KDD 2018.
  https://research.google/pubs/pub46991/

Product documentation:

- Paperless-ngx usage: https://github.com/paperless-ngx/paperless-ngx/blob/dev/docs/usage.md
- Paperless-ngx matching: https://github.com/paperless-ngx/paperless-ngx/blob/dev/docs/advanced_usage.md
- Gmail markup, Order: https://developers.google.com/workspace/gmail/markup/reference/order
- Gmail markup, Invoice: https://developers.google.com/workspace/gmail/markup/reference/invoice
- Gmail markup, registration: https://developers.google.com/workspace/gmail/markup/registering-with-google
- TripIt Inbox Sync: https://help.tripit.com/en/support/solutions/articles/103000063336-authorizing-inbox-sync
- TripIt Inbox Sync announcement (Concur): https://community.concur.com/t5/What-s-New-in-Product/Automatically-Import-Travel-Plans-with-TripIt-Inbox-Sync/ba-p/82105
- HEY Paper Trail: https://www.hey.com/features/paper-trail/
- Apple Mail Smart Mailboxes: https://support.apple.com/guide/mail/use-smart-mailboxes-mlhlp1190/mac
- Expensify Concierge receipt audit: https://docs.expensify.com/using-expensify-day-to-day/concierge-receipt-audit
- NN/g, filter categories and values: https://www.nngroup.com/articles/filter-categories-values/
- NN/g, user intent affects filter design: https://www.nngroup.com/articles/applying-filters/

Press, reviews and third-party guides (reported experience, weaker evidence):

- Gmail Purchases view: https://techcrunch.com/2025/09/11/gmail-makes-it-easier-to-track-upcoming-package-deliveries and https://www.thurrott.com/cloud/326411/gmail-adds-new-purchase-tracking-view
- Gmail Purchases complaints: https://www.ghacks.net/2019/05/18/gmail-tracks-all-your-purchases-and-it-is-difficult-to-delete-them-and-impossible-to-stop/
- iOS 26 boarding passes: https://www.macrumors.com/2025/06/09/ios-26-enhances-boarding-passes/
- Wallet relevance by date and location: https://neatpass.app/learn/wallet-pass-not-showing-lock-screen
- Expensify extraction errors: https://www.expensent.com/guides/expensify-smartscan-troubleshooting
- Dext email-in: https://www.expensent.com/guides/email-receipts-to-dext
- Dext vs Hubdoc: https://eightx.co/blog/dext-vs-hubdoc-receipt-capture

Not verified first-hand in this pass: Google Travel trip cards, Clean
Email, Raycast and Spotlight answer behaviour, and Evernote and Notion
filing debates. The search budget ran out before those; the proposal does
not rest on them.
