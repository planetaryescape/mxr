# Carving a new crate out of an existing one

`mail-threading` was already its own workspace member when extracted. The
mechanics were "publish a folder."

`list-unsubscribe` was different: the parser lived as private functions
inside `crates/mail-parse/src/lib.rs`, and the public enum lived in
`mxr-core` with five variants (including an mxr-specific `BodyLink`
fallback used by HTML body scraping). The extraction had to:

1. Scaffold a fresh `crates/list-unsubscribe/` with the contract-shaped
   public API.
2. Move the parser logic in.
3. Rewire the existing internal call site.
4. Convert at the boundary between the public 4-variant enum and mxr's
   5-variant enum.

This file captures the patterns that only show up in this carving shape.

## The boundary conversion is the new artifact

`mail-threading` exposed the same type internally and externally — no
conversion needed.

`list-unsubscribe`'s public enum has four variants (`OneClick`,
`HttpLink`, `Mailto`, `None`); mxr's has five (the four above plus
`BodyLink`). The conversion lives in `mxr-mail-parse`:

```rust
fn convert_unsubscribe(method: list_unsubscribe::UnsubscribeMethod) -> UnsubscribeMethod {
    match method {
        list_unsubscribe::UnsubscribeMethod::OneClick { url } => UnsubscribeMethod::OneClick { url: url.into() },
        list_unsubscribe::UnsubscribeMethod::HttpLink { url } => UnsubscribeMethod::HttpLink { url: url.into() },
        list_unsubscribe::UnsubscribeMethod::Mailto { address, subject } => UnsubscribeMethod::Mailto { address, subject },
        list_unsubscribe::UnsubscribeMethod::None => UnsubscribeMethod::None,
    }
}
```

That function is the seam. It exists because mxr has a policy concern
(`BodyLink` from body scraping) that doesn't belong in a parser crate. The
function is small, private, and trivially testable — and writing it was
the whole point of the carve-out.

**Rule:** when carving, design the public type for the *contract*, not for
mxr's current call sites. Then write the boundary conversion. Don't try to
re-export mxr's enum; don't try to push mxr's policy variants into the
public type. The conversion is cheap.

## Type coupling lives in unexpected places

The parser ostensibly only depended on `mail_parser::Message`. It also,
silently:

- Used `url::form_urlencoded::parse` for mailto `?subject=` extraction.
- Used `url::Url::parse` for the fallback path.

Those came in for free under the existing workspace `url = { workspace = true }`
dep. When the parser moved out, the new crate had to declare `url` as a
required dep — and `mxr-mail-parse` lost its only `url` consumer, so the
dep got dropped there too.

**Rule:** before carving, list every dep that touches the parser's call
path, not just the obvious ones. The carving doesn't just move code, it
moves transitive deps. Inside a workspace they're free; in the standalone
they're a publish-time decision (required, optional, feature-gated).

## Public API design pulls the implementation in a new direction

The internal parser took `&mail_parser::Message<'_>`. That couples every
caller to `mail-parser`. For a published crate, that's a smaller addressable
audience than `parse(&str)` — which works against any message store.

The carving forced an API shape change: `parse(&str)`,
`parse_with_post(&str, Option<&str>)`, plus an optional
`parse_from_message` adapter behind a `mail-parser` feature. mxr uses the
adapter (no behavior change for mxr); ecosystem users get the bare-string
form and can pair the crate with `mailparse`, `mailerlite`, or hand-rolled
header reading.

**Rule:** the carved API should answer "what's the smallest input the
contract needs?" not "what's the type mxr already has?" If those answers
differ, write the public API first and write the mxr adapter second.

## The boundary tests are a quality multiplier

The existing mxr test `parses_unsubscribe_mailto_subject` (at
`crates/mail-parse/src/lib.rs:625` pre-carve) hit the parser via
`parse_headers_from_pairs`, the public entry point of `mxr-mail-parse`. It
kept passing through the carve-out without modification because the call
site still went through the same public entry point — only the internal
implementation changed.

That's the property to preserve when carving: existing mxr-level tests
become unwitting integration tests for the new crate's adapter. Don't
delete them, don't rewrite them; let them continue to vouch for the seam.

## Phase 0 looks bigger but Phase 6 looks smaller

Compared to `mail-threading`'s extraction:

- **Phase 0** was bigger: scaffold a fresh crate, build the conformance
  corpus from scratch, design the API, write the README, *then* refactor
  the existing internal call site. ~2 hours of focused work.
