# Automatic alert to-dos still lack genuine live receiver provenance

D193's candidate decision, arrival history, durable claim, audit and client
explanations are implemented. It is not ready to ship as a working
automatic-alert feature: no live adapter currently supplies a genuine
verified receiver verdict and receipt.

Gmail exposes ordinary RFC headers and a timestamp that can use Date for
migrated mail. Insert bypasses most scanning; import does not perform SPF
checks. A sole `Authentication-Results: mx.google.com` does not prove Google
authored it. Gmail IMAP extensions are compatibility claims, not endpoint
identity. The candidate uses configured TLS endpoint identity separately,
then still fails closed without a genuine verdict. Outlook and generic
IMAP remain unsupported with an explanation. Sources are linked in
[blueprint 22](../blueprint/22-email-modes.md#verified-receiver-evidence-is-required-before-an-alert-creates-a-to-do).

The expanded synthetic corpus exposed an existing canonical Asks projection
that created payment-failure tasks before the alert gate. Automatic projection
now leaves those alerts to the trusted arrival path; existing tasks remain
for the user to handle. The regression must pass before acceptance.

Resolve that receiver and timestamp boundary with a trustworthy provider
attestation or independently verifiable evidence. Demonstrate legitimate
creation through the actual adapter and daemon, retaining zero synthetic
spoof/first-time/display-brand/backfill creations. Parent adversarial
acceptance, CI and published checks remain required.

No source enrollment is required. Parent clarified this on 6 November
after consulting BK's saved automatic-to-do and decide-and-ship policies.
Authentication proves a domain, not alert truth: a warmed attacker-owned
domain or a service's user-controlled text remains a disclosed heuristic
residual, with the actual domain shown in the title and reason.

```sh
mxr updates alert-audit --format json
```

BK can run this counts-only audit on his own local store after the daemon
has upgraded its schema. It opens read-only, starts no daemon, sends
nothing and prints no senders, subjects or bodies. It uses stored
provenance and current eligibility; it cannot reconstruct missing receipt
evidence or measure false tasks without human labels. No real-mail eval
is authorised for this worker.
