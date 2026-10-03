# A one-click pay link is deferred until a design survives review

**Status:** deferred · **Found:** 2026-10-03 · **Decision:** D117 (amended),
blueprint 22 "Every view follows the same trust rules"

To do's first design had a "Pay on camden.gov.uk" button that opened the
email's pay link in one click when a gate passed. A pay link in an email is
the classic phishing payload, and two review rounds of `feat/todo-mode`
found a way past every version of the gate. Phase 1 therefore ships no
one-click external action: a to-do's action opens the source email in mxr
with the link it is about highlighted, labelled "Open email to pay" (or to
verify, sign, update payment), and shows the link's domain. mxr never opens
the link from the row. The link picker stays, for the highlight.

## What the reviews found

Round 1, against "DMARC pass, link domain matches the sender's, earlier mail
from that domain":

1. **Forged authentication results.** The gate read the first
   `Authentication-Results` header that said `dmarc=pass`. A sender can add
   that header themselves, below the one the receiving provider adds.
2. **Primed history.** "Earlier mail from the domain" is satisfied by an
   attacker who sends one harmless message before the phish, from a
   lookalike domain they control.

Round 2, against "the provider's own result (topmost header with the
provider's authserv-id, `header.from` matching), mail from the domain at
least 30 days old plus either mail you sent it or 3 messages over 60 days,
and no confusable or one-edit lookalike of a domain you know":

3. **Backdated IMAP history.** IMAP message dates come from the
   sender-controlled `Date` header, so three backdated messages establish a
   domain at once.
4. **Trusted ids shared across accounts.** The trusted authserv-ids were
   one list for every IMAP account, so a forged Fastmail-style header in a
   different IMAP account was accepted.
5. **Subdomains.** `evil.camden.gov.uk` matched on the registrable domain
   and inherited `camden.gov.uk`'s history.
6. **Lookalike gaps.** The guard missed history built from one message plus
   a reply, Cyrillic skeletons it didn't fold, and letter transpositions.
7. **Redirects and click trackers.** An open redirect or tracker on the
   biller's own domain passes the domain check and sends the browser
   anywhere.

## What a design has to answer before it returns

- History from server-side receipt time (Gmail's `internalDate`, IMAP
  `INTERNALDATE`), never the `Date` header.
- Trusted authserv-ids per account, from the provider the account syncs
  with, never a shared list.
- An exact host match for the link, not the registrable domain.
- A lookalike check that folds full Unicode confusables (UTS #39 skeletons)
  and transpositions, against every domain with history.
- Resolving or refusing redirects and trackers before the link is shown as
  safe.
- A review that tries each of the seven findings above against it.

## Completion check

A design note answers each point above, a review round finds no way to put
an attacker's link behind a one-click action, and D117 and blueprint 22 are
amended again before any code ships.
