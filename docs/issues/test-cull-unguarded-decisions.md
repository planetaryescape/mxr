# Rules the test cull left without a test

**Status:** open · **Found:** 2026-11-22, checking docs against `main` at `bdffd3c1e` after the two test-cull waves (#1002–#1013, #1025–#1034)

The cull removed the only test of each rule below. The code still enforces each rule, but a regression would now pass CI. Each rule is recorded as a decision in a doc, which is how the gap was found.

To check a rule, search for its test on `main`:

```bash
git grep -n '<test name>' origin/main -- crates apps
```

## A draft with `body_text` but no `body_html` is rejected

`DraftContent` refuses a text alternative without an HTML body (`crates/core/src/types.rs`, the `"draft carries body_text without body_html"` error). The decision in [store-web-protocol.md](../code-decisions/store-web-protocol.md) says the precedence rules it replaced "silently drop a body or make safety checks analyse nothing, which is how the wrong message gets sent". `carrying_both_bodies_is_rejected_rather_than_resolved_by_precedence` still covers the markdown-plus-HTML case. The removed test was `body_text_alongside_markdown_is_also_rejected`.

## Safety analysis reads the supplied text alternative

`DraftContent::analysis_text` returns the markdown body, the supplied plain-text alternative of an HTML draft, or nothing. `DraftContent::byte_len` measures the authored body and ignores the alternative. Safety checks and size limits read these two values. The removed tests were `analysis_text_prefers_the_supplied_alternative` and `byte_len_measures_the_authored_body_not_the_alternative`.

## The web app acknowledges only the opened revision of a row

`useMarkRowSeen` (`apps/web/src/features/modes/rowSeen.ts`) sends a seen acknowledgement for the revision that was opened, and sends nothing for rows or payloads from older daemons. D190 in the [decision log](../blueprint/15-decision-log.md) owns the contract. The removed file was `apps/web/src/features/modes/rowSeen.test.tsx`. `updates.spec.ts` still checks that a peek persists its revision through reload, but nothing checks the older-daemon case.

## A loaded contact style produces a tone warning

`build_safety_context` fills `contact_styles`, and the safety check warns when a draft's tone departs from the recipient's style. The removed test was `contact_styles_loaded_emits_tone_warning` (`crates/daemon/src/handler/mutations.rs`). See [daemon-handler-comments-that-guarded-unstated-constraints.md](daemon-handler-comments-that-guarded-unstated-constraints.md).

## Fix

Restore each test from `96842311b` (before the cull), one test per rule. Each is short and runs in milliseconds.
