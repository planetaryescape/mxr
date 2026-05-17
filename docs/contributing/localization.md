# Adding a Locale to mxr

mxr's user-facing strings live in a single locale provider at
[`crates/core/src/i18n.rs`](../../crates/core/src/i18n.rs). The default
shipped locale is English (`en`). Translations are pure-Rust additive
constants — no extraction tools, no Fluent/ICU runtime dependency.

## How locale resolution works

- **Daemon-owned strings** (iCal REPLY subject prefixes, body templates):
  resolved once at daemon startup. The daemon reads `MXR_LOCALE` first, then
  falls back to the `general.locale` config key, then to `en`. Stored on
  `AppState` as `&'static Locale`.
- **TUI-rendered strings** (card chips, banners, status messages):
  resolved on each render against the active locale.
- **Web SPA strings**: fetched via `GET /api/v1/i18n` once at app startup
  and cached via TanStack Query.

The selection logic lives in `mxr_core::i18n::select(code)` — pass any IETF
language tag; unknown codes fall back to `EN`. Selection is case-insensitive.

## Adding a new locale

1. Open `crates/core/src/i18n.rs`.
2. Add a new `pub const` next to `EN`, populating every field on
   `InviteStrings` and `StatusStrings`. Look at `EN` for the field
   inventory. Each field needs a non-empty string.

   ```rust
   pub const DE: Locale = Locale {
       code: "de",
       invite: InviteStrings {
           card_title: "Kalendereinladung",
           subject_prefix_accepted: "Angenommen: ",
           subject_prefix_declined: "Abgelehnt: ",
           subject_prefix_tentative: "Mit Vorbehalt: ",
           body_template_accepted: "{email} hat die Einladung angenommen.",
           body_template_declined: "{email} hat die Einladung abgelehnt.",
           body_template_tentative: "{email} hat die Einladung mit Vorbehalt angenommen.",
           // … the rest of the fields
       },
       status: StatusStrings { /* … */ },
   };
   ```

3. Add `&DE` to `AVAILABLE_LOCALES`:

   ```rust
   pub const AVAILABLE_LOCALES: &[&Locale] = &[&EN, &DE];
   ```

4. Run `cargo test --package mxr-core i18n`. The coverage test will fail if
   any field is empty.

5. To activate the locale at runtime, set the env var or config key:

   ```bash
   MXR_LOCALE=de mxr
   ```

   …or in `~/.config/mxr/config.toml`:

   ```toml
   [general]
   locale = "de"
   ```

## Notes on field semantics

- **Subject prefixes** include their trailing separator (`": "`) so callers
  can `format!("{}{}", prefix, summary)` without inventing punctuation.
  Outlook's German build uses `"Angenommen: "`; keep that convention so
  organizers' inboxes thread your REPLY with their original REQUEST.
- **Body templates** must contain the literal placeholder `{email}`. The
  coverage test asserts this — translations missing the placeholder fail CI.
- **Card chip labels** are short (one or two words). Long labels overflow
  the TUI's 80-column card by default.
- **Status messages** are shown in the TUI status bar during the 1s
  auto-confirm window. Brevity matters — the line is truncated past the
  available width.
- **Banners** show in red/amber. Avoid trailing punctuation; rendering adds
  its own emphasis.

## Why English-only on the wire

The protocol enum `CalendarInviteActionData::{Accept, Tentative, Decline}`
is fixed in English. It maps directly to the iCal `PARTSTAT` values
(`ACCEPTED`/`TENTATIVE`/`DECLINED`) defined by RFC 5546. The mapping
between an action and its localized chip label happens inside the locale
provider — never invent locale-aware action codes on the wire.

## Adding a translation outside the workspace

Currently locales must be added inside `mxr_core`. Out-of-tree locale
plugins are not supported because the SPA's locale bundle is generated
from the same source of truth. If your translation matters to the
upstream project, please open a PR — that's the only path that keeps the
SPA, TUI, and daemon in lockstep.
