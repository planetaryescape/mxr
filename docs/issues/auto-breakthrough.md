# Signed receiver integration still needs final acceptance

The candidate now verifies original Gmail RAW and pinned Google IMAP bytes
with mail-auth 0.15.1. Complete ARC/body verification, a trusted Google sealer,
exact signed DMARC/From alignment and a receiver-bound aligned DKIM signature
replace bare Authentication-Results trust. Generic IMAP remains unsupported.
The signed-fixture research proof passed five tests; production integration,
Asks regression checks and final scoped gates are pending. Preserve NO GO
until those checks and parent adversarial acceptance complete.

Parent clarified on 6 November 2026 that fresh genuine receiver-sealed copies
may qualify even if an intact replay/import cannot be distinguished from
new delivery. Current-mailbox SMTP ingestion is not another required factor.
The enforceable promise is verified origin and receiver handling age, not
guaranteed new SMTP receipt or content truth. Known history, import, backfill,
refetch and archive remain excluded. API timestamps cannot establish freshness.

Replay must not manufacture prior history or release a claim. The candidate
counts distinct authenticated originals and excludes the candidate original;
claims bind account/domain/kind/local day and authenticated original/kind.
The former query counted three copies as three messages: retained fail-first
evidence records observed 3, required 1. Copy, reseal, cross-account, stale,
future and original adversarial cases must pass before acceptance. Preserve
all existing legitimate Asks source/correction/undo coverage through this gate.

No source enrollment is required. Parent corrected its earlier absolute
rejection of a warmed attacker-owned domain after consulting the original
issue and BK's saved automatic-creation and decide-and-ship policies. A
truthfully named warmed domain, service-controlled user content and an
indistinguishable fresh intact copy are disclosed heuristic residuals.
Report these separately without relabeling adversarial cases or claiming
universal zero false tasks. No tools, payments, external actions or task
closing are authorised by message content.

Migration 109 message_arrivals is reserved here but has no landing priority.
PR906 also proposes 109 and 110. The second candidate to land must fetch fresh
main, renumber above the actual shipped maximum and prove that DB upgrade.
Parent owns sequencing, CI, published acceptance and shipping.

```sh
mxr updates alert-audit --format json
```

BK can run this counts-only audit on his own local store. It opens read-only,
starts no daemon, sends nothing and prints no senders, subjects or bodies.
It uses persisted evidence and current eligibility; it cannot reconstruct
missing evidence or grade semantic precision without labels. No real-mail
evaluation is authorised for this worker. The exact contract and primary
sources are in [D193](../blueprint/22-email-modes.md#verified-receiver-evidence-is-required-before-an-alert-creates-a-to-do).
