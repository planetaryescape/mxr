# Classification release follow-ups

Independent review of `5377411a108024c1506cedeb7cdadef0208caca2`
found no blockers. The subsequent release-108 merge changes metadata only;
all code subtree hashes are identical. These are nonblocking limitations and
profiling questions, not reproduced production incidents or authorization for
real-mail experiments. Keep this capture outside the frozen release candidate.

## Measure address-history and predecessor queries

`store/desk.rs::addresses_written_to` scans outbound messages and expands their
recipient JSON on each sender-context load. Arrival projection can load that
context per page. Predecessor selection also uses correlated queries. Profile
a large synthetic store and inspect query plans before adding bounds or indexes;
preserve account isolation and date/ID ordering. No measured slowdown was
established in review. Request-local purpose reuse is already tracked in
`classification-projection-followups.md`; do not invent a persisted cache.

## Document conservative identical re-asks

The reader can identify an unmarked verbatim predecessor-matched request as old
history. A genuinely renewed identical ask may therefore receive no automatic
turn. Document that conservative ambiguity alongside quote-format limitations,
or require an attribution marker/additional authored context before cutting
the repeated block. Preserve protection against stale quoted requests.

## Clarify the line-local negation boundary

The supported request grammar rejects negation on its request line. A separate
following line such as `No need to reply.` does not currently negate the previous
line's request. Document that limit or add an adjacent-line/paragraph check with
paired positive/negative synthetic cases. Do not broaden phrase matching without
checking copied/group mail, old quotations and explicit reminder intent.

## Clarify projected chip and question coverage

A provider spam flag added after immutable arrival placement can leave an old
bucket chip visible. Consider showing current spam/trash state without rewriting
first-seen placement. Current-policy conflicts absent from stored Not sure
candidates may not be asked; that coverage and mixed read-gate consistency are
already tracked in `classification-projection-followups.md`. Read-only mixed
projections heal on refresh; trace reentrancy before adding locks.

## Narrow address-hint documentation

The reference sentence saying a reply can outweigh an address/domain hint should
say **newsletter** address/domain hint. Notifying local parts and subdomains
remain decisive, as the adjacent rule table already states. Align that sentence
in a docs-only change; do not change the implemented precedence to match broad
wording.

## Recover malformed legacy List-Id values deliberately

Historical repair skips identifiers rejected by normalization, leaving their
legacy metadata/index value until re-ingestion. Classification still evaluates
raw header evidence, but ancillary grouping may retain a malformed ID. Decide
whether explicit doctor recovery should clear invalid values; preserve other
metadata, caches, attachments and atomic index updates. Use synthetic old rows,
not a real mailbox rewrite.
