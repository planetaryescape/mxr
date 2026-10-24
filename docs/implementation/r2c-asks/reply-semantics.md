# Reply debt follows the speech act and the responsible person

Run the native reply checks with `INSTA_UPDATE=no MXR_ACTIVITY=off scripts/cargo-test -p mxr --lib handler::tests::reply_semantics -- --nocapture`.

An incoming “I will respond tomorrow.” is the correspondent's commitment. “Could you respond tomorrow?” asks the owner for a response. Both retain their original quote and span. The unit judge recognizes a first-person response commitment at the clause opening, so an embedded future in “Could you confirm when I will respond?” remains the owner's question. The unpublished bundle retains `asks-r2c-10`.

The native test reads current stored assessments through `classify_eval_packet`, grades their reply answers through the existing `AskGrader`, and separately checks public mode summaries and the composed desk. It constructs its own expectations without resolving a scenario generator. Primary and owned-alias replies suppress earlier owner debt; Cc and another person's salutation do not assign that person's request to the owner. Automatically generated feedback requests create no reply debt while an independent signing obligation remains a To do.

Semantic reply evaluation remains per message. Archiving removes a delivered turn while retaining the semantic answer. Inbox membership, snooze, dismissal, stored home, thread direction, pending coverage and desk precedence continue to control delivery; no evaluator filter or scorer changes are part of the speaker repair.

The bounded public-source diagnostic uses only the visible terse people follow-up and rail-tickets notice literals. The follow-up has no owner debt. At the first speaker checkpoint the rail notice still produces owner debt without automatic headers; its packet is evidence for a separate source-genre investigation, not a passing semantic claim. No held-out wording is read or selected.
