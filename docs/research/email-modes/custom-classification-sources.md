# Primary sources: custom email classification

Research for the design question: should user-defined classification into the
existing Messages, To do, Updates, Reading and Archive modes be an automation
rule action, a separate classification primitive, or both? Also, should users
be able to define entirely new categories?

Sources were read on 2026-10-09. Code/design context is `origin/main` at
`163bf2d7301a4d9759761ecc0c5bf2eb462c332a`. The root researcher fetched the
remote over HTTPS and verified both `origin/main` and this worktree HEAD.
The delegated researcher’s separate SSH fetch failed; it did not invalidate
the root’s remote verification. The source facts below are
separated from design inferences.

## Existing systems expose different semantics

### Gmail filters combine matching with side effects; labels are separate metadata

**Evidence.** Gmail’s API models a filter as `criteria` plus an `action`. Criteria
include sender, recipient, subject, Gmail search query, attachment presence and
message size. The API explicitly says filters apply to individual messages,
not whole threads. Actions can add/remove labels or forward; the guide documents
removing `INBOX` as archive and applying a user label as tagging. Its guide says
only one user-defined label is allowed per filter. Separately, Gmail labels
have a many-to-many relationship with messages and threads, and Gmail has
reserved system labels such as `INBOX` and `CATEGORY_UPDATES`.

Sources: [Gmail filter API resource](https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.settings.filters),
[Manage Gmail filters](https://developers.google.com/workspace/gmail/api/guides/filter_settings),
[Manage labels](https://developers.google.com/workspace/gmail/api/guides/labels).

**Inference.** Gmail establishes that a rule can *apply* classification metadata,
but a label is not itself the rule. It also separates built-in categories from
user labels. Reusing an automation “then” action for placement is familiar, but
would not answer whether a placement is a durable, user-correctable classification
or a one-time side effect. The product should make that distinction explicit.

### Fastmail rules are a friendly layer over Sieve routing actions

**Evidence.** Fastmail says its rules system generates Sieve and its advanced
editor supplements the generated rules. Sieve tests conditions and executes
actions. Fastmail documents `fileinto "INBOX.foldername"` as storing the message
in a folder; `stop` prevents later script rules from acting on that message.
`fileinto :copy` is shown as a way to file to another folder while keeping the
original. Fastmail recommends trying a potentially destructive rule by filing
to a folder before switching to `discard`.

Sources: [Using Sieve scripts in Fastmail](https://www.fastmail.help/hc/en-us/articles/1500000280481-Using-Sieve-scripts-in-Fastmail),
[How to use Sieve](https://www.fastmail.help/hc/en-us/articles/360060591373-How-to-use-Sieve),
[Sieve examples](https://www.fastmail.help/hc/en-us/articles/360058753794-Sieve-examples).

**Inference.** A then-action is a good fit for deterministic routing on arrival.
But folder filing has storage/disposition effects and rule-order/stop behavior;
that is materially different from adding an item to a semantic view while
preserving other aspects. If mxr’s modes can overlap, “classify as To do” should
not inherit Sieve’s implied move/stop behavior without stating it.

### HEY makes destination choice and correction scope visible

**Evidence.** HEY asks users to choose for each sender whether new mail goes to
the Imbox, Feed or Paper Trail. A user can also move one email to Paper Trail.
Changing a sender’s destination later moves that contact’s existing email to the
new destination as well. HEY labels are a distinct feature: a label tags a thread,
can be automatically added for a contact’s new email, and older conversations
can be bulk-labeled separately. HEY's product explanation describes the three
places as different attention destinations (immediate, casual reading,
transactional reference), not as arbitrary subject labels.

Sources: [Paper Trail](https://help.hey.com/article/787-paper-trail),
[Labels](https://help.hey.com/article/884-labels),
[How HEY works](https://www.hey.com/how-it-works/).

**Inference.** HEY is the clearest precedent for two correction commands with
honest scope: “move this email” and “change where this sender’s emails go.” It
also demonstrates that destination choice can be a durable sender preference,
while labels remain orthogonal metadata. Its retroactive sender change is a
specific policy users should be told about, not an assumed consequence of every
rule edit.

### Shortwave separates inbox views from labels and automation

**Evidence.** Shortwave documents Inbox Splits as customizable views based on
importance, labels, senders and search queries. Labels sync with Gmail and tag
threads; a split is a search-powered inbox view, not a label. Gmail filters or
Shortwave auto-apply rules can apply labels used by a split, and users can
manually move messages between splits. Splits show only messages still in the
inbox; “Done” archives a message without changing read state. Shortwave also
documents reapplying filters to sample messages to inspect actions and tune
rules.

Source: [Customize your Shortwave settings](https://www.shortwave.com/docs/guides/customize-your-shortwave-settings/).

**Inference.** This supports custom search views composed from classification metadata.
It does not establish that mxr’s richer workflow modes should themselves be
implemented as arbitrary search views. Rules can maintain metadata used by a view. The view
and the rule should remain separately understandable: matching a split does not
necessarily mean a message was moved, and archiving removes it from all splits.

## Protocols distinguish message content from mailbox metadata

**Evidence.** RFC 5322 defines an Internet message as header fields and an
optional body; its fields are syntactic message content, with defined fields
and optional extension fields. It does not define a standard field for a
personal inbox category. In IMAP, RFC 9051 defines flags as named attributes
associated with a message and distinguishes system flags (`\Seen`, `\Flagged`,
etc.) from keywords. A keyword is server-defined; servers *may* permit clients
to define new keywords. In JMAP, RFC 8621 says mailbox and email metadata are
not derived by parsing the message; a mailbox is analogous to a folder or label,
and an email’s `mailboxIds` set identifies membership. JMAP keywords are shared
with IMAP and users may add arbitrary keywords within IMAP-compatible character
constraints.

Sources: [RFC 5322](https://datatracker.ietf.org/doc/html/rfc5322) §§2, 3.6.8;
[RFC 9051](https://datatracker.ietf.org/doc/html/rfc9051) §2.3.2;
[RFC 8621](https://datatracker.ietf.org/doc/html/rfc8621) §§2, 4.1.1.

**Inference.** A custom category belongs in mxr’s local message/mode metadata,
not in rewritten RFC 5322 content. Protocol precedent supports both mailbox
membership and user-defined keywords, but these are different data shapes:
membership is one or more locations/views, while a keyword is an extensible tag.
For a local-first client, mxr can keep its own classification independent of
provider-specific labels/keywords and synchronize those only where a provider
supports them.

## UX lessons for the architecture decision

1. Keep **match criteria**, **classification/membership**, and **actions** as
   concepts the user can tell apart. Gmail/Shortwave show rules can apply labels;
   HEY and JMAP show that placement/metadata can also exist independently.
2. Make correction scope explicit: this message, this sender going forward, or
   a rule applied to matching messages. State whether changing a rule reclassifies
   existing mail; HEY’s documented retroactive behavior is one clear precedent.
3. Do not equate a custom label with a new mode. Labels are tags; modes/splits
   are destinations or views with their own job and behavior. Shortwave’s split
   can be composed from labels, while HEY deliberately offers a fixed set of
   destination semantics.
4. If entirely new user categories are considered, define their view and
   lifecycle semantics separately from adding a category value. Protocols allow
   arbitrary keywords, but that alone does not establish that users need
   first-class custom workflow modes.
