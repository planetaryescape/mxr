# `mxr unsubscribe <address>` tries every message from the sender

Status: open. Found 2026-10-22 while documenting unsubscribe receipts (docs sweep for 0.6.120–0.6.150).

`mxr unsubscribe --help` says an address "is rewritten to `--search "from:alice@example.com"` and acts on the most recent match". On the demo, `mxr unsubscribe <promotional sender> --dry-run --format json` reports `{"requested": 354, "selected_messages": 354}`: the address selects every message from that sender, and a real run tries each one. Per-message receipts stop the same message being unsubscribed twice, but they don't stop one request per message going to the same sender.

The help text contradicts itself. It promises the most recent match, then describes scoping "an address-wide unsubscribe" with `--search`.

Decide which is intended:
- **One request per sender (the help's first sentence).** An address resolves to the most recent message with a usable List-Unsubscribe header. The site guide (`site/src/content/docs/guides/unsubscribe.md`) changes to match.
- **Every message (current behaviour).** Fix the help text, and consider deduplicating by List-Unsubscribe target, so one sender's identical URL or mailto is requested once.

Either way, the dry run should say how many requests will go out, not only how many messages it selected.
