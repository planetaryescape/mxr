---
candidate: gmail-query
status: tier-1
decision: ship
mxr_source: crates/search/src/parser.rs, crates/search/src/ast.rs
last_reviewed: 2026-05-15
---

# `gmail-query` (proposed name)

> Parser and typed AST for Gmail-style email search queries:
> `from:foo subject:"hello" is:unread after:2024-01-01 -has:attachment`.
> Backend-agnostic — produces an AST, you choose the search engine.

## Decision: **Tier 1 — ship**

The Rust ecosystem has nothing comparable. Generic `query-parser` and
`search-query-parser` crates exist but cover toy syntax. Tantivy's
`QueryParser` produces tantivy-internal types that are not portable. mxr
has a comprehensive, hand-written parser with a clean AST that's already
decoupled from the search backend. The one missing piece — a `Display`
impl for round-trip serialisation — is a few hours of work.

## What mxr has today

**Sources:**
- `crates/search/src/parser.rs` — recursive-descent parser
- `crates/search/src/ast.rs` — typed AST enum
- `crates/search/src/query_builder.rs` — AST → tantivy Query translator
  (stays in mxr; not part of the extracted crate)

### Operators supported

A near-complete Gmail superset:

**Address operators:** `from:`, `to:`, `cc:`, `bcc:`, `deliveredto:`,
`rfc822msgid:`, `list:`

**Content operators:** `subject:`, `body:`, `filename:`

**Filter operators:** `label:`, `category:`, `in:` (folder/system label),
`is:` (`unread`, `read`, `starred`, `draft`, `sent`, `trash`, `spam`,
`answered`, `inbox`, `archived`, `reply-later`, `owed-reply` — last two
are mxr-specific)

**Has operators:** `has:` (`attachment`, `userlabels`, `nouserlabels`,
`drive`, `document`, `spreadsheet`, `presentation`, `youtube`,
`inline-image`, `link`, `link-heavy`, `link-none`, star-colour variants)

**Size operators:** `size:`, `larger:`, `smaller:` with unit suffixes
(`5M`, `200K`, `1G`)

**Date operators:** `after:`, `before:`, `date:`, `older:`, `newer:`,
`older_than:`, `newer_than:` with relative-date support (`older_than:5d`,
`newer_than:1w`)

**Boolean operators:** `AND`, `OR`, `NOT`, `-`, parentheses, braces for
field groups, `AROUND<n>` for word proximity

### AST

```rust
pub enum QueryNode {
    Text(String),
    Phrase(String),
    Field { field: QueryField, value: String },
    Filter(FilterKind),
    Label(String),
    DateRange { bound: DateBound, date: DateValue },
    Size { op: SizeOp, bytes: u64 },
    Near { left: String, right: String, distance: u32 },
    And(Box<QueryNode>, Box<QueryNode>),
    Or(Box<QueryNode>, Box<QueryNode>),
    Not(Box<QueryNode>),
}
```

Hand-written recursive descent. Not regex-based, not pest/lalrpop. Easy to
read, easy to extend, no generated code.

### Coupling to tantivy

**Parsing has none.** `parser.rs` and `ast.rs` don't import `tantivy`.
The tantivy translation lives in `query_builder.rs`, which would stay
inside mxr (or move to a separate adapter crate).

## Ecosystem state

