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
