---
title: Run a mail merge
description: Create, review, send, and schedule personalised drafts from structured recipient records.
---

Use `mxr-mailmerge` to create, review, send, or schedule one personalised draft
per recipient record. Each message can share a design while carrying its own
recipient, text, and links.

`mxr-mailmerge` is a separate executable that renders one message per record and
asks mxr to save each as a draft. mxr keeps owning mail: accounts, validation,
safety checks, providers, sending. The companion owns interpretation and
orchestration: templates, records, batch state.

## Why it is a separate binary

mxr's core is mail truth and authority. Campaigns, template engines, CSV
parsing, and per-recipient retry state are none of those things, and putting
them in the daemon would have grown the surface that has to be correct for
ordinary email to work.

It is also deliberately **not a plugin**. There is no plugin API, no loader, no
registry, no `mxr plugins install`. `mxr-mailmerge` is an ordinary program that
shells out to `mxr` and reads its JSON — the same thing you could do from a
shell script, done carefully. That is the [shell hooks over plugin
systems](/guides/architecture/) principle applied.

Concretely, the companion:

- never opens mxr's SQLite database
- never receives Gmail, SMTP, or Outlook credentials
- never talks to a mail provider
- cannot bypass mxr's HTML validation or pre-send safety pipeline, because
  every draft goes through `mxr compose`

The boundary is enforced in CI: `scripts/check_architecture_boundaries.sh`
requires `mxr-mailmerge` to have an empty set of internal dependencies.

## Installing

`mxr-mailmerge` ships in the release archive, so Homebrew and `install.sh` both
put it on your PATH alongside `mxr`:

```bash
brew upgrade mxr
mxr-mailmerge --version
```

Installing from source with `cargo install` is per-package, and the companion is
a separate package by design, so it needs its own line:

```bash
cargo install --git https://github.com/planetaryescape/mxr --locked mxr-mailmerge
```

## Workflow

Preview first. Nothing is created.

```bash
mxr-mailmerge draft \
  --account notto \
  --subject-template subject.txt \
  --html-template message.html \
  --text-template message.txt \
  --data recipients.json \
  --inline notto-logo=assets/notto-logo.png \
  --dry-run
```

Create the drafts:

```bash
mxr-mailmerge draft ... --yes
```

Review them as ordinary mxr drafts — because that is what they are:

```bash
mxr drafts --account notto --format json
```

Sending is a separate command, and confirmed separately:

```bash
mxr-mailmerge send campaign-20260729-140301 --dry-run
mxr-mailmerge send campaign-20260729-140301 --yes
```

To schedule the whole campaign, add one `--at` value to both the preview and
confirmed commands. The companion asks mxr to resolve that value once, then
schedules every personalised draft for the same absolute instant:

```bash
mxr-mailmerge send campaign-20260729-140301 \
  --at 2026-08-05T09:00:00+01:00 \
  --dry-run
mxr-mailmerge send campaign-20260729-140301 \
  --at 2026-08-05T09:00:00+01:00 \
  --yes
```

Check the resolved instant in the preview before confirming:

```text
Campaign campaign-20260729-140301 — dry run, nothing sent or scheduled
  would schedule: 40 message(s)
  delivery time:  2026-08-05T08:00:00Z
  already scheduled: 0
  already sent:      0
```

That example is 09:00 in London while British Summer Time is active. Natural
forms accepted by `mxr send --at`, such as `tomorrow 9am`, also work. Use an
RFC3339 offset when the time zone must be explicit.

If mxr rejects the time, verify it through the same non-mutating parser before
trying the campaign again:

```bash
mxr send-time person@example.com \
  --account notto \
  --at 2026-08-05T09:00:00+01:00 \
  --format json
```

An invalid or past time stops the campaign before any draft is scheduled.

Scheduling preserves the mail-merge privacy boundary: each recipient still
has an independent, single-recipient draft. It never creates one shared or
group-addressed email.

Without `--yes`, both `draft` and `send` refuse and tell you the count they
would have acted on. There is no flag combination that makes `draft` send.

## Records

CSV, JSON, or JSONL, inferred from the extension. Every record needs a `to`
property; everything else is yours to use in templates.

```json
[
  {
    "to": "person@example.com",
    "first_name": "Dumi",
    "product_definition_url": "https://example.com/product-definition/access?t=opaque-token"
  }
]
```

The whole batch is validated before anything is created. It fails, entirely, on:

- a missing or unparseable `to`
- a duplicate recipient (case-insensitive)
- a CR or LF in an address or a rendered subject — header injection
- any unresolved placeholder or missing property

Half a campaign is worse than none, so there is no partial-success mode at
render time.

## Templates

