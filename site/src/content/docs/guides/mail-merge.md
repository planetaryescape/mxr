---
title: Mail merge
description: Personalised drafts from one template and a list of records, using the mxr-mailmerge companion. Drafts by default, never sends by itself.
---

Sending the same designed email to forty people with a different link each is a
real need. It is also not an email client's job.

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
a draft, so an interrupted run resumes instead of duplicating. Sends are marked
per record, so a crash mid-send cannot re-send anyone. After a partial failure:

```bash
mxr-mailmerge send <campaign-id> --retry-failed --yes
mxr-mailmerge status <campaign-id>
```

## What it does not do

No tracking pixels. No open or click tracking. No analytics, no engagement
reporting, no contact scoring. It renders templates and asks mxr to make drafts.

If you want a marketing platform, use a marketing platform.