- **Phase 6** (consumer cutover) was tiny: one line in
  `workspace.dependencies`, removal from `workspace.members`,
  `rm -rf crates/list-unsubscribe`. Same as `mail-threading` — except the
  rewire of `mxr-mail-parse` had already landed in Phase 0, so Phase 6 was
  pure de-vendoring.

That's the trade. Future carve-outs should expect a Phase 0 that's heavier
than a publish-existing-crate but a Phase 6 that's just as light.

## When *not* to carve

If the function's interface is genuinely shaped around mxr's
internal types (e.g. it takes an `mxr_core::SyncState` or returns an
`mxr_protocol::Event`), carving means inventing a new public type and
proving the new shape is right. That's hard work, and getting it wrong
ships a v0.1.0 the world has to live with.

The carve only makes sense when the public-facing shape is *clearly more
natural* than what mxr currently uses internally. For `list-unsubscribe`,
`parse(&str) -> UnsubscribeMethod` was obviously cleaner than
`parse_list_unsubscribe(&Message) -> mxr_core::UnsubscribeMethod` —
because the spec is about header strings, and `BodyLink` is policy.

If the cleaner public shape isn't obvious, defer the carve until you can
state it in one sentence.

## Re-export bridges keep internal consumers compiling

`mail-query` (carved out of `mxr-search`) needed eight files in the
daemon to keep importing `mxr_search::ast::QueryNode`. Touching each was
unnecessary friction. `mxr-search/src/lib.rs` solved it with:

```rust
mod index;
pub mod query_builder;

pub mod ast {
    pub use mail_query::{
        DateBound, DateValue, FilterKind, ParseError, ParserOptions,
        QueryField, QueryNode, RelativeUnit, SizeOp, Visitor,
    };
}

pub use mail_query::{
    DateBound, DateValue, FilterKind, ParseError, ParserOptions,
    QueryField, QueryNode, RelativeUnit, SizeOp, Visitor,
};
```

Every existing `use mxr_search::ast::*` import compiles. Every
`mxr_search::QueryNode` path resolves. The crate-side change cost is
zero outside the four files that *pattern-match* on `FilterKind` (which
needed the new `Custom(_)` arms anyway).

**Rule:** when carving leaves behind in-tree consumers, add re-export
shims at the original crate boundary first. Defer per-call-site
refactors to a follow-up.

## Behaviour-changing carve-outs are real

The `mail-query` extraction shipped three Phase 0 behaviour changes:

1. **`older_than:5d` AST shape.** Old parser resolved to a
   `NaiveDate` at parse time; the published crate emits `Relative
   { amount, unit }`. Downstream resolution code in
   `query_builder.rs` and `search_filter.rs` needed updates.
2. **OR precedence fix.** mxr's parser had `a b OR c` produce
   `And(a, Or(b, c))` — opposite of Gmail/Lucene. Fixed to
   `Or(And(a, b), c)`. No existing test exercised the broken
   combination, but a public Gmail-vocabulary crate could not ship
   with non-Gmail precedence.
3. **Brace-group regression caught by the OR fix.** Switching
   `parse_or` from "single atom" semantics to "and-group" semantics
   broke `parse_brace_group`'s implicit-OR. Caught by mxr's
   `e2e_search_gmail_or_braces_and_field_groups` test, fixed by
   making the brace parser call `parse_unary` directly.

**Rule:** budget for behaviour fixes when carving — not just code moves.
A public crate can't ship known bugs the internal version got away with.
Run the consumer test suite carefully between every behaviour change so
you catch the secondary breakage immediately.

## `#[non_exhaustive]` cascades downstream

Applying `#[non_exhaustive]` to every public enum (the tantivy-regret
fix) means every internal mxr `match` on those enums needs a `_ => …`
arm. For `mail-query`'s carve-out that touched seven files in
`crates/daemon/` and one in `crates/search/`. The wildcard is usually
benign (degrade to AllQuery, return error, treat as no-op) but it must
be considered for each match.

Some authors prefer to apply `#[non_exhaustive]` only at the outer enum
(QueryNode) where new variants are expected, and leave inner enums
exhaustive for ergonomics. We chose universal `#[non_exhaustive]` for
safety; the cost is mechanical wildcard additions per match site.

**Rule:** when applying `#[non_exhaustive]` from the start, plan for the
downstream wildcard pass to take 20–30 minutes of grunt work per carve-
out. Worth it.