```html
<p>Hi {{ first_name }},</p>
<p><a href="{{ product_definition_url }}">Read the product definition</a></p>
```

Templates are data templates, not programs. They run under
[minijinja](https://docs.rs/minijinja/) with:

- **strict undefined** — a missing property fails the batch rather than
  rendering "Hi ,"
- **HTML escaping on by default** — a property value containing `<script>`
  becomes text, not markup
- **no loader installed** — `{% include %}`, `{% import %}` and `{% extends %}`
  have nothing to resolve and are rejected up front

There is no shell access, no filesystem access, no network access, and no raw
HTML interpolation.

## Privacy

Each recipient gets their own draft with exactly one recipient. Recipient A's
personalised link cannot appear in recipient B's message, because each is
rendered from only its own record.

Property values may be opaque access tokens, so they are treated as secrets:

- the campaign manifest stores a **hash** of each record, never its values
- summary output prints addresses and subjects only
- rendered bodies are written to a temp file, handed to mxr, and deleted
- mxr's own activity log records the content kind and counts, never bodies

## Resuming and retrying

State lives in `.mxr-mailmerge/<campaign-id>.json`, written after every record.
Rerunning `draft` with the same `--campaign-id` skips records that already have
a draft. For an initial send or schedule, and for an explicitly permitted retry
after confirmed `not_accepted`, `mxr-mailmerge` previews the saved draft and
durably records its account, reviewed revision, operation ID and, for a
schedule, resolved delivery time before invoking `mxr`. A retained command
reuses its saved identity while it checks the outcome; it does not create a new
review for the same attempt.

Immediately after an immediate send, the companion queries the exact reviewed
account and operation ID. If the response was lost, run `status` to see the
uncertain record, then check the account-scoped draft alias. On a later campaign
run for an immediate send, reconciliation starts with that alias and checks the retained operation ID
when needed. The daemon can retain an accepted result after
deleting the draft, so the campaign can reconcile it without sending again:

`mxr-mailmerge status CAMPAIGN_ID --json` prints the campaign manifest. Copy the
affected record's `expected_workspace` unchanged for either manual outcome
query; do not recapture the owner from the currently selected daemon.

```bash
mxr-mailmerge status CAMPAIGN_ID --json
mxr send-outcome 'draft:<draft-id>' --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_MANIFEST>' --format json
mxr-mailmerge send <campaign-id> --yes
```

`unknown` or an outcome-query failure remains visible as uncertain; the
companion does not automatically retry it. Re-running checks the draft-scoped
outcome and retained operation. If an operation is still `unknown`, it does not
submit another send. If no outcome claim exists, the companion may replay the
same reviewed identity; it does not create a new one. A confirmed
`not_accepted` result is a failed record. Retry only after review with the
explicit command:

```bash
mxr-mailmerge send <campaign-id> --retry-failed --yes
```

That retry takes a fresh preview and stores a new operation ID before it acts.
The campaign manifest is a local recovery checkpoint, not an end-to-end
exactly-once delivery guarantee.

Scheduling has a separate acknowledgement boundary. If the schedule command's
response is lost, the record remains `schedule_uncertain` with its reviewed
delivery time. Check the draft-scoped outcome under the manifest account and
inspect `mxr-mailmerge status`; do not treat a missing result as proof that the
schedule was rejected or automatically schedule it again:

```bash
mxr send-outcome 'draft:<draft-id>' --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_MANIFEST>' --format json
mxr-mailmerge status <campaign-id>
```

After a lost schedule acknowledgement, retry only when the retained schedule
attempt is confirmed `not_accepted` and an operator explicitly retries; an
initial schedule does not require a previous rejection. The status command
keeps a lost schedule acknowledgement separate from a confirmed schedule.
Existing manifests without the newer optional review fields remain readable,
but they cannot recreate a historical operation ID or scheduled delivery time
that was never checkpointed. Reconcile any earlier attempt before retrying. If
no reviewed schedule was saved, a retry needs an explicit `--at` value; without
it the command requests an immediate send:

```bash
mxr-mailmerge send <campaign-id> --retry-failed --at "$SCHEDULE_AT" --yes
```

For a retained scheduled attempt, query its draft alias and do not retry when
the result is missing or uncertain. Reconciliation should happen before
refining or copying an uncertain draft: refinement creates a separate draft,
and a later send of that copy is a separate submission. The global last-send
query can still refer to a different draft, so use the scheduled draft alias.
The manifest checkpoint reduces duplicate effects when its identity is retained.

## What it does not do

No tracking pixels. No open or click tracking. No analytics, no engagement
reporting, no contact scoring. It renders templates and asks mxr to make drafts.

If you want a marketing platform, use a marketing platform.