| Candidate | Status |
|---|---|
| [`query-parser`](https://crates.io/crates/query-parser) | Generic `key:value` parser, no email operators, minimal AST |
| [`search-query-parser`](https://crates.io/crates/search-query-parser) | Elasticsearch-leaning, generic |
| `tantivy::QueryParser` | Internal to tantivy, not portable, produces `Box<dyn Query>` |
| Hand-rolled implementations | Every Rust email project rolls its own |

There is no published Rust crate that parses Gmail-style email queries to
a portable AST.

## Why our code is publication-ready

- **Comprehensive operator coverage.** Matches Gmail's documented operator
  set closely.
- **Clean AST.** `QueryNode` is a clean sum type, no lifetimes, no
  backend coupling.
- **Hand-written and readable.** Anyone can extend it without learning a
  parser generator.
- **Tested.** Parser tests cover operator combinations, quoting, escaping,
  boolean precedence, relative dates, size units.

## Proposed public API

```rust
// Parse a query string. Errors are typed for good error messages.
pub fn parse(input: &str) -> Result<QueryNode, ParseError>;

// Pretty-print AST back to a query string. Round-trips through parse.
impl Display for QueryNode { /* ... */ }

// Walk the AST. Useful for backend authors who want to translate to
// tantivy / meilisearch / SQL FTS / arbitrary indexes.
pub trait Visitor {
    fn visit_text(&mut self, text: &str);
    fn visit_phrase(&mut self, phrase: &str);
    fn visit_field(&mut self, field: QueryField, value: &str);
    // ...
}

// Extensibility for non-Gmail filters. Users register custom filter
// names so mxr-specific filters like `is:owed-reply` don't ship in the
// crate's hardcoded set.
pub struct ParserOptions {
    pub custom_filters: HashSet<String>,
    pub custom_fields: HashSet<String>,
}

pub fn parse_with(input: &str, options: ParserOptions) -> Result<QueryNode, ParseError>;
```

### Custom filter extensibility

This is the only real API design choice. Today, `FilterKind` is an enum
with mxr-specific variants like `OwedReply` and `ReplyLater`. When we
extract the crate, those need to come out of the public enum and be
exposed via a custom-filter extension point.

Two options:

**Option A — closed enum + custom variant:**
```rust
pub enum FilterKind {
    Unread, Read, Starred, /* ... */ ,
    Custom(String),
}
```
Simple, but discriminating between built-in and custom filters at use
site is awkward.

**Option B — trait object:**
```rust
pub trait Filter: Debug + Send + Sync { fn name(&self) -> &str; }
pub enum FilterKind {
    BuiltIn(BuiltInFilter),
    Custom(Box<dyn Filter>),
}
```
More flexible but heavier.

Recommended: **Option A**. Most users will not register custom filters
and won't pay the extra allocation. The `Custom(String)` carries the
operator name as-typed.

## Extraction plan

**Step 1 — Repo setup.** New repo, dual MIT/Apache. Workspace member
inside mxr first if we want to iterate before publishing — then split
out once API stabilises.

**Step 2 — Move code.**
- `crates/search/src/parser.rs` → `src/parser.rs`
- `crates/search/src/ast.rs` → `src/ast.rs`
- Leave `query_builder.rs` (tantivy translator) in mxr.

**Step 3 — Extract mxr-specifics from the AST.**
- Move `OwedReply`, `ReplyLater` out of the public `FilterKind` enum and
  into `Custom("owed-reply")` / `Custom("reply-later")`.
- Update mxr's `query_builder.rs` to match against `Custom` names for
  those filters.

**Step 4 — Add `Display`.**
- Implement `Display for QueryNode` so AST → string round-trips.
- Property-test: for every parseable string `s`, `parse(s).to_string()
  ` round-trips through `parse` again.

**Step 5 — API polish.**
- Visitor trait for backend authors.
- `ParserOptions` for custom filters/fields.
- `serde` feature flag for serialising AST nodes.

**Step 6 — Documentation.**
- Rustdoc for every operator with a worked example.
- README with: full operator reference, AST walkthrough, examples of
  translating to tantivy / meilisearch / SQL FTS.
- Cite Gmail's official operator documentation as the spec inspiration.

**Step 7 — Publish.** `cargo publish`, announce alongside `mail-threading`.

**Step 8 — Replace inside mxr.** `mxr-search` depends on the new crate.

## Estimated effort

**Roughly 1 day, agent-assisted, for the Rust crate alone.**

See [00-publishing-strategy.md](./00-publishing-strategy.md) for the
AI-era effort framework. The pre-agent estimate of "2–3 days" assumed
human typing for the `Display` impl, property tests, and
`Custom`-variant refactor. With agents driving the mechanical work,
the remaining human time is API design judgment — specifically, the
custom-filter extensibility decision (Option A enum-with-`Custom`
vs Option B trait-object) and the visitor trait shape.

## TS / npm distribution

**Recommended approach: WASM via `wasm-pack`, *probably*.** Reasoning
below; this is the one Tier 1 where the choice is genuinely open.

The npm ecosystem has nothing comparable. Generic `mongo-querystring`,
`liqe`, `search-query-parser` style crates exist but cover toy syntax
and don't produce a portable AST. Audience is large — every webmail
client, every email-search admin tool, every CRM-style inbox view
wants Gmail-style operators.

**Why WASM is probably correct here (unlike the other two Tier 1s):**

The operator surface will keep evolving. Gmail adds operators
periodically (it has added `subjectincludes:`, `has:reaction`, and
folder-scoped operators in recent years). Edge cases get found in
escaping, quoting, date arithmetic, and Unicode handling.

Each of those updates becomes a parity problem if there are two
implementations. The shared-corpus pattern still works, but the
frequency of updates makes it more painful than for stable algorithms
like JWZ. WASM eliminates the parity problem by construction — one
parser, both ecosystems.

The downsides of WASM (bundle size ~300KB, startup ~5ms, JS↔WASM
marshalling per `parse()` call) are negligible for a query parser:
queries are short, parsed once per user search, not in tight loops.

**When to pick the alternative (TS port + corpus):**

If we want strictly native TS debugging (source maps to actual
TypeScript control flow), or if the WASM bundle becomes a problem in
target environments (extension bundles, edge runtimes with tight size
limits). For most webmail/Electron consumers, this isn't a real
constraint.

**Ship order.** This is the **third** Tier 1 crate, deliberately
after `01-list-unsubscribe` and `02-jwz-threading`. By the time we get
here, both the dual-publish workflow and the shared-corpus pattern
are validated. We can focus entirely on the open API questions
(custom-filter shape, Display round-trip semantics, AROUND ergonomics)
without juggling distribution unknowns.

**Effort with dual publish via WASM.** ~1 day total: Rust crate
publication + `wasm-pack` build setup + TypeScript wrapper + npm
publish. Less corpus work than the TS-port path (only Rust needs to
match the corpus; the WASM is the JS impl).

## Risks and unknowns

- **Operator drift between providers.** Gmail, Outlook, Fastmail, and
  Proton all use slightly different operator names. The crate should
  default to the Gmail vocabulary (most common) and offer aliasing in
  `ParserOptions` (`{"sender": "from"}`).

- **Quoted string escaping edge cases.** Gmail allows `"foo \"bar\""`
  with backslash-escaped inner quotes. Verify our parser handles this; if
  not, add tests and fix before publishing.

- **Date arithmetic timezone.** `older_than:5d` is computed relative to
  "now" — should this be UTC or the caller's local timezone? Expose a
  `ParserOptions.now_provider` so callers can inject for testability.

- **AST-level NOT semantics.** `from:foo -from:bar` is `from:foo AND
  NOT from:bar`. Our parser handles this; document the semantics
  explicitly in rustdoc to head off confusion.

## When to re-evaluate

- If tantivy exposes a richer public `QueryAst` type, parts of this
  could degrade to "just use tantivy's AST". Unlikely soon — tantivy's
  AST is performance-oriented, not user-friendly.
- If Gmail meaningfully changes its operator vocabulary, fast-follow with
  a release.

## Naming

Candidates:

- `gmail-query` — descriptive, but tied to Google's name
- `mail-query` — clean, parallel to `mail-parser`
- `email-query-parser` — too long
- `imap-query` — wrong; IMAP SEARCH has a different syntax (RFC 3501 §6.4.4)
- `mailsearch-query` — clunky
- `qparse` — too cryptic

Recommended: **`mail-query`**. Operator vocabulary docs can say "Gmail
syntax", but the crate name should be vendor-neutral.

## Bonus: should we also publish an IMAP SEARCH parser?

IMAP SEARCH (`SUBJECT "foo" SINCE 1-Jan-2024 NOT FLAGGED`) has its own
grammar (RFC 3501 §6.4.4). A second crate `imap-search-query` could
parse it to the same `QueryNode` AST as a normalisation layer. Out of
scope for the first release, but worth noting as a future direction.
