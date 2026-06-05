---
candidate: list-unsubscribe
status: published
decision: shipped
external_repo: https://github.com/planetaryescape/list-unsubscribe
crates_io: https://crates.io/crates/list-unsubscribe
mxr_source: crates/mail-parse/src/lib.rs (parse_list_unsubscribe + UnsubscribeMethod) — now consumed from crates.io
last_reviewed: 2026-05-16
---

> **Status: Shipped.** Published as
> [`list-unsubscribe v0.1.0`](https://crates.io/crates/list-unsubscribe) at
> [`planetaryescape/list-unsubscribe`](https://github.com/planetaryescape/list-unsubscribe).
> mxr now consumes the registry version through `mxr-mail-parse`, which
> converts the 4-variant public enum into `mxr-core`'s 5-variant
> `UnsubscribeMethod` at the boundary so the `BodyLink` HTML-body-scraping
> fallback stays local. The migration runbook is in
> [`docs/extracted-crates/implementation/03-list-unsubscribe-external-repo.md`](../../extracted-crates/implementation/03-list-unsubscribe-external-repo.md).
> The document below is kept as historical context for the original
> rationale and design decisions.

# `list-unsubscribe` (proposed name)

> Parse `List-Unsubscribe` and `List-Unsubscribe-Post` headers per RFC 2369
> and RFC 8058. Distinguish one-click POST endpoints from mailto and
> ordinary HTTP links. Return a typed enum the caller can act on.

## Decision: **Tier 1 — ship**

Small, focused, deliverability-mandated, and unfilled in the Rust
ecosystem. The mxr implementation is solid. This is the lowest-effort
Tier 1 ship — probably half a day end-to-end.

## What mxr has today

**Source:** `crates/mail-parse/src/lib.rs`

```rust
pub enum UnsubscribeMethod {
    OneClick { url: String },
    HttpLink { url: String },
    Mailto { address: String, subject: Option<String> },
    None,
}

fn parse_list_unsubscribe(message: &Message<'_>) -> UnsubscribeMethod {
    let entries: Vec<String> = match message.list_unsubscribe().as_address() {
        Some(mail_parser::Address::List(list)) => list.iter()
            .filter_map(|addr| addr.address.as_ref().map(|value| value.to_string()))
            .collect(),
        // group variants handled similarly
        _ => Vec::new(),
    };

    let one_click = message
        .header_raw("List-Unsubscribe-Post")
        .map(|value| value.to_ascii_lowercase())
        .map(|value| value.contains("list-unsubscribe=one-click"))
        .unwrap_or(false);

    if one_click {
        if let Some(url) = entries.iter()
            .find(|e| e.starts_with("https://") || e.starts_with("http://")) {
            return UnsubscribeMethod::OneClick { url: url.clone() };
        }
    }
    // ... then try mailto, then http fallback
}
```

This handles:

- RFC 2369 multi-method `List-Unsubscribe: <mailto:...>, <https://...>`
- RFC 8058 one-click via `List-Unsubscribe-Post: List-Unsubscribe=One-Click`
- `mailto:` URL parsing including `?subject=` extraction
- Preference order: one-click > mailto > http link

## Ecosystem state at extraction time

The extraction review did not find a focused crate with this exact
contract. The closest things:

| Crate | Coverage |
|---|---|
| `mail-parser` (stalwart) | Exposes raw `List-Unsubscribe` header values; does not parse to a typed action enum, does not detect RFC 8058 one-click |
| `mailparse` | Same — surface raw headers only |
| `email-address-parser` | Address parsing, irrelevant |

Projects that want to honour `List-Unsubscribe` still need a typed policy
layer on top of raw header access. This crate is meant to make that
policy layer small, testable, and reusable.

## Why this matters more than it looks

In February 2024 Gmail and Yahoo introduced new bulk-sender deliverability
requirements. **One of them is mandatory RFC 8058 one-click unsubscribe
for senders above 5000 messages/day to Gmail or Yahoo recipients.** This
elevated `List-Unsubscribe-Post` from "obscure RFC" to "required for
inbox placement". Any newer email tooling — readers, list managers,
compliance scanners — needs to parse it correctly.

The standalone crate would serve:

- Email clients implementing the "Unsubscribe" button
- Compliance tools auditing mailing-list operators
- Spam filters using `List-Unsubscribe` presence as a positive signal
- Inbox-zero / Clean-Email-style apps doing bulk unsubscribe

## Proposed public API

```rust
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click. POST to `url` with body `List-Unsubscribe=One-Click`.
    OneClick { url: Url },
    /// Plain HTTP link the user opens in a browser. May require interaction.
    HttpLink { url: Url },
    /// Send an email to `address` with optional `subject`.
    Mailto { address: String, subject: Option<String> },
    /// No `List-Unsubscribe` header found, or unparseable.
    None,
}

/// Parse from a raw header value, e.g. `"<mailto:u@x>, <https://x/u>"`.
pub fn parse_list_unsubscribe(header_value: &str) -> UnsubscribeMethod;

/// Parse with awareness of an accompanying `List-Unsubscribe-Post` header.
pub fn parse_list_unsubscribe_with_post(
    header_value: &str,
    post_header_value: Option<&str>,
) -> UnsubscribeMethod;

/// Convenience: extract from a `mail_parser::Message` if the user already has one.
#[cfg(feature = "mail-parser")]
pub fn parse_from_message(message: &mail_parser::Message<'_>) -> UnsubscribeMethod;
```

Notes:

- Use `url::Url` not `String` for HTTP variants. Catches malformed URLs at
  parse time.
- Feature-gate the `mail-parser` integration. Crate has zero required deps.
- Provide both the `&str` form (works with any header source) and the
  `Message` integration.

## Extraction plan

**Step 1 — Repo setup.** New repo, dual MIT/Apache.

**Step 2 — Move code.** Lift `UnsubscribeMethod` and `parse_list_unsubscribe`
into `src/lib.rs`. Strip dependency on `mail_parser::Message` from the
core path; rewrite to accept `&str`.

**Step 3 — Add `mail-parser` adapter behind a feature.**

**Step 4 — Test coverage.**
Hardened tests for:
- Single mailto only
- Single https only
- Both, no one-click → http preferred (per common practice)
- Both, one-click → OneClick variant returned
- One-click header present but no https URL → fall back to mailto/http
- Mailto with `?subject=Unsubscribe&body=...` (parse subject only,
  intentionally drop body to keep API tight)
- Malformed URLs (return `None`, log nothing)
- Multiple http URLs (return the first; document the choice)
- Empty header
- Header with stray whitespace and angle-bracket quirks
- One-click case-insensitive matching (`List-Unsubscribe=ONE-CLICK`)

**Step 5 — Documentation.** Rustdoc with worked examples for each
variant. README that explains the Gmail/Yahoo deliverability backstory so
adopters understand why the crate exists.

**Step 6 — Publish.**

**Step 7 — Replace inside mxr.** `mxr-mail-parse` re-exports from
`list-unsubscribe`.

## Estimated effort

**A few hours, agent-assisted.** Smallest Tier 1 ship.

See [00-publishing-strategy.md](./00-publishing-strategy.md) for the
AI-era effort framework. The pre-agent estimate of "half a day to a
day" assumed human-typing-every-line; with agents the lift collapses
to an afternoon for the Rust crate alone.

## TS / npm distribution

**Recommended approach: native TS port + shared JSON corpus.**

The npm ecosystem has no focused crate for this either. The audience
on JS (webmail clients, mail-list compliance tools, deliverability
auditors) is at least as large as on Rust.

This is the **first crate to ship** (per [00-publishing-strategy.md](./00-publishing-strategy.md))
because the unknowns are about the workflow, not the code:

- Dual repo setup (rust + ts)
- Shared `list-unsubscribe-corpus` JSON test fixtures
- CI in both repos consuming the corpus
- Dual publish (cargo publish + npm publish)
- Semver discipline across registries

Use this small crate to validate that workflow before applying it to
`02-jwz-threading` and `03-gmail-query`.

**Corpus shape.**

```json
{
  "name": "rfc8058-one-click",
  "input": {
    "list_unsubscribe": "<https://example.com/unsub?u=abc>, <mailto:u@example.com>",
    "list_unsubscribe_post": "List-Unsubscribe=One-Click"
  },
  "expected": {
    "method": "OneClick",
    "url": "https://example.com/unsub?u=abc"
  }
}
```

Both Rust and TS test harnesses load the same fixture file and assert
the same expected output. Drift between implementations becomes
impossible to ship silently.

**Effort with dual publish.** ~1 day total: Rust crate + TS port +
corpus + CI wiring. Realistic given the small surface.

## Risks and unknowns

- **`mailto:` body parameter.** Some senders encode the entire
  unsubscribe message in `?body=...`. Decision: drop body from the
  output to avoid encouraging clients to silently send pre-canned
  messages on the user's behalf. Document this choice.

- **Multiple http URLs.** Some senders provide multiple http links (e.g.
  desktop and mobile). Return the first; document.

- **Validation of the one-click endpoint.** We do **not** verify that
  POSTing actually unsubscribes. That's the caller's job. The crate's
  contract is "parse the header, classify the method".

- **One-click POST execution.** Should the crate provide a
  `oneclick.unsubscribe()` helper that POSTs? Decision: **no**. Keep
  the crate pure parsing. Let callers use `reqwest` / `ureq` / whatever
  HTTP client they prefer. Mention this in the README.

## When to re-evaluate

- If `mail-parser` or `mailparse` upstream a typed `UnsubscribeMethod`
  enum, the gap closes. Unlikely soon — both are positioned as
  spec-faithful parsers, not policy-layer interpreters.

## Naming

Candidates:

- `list-unsubscribe` — descriptive, matches RFC header name
- `mail-unsubscribe` — generic
- `rfc8058` — too cryptic
- `email-unsubscribe-parser` — too long

Recommended: **`list-unsubscribe`**. The crate name matches the header
name; users searching for the problem will find it instantly.

## Companion direction (out of scope for v1)

A future v2 could include:

- A `oneclick-executor` feature using `reqwest` to do the POST.
- A `mailto-executor` feature that hands off to `lettre` to send the
  mail.
- Parsing of the response body of one-click POSTs (most are empty, some
  return JSON confirmations).

Ship v1 as pure parsing first. Add executors only if users ask.
