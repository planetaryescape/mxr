---
title: Track deliveries
description: See what packages are arriving and when, pulled automatically from your mail.
---

mxr watches incoming mail for shipping notifications, collapses the many emails of one order (confirmation → shipped → out for delivery → delivered) into a single tracked delivery, and shows you what's on the way. Detection runs automatically as mail syncs — you browse and manage the results.

## How detection works

Three stages, all local-first:

1. **Heuristic shortlist** — a deterministic, offline pass over each new message: carrier/merchant senders, shipping subject lines, schema.org `ParcelDelivery`/`Order` markup, and **checksum-validated** tracking numbers (UPS, FedEx, USPS, DHL, Amazon, S10 international, …).
2. **LLM confirm + extract** (optional) — shortlisted-but-uncertain candidates go to your configured [LLM](/guides/llm-features/) to confirm it's really a shipment and pull out merchant, carrier, items, and ETA. Any tracking number the model returns is re-checked against the checksum library before it's trusted.
3. **Lifecycle** — emails are correlated (by tracking number, then merchant + order number) into one delivery whose status only ever advances. A "delivered" email resolves it automatically.

It optimizes for precision: a string that merely *looks* like a tracking number inside an unrelated email (a finance code, a hosting receipt) won't create a delivery on its own — it needs a real shipping signal or LLM confirmation.

:::note[Detection is on by default]
Set `[deliveries].enabled = false` in your [config](/reference/config/) to turn it off. The LLM step only runs when an LLM is [configured](/guides/llm-features/); without one, detection falls back to heuristics plus checksum-valid tracking numbers.
:::

## See what's arriving

```bash
mxr deliveries list
```

What you get: a table of in-flight deliveries (status, merchant, carrier, ETA, tracking number), newest activity first. Already-delivered and dismissed rows are hidden.

Switch the slice with `--filter`:

```bash
mxr deliveries list --filter delivered    # resolved, recent
mxr deliveries list --filter all          # everything except dismissed
mxr deliveries list --filter dismissed    # false positives you hid
```

## Inspect one delivery

```bash
mxr deliveries get DELIVERY_ID --format json
```

What you get: the full record — `merchant`, `carrier`, `tracking_number`, `tracking_url`, `order_number`, `status`, `eta_until`, `items`, `confidence`, `source` (`schema` / `llm` / `heuristic`), and `message_ids` (the source emails that built it).

Get a delivery's id from `mxr deliveries list --format json`.

## Resolve or dismiss

A delivery resolves itself when a "delivered" email arrives. Close one out by hand, or hide a false positive:

```bash
mxr deliveries resolve DELIVERY_ID    # mark delivered/done; leaves the active list
mxr deliveries dismiss DELIVERY_ID    # hide a misfire; kept under --filter dismissed
```

Both are single-row, non-destructive — the row and its provenance are retained.

## Backfill existing mail

Detection only sees new mail going forward. Catch up on what's already in your store, previewing first:

```bash
# Preview only — no writes, no LLM calls:
mxr deliveries scan --since-days 90 --dry-run
```

What you get: `{ "scanned": 941, "created": 23, "updated": 0, "shortlisted": 44, "dry_run": true }` — how many messages were examined, how many deliveries a real run would create, and how many would be sent to the LLM.

Then run it for real (the LLM step makes it slower over wide windows, so it streams to completion rather than timing out):

```bash
mxr deliveries scan --since-days 90
```

## In real life

- **What's arriving this week, soonest first:** `mxr deliveries list --format json | jq -r 'map(select(.eta_until)) | sort_by(.eta_until) | .[] | "\(.eta_until[0:10])\t\(.merchant // .carrier)\t\(.status)"'` — a date-sorted arrival board.
- **Which orders shipped but haven't landed:** `mxr deliveries list --filter active --format json | jq -r '.[] | select(.tracking_number) | "\(.merchant)\t\(.tracking_number)"'` — merchant + tracking for everything in transit.
- **Audit detection before trusting it:** `mxr deliveries scan --since-days 30 --dry-run` — see the create/shortlist counts on a known window without writing a thing.
- **Clear a stale misfire:** `mxr deliveries dismiss DELIVERY_ID` — hides one bad row without affecting the email.

## Agent prompts that work

```text
"Run `mxr deliveries list --format json` and tell me which packages are
arriving in the next 3 days, with merchant and carrier. Don't resolve or
dismiss anything — just report."
```

```text
"Use `mxr deliveries list --filter active --format json` to find any
delivery whose ETA has already passed but isn't marked delivered, and
list its merchant, tracking number, and source email ids from
`mxr deliveries get <id>`."
```

## In the apps

- **TUI:** press `7` for the **Deliveries** tab. `j`/`k` to move, `r` to resolve, `d` to dismiss, `D` to cycle the active/delivered/all filter, `g` to refresh. See [keybindings](/reference/keybindings/).
- **Web:** the **Deliveries** page in the sidebar shows the same list with resolve/dismiss buttons and a link to each delivery's thread. See [Web app](/guides/web-app/).

## See also

- [LLM features](/guides/llm-features/) — the optional confirm-and-extract step and how to configure it
- [Config](/reference/config/) — the `[deliveries]` section and the `delivery_extraction` LLM override
- [CLI: deliveries](/reference/cli/deliveries/) — every flag
- [Security & privacy](/guides/security-and-privacy/) — detection is local; nothing is sent to carriers
