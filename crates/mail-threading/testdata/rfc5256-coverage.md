# RFC 5256 coverage matrix

This matrix defines what the `mail-threading` conformance corpus covers from
RFC 5256. It is intentionally scoped to a library that receives parsed message
metadata and returns flat thread membership. It is not an IMAP server, parser,
or SORT implementation.

Status meanings:

- `covered`: fixture-backed behavior in `testdata/conformance`.
- `partial`: fixture-backed subset, but not the full RFC requirement.
- `intentional divergence`: behavior differs on purpose for this public API.
- `out of scope`: outside the crate contract.

## Summary

| RFC 5256 area | Status | Notes |
|---|---|---|
| Section 1, capability model | out of scope | IMAP server capability advertisement. |
| Section 2.1, base subject | partial | Covered for decoded strings passed by callers; RFC 2047 decoding and full i;unicode-casemap are outside this crate. |
| Section 2.2, sent date | partial | Callers pass `DateTime<Utc>`; parsing invalid dates and INTERNALDATE fallback live above this crate. |
| BASE.6.4.SORT | out of scope | This crate does not implement IMAP SORT. |
| BASE.6.4.THREAD command syntax | out of scope | This crate does not parse IMAP commands or SEARCH criteria. |
| THREAD ORDEREDSUBJECT | out of scope | Not part of the current crate claim. |
| THREAD REFERENCES | covered | Core REFERENCES/JWZ behavior is covered by fixtures, with flat output rather than IMAP nested response syntax. |
| BASE.7.2.SORT response | out of scope | Wire response formatting is an IMAP server concern. |
| BASE.7.2.THREAD response | partial | Thread grouping is covered; nested parenthesized IMAP response shape is not exposed by this API. |
| Section 5 ABNF | partial | Base-subject artifacts are covered; full command/response ABNF is out of scope. |
| Section 6 security notes | partial | Fixtures cover misleading/false References risk by making References authoritative; operational security is out of scope. |
| Section 7 internationalization | partial | ASCII/case-insensitive subject fallback is covered; full i;unicode-casemap collation is not. |

## REFERENCES algorithm

| RFC behavior | Status | Fixtures | Notes |
|---|---|---|---|
| Empty result when no messages match | covered | `empty-input` | Library equivalent of no matching searched messages. |
| One message forms one thread | covered | `single-message` | Basic root behavior. |
| Independent roots stay independent | covered | `two-independent-threads`, `no-replies` | Also checks deterministic ordering. |
| `References` reconstruct ancestry | covered | `basic-references-chain`, `multi-level-missing-phantom-chain` | Covers multi-hop parent/child linking. |
| Missing referenced ancestors become dummy messages | covered | `missing-top-reference`, `in-reply-to-only-parent-missing`, `multi-level-missing-phantom-chain` | Public output prunes phantoms by default. |
| `References` takes precedence over `In-Reply-To` | covered | `conflicting-references-beat-in-reply-to` | RFC says use `References` when valid. |
| Invalid or absent `References` falls back to first valid `In-Reply-To` | covered | `in-reply-to-only-parent-present`, `in-reply-to-only-parent-missing`, `invalid-references-fall-back-to-in-reply-to`, `invalid-threading-headers-allow-subject-fallback` | This crate accepts one parsed `In-Reply-To` value. |
| No valid reference means NIL parent | covered | `single-message`, `no-replies`, `two-independent-threads` | Current message becomes a root unless subject fallback merges it. |
| Message-ID quoting normalization | covered | `message-id-quoted-local-normalization` | Quoted and unquoted local parts match. |
| Message-ID comparisons are case-sensitive | covered | `message-id-case-sensitive` | Case is preserved after normalization. |
| Missing or invalid current Message-ID gets unique identity | covered | `missing-message-id-assigned-unique-id`, `missing-message-id-reply-to-present-parent` | Public output uses caller-stable IDs. |
| Duplicate Message-ID: first wins, later duplicates get unique IDs | covered | `duplicate-message-id-first-wins` | Sequence-number semantics are approximated by input order. |
| Adjacent reference links do not replace an existing parent | covered | `references-chain-preserves-existing-parent` | Protects truncated References behavior. |
| Current message is reparented to last reference | covered | `current-message-reparents-to-last-reference` | Step 1.B replacement behavior. |
| Do not create loops | covered | `cycle-in-references`, `self-reference` | Self-links and ancestor loops are ignored. |
| Reply arrives before parent | covered | `reply-arrives-before-parent` | Phantom container is filled when the real message appears later. |
| Phantom pruning | covered | `prune-phantoms-disabled`, `missing-top-reference`, `multi-level-missing-phantom-chain` | Default public output prunes phantoms; option can expose them. |
| Sort top-level threads and members by sent date | covered | `stable-thread-ordering-by-date`, `stable-thread-ordering-with-caller-ids`, `canonical-root-preserved-with-earlier-child` | Exact IMAP sequence-number tie-break is not modeled. |

