# Automatic breakthrough to To do is deferred

Updates shows a new sign-in, a failed payment or a delivery problem at the
top of Needs a look as a suggested to-do, with its reason. Only the user's
`t` turns one into a to-do. The first build created the to-do on arrival;
two review rounds found ways to make mxr create a task from mail an
attacker wrote, so automatic creation is off until a design survives
review. This follows the pay link decision
([one-click-pay-link.md](one-click-pay-link.md)).

## What the first build did

On sync, a message whose rules said `needs_you` became an open to-do
("Check new sign-in to Google"), claimed by a dedup key so a re-sync never
made a second. A source tuned to `breakthrough` sent every message.

## Round 1 findings

- Any sender could create a to-do: the subject is mail-controlled text, so
  `security@evil.example` with "New sign-in" made a task.
- The claim was keyed by subject template, so differently worded alerts of
  one kind made several tasks and one kind could suppress another.
- Backfilled or archived mail (not in the inbox) created tasks.
- A code mentioned in a sign-in alert gave it a 10-minute window, so the
  alert expired almost at once.

## Round 2 findings, after a DMARC and prior-mail gate

The gate required the receiving provider's `Authentication-Results` to say
`dmarc=pass` for the sender, and mail from that source before today.

- A forged `Authentication-Results` lower in the headers was accepted when
  the topmost one came from an authserv-id the account didn't trust.
- The candidate message counted as its own prior mail when it was dated
  before "today", so a first-time sender passed.
- Display-name branding: an attacker's own authenticated domain could send
  as "Google", and the task read "Check new sign-in to Google".
- The claim day was a UTC day, so alerts either side of UTC midnight made
  two tasks for one local day.
- Backfill order: older mail classified after newer mail could create a
  task for an alert the user had already dealt with.
- Outlook results carry no authserv-id, so Outlook accounts failed the gate
  silently with no explanation to the user.

## What a design must answer before automatic creation returns

1. Which `Authentication-Results` header is the provider's, for every
   provider mxr supports, and what happens when it is missing, duplicated
   or below a forged one. The answer must not depend on header order an
   attacker controls.
2. How "a source you already know" is established without the candidate
   counting itself, across backfill and re-sync, and in the user's time
   zone.
3. How the task shows who really sent it, so a borrowed display name can
   never read as the real brand. (Needs a look and `t` titles already show
   the sending host beside the name.)
4. How the claim works per source, alert kind and local day, and survives
   rules-version changes without re-creating tasks.
5. What the user sees when an alert is not trusted, per provider, so
   silence never looks like safety.
6. An eval on real mail showing false automatic tasks below an agreed
   threshold, as D117 requires for To do's badge.