## Base subject extraction

| RFC behavior | Status | Fixtures | Notes |
|---|---|---|---|
| Collapse tabs/repeated whitespace | covered | `subject-whitespace-normalization` | Operates on already decoded strings. |
| Remove `subj-refwd` leaders like `Re:` and `Fwd:` | covered | `subject-fallback-groups-headerless`, `subject-fallback-attaches-to-header-thread`, `localized-subject-prefixes` | Default also supports localized prefixes as an extension. |
| Support custom reply/forward prefixes | covered | `subject-prefixes-custom` | Extension beyond RFC 5256. |
| Remove `(fwd)` trailer | covered | `subject-trailer-fwd-normalization` | Case variants covered by implementation. |
| Remove `[Fwd: ...]` wrapper and repeat extraction | covered | `subject-forward-wrapper-normalization` | Wrapper is normalized before comparison. |
| Remove leading `subj-blob` when base remains non-empty | covered | `subject-blob-normalization` | Mailing-list tag behavior. |
| Preserve final blob when it would otherwise be the base subject | covered | `subject-blob-only-preserved` | RFC `subj-middle` edge case. |
| Skip empty thread subject during subject merge | partial | `empty-input` | No dedicated empty-subject fixture yet. |
| Determine reply/forward from base-subject artifact removal | partial | `subject-fallback-groups-headerless`, `subject-trailer-fwd-normalization`, `subject-forward-wrapper-normalization` | The crate does not expose a reply/forward classifier directly. |
| Full RFC 2047 decoding before extraction | out of scope | none | Callers should parse/decode raw messages before calling this crate. |
| Full i;unicode-casemap collation | out of scope | none | Current implementation uses pragmatic case folding for subject fallback. |

## Subject merge

| RFC behavior | Status | Fixtures | Notes |
|---|---|---|---|
| Headerless messages with same base subject can merge | covered | `subject-fallback-groups-headerless`, `invalid-threading-headers-allow-subject-fallback` | Practical subject fallback. |
| Headerless reply can attach to a header-backed thread | covered | `subject-fallback-attaches-to-header-thread` | Common degraded-header case. |
| Subject merge can be disabled | covered | `subject-merge-disabled` | Public option. |
| Same-subject header-backed roots are not force-merged | intentional divergence | `same-subject-header-threads-not-merged` | The crate favors avoiding false merges for local clients. |
| Full RFC dummy-message merge tree rules | partial | `subject-fallback-groups-headerless`, `same-subject-header-threads-not-merged` | Flat output does not expose all dummy tree states. |

## Output shape

| RFC behavior | Status | Fixtures | Notes |
|---|---|---|---|
| Thread grouping | covered | `basic-references-chain`, `two-independent-threads`, `stable-thread-ordering-by-date`, `stable-thread-ordering-with-caller-ids` | Public output is flat `Thread { root_message_id, messages }`. |
| Nested IMAP `THREAD` response | out of scope | none | A future tree API would need separate fixtures. |
| Message sequence numbers / UIDs | out of scope | none | Callers provide stable IDs; the crate does not know mailbox sequence numbers. |

## Fixture inventory

Every fixture must appear in this matrix so coverage drift is visible:

- `basic-references-chain`
- `canonical-root-preserved-with-earlier-child`
- `conflicting-references-beat-in-reply-to`
- `current-message-reparents-to-last-reference`
- `cycle-in-references`
- `duplicate-message-id-first-wins`
- `empty-input`
- `in-reply-to-only-parent-missing`
- `in-reply-to-only-parent-present`
- `invalid-references-fall-back-to-in-reply-to`
- `invalid-threading-headers-allow-subject-fallback`
- `localized-subject-prefixes`
- `message-id-case-sensitive`
- `message-id-quoted-local-normalization`
- `missing-message-id-assigned-unique-id`
- `missing-message-id-reply-to-present-parent`
- `missing-top-reference`
- `multi-level-missing-phantom-chain`
- `no-replies`
- `prune-phantoms-disabled`
- `references-chain-preserves-existing-parent`
- `reply-arrives-before-parent`
- `same-subject-header-threads-not-merged`
- `self-reference`
- `single-message`
- `stable-thread-ordering-by-date`
- `stable-thread-ordering-with-caller-ids`
- `subject-blob-normalization`
- `subject-blob-only-preserved`
- `subject-fallback-attaches-to-header-thread`
- `subject-fallback-groups-headerless`
- `subject-forward-wrapper-normalization`
- `subject-merge-disabled`
- `subject-prefixes-custom`
- `subject-trailer-fwd-normalization`
- `subject-whitespace-normalization`
- `two-independent-threads`
